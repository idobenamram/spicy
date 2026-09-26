#![no_main]

use libfuzzer_sys::fuzz_target;
use spicy_lang::testing::check_invariants;

// The lexer invariants (docs/ecad/lexer.md §7.3) on arbitrary UTF-8.
fuzz_target!(|data: &[u8]| {
    if let Ok(src) = std::str::from_utf8(data) {
        check_invariants(src);
    }
});
