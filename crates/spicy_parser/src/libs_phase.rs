use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::parser_utils::parse_ident;
use crate::{
    ParseOptions, Span,
    error::{IncludeError, SpicyError},
    netlist_types::CommandType,
    statement_phase::{Statements, StmtCursor},
};

#[derive(Debug)]
pub struct SourceMap {
    /// canonicalized paths
    paths: Vec<PathBuf>,
    contents: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct SourceFileId(u16);

/// for tests outside crate
impl SourceFileId {
    pub fn dummy() -> Self {
        Self(0)
    }
}

#[cfg(test)]
impl SourceFileId {
    pub fn new(index: u16) -> Self {
        Self(index)
    }
}

impl SourceMap {
    const MAIN_INDEX: u16 = 0;

    pub fn new(main_file: PathBuf, content: String) -> Self {
        Self {
            paths: vec![main_file],
            contents: vec![content],
        }
    }

    pub fn is_in_map(&self, path: &Path) -> Option<SourceFileId> {
        self.paths
            .iter()
            .position(|p| p == path)
            .map(|i| SourceFileId(i as u16))
    }

    pub fn new_source(
        &mut self,
        path: PathBuf,
        content: String,
    ) -> std::io::Result<(SourceFileId, &str)> {
        let new_index = SourceFileId(self.paths.len() as u16);
        let canonical_path = std::fs::canonicalize(path)?;
        self.paths.push(canonical_path);
        self.contents.push(content);
        Ok((new_index, &self.contents[new_index.0 as usize]))
    }

    pub const fn main_index(&self) -> SourceFileId {
        SourceFileId(Self::MAIN_INDEX)
    }

    pub fn get_path(&self, index: SourceFileId) -> &Path {
        self.paths
            .get(index.0 as usize)
            .map(|x| x.as_path())
            .expect("source file ids have to be valid")
    }

    pub fn get_main_content(&self) -> &str {
        self.contents
            .get(Self::MAIN_INDEX as usize)
            .expect("main index always exists")
    }

    pub fn get_content(&self, index: SourceFileId) -> &str {
        self.contents
            .get(index.0 as usize)
            .expect("index always exists")
    }
}

fn span_text<'a>(src: &'a str, span: Span) -> &'a str {
    &src[span.start..=span.end]
}

fn parse_include<'a>(
    cursor: &mut StmtCursor<'a>,
    options: &mut ParseOptions,
) -> Result<(Statements, SourceFileId), SpicyError> {
    let cursors = cursor.split_on_whitespace();
    let path_cursor = cursors
        .first()
        .ok_or_else(|| SpicyError::Include(IncludeError::ExpectedPath { span: cursor.span }))?;

    let path = span_text(
        options
            .source_map
            .get_content(path_cursor.span.source_index),
        path_cursor.span,
    )
    .trim()
    .to_string();

    handle_lib_command(options, &path, path_cursor.span, None)
}

fn parse_lib_command<'a>(
    cursor: &mut StmtCursor<'a>,
    options: &mut ParseOptions,
) -> Result<(Statements, SourceFileId), SpicyError> {
    let cursors = cursor.split_on_whitespace();
    let path_cursor = cursors
        .first()
        .ok_or_else(|| SpicyError::Include(IncludeError::ExpectedPath { span: cursor.span }))?;
    let lib_cursor_opt = cursors.get(1);
    let path = span_text(
        options
            .source_map
            .get_content(path_cursor.span.source_index),
        path_cursor.span,
    )
    .trim()
    .to_string();

    // optional target library name (case-insensitive)
    let libname_opt = lib_cursor_opt.map(|lib_cursor| {
        span_text(
            options.source_map.get_content(lib_cursor.span.source_index),
            lib_cursor.span,
        )
        .trim()
        .to_string()
    });

    handle_lib_command(options, &path, path_cursor.span, libname_opt)
}

