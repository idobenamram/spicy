use crate::{
    error::{IncludeError, SpicyError}, netlist_types::CommandType, statement_phase::{Statement, Statements, StmtCursor}, ParseOptions, Span
};

pub struct StatementWithSource<'a> {
    pub statement: Statement,
    pub source_map: u16,
}

fn span_text<'a>(src: &'a str, span: Span) -> &'a str {
    &src[span.start..=span.end]
}

fn parse_include<'a>(
    cursor: &mut StmtCursor<'a>,
    options: &ParseOptions<'a>,
) -> Result<StatementWithSource<'a>, SpicyError> {
    let cursors = cursor.split_on_whitespace();
    let path_cursor = cursors
        .first()
        .ok_or_else(|| SpicyError::Include(IncludeError::ExpectedPath { span: cursor.span }))?;

    let path = span_text(options.input, path_cursor.span).trim();
    println!("path: {}", path);
    let file_content = options.read_file(path, path_cursor.span)?;
    let statements = Statements::new(&file_content)?;


}

pub(crate) fn include_libs(
    stmts: Statements,
    options: &ParseOptions,
) -> Result<Vec<StatementWithSource>, SpicyError> {
    let mut out = Vec::new();

    for stmt in stmts.statements.into_iter() {
        let mut cursor = stmt.into_cursor();
        if cursor.consume_if_command(options.input, CommandType::Include) {
            out.push(parse_include(cursor, options)?);
        } else {
            out.push(StatementWithSource {
                statement: stmt,
                source: options.input,
            });
        }
    }

    Ok(out)
}
