# Lexer Design (M1b)

> 2026-09-26 · Design note for roadmap M1b. **Status:** implemented in `crates/spicy_lang`.
> How `spicy_lang` turns `.spl` text into tokens: the passes, the data types, the errors, and above all the tests. The token set itself is in `grammar.md` §2 and the unit rules in §5. Why hand-written: roadmap §4.4.
> Sources read for this note (2026-09-26):
> - Spade: `externals/spade/spade-ast/src/token.rs`, `spade-parser/src/lib.rs`, `spade-tests/`
> - atopile: `externals/atopile/src/atopile/compiler/parser/AtoLexer.g4`, `src/faebryk/library/Units.py`, `test/test_parse.py`
> - Rust: `compiler/rustc_lexer/src/{lib,tests}.rs`, `compiler/rustc_parse/src/lexer/{mod,tokentrees,unicode_chars}.rs`, rust-analyzer `crates/parser/src/{lexed_str,tests}.rs` and `test_data/lexer/`
> - Zig: `lib/std/zig/{tokenizer,Ast,number_literal}.zig` and `Ast/Render.zig` (codeberg master)

---

## 1. The two passes, on one line

```rust
let r1 = Resistor { value: 4k7 ± 1% }; // top
```

**Pass 1, scan.** It cuts the text into tokens and never fails. Every byte belongs to exactly one token, including spaces and comments:

```
KwLet "let"   Whitespace " "   Ident "r1"   Whitespace " "   Eq "="   Whitespace " "
Ident "Resistor"   Whitespace " "   LBrace "{"   Whitespace " "   Ident "value"   Colon ":"
Whitespace " "   Quantity "4k7"   Whitespace " "   PlusMinus "±"   Whitespace " "   Quantity "1%"
Whitespace " "   RBrace "}"   Semi ";"   Whitespace " "   LineComment "// top"   Whitespace "\n"   Eof ""
```

Joining the texts gives back the line exactly.

**Pass 2, check.** It reads the tokens and reports every problem. For each `Quantity` it also works out the value:
- `4k7` → 4 700, with no unit (the parser later takes it as ohms, from the `value:` field);
- `1%` → 1 percent.

Nothing is wrong on this line, so pass 2 reports nothing.

**With a mistake:** `let r2 = Resistor { value: 10 kΩ ± 1% };`

- **Pass 1** still doesn't fail: `Quantity "10"`, `Whitespace " "`, `IdentNonAscii "kΩ"`, …
- **Pass 2** turns that pattern into one error with a fix:

  ```
  error[E-unit-space]: a unit must touch its number
   --> r.spl:1:28
    |
  1 | let r2 = Resistor { value: 10 kΩ ± 1% };
    |                            ^^^^^ `kΩ` reads as a separate word
    = help: remove the space: `10kΩ`
  ```

  The same span would also have given "identifiers are ASCII" for `kΩ`. It doesn't, because each error claims its span and later errors on a claimed span are dropped (§6).

**Why two passes:** pass 1 stays a small, fast, total function that's easy to fuzz. All the judgment (messages, fixes, unit decoding) lives in pass 2, where it can look at several tokens at once. This is exactly rustc's split (§2.3).

---

## 2. How others do it

### 2.1 Summary

| | **Spade** | **atopile** | **Rust** (rustc + rust-analyzer) | **Zig** |
|---|---|---|---|---|
| How it's built | `logos` derive on an enum | ANTLR4 grammar, generated Python | Hand-written `rustc_lexer`, a separate crate, reused by rust-analyzer | Hand-written state machine, one file |
| What a token holds | kind + byte range | ANTLR token (type, text, position) | `rustc_lexer`: kind + **length only**. rust-analyzer: parallel `Vec`s of kind and start | **kind + start**, struct-of-arrays (`MultiArrayList`), `u32` offsets |
| Whitespace, comments | Whitespace dropped; comments into a side list | On a hidden channel: in the stream, invisible to the parser | Real tokens | Dropped. `zig fmt` finds comments by scanning the source between tokens |
| Bad input | Returns an error: "Lexer error, unexpected symbol" | ANTLR error listener | **Never fails.** Problems are flags on tokens (`terminated: false`, `Unknown`, `InvalidIdent`) and a second layer reports them | **Never fails.** An `invalid` token up to the end of the line, then it resumes |
| Multi-char operators | In the lexer (`<=`) | In the lexer | `rustc_lexer` emits single characters only; they're glued later, when nothing separates them (`tokentrees.rs`), because macros need single characters | In the lexer |
| Numbers with suffixes | Integer regex with a size suffix | `NUMBER` and the unit are two separate tokens, joined by the grammar (`quantity: number unit?`) | The suffix is part of the literal (`1u8`: `suffix_start`) and is checked later | **Permissive:** `9z3` is one `number_literal`; a separate `parseNumberLiteral` pass gives the precise error |
| Look-alike characters | — | — | A table of 264 (`unicode_chars.rs`: `−` "Minus Sign" → `-`, `;` "Greek Question Mark" → `;`) | — |
| Keywords | `#[token("let")]` | Grammar literals | Identifiers, keyword check later | Scan the identifier, then a compile-time string map |
| Dependencies | `logos` (proc macro) | ANTLR runtime | `unicode-ident`, `unicode-properties` | none |

