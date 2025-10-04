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
    paths: Vec<PathBuf>,
    contents: Vec<String>,
}

impl SourceMap {
    const MAIN_INDEX: u16 = 0;

    pub fn new(main_file: PathBuf, content: String) -> Self {
        Self {
            paths: vec![main_file],
            contents: vec![content],
        }
    }

    pub fn new_source(&mut self, path: PathBuf, content: String) -> u16 {
        let new_index = self.paths.len() as u16;
        self.paths.push(path);
        self.contents.push(content);
        new_index
    }

    pub const fn main_index(&self) -> u16 {
        Self::MAIN_INDEX
    }

    pub fn get_path(&self, index: u16) -> Option<&Path> {
        self.paths.get(index as usize).map(|x| x.as_path())
    }

    pub fn get_main_content(&self) -> &str {
        self.contents
            .get(Self::MAIN_INDEX as usize)
            .expect("main index always exists")
    }

    pub fn get_content(&self, index: u16) -> &str {
        self.contents
            .get(index as usize)
            .expect("index always exists")
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
