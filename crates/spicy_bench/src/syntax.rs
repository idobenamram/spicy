//! Inputs for the lexer and the parser alone: a file of long expressions, and a file of
//! mistakes. Neither is meant to resolve; their benchmarks stop after the parse.

use std::fmt::Write;

/// A contract of `n` specs, each 24 terms long: `spec s00042: <12 terms> within
/// <6 terms>..=<6 terms>;`. The terms go through every form in [`TERMS`] and every
/// quantity in [`QUANTITIES`], joined by the four arithmetic operators, so each spec
/// runs every level of the Pratt loop (comparison, range, tolerance, sum, product,
/// unary minus, postfix) and decodes each form of quantity twice. It replaces the
/// earlier `long_expr.spl`, `deep_120.spl`, `neg_chain.spl` and `quantities.spl`.
///
/// It parses clean. Resolve would reject it (a contract of no block), so only the lexer
/// and the parser read it.
pub fn expressions(n: usize) -> String {
    let mut src = String::from("contract E {\n");
    for i in 0..n {
        // Every quantity twice per spec (7 and 12 are coprime), each spec starting at
        // another one.
        let terms: Vec<String> = (0..24)
            .map(|t| {
                let quantity = QUANTITIES[(i * 5 + t * 7) % QUANTITIES.len()];
                TERMS[(i + t) % TERMS.len()].replace("{q}", quantity)
            })
            .collect();
        writeln!(
            src,
            "    spec s{i:05}: {} within {}..={};",
            chain(&terms[..12]),
            chain(&terms[12..18]),
            chain(&terms[18..]),
        )
        .unwrap();
    }
    src.push_str("}\n");
    src
}

/// One quantity of each form the lexer decodes: a prefix alone, a decimal with a
/// non-ASCII prefix and a unit, a signed exponent, digit separators, a non-ASCII unit,
/// a prefix and a unit, the infix form, a unit alone, an exponent with a two-letter
/// unit, degrees, a percentage, decibels.
const QUANTITIES: [&str; 12] = [
    "1k",
    "2.2µF",
    "4.7e-3V",
    "1_000_000",
    "47kΩ",
    "100nF",
    "4k7",
    "12V",
    "1.5e9Hz",
    "10°C",
    "5%",
    "3dB",
];

/// The forms of a term around its quantity `{q}`: alone, under a unary minus, in nested
/// parentheses, with a tolerance (in parentheses, as `±`'s operand rule wants it inside
/// arithmetic), and as the argument of a method chain.
const TERMS: [&str; 5] = ["{q}", "-{q}", "(({q}))", "({q} ± 1%)", "h.at({q}).mag()"];

/// The operators between the terms of a chain, in turn.
const OPERATORS: [&str; 4] = ["+", "-", "*", "/"];

/// `terms` joined by the [`OPERATORS`] in turn: `a + b - c * d / e + …`.
fn chain(terms: &[String]) -> String {
    let mut out = terms[0].clone();
    for (k, term) in terms[1..].iter().enumerate() {
        write!(out, " {} {term}", OPERATORS[k % OPERATORS.len()]).unwrap();
    }
    out
}

/// A file of `n` copies of each broken line in [`BROKEN`], in its place: one block,
/// circuit, setup and contract that hold all their copies, then the broken items. Each
/// line has one mistake, which the lexer or the parser reports, and every kind of line
/// has some (ports, statements of both bodies, setup entries, items), so each way the
/// parser skips past a mistake runs: an item, a statement, a value, an entry. It
/// replaces the earlier `err_*.spl`.
pub fn syntax_errors(n: usize) -> String {
    let mut src = String::new();
    for place in BROKEN {
        src.push_str(place.open);
        for i in 0..n {
            for (line, _) in place.lines {
                writeln!(src, "{}", numbered(line, i)).unwrap();
            }
        }
        src.push_str(place.close);
    }
    src
}

/// Where a kind of broken line goes, and its lines.
struct Place {
    /// The text before the lines, and after them.
    open: &'static str,
    close: &'static str,
    /// Each broken line, with the one problem it's meant to have. `{i}` in a line
    /// stands for its copy's number.
    lines: &'static [(&'static str, &'static str)],
}

