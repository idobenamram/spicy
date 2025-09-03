use std::collections::HashMap;

use serde::Serialize;

use crate::expr::ScopeArena;
use crate::expr::{Params, Scope, ScopeId};
use crate::netlist_types::Node;
use crate::netlist_types::{CommandType, DeviceType};
use crate::parser_utils::{
    parse_dot_param, parse_equal_expr, parse_ident, parse_node, parse_value_or_placeholder,
};
use crate::statement_phase::{Statements, StmtCursor};
use crate::{lexer::TokenKind, statement_phase::Statement};

#[cfg(test)]
use crate::test_utils::serialize_sorted_map;

pub struct UnexpandedDeck {
    pub scope_arena: ScopeArena,
    pub global_params: ScopeId,
    pub subckt_table: SubcktTable,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubcktDecl {
    pub name: String,
    pub nodes: Vec<Node>,
    pub default_params: Params,
    pub local_params: Params,
    pub body: Vec<Statement>, // statements between .subckt and .ends (already placeholderized)
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct SubcktTable {
    #[cfg_attr(test, serde(serialize_with = "serialize_sorted_map"))]
    pub map: HashMap<String, SubcktDecl>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScopedStmt {
    pub stmt: Statement,
    pub scope: ScopeId,
}

pub fn collect_subckts(stmts: Statements, input: &str) -> UnexpandedDeck {
    let mut out = Vec::new();
    let mut table = SubcktTable::default();
    let mut scope_arena = ScopeArena::new();
    let (root_env, root_env_id) = scope_arena.new_root();
    let mut it = stmts.statements.into_iter();

    while let Some(s) = it.next() {
        let mut cursor = s.into_cursor();
        if cursor.consume_if_command(input, CommandType::Param) {
            parse_dot_param(&mut cursor, input, &mut root_env.param_map);
            continue;
        }
        if cursor.consume_if_command(input, CommandType::Subcircuit) {
            let mut subckt = parse_subckt_command(&mut cursor, input);
            let mut body = Vec::new();
            // TODO: this doesn't support nested subcircuits
            while let Some(next) = it.next() {
                let mut inner_cursor = next.into_cursor();
                if inner_cursor.consume_if_command(input, CommandType::Param) {
                    println!("parsing param in subcircuit");
                    parse_dot_param(&mut inner_cursor, input, &mut subckt.local_params);
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
    UnexpandedDeck {
        scope_arena,
        global_params: root_env_id,
        subckt_table: table,
        statements: out,
    }
}

// SUBCKT subnam N1 <N2 N3 ...>
fn parse_subckt_command(cursor: &mut StmtCursor, src: &str) -> SubcktDecl {
    let name = parse_ident(cursor, src);

    let first_node = parse_node(cursor, src);

    let mut nodes = vec![first_node];
    let mut default_params = Params::new();

    loop {
        cursor.skip_ws();
        let Some(_) = cursor.peek() else {
            break;
        };
        let node = parse_node(cursor, src);
        if let Some(_) = cursor.consume(TokenKind::Equal) {
            let param_name = node.name;
            let value = parse_value_or_placeholder(cursor, src);
            default_params.set_param(param_name, value);
        } else {
            // TODO: technically we can't parse nodes after we saw parameters
            nodes.push(node);
        }
    }

    SubcktDecl {
        name,
        nodes,
        default_params,
        local_params: Params::new(),
        body: vec![],
    }
}

fn parse_x_device(cursor: &mut StmtCursor, src: &str) -> (Vec<Node>, String, Params) {
    // Phase 1: parse only nodes (last one is the subcircuit name)
    let first_node = parse_node(cursor, src);

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

        let node = parse_node(cursor, src);
        nodes.push(node);
    }

    // The last parsed node is the subcircuit name
    let subcircuit_name = nodes.pop().expect("Subcircuit name is required").name;

    assert!(!nodes.is_empty(), "must have at least one node");

    // Phase 2: parse only parameters (IDENT '=' value)
    let mut param_overrides = Params::new();
    loop {
        cursor.skip_ws();
        if cursor.done() {
            break;
        }

        let (param_name, value) = parse_equal_expr(cursor, src);
        param_overrides.set_param(param_name, value);
    }

    (nodes, subcircuit_name, param_overrides)
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpandedDeck {
    pub scope_arena: ScopeArena,
    pub global_params: ScopeId,
    pub subckt_table: SubcktTable,
    pub statements: Vec<ScopedStmt>,
}

/// Expand `X...` instances. For now assume: Xname n1 n2 subcktName [param=value ...]
pub fn expand_subckts<'a>(mut unexpanded_deck: UnexpandedDeck, src: &'a str) -> ExpandedDeck {
    let mut out = Vec::new();

    let root_scope_id = unexpanded_deck.global_params;
    for s in unexpanded_deck.statements.into_iter() {
        let mut cursor = s.into_cursor();

        if let Some(instance_name) = cursor.consume_if_device(src, DeviceType::Subcircuit) {
            let instance_name = instance_name.to_string();
            let (nodes, instance_subckt, param_overrides) = parse_x_device(&mut cursor, src);

            let Some(subckt_def) = unexpanded_deck.subckt_table.map.get(&instance_subckt) else {
                panic!("subcircuit not found: {}", instance_subckt);
            };

            // arity check
            if nodes.len() != subckt_def.nodes.len() {
                panic!(
                    "subcircuit {} has {} nodes, expected {}",
                    instance_subckt,
                    nodes.len(),
                    subckt_def.nodes.len()
                );
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
    ExpandedDeck {
        scope_arena: unexpanded_deck.scope_arena,
        global_params: unexpanded_deck.global_params,
        subckt_table: unexpanded_deck.subckt_table,
        statements: out,
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::expression_phase::substitute_expressions;
    use std::path::PathBuf;

    #[rstest]
    fn test_subcircuit_phase(#[files("tests/subcircuit_inputs/*.spicy")] input: PathBuf) {
        let input_content = std::fs::read_to_string(&input).expect("failed to read input file");
        let mut statements = Statements::new(&input_content);
        let _placeholders_map = substitute_expressions(&mut statements, &input_content);
        let unexpanded_deck = collect_subckts(statements, &input_content);
        let expanded_deck = expand_subckts(unexpanded_deck, &input_content);

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
}