atopile's unit decoding (`decode_symbol` in `Units.py`) is the same idea as ours (`grammar.md` §5):
1. an exact unit match;
2. otherwise "a known unit at the end, and a prefix that this unit accepts" (units carry an `is_si_prefixed` flag);
3. otherwise an error listing close matches.

### 2.2 How they test

| | **Spade** | **atopile** | **Rust** | **Zig** |
|---|---|---|---|---|
| Lexer unit tests | A handful in `token.rs` (identifiers, integer literals, doc comments) | None found | `rustc_lexer/src/tests.rs`: `check_lexing(src, expect![[…]])`, an inline snapshot of the token dump (`expect_test`) | `testTokenize(src, &.{ .tag, … })`, 34 test blocks inline in `tokenizer.zig`, plus a table of number cases in `number_literal.zig` |
| Data-driven cases | Parser test files | End-to-end example builds | rust-analyzer: `test_data/lexer/ok/*.rs` and `err/*.rs`, each with a `.rast` file of `KIND "text" error: …` lines. One test walks the directory; `UPDATE_EXPECT=1` rewrites the expectations | — |
| Error messages | ~800 insta snapshots of rendered errors (`spade-tests`) | pytest asserting message strings (3 tests in `test_parse.py`) | UI tests: a `.stderr` file per case, updated with `--bless` | — |
| Fuzzing | None found | None found | Extensive (outside the lexer crate) | **Properties** checked on random input (`testPropertiesUpheld`): end ≥ start; an invalid token ends at a newline or EOF; EOF is zero-length at the end. The input bytes are weighted toward whitespace and printable ASCII |

### 2.3 What we take from each

- **Rust:** a lexer that never fails, with problems carried by the tokens, and a second layer that reports them. The literal's suffix is part of the literal. A look-alike table (we need a short one, §6.3). rust-analyzer's case-file testing, with `KIND "text"` dumps.
- **Zig:**
  - The struct-of-arrays token list with `u32` starts.
  - Permissive number scanning with precise errors afterwards (`9z3` is one token).
  - Keywords by string lookup after scanning an identifier.
  - Invariants checked on random input.
  - Unlike Zig, we keep comments as tokens, since we have block comments and Zig doesn't.
