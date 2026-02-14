use crate::SourceMap;
use crate::devices::{
    BjtSpec, CapacitorSpec, Devices, DiodeSpec, IndependentSourceSpec, InductorSpec, ResistorSpec,
};
use crate::error::{ParserError, SpicyError};
use crate::expr::{PlaceholderMap, Scope, Value};
use crate::lexer::{Token, TokenKind, token_text};
use crate::netlist_models::DeviceModel;
use crate::netlist_types::{
    AcCommand, AcSweepType, Command, CommandType, CurrentBranchIndex, DcCommand, DeviceType,
    NodeName, OpCommand, Phasor, TranCommand,
};
use crate::netlist_waveform::WaveForm;
use crate::parser_utils::{
    parse_bool, parse_expr_into_value, parse_ident, parse_node, parse_usize,
};
use crate::statement_phase::StmtCursor;
use crate::subcircuit_phase::{ExpandedDeck, ScopedStmt};

use crate::node_mapping::NodeMapping;

#[derive(Debug)]
pub struct Deck {
    pub title: String,
    pub node_mapping: NodeMapping,
    pub commands: Vec<Command>,
    pub devices: Devices,
}

#[derive(Debug)]
pub(crate) struct ParamSlot<'s> {
    pub canonical: &'s str,
    // pub aliases: Vec<&'s str>,
    pub is_ident: bool,
    pub is_flag: bool,
}

impl<'s> ParamSlot<'s> {
    pub fn ident(canonical: &'s str) -> Self {
        Self {
            canonical,
            is_ident: true,
            is_flag: false,
        }
    }

    pub fn other(canonical: &'s str) -> Self {
        Self {
            canonical,
            is_ident: false,
            is_flag: false,
        }
    }

    pub fn flag(canonical: &'s str) -> Self {
        Self {
            canonical,
            is_ident: true,
            is_flag: true,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ParsedParam<'s> {
    pub name: &'s str,
    pub cursor: StmtCursor<'s>,
}

pub(crate) struct ParamParser<'s> {
    input: &'s str,
    params_order: Vec<ParamSlot<'s>>,
    param_cursors: Vec<StmtCursor<'s>>,
    current_cursor: usize,
    current_param: usize,
    named_mode: bool,
}

impl<'s> ParamParser<'s> {
    pub(crate) fn new(
        input: &'s str,
        params_order: Vec<ParamSlot<'s>>,
        cursor: &StmtCursor<'s>,
    ) -> Self {
        ParamParser {
            input,
            params_order,
            param_cursors: cursor.split_on_whitespace(),
            named_mode: false,
            current_param: 0,
            current_cursor: 0,
        }
    }

    fn parse_named_param(
        &mut self,
        mut cursor: StmtCursor<'s>,
    ) -> Result<ParsedParam<'s>, SpicyError> {
        let Ok(ident) = cursor.expect(TokenKind::Ident) else {
            return Err(ParserError::MissingToken {
                message: "ident",
                span: Some(cursor.span),
            }
            .into());
        };
        let ident_str = token_text(self.input, ident);

        let Some(param) = self.params_order.iter().find(|p| p.canonical == ident_str) else {
            return Err(ParserError::InvalidParam {
                param: ident_str.to_string(),
                span: cursor.span,
            }
            .into());
        };

        if !param.is_flag {
            let Ok(_equal_sign) = cursor.expect(TokenKind::Equal) else {
                return Err(ParserError::MissingToken {
                    message: "equal",
                    span: Some(cursor.span),
                }
                .into());
            };
            return Ok(ParsedParam {
                name: ident_str,
                cursor,
            });
        }

        if cursor.consume(TokenKind::Equal).is_some() {
            return Err(ParserError::MissingToken {
                message: "flag does not take a value",
                span: Some(cursor.span),
            }
            .into());
        }
        Ok(ParsedParam {
            name: ident_str,
            cursor,
        })
    }
}

impl<'s> Iterator for ParamParser<'s> {
    type Item = Result<ParsedParam<'s>, SpicyError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_cursor >= self.param_cursors.len() {
            None
        } else {
            let cursor = self.param_cursors[self.current_cursor].clone();

            let item = if !self.named_mode {
                if cursor.contains(TokenKind::Equal) {
                    self.named_mode = true;
                    match self.parse_named_param(cursor) {
                        Ok(param) => Some(Ok(param)),
                        Err(e) => Some(Err(e)),
                    }
                } else {
                    let is_ident = cursor
                        .peek_non_whitespace()
                        .map(|t| t.kind == TokenKind::Ident)
                        .unwrap_or(false);

                    match self.params_order.get(self.current_param) {
                        Some(p) => {
                            if is_ident != p.is_ident {
                                self.current_param += 1;
                                match self.params_order.get(self.current_param) {
                                    Some(p) => {
                                        Some(Ok(ParsedParam { name: p.canonical, cursor }))
                                    }
                                    None => Some(Err(ParserError::TooManyParameters {
                                        index: self.current_param,
                                        span: cursor.span,
                                    }
                                    .into())),
                                }
                            } else {
                                Some(Ok(ParsedParam { name: p.canonical, cursor }))
                            }
                        }
                        None => Some(Err(ParserError::TooManyParameters {
                            index: self.current_param,
                            span: cursor.span,
                        }
                        .into())),
                    }
                }
            } else {
                match self.parse_named_param(cursor) {
                    Ok(param) => Some(Ok(param)),
                    Err(e) => Some(Err(e)),
                }
            };
            self.current_param += 1;
            self.current_cursor += 1;
            item
        }
    }
}

pub(crate) struct InstanceParser<'s> {
    expanded_deck: ExpandedDeck,
    placeholder_map: PlaceholderMap,
    source_map: &'s SourceMap,
}

