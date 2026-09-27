#![no_main]

use libfuzzer_sys::fuzz_target;
use spicy_lang::testing::check_parse_invariants;

// The parser invariants (docs/ecad/ast.md §5), which include the lexer's, on arbitrary
// UTF-8.
fuzz_target!(|data: &[u8]| {
    if let Ok(src) = std::str::from_utf8(data) {
        check_parse_invariants(src);
    }
});