- **atopile:** the unit-decoding order, and "close matches" in the error.
- **Spade:** rendered-error snapshots as the main regression suite.
- **What we don't take:**
  - Gluing multi-character operators after lexing (rustc does it for macros, which we don't have).
  - Dropping comments (Zig, Spade).
  - Separate number and unit tokens (atopile): with whitespace hidden, its grammar also accepts `10 kohm`.

---

## 3. Decisions

| # | Decision | Why |
|---|---|---|
| L1 | **Two passes:** `scan` (total, never fails) and `check` (diagnostics + quantity decoding) | Rust's split. Pass 1 is easy to make fast and to fuzz. Pass 2 can look across tokens (`10 kΩ`, `+-`) |
| L2 | **Every byte is in exactly one token**, trivia included, and a zero-length `Eof` token closes the list | Byte-for-byte round trip (roadmap §4.4). The parser never needs to check for end of input separately |
| L3 | **Struct-of-arrays:** `kinds: Vec<TokenKind>` (1 byte each) and `starts: Vec<u32>`. Token `i` covers `starts[i]..starts[i+1]` | Zig and rust-analyzer. 5 bytes per token, no spans stored twice, cache-friendly. `u32` caps a file at 4 GiB, which is checked on load |
| L4 | **Multi-character operators are formed in pass 1** (`<=`, `..=`, `::`, `->`, `+/-`) | No macros, so there's no reason for rustc's single-character layer |
| L5 | **Quantities scan permissively:** a digit starts a `Quantity`, which then takes every letter, digit, `_`, `µ`, `μ`, `Ω`, `Ω`, `°`, `%` (and the `°` look-alikes `º` `˚`) glued to it. Pass 2 decodes it | Zig's approach. `4.7k7` and `47q` are one token each and get one precise error each, instead of confusing follow-on errors |
| L6 | **The value is correctly rounded, with no allocation:** the digits are read as an integer and a power of ten (`100nF` → 100 × 10^-9), then **Clinger's fast path**: when the digits fit in 53 bits and the power is at most 22, one exact multiply or divide (100 / 1e9). Otherwise std's `str::parse::<f64>` runs the full algorithm. This is what Rust's `dec2flt` (`can_use_fast_path`) and Zig's `parse_float` (`isFastPath`) do internally | Multiplying by an inexact power loses the last bit: `100.0 * 1e-9` is `1.0000000000000001e-07`, but 100 / 1e9 is `1e-07`. Across 14 common values (1 … 680) × 8 prefixes, **26 of 112** such multiplications are off in the last bit. Exact values keep export round trips exact (`100n` prints back as `100n`). A test checks the fast path against std on 200 000 random inputs and on both sides of every limit |
| L7 | **Unknown characters are one token per character** (`Unknown`), and the rest of the line keeps lexing | Rust's behavior (Zig skips to the end of the line). A look-alike fix applies to one character, and the parser still sees the tokens after it |
| L8 | **Identifiers continue over non-ASCII letters** (`char::is_alphanumeric`) but get the kind `IdentNonAscii`, which pass 2 rejects | One clear error per identifier (`r1_α`: "identifiers are ASCII"), not a fragment and a stray character. It also lets `10 kΩ` be recognized as a spaced-out unit |
| L9 | **Keywords by lookup after scanning an identifier.** The language's keywords get their own kinds; all reserved words share `KwReserved`. `mode`, `event`, `observe`, `emits`, `with` and `on` are contextual: identifiers the parser recognizes only in their position, so they stay usable as names (a part's `mode:` field) | Zig's approach. The parser reports "`fn` is reserved" from the text. Contextual words as Rust's `union` and grammar.md §2.3's `part` |
| L10 | **The lexer's errors are data** (an enum with spans and optional fixes), rendered through codespan-reporting in one shared module | The editor, the language server and the AI get structured fixes (P8). Snapshot tests see the rendered text |
| L11 | **No parsing dependencies.** A ~40-line byte cursor, not `unscanny`; no Unicode tables. (codespan-reporting renders the diagnostics, nothing else) | Roadmap §4.4. ASCII fast path; non-ASCII is decoded only where it appears |

---

## 4. Data types (sketch)

```rust
/// Output of pass 1. Borrows the source; one entry per token.
pub struct Tokens<'src> {
    src: &'src str,
    kinds: Vec<TokenKind>,   // the last one is Eof
    starts: Vec<u32>,        // len = kinds.len() + 1; the extra entry is src.len()
}
impl Tokens<'_> {
    pub fn len(&self) -> usize;
    pub fn kind(&self, i: TokenIdx) -> TokenKind;
    pub fn span(&self, i: TokenIdx) -> Span;         // starts[i]..starts[i+1]
    pub fn text(&self, i: TokenIdx) -> &str;
}

#[repr(u8)]
pub enum TokenKind {
    // trivia (the parser skips these)
    Whitespace, LineComment, BlockComment, UnterminatedBlockComment,
    // significant
    DocComment,
    Ident, IdentNonAscii,
    KwBlock, KwCircuit, KwSetup, KwContract, KwEnv, KwConst, KwPub, KwPort, KwNet, KwLet,
    KwAssume, KwSpec, KwRated, KwEnsure, KwWithin, KwFor, KwIn, KwReserved,
    Quantity, Str, UnterminatedStr,
    LBrace, RBrace, LParen, RParen, LBracket, RBracket, Lt, Gt, Le, Ge,
    Comma, Semi, Colon, ColonColon, Dot, DotDot, DotDotEq, Eq,
    Plus, Minus, Arrow, Star, Slash, PlusMinus, Pound, Question,
    Unknown,
    Eof,
}

/// Output of decoding one Quantity token (pass 2; the parser calls the same function).
pub struct QuantityLit {
    pub value: f64,              // prefix applied: `1kHz` → 1000.0
    pub unit: Option<Unit>,      // None for `47k`, `3`
}

pub fn scan(src: &str) -> Tokens<'_>;                              // pass 1
pub fn check(tokens: &Tokens) -> Vec<LexError>;                    // pass 2
pub fn decode_quantity(text: &str) -> Result<QuantityLit, QuantityError>;
```