fn handle_lib_command(
    options: &mut ParseOptions,
    path: &str,
    path_span: Span,
    libname: Option<String>,
) -> Result<(Statements, SourceFileId), SpicyError> {
    let (source_index, file_content) = options.read_file(&path, path_span)?;
    let all = Statements::new(&file_content, source_index)?;
    if libname.is_none() {
        // Behave like include: return all statements except .LIB/.ENDL wrappers
        let mut filtered = Vec::new();
        for s in all.statements.into_iter() {
            let src = file_content;
            let mut c = s.into_cursor();
            if c.consume_if_command(src, CommandType::Lib) {
                continue;
            }
            if c.consume_if_command(src, CommandType::Endl) {
                continue;
            }
            filtered.push(s);
        }
        return Ok((
            Statements {
                statements: filtered,
            },
            source_index,
        ));
    }
    let libname = libname.unwrap();

    // Extract only the statements within .LIB <libname> ... .ENDL
    let mut in_block = false;
    let mut out_stmts = Vec::new();
    for s in all.statements.into_iter() {
        let src = file_content;
        let mut c = s.into_cursor();
        if c.consume_if_command(src, CommandType::Lib) {
            let name = parse_ident(&mut c, src)?;
            if !in_block && name.text.eq_ignore_ascii_case(&libname) {
                in_block = true;
            }
            continue;
        } else if c.consume_if_command(src, CommandType::Endl) {
            if in_block {
                break;
            }
            continue;
        }

        if in_block {
            out_stmts.push(s);
        }
    }

    if out_stmts.is_empty() {
        return Err(SpicyError::Include(IncludeError::LibSectionNotFound {
            span: path_span,
            lib: libname.to_string(),
            path: options.source_map.get_path(source_index).to_path_buf(),
        }));
    }

    Ok((
        Statements {
            statements: out_stmts,
        },
        source_index,
    ))
}

fn expand_includes(
    stmts: Statements,
    options: &mut ParseOptions,
    depth: usize,
    stack: &mut HashSet<PathBuf>,
) -> Result<Statements, SpicyError> {
    let mut out = Vec::new();

    for stmt in stmts.statements.into_iter() {
        // TODO: kinda sucky that you have to get the input for each statement
        let input = options.source_map.get_content(stmt.span.source_index);
        let mut cursor = stmt.into_cursor();

        if let Some(command) =
            cursor.consume_if_commands(input, &[CommandType::Include, CommandType::Lib])
        {
            let (included_stmts, source_id) = match command {
                CommandType::Include => parse_include(&mut cursor, options)?,
                CommandType::Lib => parse_lib_command(&mut cursor, options)?,
                _ => unreachable!(),
            };

            // cycle detection using canonicalized path
            let path = options.source_map.get_path(source_id).to_path_buf();
            if stack.contains(&path) {
                return Err(SpicyError::Include(IncludeError::CycleDetected {
                    span: cursor.span,
                    path: path,
                }));
            }
            if depth + 1 > options.max_include_depth {
                return Err(SpicyError::Include(IncludeError::MaxDepthExceeded {
                    span: cursor.span,
                    depth: depth + 1,
                }));
            }
            stack.insert(path.clone());
            let expanded = expand_includes(included_stmts, options, depth + 1, stack)?;
            // pop stack for this include path
            let _ = stack.remove(&path);
            out.extend(expanded.statements);
        } else {
            out.push(stmt);
        }
    }

    Ok(Statements { statements: out })
}

