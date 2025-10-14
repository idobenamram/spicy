use std::collections::HashMap;

use serde::Serialize;

use crate::SourceMap;
use crate::error::{SpicyError, SubcircuitError};
use crate::expr::{Params, Scope, ScopeId};
use crate::expr::{PlaceholderMap, ScopeArena};
use crate::netlist_models::{ModelStatementTable, ModelTable};
use crate::netlist_models::partial_parse_model_command;
use crate::netlist_types::Node;
use crate::netlist_types::{CommandType, DeviceType};
use crate::parser_utils::{
    parse_dot_param, parse_equal_expr, parse_ident, parse_node, parse_value_or_placeholder,
};
use crate::statement_phase::{Statements, StmtCursor};
use crate::{lexer::TokenKind, statement_phase::Statement};

#[cfg(test)]
use crate::test_utils::serialize_sorted_map;

#[derive(Debug)]
pub(crate) struct UnexpandedDeck {
    pub scope_arena: ScopeArena,
    pub global_params: ScopeId,
    pub model_table: ModelStatementTable,
    pub subckt_table: SubcktTable,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SubcktDecl {
    pub name: String,
    pub nodes: Vec<Node>,
    pub default_params: Params,
    pub local_params: Params,
    pub body: Vec<Statement>, // statements between .subckt and .ends (already placeholderized)
}

#[derive(Debug, Default, Clone, Serialize)]
pub(crate) struct SubcktTable {
    #[cfg_attr(test, serde(serialize_with = "serialize_sorted_map"))]
    pub map: HashMap<String, SubcktDecl>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScopedStmt {
    pub stmt: Statement,
    pub scope: ScopeId,
}

pub(crate) fn collect_subckts(
    stmts: Statements,
    source_map: &SourceMap,
) -> Result<UnexpandedDeck, SpicyError> {
    let mut out = Vec::new();
    let mut table = SubcktTable::default();
    let mut model_table = ModelStatementTable::default();
    let mut scope_arena = ScopeArena::new();
    let (root_env, root_env_id) = scope_arena.new_root();
    let mut it = stmts.statements.into_iter();

    while let Some(s) = it.next() {
        let mut cursor = s.into_cursor();
        // todo: fix this
        let input = source_map.get_content(s.span.source_index);

        if cursor.consume_if_command(input, CommandType::Param) {
            parse_dot_param(&mut cursor, input, &mut root_env.param_map)?;
            continue;
        }

        if cursor.consume_if_command(input, CommandType::Model) {
            let model_statement = partial_parse_model_command(cursor, input)?;
            model_table.insert(model_statement)?;
            continue;
        }

        if cursor.consume_if_command(input, CommandType::Subcircuit) {
            let mut subckt = parse_subckt_command(&mut cursor, input)?;
            let mut body = Vec::new();
            // TODO: this doesn't support nested subcircuits
            while let Some(next) = it.next() {
                let mut inner_cursor = next.into_cursor();
                if inner_cursor.consume_if_command(input, CommandType::Param) {
                    parse_dot_param(&mut inner_cursor, input, &mut subckt.local_params)?;
                    continue;
                }
                if inner_cursor.consume_if_command(input, CommandType::Model) {
                    let model_statement = partial_parse_model_command(inner_cursor, input)?;
                    model_table.insert(model_statement)?;
                    continue;
                }
                // collect body until .ends
                if inner_cursor.consume_if_command(input, CommandType::Ends) {
                    // TODO: the .ends command also has the subcircuit name, add assert here
                    break;
                }
                body.push(next);
            }
            subckt.body = body;
            table.map.insert(subckt.name.clone(), subckt);

            continue;
        }
        out.push(s);
    }
    Ok(UnexpandedDeck {
        scope_arena,
        global_params: root_env_id,
        model_table,
        subckt_table: table,
        statements: out,
    })
}

// SUBCKT subnam N1 <N2 N3 ...>
fn parse_subckt_command(cursor: &mut StmtCursor, src: &str) -> Result<SubcktDecl, SpicyError> {
    let name = parse_ident(cursor, src)?;

    let first_node = parse_node(cursor, src)?;

    let mut nodes = vec![first_node];
    let mut default_params = Params::new();

    loop {
        cursor.skip_ws();
        let Some(_) = cursor.peek() else {
            break;
        };
        let node = parse_node(cursor, src)?;
        if let Some(_) = cursor.consume(TokenKind::Equal) {
            let param_name = node.name;
            let value = parse_value_or_placeholder(cursor, src)?;
            default_params.set_param(param_name, value);
        } else {
            // TODO: technically we can't parse nodes after we saw parameters
            nodes.push(node);
        }
    }

    Ok(SubcktDecl {
        name: name.text.to_string(),
        nodes,
        default_params,
        local_params: Params::new(),
        body: vec![],
    })
}

fn parse_x_device(
    cursor: &mut StmtCursor,
    src: &str,
) -> Result<(Vec<Node>, String, Params), SpicyError> {
    // Phase 1: parse only nodes (last one is the subcircuit name)
    let first_node = parse_node(cursor, src)?;

    let mut nodes = vec![first_node];

    loop {
        let mark = cursor.checkpoint();
        cursor.skip_ws();
        let is_param_start = if let Some(_) = cursor.consume(TokenKind::Ident) {
            let eq = cursor.consume(TokenKind::Equal).is_some();
            eq
        } else {
            false
        };
        cursor.rewind(mark);

        if is_param_start || cursor.done() {
            break;
        }

        let node = parse_node(cursor, src)?;
        nodes.push(node);
    }

    // The last parsed node is the subcircuit name
    let subcircuit_name = nodes
        .pop()
        .ok_or_else(|| SubcircuitError::MissingSubcircuitName {
            span: cursor.peek_span(),
        })?
        .name;

    if nodes.is_empty() {
        return Err(SubcircuitError::NoNodes {
            name: subcircuit_name,
            span: cursor.span,
        }
        .into());
    }

    // Phase 2: parse only parameters (IDENT '=' value)
    let mut param_overrides = Params::new();
    loop {
        cursor.skip_ws();
        if cursor.done() {
            break;
        }

        let (param_name, value) = parse_equal_expr(cursor, src)?;
        param_overrides.set_param(param_name.text.to_string(), value);
    }

    Ok((nodes, subcircuit_name, param_overrides))
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExpandedDeck {
    pub scope_arena: ScopeArena,
    pub model_table: ModelTable,
    pub global_params: ScopeId,
    pub subckt_table: SubcktTable,
    pub statements: Vec<ScopedStmt>,
}

/// Expand `X...` instances. For now assume: Xname n1 n2 subcktName [param=value ...]
pub(crate) fn expand_subckts<'a>(
    mut unexpanded_deck: UnexpandedDeck,
    source_map: &SourceMap,
    placeholder_map: &PlaceholderMap,
) -> Result<ExpandedDeck, SpicyError> {
    let mut out = Vec::new();

    let root_scope_id = unexpanded_deck.global_params;
    for s in unexpanded_deck.statements.into_iter() {
        let mut cursor = s.into_cursor();

        let src = source_map.get_content(s.span.source_index);
        if let Some(instance_name) = cursor.consume_if_device(src, DeviceType::Subcircuit) {
            let instance_name = instance_name.to_string();
            let (nodes, instance_subckt, param_overrides) = parse_x_device(&mut cursor, src)?;

            let Some(subckt_def) = unexpanded_deck.subckt_table.map.get(&instance_subckt) else {
                return Err(SubcircuitError::NotFound {
                    name: instance_subckt,
                }
                .into());
            };

            // arity check
            if nodes.len() != subckt_def.nodes.len() {
                return Err(SubcircuitError::ArityMismatch {
                    name: instance_subckt,
                    found: nodes.len(),
                    expected: subckt_def.nodes.len(),
                }
                .into());
            }

            let mut instance_params = subckt_def.default_params.clone();
            // will override any default params
            instance_params.merge(param_overrides);
            // will override any instance params
            instance_params.merge(subckt_def.local_params.clone());
            // pin map
            let mut node_mapping = HashMap::new();
            for (f, a) in subckt_def.nodes.iter().cloned().zip(nodes.into_iter()) {
                node_mapping.insert(f, a);
            }

            let child_scope = Scope::new(Some(instance_name), instance_params, node_mapping);
            let child_scope_id = unexpanded_deck
                .scope_arena
                .new_child(root_scope_id, child_scope);

            for stmt in subckt_def.body.iter() {
                out.push(ScopedStmt {
                    stmt: stmt.clone(),
                    scope: child_scope_id,
                });
            }
            continue;
        }
        out.push(ScopedStmt {
            stmt: s,
            scope: root_scope_id,
        });
    }

    let models = unexpanded_deck.model_table.into_model_table(
        source_map,
        placeholder_map,
        &unexpanded_deck
            .scope_arena
            .get(unexpanded_deck.global_params)
            .expect("we always have a global scope"),
    )?;

    Ok(ExpandedDeck {
        scope_arena: unexpanded_deck.scope_arena,
        global_params: unexpanded_deck.global_params,
        subckt_table: unexpanded_deck.subckt_table,
        model_table: models,
        statements: out,
    })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::ParseOptions;
    use crate::expression_phase::substitute_expressions;
    use std::path::PathBuf;

    #[rstest]
    fn test_subcircuit_phase(#[files("tests/subcircuit_inputs/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let source_map = SourceMap::new(input.clone(), input_content.clone());
        let input_options = ParseOptions {
            source_map,
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            max_include_depth: 10,
        };
        let mut statements = Statements::new(&input_content, input_options.source_map.main_index())
            .expect("statements");
        let placeholders_map = substitute_expressions(&mut statements, &input_options)
            .expect("substitute expressions");
        let unexpanded_deck =
            collect_subckts(statements, &input_options.source_map).expect("collect subckts");
        let expanded_deck = expand_subckts(
            unexpanded_deck,
            &input_options.source_map,
            &placeholders_map,
        )
        .expect("expand subckts");

        let name = format!(
            "subcircuit-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );

        let json = serde_json::to_string_pretty(&expanded_deck).expect("serialize output to json");
        insta::assert_snapshot!(name, json);
    }

    #[rstest]
    fn test_model_parsing(#[files("tests/model_inputs/*.spicy")] input: PathBuf) {
        use serde_json;

        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let source_map = SourceMap::new(input.clone(), input_content.clone());
        let input_options = ParseOptions {
            source_map,
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            max_include_depth: 10,
        };

        let mut statements = Statements::new(&input_content, input_options.source_map.main_index())
            .expect("statements");
        let placeholders_map = substitute_expressions(&mut statements, &input_options)
            .expect("substitute expressions");
        let unexpanded_deck = collect_subckts(statements, &input_options.source_map)
            .expect("collect subckts and models");

        let global_scope = unexpanded_deck
            .scope_arena
            .get(unexpanded_deck.global_params)
            .expect("global scope exists");
        let model_table = unexpanded_deck
            .model_table
            .into_model_table(
                &input_options.source_map,
                &placeholders_map,
                global_scope,
            )
            .expect("build model table");

        let name = format!(
            "models-{}",
            input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );

        let json = serde_json::to_string_pretty(&model_table).expect("serialize to json");
        insta::assert_snapshot!(name, json);
    }

    #[test]
    fn test_duplicate_model_name_error() {
        use crate::libs_phase::SourceMap;
        use crate::error::{SpicyError, SubcircuitError};

        let input_content = "\
* duplicate models\n
.model R1 R resistance=10\n
.model R1 R resistance=20\n
";
        let source_map = SourceMap::new(PathBuf::from("inline"), input_content.to_string());
        let input_options = ParseOptions {
            source_map,
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            max_include_depth: 10,
        };

        let mut statements = Statements::new(&input_content, input_options.source_map.main_index())
            .expect("statements");
        let _ = substitute_expressions(&mut statements, &input_options)
            .expect("substitute expressions");

        let err = collect_subckts(statements, &input_options.source_map)
            .expect_err("expected duplicate model error");

        match err {
            SpicyError::Subcircuit(SubcircuitError::ModelAlreadyExists { name, .. }) => {
                assert_eq!(name, "R1");
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_invalid_model_type_error() {
        use crate::libs_phase::SourceMap;
        use crate::error::{SpicyError, SubcircuitError};

        let input_content = "\
* invalid model type\n
.model M1 X foo=1\n
";
        let source_map = SourceMap::new(PathBuf::from("inline"), input_content.to_string());
        let input_options = ParseOptions {
            source_map,
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            max_include_depth: 10,
        };

        let mut statements = Statements::new(&input_content, input_options.source_map.main_index())
            .expect("statements");
        let _ = substitute_expressions(&mut statements, &input_options)
            .expect("substitute expressions");
        let err = collect_subckts(statements, &input_options.source_map)
            .expect_err("expected invalid model type error");

        match err {
            SpicyError::Subcircuit(SubcircuitError::InvalidDeviceModelType { s, .. }) => {
                assert_eq!(s, "X");
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }
}