- **`Unit` is `spicy_model::units::Unit`** (`Volt`, `Ohm`, `Celsius`, `Percent`, …): the lexer maps each spelling (`ohm`, `Ω`, U+2126) to it, and the model's `Unit::quantity` maps it to a dimension, in one place (roadmap §2.4). The formatter reprints the original text, so no spelling information is needed here.
- **`decode_quantity` is called by pass 2** (to find errors everywhere, even in regions the parser skips while recovering) **and by the parser** (to get the value). It's a pure function on a few bytes, so calling it twice costs nothing and saves keeping a side table.

---

## 5. Pass 1: scanning rules

At each position, the first matching rule wins:

| Starts with | Token | Rule |
|---|---|---|
| UTF-8 BOM at offset 0 | `Whitespace` | Kept as trivia, so the round trip is exact (Zig and rustc skip it) |
| space, `\t`, `\r`, `\n` | `Whitespace` | The whole run |
| `///` but not `////` | `DocComment` | To end of line |
| `//` | `LineComment` | To end of line (`////` too, as in Rust) |
| `/*` | `BlockComment` | Nested depth count. Reaching EOF gives `UnterminatedBlockComment` |
| ASCII letter or `_` | `Ident` / `Kw*` | Take `[A-Za-z0-9_]` and any non-ASCII alphanumeric. Any non-ASCII inside makes it `IdentNonAscii`. Then the keyword lookup |
| non-ASCII alphanumeric (`µ`, `Ω`, `α`) | `IdentNonAscii` | As above |
| digit | `Quantity` | Mantissa `[0-9][0-9_]*`; then `.` only if a digit follows; then an exponent `[eE][+-]?digit…` only if a digit follows `e` (or the sign); then the suffix, every glued `[A-Za-z0-9_]` / `µ μ Ω Ω ° %`, and the `°` look-alikes `º ˚` (`quantity_len`) |
| `"` | `Str` | To the closing `"` on the same line; `\"` and `\\` don't close it. Without a closing `"` before the end of the line: `UnterminatedStr`, up to the line end |
| `+/-` | `PlusMinus` | Only when the three characters touch |
| `±` | `PlusMinus` | |
| `..=`, `..`, `::`, `->`, `<=`, `>=` | as named | Longest match first; `->` (`Arrow`) only when the two characters touch |
| one of `{}()[]<>,;:.=+-*/#?` | as named | |
| anything else (including a lone `%`, `−`, `;`, U+00A0) | `Unknown` | One character |
| end of input | `Eof` | Zero length |

`scan` never fails, allocates only the two vectors, and visits each byte a bounded number of times.

---

## 6. Pass 2: checks

### 6.1 What it reports

Numbers `#…` refer to the error list in `grammar.md` §7.