pub(crate) fn include_libs(
    stmts: Statements,
    options: &mut ParseOptions,
) -> Result<Statements, SpicyError> {
    let mut stack = HashSet::new();
    let main_path = options
        .source_map
        .get_path(options.source_map.main_index())
        .to_path_buf();
    stack.insert(main_path);
    expand_includes(stmts, options, 0, &mut stack)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn make_opts(main: &Path, work_dir: &Path, max_depth: usize) -> ParseOptions {
        let main_path = main.to_path_buf();
        let content = std::fs::read_to_string(&main_path).expect("read main");
        ParseOptions {
            work_dir: work_dir.to_path_buf(),
            source_path: main_path.clone(),
            source_map: SourceMap::new(main_path, content),
            max_include_depth: max_depth,
        }
    }

    #[test]
    fn include_flat_ok() {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = crate_dir.join("tests/include_inputs/root_flat.spicy");
        let dir = root.parent().unwrap();
        let mut opts = make_opts(&root, dir, 8);
        let stmts = Statements::new(
            opts.source_map.get_main_content(),
            opts.source_map.main_index(),
        )
        .unwrap();
        let expanded = include_libs(stmts, &mut opts).unwrap();
        assert_eq!(expanded.statements.len(), 5);
    }

    #[test]
    fn lib_select_section_ok() {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = crate_dir.join("tests/include_inputs/root_lib_select.spicy");
        let dir = root.parent().unwrap();
        let mut opts = make_opts(&root, dir, 8);
        let stmts = Statements::new(
            opts.source_map.get_main_content(),
            opts.source_map.main_index(),
        )
        .unwrap();
        let expanded = include_libs(stmts, &mut opts).unwrap();
        // Expect: selected mos2 section (Rlib2) + Rmain
        assert_eq!(expanded.statements.len(), 4);
        let mut found = false;
        for st in &expanded.statements {
            let src = opts.source_map.get_content(st.span.source_index);
            if src[st.span.start..=st.span.end].contains("Rlib2") {
                found = true;
                break;
            }
        }
        assert!(found, "expected Rlib2 from mos2 section");
    }

    #[test]
    fn lib_without_name_behaves_like_include() {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = crate_dir.join("tests/include_inputs/root_lib_all.spicy");
        let dir = root.parent().unwrap();
        let mut opts = make_opts(&root, dir, 8);
        let stmts = Statements::new(
            opts.source_map.get_main_content(),
            opts.source_map.main_index(),
        )
        .unwrap();
        let expanded = include_libs(stmts, &mut opts).unwrap();
        // Should include both library sections' statements (Rlib1 and Rlib2) plus Rmain
        let src_strings: Vec<String> = expanded
            .statements
            .iter()
            .map(|st| {
                opts.source_map.get_content(st.span.source_index)[st.span.start..=st.span.end]
                    .to_string()
            })
            .collect();
        let has_lib1 = src_strings.iter().any(|s| s.contains("Rlib1"));
        let has_lib2 = src_strings.iter().any(|s| s.contains("Rlib2"));
        assert!(
            has_lib1 && has_lib2,
            "expected both Rlib1 and Rlib2 in expansion"
        );
    }

    #[test]
    fn include_nested_ok() {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = crate_dir.join("tests/include_inputs/root_nested.spicy");
        let dir = root.parent().unwrap();
        let mut opts = make_opts(&root, dir, 8);
        let stmts = Statements::new(
            opts.source_map.get_main_content(),
            opts.source_map.main_index(),
        )
        .unwrap();
        let expanded = include_libs(stmts, &mut opts).unwrap();
        assert_eq!(expanded.statements.len(), 7);
    }

    #[test]
    fn include_duplicate_ok() {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = crate_dir.join("tests/include_inputs/root_duplicate.spicy");
        let dir = root.parent().unwrap();
        let mut opts = make_opts(&root, dir, 8);
        let stmts = Statements::new(
            opts.source_map.get_main_content(),
            opts.source_map.main_index(),
        )
        .unwrap();
        let expanded = include_libs(stmts, &mut opts).unwrap();
        // lib_a twice + local R1
        assert_eq!(expanded.statements.len(), 7);
    }

    #[test]
    fn include_cycle_detected() {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = crate_dir.join("tests/include_inputs/root_cycle_a.spicy");
        let dir = root.parent().unwrap();
        let mut opts = make_opts(&root, dir, 8);
        let stmts = Statements::new(
            opts.source_map.get_main_content(),
            opts.source_map.main_index(),
        )
        .unwrap();
        let err = include_libs(stmts, &mut opts).unwrap_err();
        match err {
            SpicyError::Include(IncludeError::CycleDetected { .. }) => {}
            other => panic!("expected CycleDetected, got {:?}", other),
        }
    }

    #[test]
    fn include_max_depth_exceeded() {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = crate_dir.join("tests/include_inputs/root_depth_0.spicy");
        let dir = root.parent().unwrap();
        // chain length is 5 files; set depth small to trigger
        let mut opts = make_opts(&root, dir, 2);
        let stmts = Statements::new(
            opts.source_map.get_main_content(),
            opts.source_map.main_index(),
        )
        .unwrap();
        let err = include_libs(stmts, &mut opts).unwrap_err();
        match err {
            SpicyError::Include(IncludeError::MaxDepthExceeded { .. }) => {}
            other => panic!("expected MaxDepthExceeded, got {:?}", other),
        }
    }

    #[test]
    fn include_file_not_found() {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = crate_dir.join("tests/include_inputs/root_not_found.spicy");
        let dir = root.parent().unwrap();
        let mut opts = make_opts(&root, dir, 8);
        let stmts = Statements::new(
            opts.source_map.get_main_content(),
            opts.source_map.main_index(),
        )
        .unwrap();
        let err = include_libs(stmts, &mut opts).unwrap_err();
        match err {
            SpicyError::Include(IncludeError::FileNotFound { .. }) => {}
            other => panic!("expected FileNotFound, got {:?}", other),
        }
    }

    #[test]
    fn include_absolute_path_ok() {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let abs = crate_dir.join("tests/include_inputs/lib_a.spicy");
        let main_content = format!(".include {}\nRmain x y 10\n", abs.display());
        // Create a dummy main path under tests dir
        let dummy_main = crate_dir.join("tests/include_inputs/dummy_main.spicy");
        let mut opts = ParseOptions {
            work_dir: crate_dir.join("tests/include_inputs"),
            source_path: dummy_main.clone(),
            source_map: SourceMap::new(dummy_main.clone(), main_content),
            max_include_depth: 8,
        };
        let stmts = Statements::new(
            opts.source_map.get_main_content(),
            opts.source_map.main_index(),
        )
        .unwrap();
        let expanded = include_libs(stmts, &mut opts).unwrap();
        assert_eq!(expanded.statements.len(), 3);
    }

    #[test]
    fn include_prefers_work_dir_over_source_parent() {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let work_dir = crate_dir.join("tests/include_inputs/alt");
        let root = crate_dir.join("tests/include_inputs/sub/root_search_precedence.spicy");
        let mut opts = make_opts(&root, &work_dir, 8);
        let stmts = Statements::new(
            opts.source_map.get_main_content(),
            opts.source_map.main_index(),
        )
        .unwrap();
        let expanded = include_libs(stmts, &mut opts).unwrap();
        // Find a statement from lib_a and assert its path comes from alt/
        let main_idx = opts.source_map.main_index();
        let mut found_alt = false;
        for st in &expanded.statements {
            if st.span.source_index != main_idx {
                let p = opts.source_map.get_path(st.span.source_index);
                if p.file_name().and_then(|s| s.to_str()) == Some("lib_a.spicy") {
                    if p.parent()
                        .and_then(|pp| pp.file_name())
                        .and_then(|s| s.to_str())
                        == Some("alt")
                    {
                        found_alt = true;
                        break;
                    }
                }
            }
        }
        assert!(found_alt, "expected lib_a resolved from work_dir alt");
    }

    #[test]
    fn include_uses_source_parent_when_work_dir_missing() {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // Work dir points to non-existent (or empty) place for lib_c
        let work_dir = crate_dir.join("tests/include_inputs/alt");
        // Root lives in parent directory; should resolve lib_c from parent
        let root = crate_dir.join("tests/include_inputs/root_parent_only.spicy");
        let mut opts = make_opts(&root, &work_dir, 8);
        let stmts = Statements::new(
            opts.source_map.get_main_content(),
            opts.source_map.main_index(),
        )
        .unwrap();
        let expanded = include_libs(stmts, &mut opts).unwrap();
        let main_idx = opts.source_map.main_index();
        let mut found_parent = false;
        for st in &expanded.statements {
            if st.span.source_index != main_idx {
                let p = opts.source_map.get_path(st.span.source_index);
                if p.file_name().and_then(|s| s.to_str()) == Some("lib_c.spicy") {
                    if p.parent()
                        .and_then(|pp| pp.file_name())
                        .and_then(|s| s.to_str())
                        == Some("include_inputs")
                    {
                        found_parent = true;
                        break;
                    }
                }
            }
        }
        assert!(
            found_parent,
            "expected lib_c resolved from source parent directory"
        );
    }
}
