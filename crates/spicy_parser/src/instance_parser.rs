use crate::SourceMap;
use crate::error::{ParserError, SpicyError};
use crate::expr::{PlaceholderMap, Scope, Value};
use crate::lexer::{Token, TokenKind, token_text};
use crate::netlist_types::{
    AcCommand, AcSweepType, Capacitor, Command, CommandType, DcCommand, Device, DeviceType,
    IndependentSource, Inductor, Node, OpCommand, Phasor, Resistor, TranCommand,
};
use crate::netlist_waveform::WaveForm;
use crate::parser_utils::{parse_bool, parse_ident, parse_node, parse_usize, parse_value};
use crate::statement_phase::{StmtCursor};
use crate::subcircuit_phase::{ExpandedDeck, ScopedStmt};

#[derive(Debug)]
pub struct Deck {
    pub title: String,
    pub commands: Vec<Command>,
    pub devices: Vec<Device>,
}

pub(crate) struct ParamParser<'s> {
    input: &'s str,
    params_order: Vec<&'s str>,
    param_cursors: Vec<StmtCursor<'s>>,
    current_param: usize,
    named_mode: bool,
}

impl<'s> ParamParser<'s> {
    pub(crate) fn new(input: &'s str, params_order: Vec<&'s str>, cursor: &StmtCursor<'s>) -> Self {
        ParamParser {
            input,
            params_order,
            param_cursors: cursor.split_on_whitespace(),
            named_mode: false,
            current_param: 0,
        }
    }

    fn parse_named_param(&mut self, cursor: &mut StmtCursor) -> Result<&'s str, SpicyError> {
        let Ok(ident) = cursor.expect(TokenKind::Ident) else {
            return Err(ParserError::MissingToken {
                message: "ident",
                span: Some(cursor.span),
            }
            .into());
        };
        let ident_str = token_text(self.input, ident);

        if !self.params_order.contains(&ident_str) {
            return Err(ParserError::InvalidParam {
                param: ident_str.to_string(),
                span: cursor.span,
            }
            .into());
        }

        let Ok(_equal_sign) = cursor.expect(TokenKind::Equal) else {
            return Err(ParserError::MissingToken {
                message: "equal",
                span: Some(cursor.span),
            }
            .into());
        };
        Ok(ident_str)
    }
}

impl<'s> Iterator for ParamParser<'s> {
    type Item = Result<(&'s str, StmtCursor<'s>), SpicyError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_param >= self.param_cursors.len() {
            None
        } else {
            let mut cursor = self.param_cursors[self.current_param].clone();
            let item = if !self.named_mode {
                if cursor.contains(TokenKind::Equal) {
                    self.named_mode = true;
                    match self.parse_named_param(&mut cursor) {
                        Ok(ident) => Some(Ok((ident, cursor))),
                        Err(e) => Some(Err(e)),
                    }
                } else {
                    match self.params_order.get(self.current_param) {
                        Some(p) => Some(Ok((*p, cursor))),
                        None => Some(Err(ParserError::TooManyParameters {
                            index: self.current_param,
                            span: cursor.span,
                        }
                        .into())),
                    }
                }
            } else {
                match self.parse_named_param(&mut cursor) {
                    Ok(ident) => Some(Ok((ident, cursor))),
                    Err(e) => Some(Err(e)),
                }
            };
            self.current_param += 1;
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
        let comment = input[statement.stmt.span.start..=statement.stmt.span.end].to_string();
        comment
    }