| Check | Looks at | Example | Diagnostic |
|---|---|---|---|
| Quantity decoding | each `Quantity` | `47q`, `1Meg`, `k°C`, `4.7k7`, `4k7k`, `4k7%`, `1e`, `1e400`, `1e-400` | #9 unknown suffix (with close matches; a fix only when there's exactly one, since `1mhz` could be `mHz` or `MHz`), #10 `Meg` → `M`, #11 no prefix on this unit (`4k7%` → `4.7%`; `°C`, `%`, `dB`, `h` and `y` take none; `10mh`, `10µh` and `4u7h` get no fix, since `h` may be `H`, and the help offers `mH`, `µH`, `uH`), #12 decimal point *or* infix prefix, not both; a second prefix after an infix one; missing exponent digits; value too large or too small |
| Space before a unit | `Quantity`, whitespace, then a word that decodes as a suffix and doesn't start with a digit (`1 2` is two numbers) | `10 V`, `10 kΩ` | #13 remove the space |
| Bare decimal point | `Dot` touching a following `Quantity` (not right after a name, a string or `)`, where it's a field access: `x.5`); a `Quantity` touching a following `Dot` that isn't a field access. Only when the fixed text decodes: `1.5.` and `.4k7` are left to the parser and the number check | `.5`, `1.` | #14 write `0.5` / `1.0` |
| `+-`, `--` | `Plus` touching `Minus`; a run of touching `Minus` (one error for the run, checked from its first `-`) | `12V +- 5%`, `----x` | #2 did you mean `±` or `+/-`; #3 |
| Lone `%` | `Unknown "%"` | `a % 3` | #15 `%` only means percent, glued to a number. After a number (`± 1 %`) the space check wins instead: "remove the space: `1%`" |
| Unterminated comment or string | `UnterminatedBlockComment`, `UnterminatedStr` | `/* …`, `"not closed` | At the opener, `/*` or `"`: #22, and "unterminated string" (a string is one line) |
| Unknown escape | `Str` | `"a\nb"` | "unknown escape `\n`": the only escapes are `\"` and `\\`. One error per string, at its first |
| Look-alikes | `Unknown` in the table (§6.3) | `−`, `;`, `≤` | #23 "this is `−` (U+2212 MINUS SIGN), not `-`", with a replacement |
| Other unknown characters | `Unknown` | `§` | unexpected character, showing its code point |
| Non-ASCII identifier | `IdentNonAscii` | `r1_α` | #24 identifiers are ASCII |

The remaining `grammar.md` §7 errors belong to the parser (M1c). That moves #2 and #3 from the parser to the lexer; `grammar.md` is updated to match.

### 6.2 One loop, one error per span

`check` is **one loop over the tokens** with a `match` on each token's kind, so each token is looked at once. This is how rustc does it (it validates each raw token as it's produced, in `next_token_from_cursor`), and rust-analyzer too (`LexedStr::new`). Zig goes further and checks a number literal only when `AstGen` uses it. We don't, because we want errors inside regions the parser skips while recovering.

Each check looks at one token kind and returns at most one error; the loop decides what's kept. **Each error claims its span.** An error that starts inside a claimed span is dropped. For a `Quantity`, the checks run most specific first: a spaced-out unit, then the number itself, then a trailing `.`. That's how `10 kΩ` gives only "remove the space", not also "identifiers are ASCII" for `kΩ`.

Every check reports at or after its own token, so claims only move forward. One `claimed_until` offset tracks them, and the errors come out already in source order.

**Measured on 10 MB** (release, the amplifier repeated): scanning takes about 24 ms and checking about 9 ms. Checking took about 20 ms when it was seven separate passes over the tokens.

### 6.3 The look-alike table

rustc has 264 entries. We start with the ones that come from datasheets, PDFs and word processors (the AI will paste from them):

| Character | Name | Suggest |
|---|---|---|
| `−` `–` `—` `‐` | minus sign, en dash, em dash, hyphen | `-` |
| `;` | Greek question mark | `;` |
| `≤` `≥` | | `<=` `>=` |
| `→` | rightwards arrow | `->` |
| `×` `·` | multiplication sign, middle dot | `*` |
| `∕` `÷` | division slash, division sign | `/` |
| `º` `˚` | masculine ordinal, ring above | `°`. `º` is a letter, so it only gets this fix inside a number's suffix (`10ºC` → `10°C`); elsewhere it reads as an identifier character |
| U+00A0, U+2009, U+202F | no-break space, thin space, narrow no-break space | a normal space, or no space before a unit (§6.1) |
| `＝` `：` `，` `（` `）` | fullwidth forms | the ASCII character |

`µ`/`μ` and `Ω`/`Ω` aren't here: both forms are accepted (`grammar.md` §5.1).

---

## 7. Testing

Four kinds of test, each with a job the others can't do.

### 7.1 Case files: the main suite

The rust-analyzer layout:

```
crates/spicy_lang/test_data/lexer/
  ok/
    quantities.spl        every row of grammar.md §5.2, one per line
    comments.spl          //, ///, ////, nested /* /* */ */
    punctuation.spl       every operator, including ..= next to numbers (0..N, 100..=300, 0.5..=1)
    strings.spl           escapes, and nothing inside a string is a comment, a number or a look-alike
    …
  err/
    unit_space.spl        10 V, 10 kΩ, 10 °C (with U+2009)
    lookalikes.spl        one line per table row in §6.3
    …
```

