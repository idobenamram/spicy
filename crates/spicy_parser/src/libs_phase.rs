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
    map: HashMap<u16, PathBuf>,
    content_map: HashMap<PathBuf, String>,
}

impl SourceMap {
    const MAIN_INDEX: u16 = 0;

    pub fn new(main_file: PathBuf, content: String) -> Self {
        let mut map = HashMap::new();
        map.insert(Self::MAIN_INDEX, main_file.clone());
        let mut content_map = HashMap::new();
        content_map.insert(main_file, content);
        Self { map, content_map }
    }

    pub fn new_source(&mut self, path: PathBuf, content: String) -> u16 {
        let new_index = self.map.len() as u16;
        self.map.insert(new_index, path.clone());
        self.content_map.insert(path, content);
        new_index
    }

    pub const fn main_index(&self) -> u16 {
        Self::MAIN_INDEX
    }

    pub fn get_path(&self, index: u16) -> Option<&Path> {
        self.map.get(&index).map(|x| x.as_path())
    }

    pub fn get_main_content(&self) -> &str {
        self.content_map
            .get(
                self.map
                    .get(&Self::MAIN_INDEX)
                    .expect("main index always exists"),
            )
            .map(|s| s.as_str())
            .unwrap()
    }

    pub fn get_content(&self, index: u16) -> Option<&str> {
        let path = self.get_path(index)?;
        self.content_map.get(path).map(|s| s.as_str())
    }
}

fn span_text<'a>(src: &'a str, span: Span) -> &'a str {
    &src[span.start..=span.end]
}

fn parse_include<'a>(
    cursor: &mut StmtCursor<'a>,
    options: &mut ParseOptions,
) -> Result<Statements, SpicyError> {
    let cursors = cursor.split_on_whitespace();
    let path_cursor = cursors
        .first()
        .ok_or_else(|| SpicyError::Include(IncludeError::ExpectedPath { span: cursor.span }))?;

    let path = span_text(options.source_map.get_main_content(), path_cursor.span)
        .trim()
        .to_string();
    let (file_content, source_index) = options.read_file(&path, path_cursor.span)?;
    Statements::new(&file_content, source_index)
}

pub(crate) fn include_libs(
    stmts: Statements,
    options: &mut ParseOptions,
) -> Result<Statements, SpicyError> {
    let mut out = Vec::new();

    for stmt in stmts.statements.into_iter() {
        let mut cursor = stmt.into_cursor();
        if cursor.consume_if_command(options.source_map.get_main_content(), CommandType::Include) {
            let statements = parse_include(&mut cursor, options)?;
            out.extend(statements.statements);
        } else {
            out.push(stmt);
        }
    }

    Ok(Statements { statements: out })
}