/// The broken lines of [`syntax_errors`], each with the problem it's meant to have.
const BROKEN: [Place; 5] = [
    // A block's ports: the entries of a comma list. (They hold the earlier files'
    // `port` line: a port is declared in its block now.)
    Place {
        open: "block Bad {\n",
        lines: &[
            // Missing `,` before the next port (soft: reported, and the list goes on).
            ("    a{i}: Pin", "MissingComma"),
            // A value where the type goes: the port keeps its name, its type is skipped.
            ("    b{i}: 12V,", "Expected"),
            // Missing `:`: the whole port is skipped, up to its `,`.
            ("    c{i} Power<In>,", "Expected"),
        ],
        close: "}\n\n",
    },
    // A circuit's statements: the earlier files' other eight lines.
    Place {
        open: "circuit Bad {\n",
        lines: &[
            // An unknown suffix, with the suggestions that come with it.
            (
                "    let r{i} = Resistor { a: vcc, value: 47q ± 1% };",
                "UnknownSuffix",
            ),
            // A space before the unit. The parser's error at `kΩ` repeats it, and is
            // dropped.
            (
                "    let s{i} = Resistor { a: vcc, value: 10 kΩ };",
                "UnitSpace",
            ),
            // Missing name: the statement is skipped, with the braces after the error.
            ("    let = Resistor { a: vcc };", "Expected"),
            // Missing `;` before the next statement (soft).
            ("    let c{i} = 1uF ± 20%", "MissingSemi"),
            // A look-alike minus, read as `-`.
            ("    let x{i} = 3 − 1;", "Lookalike"),
            // A decimal point and an infix prefix: the number becomes an error node.
            ("    let y{i} = 4.7k7;", "DecimalAndInfix"),
            // `±` over a sum: both readings are written out.
            ("    let z{i} = a ± b + c;", "AmbiguousTolerance"),
            // A lone `%`. The parser's "expected an expression" repeats it, and is
            // dropped; the value is skipped up to the `;`.
            ("    let q{i} = % 5;", "LonePercent"),
        ],
        close: "}\n\n",
    },
    // A setup's entries: the entries of a comma list, with expressions for values.
    Place {
        open: "setup Run for Bad {\n",
        lines: &[
            // `=` for `:` in a field (soft: read as `:`).
            ("    v{i}: Supply { v = 12V },", "FieldEquals"),
            // Missing `:` after the key: the entry is skipped.
            ("    t{i} 25°C,", "Expected"),
        ],
        close: "}\n\n",
    },
    // A contract's statements.
    Place {
        open: "contract Bad {\n",
        lines: &[
            // A spec without a name: the statement is skipped.
            ("    spec dc(out.v) within 1V..=2V;", "SpecNeedsName"),
            // A spec that compares nothing.
            ("    spec g{i}: out.v;", "NotARelation"),
        ],
        close: "}\n\n",
    },
    // Items.
    Place {
        open: "",
        lines: &[
            // Missing `:`: the item is skipped, up to the next one.
            ("const K{i} Ohm = 1k;", "Expected"),
            // `pub` on an item other than a block (soft).
            ("pub circuit P{i} {}", "PubNotAllowed"),
        ],
        close: "",
    },
];

/// `line` with its `{i}` replaced by copy `i`'s number, in five digits.
fn numbered(line: &str, i: usize) -> String {
    line.replace("{i}", &format!("{i:05}"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use spicy_errors::DiagKind;
    use spicy_lang::lexer::TokenKind;
    use spicy_lang::parser::parse;

    /// The text parses clean, and every operator of the precedence table (grammar.md
    /// §4.1) is in it, so each one runs its level of the Pratt loop.
    #[test]
    fn expressions_parse_clean_with_every_operator() {
        use TokenKind::*;
        let src = expressions(12);
        let parsed = parse(&src);
        assert!(parsed.lex_errors.is_empty(), "{:?}", parsed.lex_errors);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let tokens = &parsed.tokens;
        let kinds: HashSet<TokenKind> = (0..tokens.len()).map(|i| tokens.kind(i)).collect();
        for kind in [
            KwWithin, DotDotEq, PlusMinus, Plus, Minus, Star, Slash, LParen, Dot,
        ] {
            assert!(kinds.contains(&kind), "{kind:?}");
        }
    }

    /// Each broken line has exactly the one problem [`BROKEN`] gives it, and no other
    /// line has any: every problem of the lexer and the parser, in source order, with
    /// the line it's on.
    #[test]
    fn each_broken_line_has_its_one_problem() {
        let copies = 2;
        let src = syntax_errors(copies);
        let parsed = parse(&src);
        let lines: Vec<&str> = src.lines().collect();
        let mut found: Vec<(u32, &str)> = parsed
            .lex_errors
            .iter()
            .map(|e| (e.span.start, e.kind.name()))
            .chain(parsed.errors.iter().map(|e| (e.span.start, e.kind.name())))
            .collect();
        found.sort();
        let found: Vec<(String, &str)> = found
            .into_iter()
            .map(|(at, kind)| {
                let line = src[..at as usize].matches('\n').count();
                (lines[line].to_string(), kind)
            })
            .collect();

        let expected: Vec<(String, &str)> = BROKEN
            .iter()
            .flat_map(|place| {
                (0..copies).flat_map(|i| {
                    place
                        .lines
                        .iter()
                        .map(move |&(line, kind)| (numbered(line, i), kind))
                })
            })
            .collect();
        assert_eq!(found, expected);
    }
}