For each case file, the test writes two snapshots:
- **the token dump**, one `KIND "text"` line per token, with ` error: …` after any token an error starts on (rust-analyzer's `.rast` format);
- **for `err/` files, the rendered diagnostics** as plain text (no color), the way the user sees them (Spade's and rustc's regression suites).

**The tool is `insta`, which the repo already uses:**
- Differences are reviewed with `cargo insta review`.
- The `glob!` macro runs the test once per file. It's a dev-only feature, so nothing is added to the shipped build.

The alternative is a ~40-line runner of our own, like `spicy_simulate`'s `test_util.rs` with its `SPICY_UPDATE_SNAPSHOTS=1`. It would avoid the `glob` feature, but you'd lose `cargo insta review`.

`circuits/ce_amp.spl` has its own test (`include_str!`), so the case files never drift from it.

**A coverage rule:** a test lists every `LexErrorKind` variant (in a hand-written `ALL_NAMES` array, test-only) and fails if some variant never appears in an `err/` snapshot. Every error kind is tested at least once, and adding a variant without a test fails CI.

### 7.2 Unit tests for quantity decoding

A plain table, `(input, expected)`:
- every row of `grammar.md` §5.2;
- every rejection in §6.1.

**The exactness test:** for each E-series value (E24, one decade) × every prefix, `decode_quantity` must equal the `f64` parsed from the equivalent decimal string. It pins decision L6: the 26 cases where multiplying would be off must be exact.

### 7.3 Invariants, on every input

These must hold for **any** `&str`:
1. `scan` doesn't panic; neither does `check`.
2. The tokens tile the input: `starts[0] == 0`, starts strictly increase, the last start equals `src.len()`, and every start is on a character boundary.
3. **Round trip:** joining every token's text gives back the input.
4. Only `Eof` is empty, and it's last.
5. Every error span lies inside the input, on character boundaries, and claimed spans don't overlap (§6.2).
6. **Stable when appended to:** lexing `src` and then `src + "\n"` gives the same tokens, except the final whitespace. (A cheap way to catch "the last token swallowed EOF" bugs.)

They're checked in three places:
- **On every case file.** Free, runs with the suite.
- **A seeded random test in `cargo test`:** a hand-written xorshift generator, 10 000 inputs of up to 48 pieces each. The alphabet is weighted, as in Zig: mostly the characters that matter (`0-9 k m M . = ± + / - * ( ) { } ; : _ µ Ω ° % " "` and newline), some random Unicode. Deterministic, dependency-free, and runs in CI on stable.
- **A cargo-fuzz target** next to the existing `fuzz_spicy_parser`, for long runs on nightly.

### 7.4 Speed

An `#[ignore]` test lexes a generated ~10 MB file (the amplifier repeated) and prints MB/s. It doesn't gate anything and needs no benchmark crate. It's there so "fast" is a number we've measured, not a claim.

---

## 8. Files and dependencies

```
crates/spicy_lang/
  Cargo.toml             deps: codespan-reporting · dev-deps: insta (glob)
  README.md              responsibility (roadmap §2.3)
  src/lib.rs
  src/lexer/mod.rs       scan (pass 1): next_kind, one rule per token, the cursor last
  src/lexer/token.rs     TokenKind, Tokens, Span
  src/lexer/check.rs     check (pass 2): one check per token kind, the claim rule
  src/lexer/error.rs     LexErrorKind and its text (as parser/error.rs, resolve/error.rs)
  src/lexer/quantity.rs  everything about numbers and units, each rule once: where a
                         Quantity ends (quantity_len, used by scan and check),
                         decode_quantity, the unit and prefix tables, suggestions
  src/lexer/lookalike.rs the §6.3 table
  src/diagnostic.rs      error data → codespan-reporting rendering (shared with the parser)
  test_data/lexer/{ok,err}/
fuzz/fuzz_targets/spicy_lang_lexer.rs
```

**Out of scope for the lexer:** reading files, checking UTF-8, and the 4 GiB limit. That belongs to whoever loads source files (the CLI's `check`/`export` commands, M1d–M1f); it isn't built yet.

---

## 9. Open questions

1. **`insta` with `glob`, or our own runner** (§7.1)? I'd use insta: the review workflow is worth a dev-only feature.
2. **Should `//!` (inner doc) and `/** */` be recognized** now, or lexed as plain comments until they mean something? I'd lex them as plain comments: a comment can't change meaning, so this doesn't bend P1.
3. **Error codes:** `language.md` uses `E-unit`, `E-rail`. I'd use the same style here (`E-unit-space`, `E-unit-suffix`, `E-lookalike`) and keep a list in `grammar.md` §7.
