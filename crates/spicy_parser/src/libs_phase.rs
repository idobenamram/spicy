use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    ParseOptions, Span,
    error::{IncludeError, SpicyError},
    netlist_types::CommandType,
    statement_phase::{Statements, StmtCursor},
};

#[derive(Debug)]
pub struct SourceMap {
    pub map: HashMap<u16, PathBuf>,
}

impl SourceMap {
    const MAIN_INDEX: u16 = 0;

    pub fn new(main_file: PathBuf) -> Self {
        let mut map = HashMap::new();
        map.insert(Self::MAIN_INDEX, main_file);
        Self { map }
    }

    pub fn new_source(&mut self, path: PathBuf) -> u16 {
        let new_index = self.map.len() as u16;
        self.map.insert(new_index, path);
        new_index
    }

    pub const fn main_index(&self) -> u16 {
        Self::MAIN_INDEX
    }

    pub fn get_source(&self, index: u16) -> Option<&Path> {
        self.map.get(&index).map(|x| x.as_path())
    }
}

fn span_text<'a>(src: &'a str, span: Span) -> &'a str {
    &src[span.start..=span.end]
}

fn parse_include<'a>(
    cursor: &mut StmtCursor<'a>,
    options: &ParseOptions<'a>,
    source_map: &mut SourceMap,
) -> Result<Statements, SpicyError> {
    let cursors = cursor.split_on_whitespace();
    let path_cursor = cursors
        .first()
        .ok_or_else(|| SpicyError::Include(IncludeError::ExpectedPath { span: cursor.span }))?;

    let path = span_text(options.input, path_cursor.span).trim();
    let (file_content, source_index) = options.read_file(path, path_cursor.span, source_map)?;
    Statements::new(&file_content, source_index)
}

pub(crate) fn include_libs(
    stmts: Statements,
    options: &ParseOptions,
    source_map: &mut SourceMap,
) -> Result<Statements, SpicyError> {
    let mut out = Vec::new();

    for stmt in stmts.statements.into_iter() {
        let mut cursor = stmt.into_cursor();
        if cursor.consume_if_command(options.input, CommandType::Include) {
            let statements = parse_include(&mut cursor, options, source_map)?;
            out.extend(statements.statements);
        } else {
            out.push(stmt);
        }
    }

    Ok(Statements { statements: out })
}
