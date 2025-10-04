pub mod error;
mod expr;
mod expression_phase;
pub mod instance_parser;
mod lexer;
mod libs_phase;
pub mod netlist_types;
pub mod netlist_waveform;
mod parser_utils;
mod statement_phase;
mod subcircuit_phase;
use std::path::{Path, PathBuf};

pub use expr::Value;
pub use lexer::Span;
pub use libs_phase::SourceMap;

use crate::{
    error::{IncludeError, SpicyError},
    expression_phase::substitute_expressions,
    instance_parser::{Deck, InstanceParser},
    libs_phase::include_libs,
    subcircuit_phase::{collect_subckts, expand_subckts},
};

#[cfg(test)]
mod test_utils;

pub struct ParseOptions<'a> {
    pub work_dir: PathBuf,
    pub source_path: PathBuf,
    pub input: &'a str,
}

impl<'a> ParseOptions<'a> {
    pub fn read_file(
        &self,
        path_str: &str,
        span: Span,
        source_map: &mut SourceMap,
    ) -> Result<(String, u16), SpicyError> {
        let path = Path::new(path_str);
        if path.is_absolute() {
            let content = std::fs::read_to_string(&path).map_err(|error| {
                SpicyError::Include(IncludeError::IOError {
                    path: path.to_path_buf(),
                    span,
                    error,
                })
            })?;
            let path_buf = path.to_path_buf();
            let source_index = source_map.new_source(path_buf.clone());
            return Ok((content, source_index));
        }

        let mut checked_paths = vec![];
        // Try joining with work_dir first
        let candidate1 = self.work_dir.join(path);
        if candidate1.exists() {
            let content = std::fs::read_to_string(&candidate1).map_err(|e| {
                SpicyError::Include(IncludeError::IOError {
                    path: candidate1.clone(),
                    span,
                    error: e,
                })
            })?;
            let path_buf = candidate1.clone();
            let source_index = source_map.new_source(path_buf.clone());
            return Ok((content, source_index));
        }
        checked_paths.push(candidate1);

        // Try joining with the parent of source_path
        if let Some(parent) = self.source_path.parent() {
            let candidate2 = parent.join(path);
            if candidate2.exists() {
                let content = std::fs::read_to_string(&candidate2).map_err(|e| {
                    SpicyError::Include(IncludeError::IOError {
                        path: candidate2.clone(),
                        span,
                        error: e,
                    })
                })?;
                let path_buf = candidate2.clone();
                let source_index = source_map.new_source(path_buf.clone());
                return Ok((content, source_index));
            }
            checked_paths.push(candidate2);
        }

        // Not found in either location
        Err(SpicyError::Include(
            crate::error::IncludeError::FileNotFound {
                path: path.to_path_buf(),
                checked_paths,
                span,
            },
        ))
    }
}

pub fn parse(options: &ParseOptions, source_map: &mut SourceMap) -> Result<Deck, SpicyError> {
    let stream = statement_phase::Statements::new(&options.input, source_map.main_index())?;
    let mut stream = include_libs(stream, options, source_map)?;
    let placeholders_map = substitute_expressions(&mut stream, &options.input)?;
    let unexpanded_deck = collect_subckts(stream, &options.input)?;
    let expanded_deck = expand_subckts(unexpanded_deck, &options.input)?;
    let mut parser = InstanceParser::new(expanded_deck, placeholders_map, options.input);
    let deck = parser.parse()?;

    Ok(deck)
}