    fn parse_value(&self, cursor: &mut StmtCursor, scope: &Scope) -> Result<Value, SpicyError> {
        cursor.skip_ws();
        if let Some(token) = cursor.consume(TokenKind::Placeholder) {
            let id = token.id.expect("must have a placeholder id");
            // TODO: maybe we can change the expression to only evaluate once
            let expr = self
                .placeholder_map
                .get(id)
                .clone();
            let evaluated = expr.evaluate(scope)?;
            return Ok(evaluated);
        }
        let input = self.source_map.get_content(cursor.span.source_index);
        Ok(parse_value(cursor, input)?)
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
                        offset: values.get(0).cloned().ok_or_else(|| {
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
                        initial_value: values.get(0).cloned().ok_or_else(|| {
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
                        voltage1: values.get(0).cloned().ok_or_else(|| {
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

        return Ok(waveform);
    }

    fn parse_node(&self, cursor: &mut StmtCursor, scope: &Scope) -> Result<Node, SpicyError> {
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
            let expr = self
                .placeholder_map
                .get(id)
                .clone();
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
        Ok(parse_bool(cursor, input)?)
    }

    fn parse_usize(&self, cursor: &mut StmtCursor, scope: &Scope) -> Result<usize, SpicyError> {
        if let Some(token) = cursor.consume(TokenKind::Placeholder) {
            let id = token.id.expect("must have a placeholder id");
            // TOOD: maybe we can change the expresion to only evalute once
            let expr = self
                .placeholder_map
                .get(id)
                .clone();
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
        Ok(parse_usize(cursor, input)?)
    }

    // RXXXXXXX n+ n- <resistance|r=>value <ac=val> <m=val>
    // + <scale=val> <temp=val> <dtemp=val> <tc1=val> <tc2=val>
    // + <noisy=0|1>
    fn parse_resistor(
        &self,
        name: String,
        cursor: &mut StmtCursor,
        scope: &Scope,
    ) -> Result<Resistor, SpicyError> {
        let positive = self.parse_node(cursor, scope)?;
        let negative = self.parse_node(cursor, scope)?;

        let resistance = self.parse_value(cursor, scope)?;
        let mut resistor = Resistor::new(name, cursor.span, positive, negative, resistance);

        let params_order = vec!["ac", "m", "scale", "temp", "dtemp", "tc1", "tc2", "noisy"];
        let input = self.source_map.get_content(cursor.span.source_index);
        let params = ParamParser::new(input, params_order, cursor);
        for item in params {
            let (ident, mut cursor) = item?;
            match ident {
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
    ) -> Result<Capacitor, SpicyError> {
        let positive = self.parse_node(cursor, scope)?;
        let negative = self.parse_node(cursor, scope)?;

        // TODO: support models
        let capacitance = self.parse_value(cursor, scope)?;
        let mut capacitor = Capacitor::new(name, cursor.span, positive, negative, capacitance);

        let params_order = vec!["m", "scale", "temp", "dtemp", "tc1", "tc2", "ic"];
        let input = self.source_map.get_content(cursor.span.source_index);
        let params = ParamParser::new(input, params_order, cursor);

        for item in params {
            let (ident, mut cursor) = item?;
            match ident {
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
    ) -> Result<Inductor, SpicyError> {
        let positive = self.parse_node(cursor, scope)?;
        let negative = self.parse_node(cursor, scope)?;

        let inductance = self.parse_value(cursor, scope)?;

        let mut inductor = Inductor::new(name, cursor.span, positive, negative, inductance);

        let params_order = vec!["nt", "m", "scale", "temp", "dtemp", "tc1", "tc2", "ic"];
        let input = self.source_map.get_content(cursor.span.source_index);
        let params = ParamParser::new(input, params_order, cursor);

        for item in params {
            let (ident, mut cursor) = item?;
            match ident {
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

    fn parse_source_value(
        &self,
        cursor: &mut StmtCursor,
        scope: &Scope,
        independent_source: &mut IndependentSource,
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
                    if let Some(_) = cursor.peek_non_whitespace() {
                        let phase = self.parse_value(cursor, scope)?;
                        phasor.set_phase(phase);
                    }

                    independent_source.set_ac(phasor);
                }
                _ => independent_source.set_dc(self.parse_waveform(&token, cursor, scope)?),
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
    ) -> Result<IndependentSource, SpicyError> {
        let positive = self.parse_node(cursor, scope)?;
        let negative = self.parse_node(cursor, scope)?;

        let mut independent_source = IndependentSource::new(name, positive, negative);

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

    fn parse_device(&self, statement: &ScopedStmt) -> Result<Device, SpicyError> {
        let mut cursor = statement.stmt.into_cursor();
        let ident = cursor.expect(TokenKind::Ident)?;

        let input = self.source_map.get_content(ident.span.source_index);
        let ident_string = token_text(input, ident).to_string();
        let (first, _) = ident_string.split_at(1);

        let element_type = DeviceType::from_str(first)?;
        let scope = self
            .expanded_deck
            .scope_arena
            .get(statement.scope)
            .ok_or_else(|| ParserError::MissingScope {
                span: statement.stmt.span,
            })?;

        let name = scope.get_device_name(&ident_string);

        match element_type {
            DeviceType::Resistor => Ok(Device::Resistor(self.parse_resistor(
                name,
                &mut cursor,
                scope,
            )?)),
            DeviceType::Capacitor => Ok(Device::Capacitor(self.parse_capacitor(
                name,
                &mut cursor,
                scope,
            )?)),
            DeviceType::Inductor => Ok(Device::Inductor(self.parse_inductor(
                name,
                &mut cursor,
                scope,
            )?)),
            DeviceType::VoltageSource => Ok(Device::VoltageSource(self.parse_independent_source(
                name,
                &mut cursor,
                scope,
            )?)),
            DeviceType::CurrentSource => Ok(Device::CurrentSource(self.parse_independent_source(
                name,
                &mut cursor,
                scope,
            )?)),
            _ => {
                return Err(ParserError::InvalidDeviceType {
                    s: element_type.to_char().to_string(),
                }
                .into());
            }
        }
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

        let mut uic = false;
        match cursor.peek_non_whitespace() {
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
            _ => {
                unimplemented!("tstart and tmax are not yet implemented")
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
        let mut cursor = statement.stmt.into_cursor();
        cursor.expect(TokenKind::Dot)?;
        let ident = cursor.expect(TokenKind::Ident)?;
        let input = self.source_map.get_content(ident.span.source_index);
        let ident_string = token_text(input, ident);
        let command_type = CommandType::from_str(&ident_string).ok_or_else(|| {
            ParserError::InvalidCommandType {
                s: ident_string.to_string(),
                span: cursor.span,
            }
        })?;

        let scope = self
            .expanded_deck
            .scope_arena
            .get(statement.scope)
            .ok_or_else(|| ParserError::MissingScope {
                span: statement.stmt.span,
            })?;

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
        let title = self.parse_title(
            &statements_iter
                .next()
                .ok_or_else(|| ParserError::MissingTitle)?,
        );

        let mut commands = vec![];
        let mut devices = vec![];

        while let Some(statement) = statements_iter.next() {
            let cursor = statement.stmt.into_cursor();

            let first_token = cursor.peek().ok_or_else(|| ParserError::MissingToken {
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
                    let device = self.parse_device(&statement)?;
                    devices.push(device);
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
            commands,
            devices,
        })
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::{ParseOptions, libs_phase::SourceMap};

    use std::path::PathBuf;

    #[rstest]
    fn test_parser(#[files("tests/parser_inputs/*.spicy")] input: PathBuf) {
        use crate::parse;

        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let mut source_map = SourceMap::new(input.clone(), input_content);
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
}