impl<'s> InstanceParser<'s> {
    pub(crate) fn new(
        expanded_deck: ExpandedDeck,
        placeholder_map: PlaceholderMap,
        source_map: &'s SourceMap,
    ) -> Self {
        InstanceParser {
            expanded_deck,
            placeholder_map,
            source_map,
        }
    }

    fn parse_title(&self, statement: &ScopedStmt) -> String {
        let input = self
            .source_map
            .get_content(statement.stmt.span.source_index);
        input[statement.stmt.span.start..=statement.stmt.span.end].to_string()
    }

    fn parse_comment(&self, statement: &ScopedStmt) -> String {
        let input = self
            .source_map
            .get_content(statement.stmt.span.source_index);

        input[statement.stmt.span.start..=statement.stmt.span.end].to_string()
    }

    fn parse_value(&self, cursor: &mut StmtCursor, scope: &Scope) -> Result<Value, SpicyError> {
        let input = self.source_map.get_content(cursor.span.source_index);
        parse_expr_into_value(cursor, input, &self.placeholder_map, scope)
    }

    fn parse_in_parentheses(
        &self,
        cursor: &mut StmtCursor,
        scope: &Scope,
    ) -> Result<Vec<Value>, SpicyError> {
        cursor.expect(TokenKind::LeftParen)?;
        let in_parentheses = cursor.split_on(TokenKind::RightParen)?;
        let mut values = Vec::new();
        for mut value_tokens in in_parentheses.split_on_whitespace().into_iter() {
            // TODO: should probably support typechecking here
            let value = self.parse_value(&mut value_tokens, scope)?;
            values.push(value);
        }
        cursor.expect(TokenKind::RightParen)?;

        Ok(values)
    }

    fn parse_waveform(
        &self,
        ident_token: &Token,
        cursor: &mut StmtCursor,
        scope: &Scope,
    ) -> Result<WaveForm, SpicyError> {
        let input = self.source_map.get_content(ident_token.span.source_index);
        let ident = token_text(input, ident_token);
        let waveform =
            match ident.to_uppercase().as_str() {
                "SIN" => {
                    let values = self.parse_in_parentheses(cursor, scope)?;
                    WaveForm::Sinusoidal {
                        offset: values.first().cloned().ok_or_else(|| {
                            ParserError::MissingToken {
                                message: "expected offset value for SIN waveform",
                                span: cursor.peek_span(),
                            }
                        })?,
                        amplitude: values.get(1).cloned().ok_or_else(|| {
                            ParserError::MissingToken {
                                message: "expected amplitude value for SIN waveform",
                                span: cursor.peek_span(),
                            }
                        })?,
                        frequency: values.get(2).cloned(),
                        delay: values.get(3).cloned(),
                        damping_factor: values.get(4).cloned(),
                        phase: values.get(5).cloned(),
                    }
                }
                "EXP" => {
                    let values = self.parse_in_parentheses(cursor, scope)?;
                    WaveForm::Exponential {
                        initial_value: values.first().cloned().ok_or_else(|| {
                            ParserError::MissingToken {
                                message: "expected initial value for EXP waveform",
                                span: cursor.peek_span(),
                            }
                        })?,
                        pulsed_value: values.get(1).cloned().ok_or_else(|| {
                            ParserError::MissingToken {
                                message: "expected pulsed value for EXP waveform",
                                span: cursor.peek_span(),
                            }
                        })?,
                        rise_delay_time: values.get(2).cloned(),
                        rise_time_constant: values.get(3).cloned(),
                        fall_delay_time: values.get(4).cloned(),
                        fall_time_constant: values.get(5).cloned(),
                    }
                }
                "PULSE" => {
                    // TODO: the right thing to do here for type safety is probably something like we did
                    // with ParamParser, then we don't need to cast the number of pulses to a u64
                    let values = self.parse_in_parentheses(cursor, scope)?;
                    WaveForm::Pulse {
                        voltage1: values.first().cloned().ok_or_else(|| {
                            ParserError::MissingToken {
                                message: "expected voltage1 value for PULSE waveform",
                                span: cursor.peek_span(),
                            }
                        })?,
                        voltage2: values.get(1).cloned().ok_or_else(|| {
                            ParserError::MissingToken {
                                message: "expected voltage2 value for PULSE waveform",
                                span: cursor.peek_span(),
                            }
                        })?,
                        delay: values.get(2).cloned(),
                        rise_time: values.get(3).cloned(),
                        fall_time: values.get(4).cloned(),
                        pulse_width: values.get(5).cloned(),
                        period: values.get(6).cloned(),
                        number_of_pulses: values.get(7).cloned().map(|v| v.get_value() as u64),
                    }
                }
                _ => {
                    return Err(ParserError::InvalidOperation {
                        operation: ident.to_string(),
                        span: ident_token.span,
                    }
                    .into());
                }
            };

        Ok(waveform)
    }

    fn parse_node(&self, cursor: &mut StmtCursor, scope: &Scope) -> Result<NodeName, SpicyError> {
        let input = self.source_map.get_content(cursor.span.source_index);
        let node = parse_node(cursor, input)?;

        if let Some(node) = scope.node_mapping.get(&node) {
            Ok(node.clone())
        } else {
            Ok(node)
        }
    }

    fn parse_bool(&self, cursor: &mut StmtCursor, scope: &Scope) -> Result<bool, SpicyError> {
        if let Some(token) = cursor.consume(TokenKind::Placeholder) {
            let id = token.id.expect("must have a placeholder id");
            // TOOD: maybe we can change the expresion to only evalute once
            let expr = self.placeholder_map.get(id).clone();
            let evaluated = expr.evaluate(scope)?;
            // TODO: kinda ugly
            if evaluated.get_value() == 0.0 {
                return Ok(false);
            }
            if evaluated.get_value() == 1.0 {
                return Ok(true);
            }
            return Err(ParserError::ExpectedBoolZeroOrOne { span: token.span }.into());
        }
        let input = self.source_map.get_content(cursor.span.source_index);
        parse_bool(cursor, input)
    }

