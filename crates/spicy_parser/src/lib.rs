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
    pub fn read_file(&self, path_str: &str, span: Span) -> Result<String, SpicyError> {
        let path = Path::new(path_str);
        if path.is_absolute() {
            return std::fs::read_to_string(&path).map_err(|e| {
                SpicyError::Include(crate::error::IncludeError::FileNotFound {
                    path: path.to_path_buf(),
                    checked_paths: vec![],
                    span,
                })
            });
        }

        let mut checked_paths = vec![];
        // Try joining with work_dir first
        let candidate1 = self.work_dir.join(path);
        if candidate1.exists() {
            return std::fs::read_to_string(&candidate1).map_err(|e| {
                SpicyError::Include(IncludeError::IOError {
                    path: candidate1,
                    span,
                    error: e,
                })
            });
        }
        checked_paths.push(candidate1);

        // Try joining with the parent of source_path
        if let Some(parent) = self.source_path.parent() {
            let candidate2 = parent.join(path);
            if candidate2.exists() {
                return std::fs::read_to_string(&candidate2).map_err(|e| {
                    SpicyError::Include(IncludeError::IOError {
                        path: candidate2,
                        span,
                        error: e,
                    })
                });
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

pub fn parse(options: &ParseOptions) -> Result<Deck, SpicyError> {
    let mut stream = statement_phase::Statements::new(&options.input)?;
    let stream_with_libs = include_libs(stream, options)?;
    let placeholders_map = substitute_expressions(&mut stream, &options.input)?;
    let unexpanded_deck = collect_subckts(stream, &options.input)?;
    let expanded_deck = expand_subckts(unexpanded_deck, &options.input)?;
    let mut parser = InstanceParser::new(expanded_deck, placeholders_map, options.input);
    let deck = parser.parse()?;

    Ok(deck)
}
