#![no_main]

use libfuzzer_sys::fuzz_target;
use spicy_parser::{parse, ParseOptions, SourceMap};

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let path = std::path::PathBuf::from("fuzz_input.spicy");
        let source_map = SourceMap::new(path.clone(), s.to_string());
        let mut options = ParseOptions {
            work_dir: std::path::PathBuf::from("."),
            source_path: path,
            source_map,
            max_include_depth: 0,
        };
        let _ = parse(&mut options);
    }
});
