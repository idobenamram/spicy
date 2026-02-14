use serde::Serialize;
use std::collections::HashMap;

#[cfg(test)]
use crate::test_utils::serialize_sorted_map;
use crate::{
    SourceMap, Span, Value,
    error::{ParserError, SpicyError, SubcircuitError},
    expr::{PlaceholderMap, Scope},
    lexer::TokenKind,
    parser_utils::{Ident, parse_expr_into_value, parse_ident},
    statement_phase::{Statement, StmtCursor},
};

#[derive(Debug, Default, Clone, Serialize)]
pub(crate) struct ModelTable {
    #[cfg_attr(test, serde(serialize_with = "serialize_sorted_map"))]
    pub(crate) map: HashMap<String, DeviceModel>,
}

impl ModelTable {
    pub(crate) fn get(&self, model: &str) -> Option<&DeviceModel> {
        self.map.get(model)
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub(crate) struct ModelStatementTable {
    #[cfg_attr(test, serde(serialize_with = "serialize_sorted_map"))]
    pub(crate) map: HashMap<String, ModelStatement>,
}

impl ModelStatementTable {
    pub(crate) fn insert(&mut self, model_statement: ModelStatement) -> Result<(), SpicyError> {
        if self.map.contains_key(&model_statement.name) {
            return Err(SubcircuitError::ModelAlreadyExists {
                name: model_statement.name.clone(),
                span: model_statement.statement.span, // TODO: would have been nice to have both the first place and the second place we see the ident name
            }
            .into());
        } else {
            self.map
                .insert(model_statement.name.clone(), model_statement);
        }

        Ok(())
    }

    pub(crate) fn into_model_table(
        self,
        source_map: &SourceMap,
        placeholder_map: &PlaceholderMap,
        scope: &Scope,
    ) -> Result<ModelTable, SpicyError> {
        let map = self
            .map
            .into_iter()
            .map(|(name, model_statement)| {
                model_statement_to_device_model(model_statement, source_map, placeholder_map, scope)
                    .map(|device_model| (name, device_model))
            })
            .collect::<Result<HashMap<String, DeviceModel>, SpicyError>>()?;

        Ok(ModelTable { map })
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ModelStatement {
    pub name: String,
    pub model_type: DeviceModelType,
    pub statement: Statement,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum BjtPolarity {
    Npn,
    Pnp,
}

impl Default for BjtPolarity {
    fn default() -> Self {
        Self::Npn
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) enum DeviceModelType {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Bjt(BjtPolarity),
}

impl DeviceModelType {
    pub fn from_str(s: &str, span: Span) -> Result<DeviceModelType, SpicyError> {
        match s.to_uppercase().as_str() {
            "R" => Ok(DeviceModelType::Resistor),
            "C" => Ok(DeviceModelType::Capacitor),
            "L" => Ok(DeviceModelType::Inductor),
            "D" => Ok(DeviceModelType::Diode),
            "NPN" => Ok(DeviceModelType::Bjt(BjtPolarity::Npn)),
            "PNP" => Ok(DeviceModelType::Bjt(BjtPolarity::Pnp)),
            _ => Err(SubcircuitError::InvalidDeviceModelType {
                s: s.to_string(),
                span,
            }
            .into()),
        }
    }
}

pub(crate) fn partial_parse_model_command(
    mut cursor: StmtCursor,
    src: &str,
) -> Result<ModelStatement, SpicyError> {
    let name = parse_ident(&mut cursor, src)?;
    let model_type_str = parse_ident(&mut cursor, src)?;
    let model_type = DeviceModelType::from_str(model_type_str.text, model_type_str.span)?;
    // TODO: kinda sucky we have to clone the statement tokens
    let statement = cursor.into_statement();

    Ok(ModelStatement {
        name: name.text.to_string(),
        model_type,
        statement,
    })
}

/// only supports params from global scope
fn model_statement_to_device_model(
    model_statement: ModelStatement,
    source_map: &SourceMap,
    placeholder_map: &PlaceholderMap,
    scope: &Scope,
) -> Result<DeviceModel, SpicyError> {
    let input = source_map.get_content(model_statement.statement.span.source_index);

    let mut cursor = model_statement.statement.as_cursor();
    cursor.skip_ws();
    let params_cursors = if cursor.consume(TokenKind::LeftParen).is_some() {
        let in_parentheses = cursor.split_on(TokenKind::RightParen)?;
        let params = in_parentheses.split_on_whitespace();
        cursor.expect(TokenKind::RightParen)?;
        params
    } else {
        cursor.split_on_whitespace()
    };

    let mut params = Vec::new();
    for mut param in params_cursors {
        let ident = parse_ident(&mut param, input)?;
        param.expect(TokenKind::Equal)?;
        let value = parse_expr_into_value(&mut param, input, placeholder_map, scope)?;

        params.push((ident, value));
    }

    Ok(match model_statement.model_type {
        DeviceModelType::Resistor => DeviceModel::Resistor(ResistorModel::new(params)?),
        DeviceModelType::Capacitor => DeviceModel::Capacitor(CapacitorModel::new(params)?),
        DeviceModelType::Inductor => DeviceModel::Inductor(InductorModel::new(params)?),
        DeviceModelType::Diode => DeviceModel::Diode(DiodeModel::new(params)?),
        DeviceModelType::Bjt(polarity) => DeviceModel::Bjt(BjtModel::new(polarity, params)?),
    })
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ResistorModel {
    pub resistance: Option<Value>,
    pub tc1: Option<Value>,
    pub tc2: Option<Value>,
    pub w: Option<Value>,
    pub l: Option<Value>,
}

impl ResistorModel {
    pub(crate) fn new(params: Vec<(Ident, Value)>) -> Result<Self, SpicyError> {
        let mut model = Self::default();

        for (ident, value) in params {
            match ident.text {
                "resistance" => model.resistance = Some(value),
                "tc1" => model.tc1 = Some(value),
                "tc2" => model.tc2 = Some(value),
                "w" => model.w = Some(value),
                "l" => model.l = Some(value),
                _ => {
                    return Err(ParserError::InvalidParam {
                        param: ident.text.to_string(),
                        span: ident.span,
                    }
                    .into());
                }
            }
        }
        Ok(model)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CapacitorModel {
    pub cap: Option<Value>,
    pub tc1: Option<Value>,
    pub tc2: Option<Value>,
}

impl CapacitorModel {
    pub(crate) fn new(params: Vec<(Ident, Value)>) -> Result<Self, SpicyError> {
        let mut model = Self::default();

        for (ident, value) in params {
            match ident.text {
                "cap" => model.cap = Some(value),
                "tc1" => model.tc1 = Some(value),
                "tc2" => model.tc2 = Some(value),
                _ => {
                    return Err(ParserError::InvalidParam {
                        param: ident.text.to_string(),
                        span: ident.span,
                    }
                    .into());
                }
            }
        }
        Ok(model)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct InductorModel {
    pub inductance: Option<Value>,
    pub tc1: Option<Value>,
    pub tc2: Option<Value>,
}

impl InductorModel {
    pub(crate) fn new(params: Vec<(Ident, Value)>) -> Result<Self, SpicyError> {
        let mut model = Self::default();

        for (ident, value) in params {
            match ident.text {
                "ind" => model.inductance = Some(value),
                "tc1" => model.tc1 = Some(value),
                "tc2" => model.tc2 = Some(value),
                _ => {
                    return Err(ParserError::InvalidParam {
                        param: ident.text.to_string(),
                        span: ident.span,
                    }
                    .into());
                }
            }
        }
        Ok(model)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DiodeModel {
    pub is: Option<Value>,
    pub n: Option<Value>,
    pub rs: Option<Value>,
}

impl DiodeModel {
    pub(crate) fn new(params: Vec<(Ident, Value)>) -> Result<Self, SpicyError> {
        let mut model = Self::default();

        for (ident, value) in params {
            match ident.text {
                "is" => model.is = Some(value),
                "n" => model.n = Some(value),
                "rs" => model.rs = Some(value),
                _ => {
                    return Err(ParserError::InvalidParam {
                        param: ident.text.to_string(),
                        span: ident.span,
                    }
                    .into());
                }
            }
        }
        Ok(model)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BjtModel {
    pub polarity: BjtPolarity,
    pub is: Option<Value>,
    pub bf: Option<Value>,
    pub br: Option<Value>,
    pub nf: Option<Value>,
    pub nr: Option<Value>,
}

impl BjtModel {
    pub(crate) fn new(
        polarity: BjtPolarity,
        params: Vec<(Ident, Value)>,
    ) -> Result<Self, SpicyError> {
        let mut model = Self {
            polarity,
            ..Self::default()
        };

        for (ident, value) in params {
            match ident.text {
                "is" => model.is = Some(value),
                "bf" => model.bf = Some(value),
                "br" => model.br = Some(value),
                "nf" => model.nf = Some(value),
                "nr" => model.nr = Some(value),
                _ => {
                    return Err(ParserError::InvalidParam {
                        param: ident.text.to_string(),
                        span: ident.span,
                    }
                    .into());
                }
            }
        }
        Ok(model)
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) enum DeviceModel {
    Resistor(ResistorModel),
    Capacitor(CapacitorModel),
    Inductor(InductorModel),
    Diode(DiodeModel),
    Bjt(BjtModel),
}