    fn parse_usize(&self, cursor: &mut StmtCursor, scope: &Scope) -> Result<usize, SpicyError> {
        if let Some(token) = cursor.consume(TokenKind::Placeholder) {
            let id = token.id.expect("must have a placeholder id");
            // TOOD: maybe we can change the expresion to only evalute once
            let expr = self.placeholder_map.get(id).clone();
            let evaluated = expr.evaluate(scope)?;
            let value = evaluated.get_value();
            // TODO: baba
            // Check if value is an integer (no fractional part)
            if value.fract() == 0.0 {
                // Safe to cast to usize if non-negative
                if value >= 0.0 {
                    return Ok(value as usize);
                } else {
                    return Err(ParserError::InvalidNumericLiteral {
                        span: Some(token.span),
                        lexeme: format!("{:?}", evaluated),
                    }
                    .into());
                }
            } else {
                return Err(ParserError::InvalidNumericLiteral {
                    span: Some(token.span),
                    lexeme: format!("{:?}", evaluated),
                }
                .into());
            }
        }
        let input = self.source_map.get_content(cursor.span.source_index);
        parse_usize(cursor, input)
    }

    // RXXXXXXX n+ n- <resistance|r=>value <ac=val> <m=val>
    // + <scale=val> <temp=val> <dtemp=val> <tc1=val> <tc2=val>
    // + <noisy=0|1>
    fn parse_resistor(
        &self,
        name: String,
        cursor: &mut StmtCursor,
        scope: &Scope,
        node_mapping: &mut NodeMapping,
    ) -> Result<ResistorSpec, SpicyError> {
        let positive = self.parse_node(cursor, scope)?;
        let negative = self.parse_node(cursor, scope)?;

        let positive_node = node_mapping.insert_node(positive);
        let negative_node = node_mapping.insert_node(negative);

        let mut resistor = ResistorSpec::new(name, cursor.span, positive_node, negative_node);

        let params_order = vec![
            ParamSlot::other("resistance"),
            ParamSlot::ident("mname"),
            ParamSlot::other("ac"),
            ParamSlot::other("m"),
            ParamSlot::other("scale"),
            ParamSlot::other("temp"),
            ParamSlot::other("dtemp"),
            ParamSlot::other("tc1"),
            ParamSlot::other("tc2"),
            ParamSlot::other("noisy"),
        ];
        let input = self.source_map.get_content(cursor.span.source_index);
        let params = ParamParser::new(input, params_order, cursor);
        for item in params {
            let ParsedParam {
                name: ident,
                mut cursor,
            } = item?;
            match ident {
                "resistance" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    resistor.set_resistance(value);
                }
                "mname" => {
                    let model_name = parse_ident(&mut cursor, input)?;
                    let model = self
                        .expanded_deck
                        .model_table
                        .get(model_name.text)
                        .ok_or_else(|| ParserError::MissingModel {
                            model: model_name.text.to_string(),
                            span: model_name.span,
                        })?;
                    if let DeviceModel::Resistor(model) = model {
                        resistor.set_model(model.clone());
                    } else {
                        return Err(ParserError::InvalidModel {
                            model: model_name.text.to_string(),
                            span: model_name.span,
                        }
                        .into());
                    }
                }
                "ac" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    resistor.set_ac(value);
                }
                "m" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    resistor.set_m(value);
                }
                "scale" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    resistor.set_scale(value);
                }
                "temp" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    resistor.set_temp(value);
                }
                "dtemp" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    resistor.set_dtemp(value);
                }
                "tc1" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    resistor.set_tc1(value);
                }
                "tc2" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    resistor.set_tc2(value);
                }
                "noisy" => {
                    let value = self.parse_bool(&mut cursor, scope)?;
                    resistor.set_noisy(value);
                }
                _ => {
                    return Err(ParserError::InvalidParam {
                        param: ident.to_string(),
                        span: cursor.span,
                    }
                    .into());
                }
            }
        }
        Ok(resistor)
    }

    // CXXXXXXX n+ n- <value> <mname> <m=val> <scale=val> <temp=val>
    // + <dtemp=val> <tc1=val> <tc2=val> <ic=init_condition>
    fn parse_capacitor(
        &self,
        name: String,
        cursor: &mut StmtCursor,
        scope: &Scope,
        node_mapping: &mut NodeMapping,
    ) -> Result<CapacitorSpec, SpicyError> {
        let positive = self.parse_node(cursor, scope)?;
        let negative = self.parse_node(cursor, scope)?;

        let positive_node = node_mapping.insert_node(positive);
        let negative_node = node_mapping.insert_node(negative);

        let mut capacitor = CapacitorSpec::new(name, cursor.span, positive_node, negative_node);

        let params_order = vec![
            ParamSlot::other("capacitance"),
            ParamSlot::ident("mname"),
            ParamSlot::other("m"),
            ParamSlot::other("scale"),
            ParamSlot::other("temp"),
            ParamSlot::other("dtemp"),
            ParamSlot::other("tc1"),
            ParamSlot::other("tc2"),
            ParamSlot::other("ic"),
        ];
        let input = self.source_map.get_content(cursor.span.source_index);
        let params = ParamParser::new(input, params_order, cursor);

        for item in params {
            let ParsedParam {
                name: ident,
                mut cursor,
            } = item?;
            match ident {
                "capacitance" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    capacitor.set_capacitance(value);
                }
                "mname" => {
                    let model_name = parse_ident(&mut cursor, input)?;
                    let model = self
                        .expanded_deck
                        .model_table
                        .get(model_name.text)
                        .ok_or_else(|| ParserError::MissingModel {
                            model: model_name.text.to_string(),
                            span: model_name.span,
                        })?;
                    if let DeviceModel::Capacitor(model) = model {
                        capacitor.set_model(model.clone());
                    } else {
                        return Err(ParserError::InvalidModel {
                            model: model_name.text.to_string(),
                            span: model_name.span,
                        }
                        .into());
                    }
                }
                "m" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    capacitor.set_m(value);
                }
                "scale" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    capacitor.set_scale(value);
                }
                "temp" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    capacitor.set_temp(value);
                }
                "dtemp" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    capacitor.set_dtemp(value);
                }
                "tc1" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    capacitor.set_tc1(value);
                }
                "tc2" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    capacitor.set_tc2(value);
                }
                "ic" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    capacitor.set_ic(value);
                }
                _ => {
                    return Err(ParserError::InvalidParam {
                        param: ident.to_string(),
                        span: cursor.span,
                    }
                    .into());
                }
            }
        }

        Ok(capacitor)
    }

    // LYYYYYYY n+ n- <value> <mname> <nt=val> <m=val>
    // + <scale=val> <temp=val> <dtemp=val> <tc1=val>
    // + <tc2=val> <ic=init_condition>
    fn parse_inductor(
        &self,
        name: String,
        cursor: &mut StmtCursor,
        scope: &Scope,
        node_mapping: &mut NodeMapping,
    ) -> Result<InductorSpec, SpicyError> {
        let positive = self.parse_node(cursor, scope)?;
        let negative = self.parse_node(cursor, scope)?;

        let positive_node = node_mapping.insert_node(positive);
        let negative_node = node_mapping.insert_node(negative);
        let current_branch = node_mapping.insert_branch(name.clone());

        let mut inductor = InductorSpec::new(
            name,
            cursor.span,
            positive_node,
            negative_node,
            current_branch,
        );

        let params_order = vec![
            ParamSlot::other("inductance"),
            ParamSlot::ident("mname"),
            ParamSlot::other("nt"),
            ParamSlot::other("m"),
            ParamSlot::other("scale"),
            ParamSlot::other("temp"),
            ParamSlot::other("dtemp"),
            ParamSlot::other("tc1"),
            ParamSlot::other("tc2"),
            ParamSlot::other("ic"),
        ];
        let input = self.source_map.get_content(cursor.span.source_index);
        let params = ParamParser::new(input, params_order, cursor);

        for item in params {
            let ParsedParam {
                name: ident,
                mut cursor,
            } = item?;
            match ident {
                "inductance" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    inductor.set_inductance(value);
                }
                "mname" => {
                    let model_name = parse_ident(&mut cursor, input)?;
                    let model = self
                        .expanded_deck
                        .model_table
                        .get(model_name.text)
                        .ok_or_else(|| ParserError::MissingModel {
                            model: model_name.text.to_string(),
                            span: model_name.span,
                        })?;
                    if let DeviceModel::Inductor(model) = model {
                        inductor.set_model(model.clone());
                    } else {
                        return Err(ParserError::InvalidModel {
                            model: model_name.text.to_string(),
                            span: model_name.span,
                        }
                        .into());
                    }
                }
                "nt" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    inductor.set_nt(value);
                }
                "m" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    inductor.set_m(value);
                }
                "scale" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    inductor.set_scale(value);
                }
                "temp" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    inductor.set_temp(value);
                }
                "dtemp" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    inductor.set_dtemp(value);
                }
                "tc1" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    inductor.set_tc1(value);
                }
                "tc2" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    inductor.set_tc2(value);
                }
                "ic" => {
                    let value = self.parse_value(&mut cursor, scope)?;
                    inductor.set_ic(value);
                }
                _ => {
                    return Err(ParserError::InvalidParam {
                        param: ident.to_string(),
                        span: cursor.span,
                    }
                    .into());
                }
            }
        }

        Ok(inductor)
    }

    // DXXXXXXX n+ n- mname <area=val> <m=val> <pj=val> <off>
    // + <ic=vd> <temp=val> <dtemp=val>
    // + <lm=val> <wm=val> <lp=val> <wp=val>
    fn parse_diode(
        &self,
        name: String,
        cursor: &mut StmtCursor,
        scope: &Scope,
        node_mapping: &mut NodeMapping,
    ) -> Result<DiodeSpec, SpicyError> {
        let positive = self.parse_node(cursor, scope)?;
        let negative = self.parse_node(cursor, scope)?;

        let positive_node = node_mapping.insert_node(positive);
        let negative_node = node_mapping.insert_node(negative);

        let input = self.source_map.get_content(cursor.span.source_index);
        let model_name = parse_ident(cursor, input)?;
        let model = self
            .expanded_deck
            .model_table
            .get(model_name.text)
            .ok_or_else(|| ParserError::MissingModel {
                model: model_name.text.to_string(),
                span: model_name.span,
            })?;

        let DeviceModel::Diode(model) = model else {
            return Err(ParserError::InvalidModel {
                model: model_name.text.to_string(),
                span: model_name.span,
            }
            .into());
        };

        let mut diode = DiodeSpec::new(
            name,
            cursor.span,
            positive_node,
            negative_node,
            model.clone(),
        );

        let params_order = vec![
            ParamSlot::other("area"),
            ParamSlot::other("m"),
            ParamSlot::other("pj"),
            ParamSlot::flag("off"),
            ParamSlot::other("ic"),
            ParamSlot::other("temp"),
            ParamSlot::other("dtemp"),
            ParamSlot::other("lm"),
            ParamSlot::other("wm"),
            ParamSlot::other("lp"),
            ParamSlot::other("wp"),
        ];
        let params = ParamParser::new(input, params_order, cursor);
        for item in params {
            let ParsedParam {
                name: ident,
                mut cursor,
            } = item?;
            match ident {
                "area" => diode.set_area(self.parse_value(&mut cursor, scope)?),
                "m" => diode.set_m(self.parse_value(&mut cursor, scope)?),
                "pj" => diode.set_pj(self.parse_value(&mut cursor, scope)?),
                "off" => diode.set_off(true),
                "ic" => diode.set_ic(self.parse_value(&mut cursor, scope)?),
                "temp" => diode.set_temp(self.parse_value(&mut cursor, scope)?),
                "dtemp" => diode.set_dtemp(self.parse_value(&mut cursor, scope)?),
                "lm" => diode.set_lm(self.parse_value(&mut cursor, scope)?),
                "wm" => diode.set_wm(self.parse_value(&mut cursor, scope)?),
                "lp" => diode.set_lp(self.parse_value(&mut cursor, scope)?),
                "wp" => diode.set_wp(self.parse_value(&mut cursor, scope)?),
                _ => {
                    return Err(ParserError::InvalidParam {
                        param: ident.to_string(),
                        span: cursor.span,
                    }
                    .into());
                }
            }
        }

        Ok(diode)
    }

    // QXXXXXXX nc nb ne mname <area=val>
    // + <m=val> <off> <ic=vbe,vce>
    fn parse_bjt(
        &self,
        name: String,
        cursor: &mut StmtCursor,
        scope: &Scope,
        node_mapping: &mut NodeMapping,
    ) -> Result<BjtSpec, SpicyError> {
        let collector = self.parse_node(cursor, scope)?;
        let base = self.parse_node(cursor, scope)?;
        let emitter = self.parse_node(cursor, scope)?;

        let collector_node = node_mapping.insert_node(collector);
        let base_node = node_mapping.insert_node(base);
        let emitter_node = node_mapping.insert_node(emitter);

        let input = self.source_map.get_content(cursor.span.source_index);
        let model_name = parse_ident(cursor, input)?;
        let model = self
            .expanded_deck
            .model_table
            .get(model_name.text)
            .ok_or_else(|| ParserError::MissingModel {
                model: model_name.text.to_string(),
                span: model_name.span,
            })?;

        let DeviceModel::Bjt(model) = model else {
            return Err(ParserError::InvalidModel {
                model: model_name.text.to_string(),
                span: model_name.span,
            }
            .into());
        };

        let mut bjt = BjtSpec::new(
            name,
            cursor.span,
            collector_node,
            base_node,
            emitter_node,
            model.clone(),
        );

        let params_order = vec![
            ParamSlot::other("area"),
            ParamSlot::other("m"),
            ParamSlot::flag("off"),
            ParamSlot::other("ic"),
        ];
        let params = ParamParser::new(input, params_order, cursor);
        for item in params {
            let ParsedParam {
                name: ident,
                mut cursor,
            } = item?;
            match ident {
                "area" => bjt.set_area(self.parse_value(&mut cursor, scope)?),
                "m" => bjt.set_m(self.parse_value(&mut cursor, scope)?),
                "off" => bjt.set_off(true),
                "ic" => {
                    let vbe = self.parse_value(&mut cursor, scope)?;
                    let vce = if cursor.consume(TokenKind::Comma).is_some() {
                        Some(self.parse_value(&mut cursor, scope)?)
                    } else {
                        None
                    };
                    bjt.set_ic(vbe, vce);
                }
                _ => {
                    return Err(ParserError::InvalidParam {
                        param: ident.to_string(),
                        span: cursor.span,
                    }
                    .into());
                }
            }
        }

        Ok(bjt)
    }

    fn parse_source_value(
        &self,
        cursor: &mut StmtCursor,
        scope: &Scope,
        independent_source: &mut IndependentSourceSpec,
    ) -> Result<(), SpicyError> {
        cursor.skip_ws();
        if let Some(token) = cursor.consume(TokenKind::Ident) {
            let input = self.source_map.get_content(token.span.source_index);

            let operation = token_text(input, token);

            match operation {
                "DC" => {
                    independent_source.set_dc(WaveForm::Constant(self.parse_value(cursor, scope)?))
                }
                "AC" => {
                    let mag = self.parse_value(cursor, scope)?;
                    let mut phasor = Phasor::new(mag);
                    if cursor.peek_non_whitespace().is_some() {
                        let phase = self.parse_value(cursor, scope)?;
                        phasor.set_phase(phase);
                    }

                    independent_source.set_ac(phasor);
                }
                _ => independent_source.set_dc(self.parse_waveform(token, cursor, scope)?),
            };
        } else {
            independent_source.set_dc(WaveForm::Constant(self.parse_value(cursor, scope)?))
        }

        Ok(())
    }

    // VXXXXXXX N+ N- <<DC> DC/TRAN VALUE> <AC <ACMAG <ACPHASE>>>
    // + <DISTOF1 <F1MAG <F1PHASE>>> <DISTOF2 <F2MAG <F2PHASE>>>
    // IYYYYYYY N+ N- <<DC> DC/TRAN VALUE> <AC <ACMAG <ACPHASE>>>
    // + <DISTOF1 <F1MAG <F1PHASE>>> <DISTOF2 <F2MAG <F2PHASE>>>
    fn parse_independent_source(
        &self,
        name: String,
        cursor: &mut StmtCursor,
        scope: &Scope,
        node_mapping: &mut NodeMapping,
        alloc_branch: bool,
    ) -> Result<IndependentSourceSpec, SpicyError> {
        let positive = self.parse_node(cursor, scope)?;
        let negative = self.parse_node(cursor, scope)?;

        let positive_node = node_mapping.insert_node(positive);
        let negative_node = node_mapping.insert_node(negative);
        let current_branch = if alloc_branch {
            node_mapping.insert_branch(name.clone())
        } else {
            CurrentBranchIndex(0)
        };

        let mut independent_source =
            IndependentSourceSpec::new(name, positive_node, negative_node, current_branch);

        self.parse_source_value(cursor, scope, &mut independent_source)?;
        let next_token = cursor.peek_non_whitespace();

        match next_token {
            Some(token) if token.kind == TokenKind::Ident => {
                self.parse_source_value(cursor, scope, &mut independent_source)?;
            }
            Some(token) => {
                return Err(ParserError::UnexpectedToken {
                    expected: "ident".to_string(),
                    found: token.kind,
                    span: token.span,
                }
                .into());
            }
            None => {}
        }

        Ok(independent_source)
    }

    fn parse_device(
        &self,
        statement: &ScopedStmt,
        node_mapping: &mut NodeMapping,
        devices: &mut Devices,
    ) -> Result<(), SpicyError> {
        let mut cursor = statement.stmt.as_cursor();
        let ident = cursor.expect(TokenKind::Ident)?;

        let input = self.source_map.get_content(ident.span.source_index);
        let ident_string = token_text(input, ident).to_string();
        // Identifiers can be UTF-8; don't use byte offsets.
        let first = ident_string
            .chars()
            .next()
            .expect("lexer produced an Ident token, so it must be non-empty");
        let element_type = DeviceType::from_char(first)?;
        let scope = self.expanded_deck.scope_arena.get(statement.scope);

        let name = scope.get_device_name(&ident_string);

        match element_type {
            DeviceType::Resistor => devices.resistors.push(self.parse_resistor(
                name,
                &mut cursor,
                scope,
                node_mapping,
            )?),
            DeviceType::Capacitor => devices.capacitors.push(self.parse_capacitor(
                name,
                &mut cursor,
                scope,
                node_mapping,
            )?),
            DeviceType::Inductor => devices.inductors.push(self.parse_inductor(
                name,
                &mut cursor,
                scope,
                node_mapping,
            )?),
            DeviceType::Diode => {
                devices
                    .diodes
                    .push(self.parse_diode(name, &mut cursor, scope, node_mapping)?)
            }
            DeviceType::Bjt => devices
                .bjts
                .push(self.parse_bjt(name, &mut cursor, scope, node_mapping)?),
            DeviceType::VoltageSource => devices.voltage_sources.push(
                self.parse_independent_source(name, &mut cursor, scope, node_mapping, true)?,
            ),
            DeviceType::CurrentSource => devices.current_sources.push(
                self.parse_independent_source(name, &mut cursor, scope, node_mapping, false)?,
            ),
            _ => {
                return Err(ParserError::InvalidDeviceType {
                    s: element_type.to_char().to_string(),
                }
                .into());
            }
        };

        Ok(())
    }

    // .dc srcnam vstart vstop vincr [src2 start2 stop2 incr2]
    fn parse_dc_command(
        &self,
        cursor: &mut StmtCursor,
        scope: &Scope,
    ) -> Result<DcCommand, SpicyError> {
        let input = self.source_map.get_content(cursor.span.source_index);
        let srcnam = parse_ident(cursor, input)?;
        let vstart = self.parse_value(cursor, scope)?;
        let vstop = self.parse_value(cursor, scope)?;
        let vincr = self.parse_value(cursor, scope)?;

        Ok(DcCommand {
            span: cursor.span,
            srcnam: srcnam.text.to_string(),
            vstart,
            vstop,
            vincr,
        })
    }

    fn parse_ac_command(
        &self,
        cursor: &mut StmtCursor,
        scope: &Scope,
    ) -> Result<AcCommand, SpicyError> {
        let input = self.source_map.get_content(cursor.span.source_index);
        let ac_sweep_type = parse_ident(cursor, input)?;
        let points_per_sweep = self.parse_usize(cursor, scope)?;
        let ac_sweep_type = match ac_sweep_type.text {
            "DEC" | "dec" => AcSweepType::Dec(points_per_sweep),
            "OCT" | "oct" => AcSweepType::Oct(points_per_sweep),
            "LIN" | "lin" => AcSweepType::Lin(points_per_sweep),
            _ => {
                return Err(ParserError::InvalidOperation {
                    operation: ac_sweep_type.text.to_string(),
                    span: cursor.span,
                }
                .into());
            }
        };
        let fstart = self.parse_value(cursor, scope)?;
        let fstop = self.parse_value(cursor, scope)?;

        Ok(AcCommand {
            span: cursor.span,
            ac_sweep_type,
            fstart,
            fstop,
        })
    }

    fn parse_trans_command(
        &self,
        cursor: &mut StmtCursor,
        scope: &Scope,
    ) -> Result<TranCommand, SpicyError> {
        let tstep = self.parse_value(cursor, scope)?;
        let tstop = self.parse_value(cursor, scope)?;

        #[allow(unused_assignments)]
        let mut uic = false;
        match cursor.peek_non_whitespace() {
            // .tran tstep tstop
            None => {}
            Some(t) if t.kind == TokenKind::Ident => {
                let input = self.source_map.get_content(t.span.source_index);
                let ident = parse_ident(cursor, input)?;
                if ident.text.to_uppercase() == "UIC" {
                    uic = true;
                } else {
                    return Err(ParserError::UnexpectedToken {
                        expected: "UIC".to_string(),
                        found: t.kind,
                        span: t.span,
                    }
                    .into());
                }
            }
            // TODO: .tran tstep tstop tstart [tmax] ... (not yet supported)
            Some(_) => {
                unimplemented!("tstart and tmax are not yet implemented");
            }
        }

        Ok(TranCommand {
            span: cursor.span,
            tstep,
            tstop,
            uic,
        })
    }

    fn parse_command(&self, statement: &ScopedStmt) -> Result<Command, SpicyError> {
        let mut cursor = statement.stmt.as_cursor();
        cursor.expect(TokenKind::Dot)?;
        let ident = cursor.expect(TokenKind::Ident)?;
        let input = self.source_map.get_content(ident.span.source_index);
        let ident_string = token_text(input, ident);
        let command_type: CommandType =
            ident_string
                .parse()
                .map_err(|_| ParserError::InvalidCommandType {
                    s: ident_string.to_string(),
                    span: cursor.span,
                })?;

        let scope = self.expanded_deck.scope_arena.get(statement.scope);

        let command = match command_type {
            CommandType::DC => Command::Dc(self.parse_dc_command(&mut cursor, scope)?),
            CommandType::Op => Command::Op(OpCommand { span: cursor.span }),
            CommandType::AC => Command::Ac(self.parse_ac_command(&mut cursor, scope)?),
            CommandType::Tran => Command::Tran(self.parse_trans_command(&mut cursor, scope)?),
            CommandType::End => Command::End,
            _ => {
                return Err(ParserError::UnexpectedCommandType {
                    s: command_type.to_string(),
                    span: cursor.span,
                }
                .into());
            }
        };

        Ok(command)
    }

    pub(crate) fn parse(&mut self) -> Result<Deck, SpicyError> {
        // TODO: clone is sadge
        let mut statements_iter = self.expanded_deck.statements.clone().into_iter();
        // first line should be a title
        let title = self.parse_title(&statements_iter.next().ok_or(ParserError::MissingTitle)?);

        let mut commands = vec![];
        let mut devices = Devices::new();
        let mut node_mapping = NodeMapping::new();

        for statement in statements_iter {
            let cursor = statement.stmt.as_cursor();

            let first_token = cursor.peek().ok_or(ParserError::MissingToken {
                message: "token",
                span: Some(cursor.span),
            })?;

            match first_token.kind {
                TokenKind::Dot => {
                    match self.parse_command(&statement)? {
                        Command::End => {
                            // once we see an end command we stop
                            break;
                        }
                        command => {
                            commands.push(command);
                        }
                    }
                }
                // comment
                TokenKind::Asterisk => {
                    let _ = self.parse_comment(&statement);
                    // TODO: save comments?
                }
                TokenKind::Ident => {
                    self.parse_device(&statement, &mut node_mapping, &mut devices)?;
                }
                _ => {
                    return Err(ParserError::UnexpectedToken {
                        expected: "command or element".to_string(),
                        found: first_token.kind,
                        span: first_token.span,
                    }
                    .into());
                }
            }
        }

        Ok(Deck {
            title,
            node_mapping,
            commands,
            devices,
        })
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::{ParamParser, ParamSlot, ParsedParam};
    use crate::{
        ParseOptions,
        error::{ParserError, SpicyError},
        libs_phase::{SourceFileId, SourceMap},
        parser_utils::{parse_ident, parse_value},
        statement_phase::Statements,
    };

    use std::path::PathBuf;

    #[rstest]
    fn test_parser(#[files("tests/parser_inputs/*.spicy")] input: PathBuf) {
        use crate::parse;

        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let source_map = SourceMap::new(input.clone(), input_content);
        let mut input_options = ParseOptions {
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            source_map,
            max_include_depth: 10,
        };
        let deck = parse(&mut input_options).expect("parse");

        let name = format!(
            "parser-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        insta::assert_debug_snapshot!(name, deck);
    }

    #[test]
    fn test_param_parser_positional_flag_and_named() {
        let input = "1 2 off ic=0.7\n";
        let statements =
            Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = statements.statements[0].as_cursor();
        let params_order = vec![
            ParamSlot::other("area"),
            ParamSlot::other("m"),
            ParamSlot::flag("off"),
            ParamSlot::other("ic"),
        ];
        let mut params = ParamParser::new(input, params_order, &cursor);

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("area").expect("area ok");
        assert_eq!(name, "area");
        assert_eq!(parse_value(&mut cursor, input).expect("area value").get_value(), 1.0);

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("m").expect("m ok");
        assert_eq!(name, "m");
        assert_eq!(parse_value(&mut cursor, input).expect("m value").get_value(), 2.0);

        let ParsedParam {
            name,
            cursor: _,
        } = params.next().expect("off").expect("off ok");
        assert_eq!(name, "off");

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("ic").expect("ic ok");
        assert_eq!(name, "ic");
        assert_eq!(
            parse_value(&mut cursor, input).expect("ic value").get_value(),
            0.7
        );

        assert!(params.next().is_none());
    }

    #[test]
    fn test_param_parser_named_flag_rejects_value() {
        let input = "off=0 area=2\n";
        let statements =
            Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = statements.statements[0].as_cursor();
        let params_order = vec![ParamSlot::other("area"), ParamSlot::flag("off")];
        let mut params = ParamParser::new(input, params_order, &cursor);

        let first = params.next().expect("off");
        let err = first.expect_err("off should reject value");
        match err {
            SpicyError::Parser(ParserError::MissingToken { message, .. }) => {
                assert_eq!(message, "flag does not take a value");
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_param_parser_named_flag_after_named() {
        let input = "area=2 off\n";
        let statements =
            Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = statements.statements[0].as_cursor();
        let params_order = vec![ParamSlot::other("area"), ParamSlot::flag("off")];
        let mut params = ParamParser::new(input, params_order, &cursor);

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("area").expect("area ok");
        assert_eq!(name, "area");
        assert_eq!(parse_value(&mut cursor, input).expect("area value").get_value(), 2.0);

        let ParsedParam { name, cursor: _ } = params.next().expect("off").expect("off ok");
        assert_eq!(name, "off");

        assert!(params.next().is_none());
    }

    #[test]
    fn test_param_parser_ident_slot_skip() {
        let input = "1 2\n";
        let statements =
            Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = statements.statements[0].as_cursor();
        let params_order = vec![
            ParamSlot::other("resistance"),
            ParamSlot::ident("mname"),
            ParamSlot::other("m"),
        ];
        let mut params = ParamParser::new(input, params_order, &cursor);

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("resistance").expect("resistance ok");
        assert_eq!(name, "resistance");
        assert_eq!(
            parse_value(&mut cursor, input)
                .expect("resistance value")
                .get_value(),
            1.0
        );

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("m").expect("m ok");
        assert_eq!(name, "m");
        assert_eq!(parse_value(&mut cursor, input).expect("m value").get_value(), 2.0);

        assert!(params.next().is_none());
    }

    #[test]
    fn test_param_parser_ident_slot_value() {
        let input = "1 modelX 2\n";
        let statements =
            Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = statements.statements[0].as_cursor();
        let params_order = vec![
            ParamSlot::other("resistance"),
            ParamSlot::ident("mname"),
            ParamSlot::other("m"),
        ];
        let mut params = ParamParser::new(input, params_order, &cursor);

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("resistance").expect("resistance ok");
        assert_eq!(name, "resistance");
        assert_eq!(
            parse_value(&mut cursor, input)
                .expect("resistance value")
                .get_value(),
            1.0
        );

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("mname").expect("mname ok");
        assert_eq!(name, "mname");
        assert_eq!(
            parse_ident(&mut cursor, input)
                .expect("mname ident")
                .text,
            "modelX"
        );

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("m").expect("m ok");
        assert_eq!(name, "m");
        assert_eq!(parse_value(&mut cursor, input).expect("m value").get_value(), 2.0);

        assert!(params.next().is_none());
    }

    #[test]
    fn test_param_parser_named_missing_equal() {
        let input = "area=1 m 2\n";
        let statements =
            Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = statements.statements[0].as_cursor();
        let params_order = vec![ParamSlot::other("area"), ParamSlot::other("m")];
        let mut params = ParamParser::new(input, params_order, &cursor);

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("area").expect("area ok");
        assert_eq!(name, "area");
        assert_eq!(parse_value(&mut cursor, input).expect("area value").get_value(), 1.0);

        let err = params.next().expect("m").expect_err("m should need '='");
        match err {
            SpicyError::Parser(ParserError::MissingToken { message, .. }) => {
                assert_eq!(message, "equal");
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_param_parser_named_missing_value() {
        let input = "area=\n";
        let statements =
            Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = statements.statements[0].as_cursor();
        let params_order = vec![ParamSlot::other("area")];
        let mut params = ParamParser::new(input, params_order, &cursor);

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("area").expect("area ok");
        assert_eq!(name, "area");
        assert!(parse_value(&mut cursor, input).is_err());
        assert!(params.next().is_none());
    }

    #[test]
    fn test_param_parser_named_invalid_param() {
        let input = "bad=1\n";
        let statements =
            Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = statements.statements[0].as_cursor();
        let params_order = vec![ParamSlot::other("area")];
        let mut params = ParamParser::new(input, params_order, &cursor);

        let err = params.next().expect("bad").expect_err("bad should fail");
        match err {
            SpicyError::Parser(ParserError::InvalidParam { param, .. }) => {
                assert_eq!(param, "bad");
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_param_parser_named_rejects_positional() {
        let input = "area=1 2\n";
        let statements =
            Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = statements.statements[0].as_cursor();
        let params_order = vec![ParamSlot::other("area"), ParamSlot::other("m")];
        let mut params = ParamParser::new(input, params_order, &cursor);

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("area").expect("area ok");
        assert_eq!(name, "area");
        assert_eq!(parse_value(&mut cursor, input).expect("area value").get_value(), 1.0);

        let err = params.next().expect("m").expect_err("positional not allowed");
        match err {
            SpicyError::Parser(ParserError::MissingToken { message, .. }) => {
                assert_eq!(message, "ident");
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_param_parser_too_many_parameters() {
        let input = "1 2 3\n";
        let statements =
            Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = statements.statements[0].as_cursor();
        let params_order = vec![ParamSlot::other("a"), ParamSlot::other("b")];
        let mut params = ParamParser::new(input, params_order, &cursor);

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("a").expect("a ok");
        assert_eq!(name, "a");
        assert_eq!(parse_value(&mut cursor, input).expect("a value").get_value(), 1.0);

        let ParsedParam {
            name,
            mut cursor,
        } = params.next().expect("b").expect("b ok");
        assert_eq!(name, "b");
        assert_eq!(parse_value(&mut cursor, input).expect("b value").get_value(), 2.0);

        let err = params.next().expect("extra").expect_err("too many params");
        match err {
            SpicyError::Parser(ParserError::TooManyParameters { index, .. }) => {
                assert_eq!(index, 2);
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_param_parser_flag_only_positional() {
        let input = "off\n";
        let statements =
            Statements::new(input, SourceFileId::new(0)).expect("non-empty statement");
        let cursor = statements.statements[0].as_cursor();
        let params_order = vec![ParamSlot::flag("off")];
        let mut params = ParamParser::new(input, params_order, &cursor);

        let ParsedParam { name, cursor: _ } = params.next().expect("off").expect("off ok");
        assert_eq!(name, "off");
        assert!(params.next().is_none());
    }
}
