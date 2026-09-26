//! Reading a `Quantity` token: `47k`, `4k7`, `1uF`, `10°C`, `5%` (grammar.md §2.5, §5).
//!
//! The scanner and the decoder share [`scan_mantissa`] and [`is_suffix_char`], so they
//! can never disagree about where a number ends.

use std::ops::Range;

/// What a unit spelling means. Only the spelling: dimensions live in `spicy_model`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnitSym {
    Volt,
    Amp,
    Ohm,
    Farad,
    Henry,
    Hertz,
    Second,
    Watt,
    Kelvin,
    Celsius,
    Percent,
    Decibel,
}

impl UnitSym {
    /// The canonical spelling, used in messages.
    pub fn symbol(self) -> &'static str {
        match self {
            UnitSym::Volt => "V",
            UnitSym::Amp => "A",
            UnitSym::Ohm => "Ω",
            UnitSym::Farad => "F",
            UnitSym::Henry => "H",
            UnitSym::Hertz => "Hz",
            UnitSym::Second => "s",
            UnitSym::Watt => "W",
            UnitSym::Kelvin => "K",
            UnitSym::Celsius => "°C",
            UnitSym::Percent => "%",
            UnitSym::Decibel => "dB",
        }
    }

    /// `k°C`, `m%` and `kdB` make no sense, so these units take no prefix.
    pub fn accepts_prefix(self) -> bool {
        !matches!(self, UnitSym::Celsius | UnitSym::Percent | UnitSym::Decibel)
    }
}

/// Every accepted unit spelling. `Ω` appears twice: U+03A9 (Greek capital omega) and
/// U+2126 (ohm sign) look identical, so both are accepted.
const UNITS: &[(&str, UnitSym)] = &[
    ("V", UnitSym::Volt),
    ("A", UnitSym::Amp),
    ("\u{3A9}", UnitSym::Ohm),
    ("\u{2126}", UnitSym::Ohm),
    ("ohm", UnitSym::Ohm),
    ("F", UnitSym::Farad),
    ("H", UnitSym::Henry),
    ("Hz", UnitSym::Hertz),
    ("s", UnitSym::Second),
    ("W", UnitSym::Watt),
    ("K", UnitSym::Kelvin),
    ("°C", UnitSym::Celsius),
    ("%", UnitSym::Percent),
    ("dB", UnitSym::Decibel),
];

/// SI prefixes, case-sensitive. Micro has three spellings: `u`, `µ` (U+00B5, micro sign)
/// and `μ` (U+03BC, Greek mu).
const PREFIXES: &[(char, i32)] = &[
    ('f', -15),
    ('p', -12),
    ('n', -9),
    ('u', -6),
    ('\u{B5}', -6),
    ('\u{3BC}', -6),
    ('m', -3),
    ('k', 3),
    ('M', 6),
    ('G', 9),
    ('T', 12),
];

/// A decoded quantity literal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuantityLit {
    /// The value with the prefix applied: `1kHz` → 1000.0, `47k` → 47000.0.
    pub value: f64,
    /// `None` for a bare number (`47k`, `3`); the unit then comes from context.
    pub unit: Option<UnitSym>,
    /// A plain integer: no decimal point, exponent, prefix or unit (`3`, `1_000`).
    pub is_integer: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuantityError {
    pub kind: QuantityErrorKind,
    /// Byte range within the token's text.
    pub range: Range<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QuantityErrorKind {
    /// `47q`, `KHz`. Suggestions are complete suffix spellings (`kHz`).
    UnknownSuffix {
        suffix: String,
        suggestions: Vec<String>,
    },
    /// `1Meg`: SPICE's mega.
    Meg,
    /// `k°C`, `m%`: `unit` is the unit's canonical symbol.
    PrefixNotAllowed { prefix: char, unit: &'static str },
    /// `4k7k`: the infix letter is already the prefix.
    SecondPrefix { prefix: char },
    /// `4.7k7`, `1e3k7`: an infix prefix needs a plain integer before it.
    DecimalAndInfix,
    /// `1e`: an exponent marker with no digits.
    MissingExponentDigits,
    /// `1e400`: beyond `f64`.
    TooLarge,
}

/// Where the parts of a quantity's number end. Byte offsets into the token text,
/// which must start with an ASCII digit: `4.7e-3V` is int `4`, fraction `7`, exponent
/// `-3`, then the suffix `V`.
pub(crate) struct Mantissa {
    /// End of the integer digits.
    pub int_end: usize,
    /// The fraction digits, after the `.` (empty without a decimal point).
    pub frac: Range<usize>,
    /// The exponent's sign and digits, after the `e`/`E`, if there is one.
    pub exponent: Option<Range<usize>>,
    /// End of the whole number; the suffix starts here.
    pub len: usize,
}

impl Mantissa {
    pub fn point(&self) -> bool {
        self.frac.end > self.int_end
    }
}

fn eat_digits(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
        i += 1;
    }
    i
}

/// Digits and `_`; then `.` only if a digit follows (so `100..=300` and `0..N` work);
/// then an exponent only if a digit follows the `e` or its sign.
pub(crate) fn scan_mantissa(b: &[u8]) -> Mantissa {
    debug_assert!(b.first().is_some_and(u8::is_ascii_digit));
    let int_end = eat_digits(b, 0);
    let mut i = int_end;
    let mut frac = int_end..int_end;
    if b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(u8::is_ascii_digit) {
        i = eat_digits(b, i + 1);
        frac = int_end + 1..i;
    }
    let mut exponent = None;
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if b.get(j).is_some_and(u8::is_ascii_digit) {
            let end = eat_digits(b, j);
            exponent = Some(i + 1..end);
            i = end;
        }
    }
    Mantissa {
        int_end,
        frac,
        exponent,
        len: i,
    }
}

/// Characters a quantity's suffix may contain. Deliberately broad (lexer.md L5): `47q`
/// and `4.7k7` stay one token each and get one precise error each. `º` and `˚` are
/// look-alikes of `°`, kept so `10ºC` is one token with a "did you mean `°C`" fix.
pub(crate) fn is_suffix_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '_' | '%' | '\u{B5}' | '\u{3BC}' | '\u{3A9}' | '\u{2126}' | '°' | 'º' | '˚'
        )
}

fn unit_exact(s: &str) -> Option<UnitSym> {
    UNITS
        .iter()
        .find(|(spelling, _)| *spelling == s)
        .map(|&(_, u)| u)
}

fn prefix_exact(s: &str) -> Option<i32> {
    let mut chars = s.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    prefix_of(c)
}

fn prefix_of(c: char) -> Option<i32> {
    PREFIXES.iter().find(|&&(p, _)| p == c).map(|&(_, e)| e)
}

/// Splits a suffix into (power of ten, unit), in the order of grammar.md §5.2:
/// the whole suffix as a unit, then as a prefix, then prefix + unit.
fn split_suffix(s: &str) -> Result<(i32, Option<UnitSym>), QuantityErrorKind> {
    if let Some(unit) = unit_exact(s) {
        return Ok((0, Some(unit)));
    }
    if let Some(exp) = prefix_exact(s) {
        return Ok((exp, None));
    }
    let mut chars = s.chars();
    if let Some(first) = chars.next()
        && let Some(exp) = prefix_of(first)
        && let Some(unit) = unit_exact(chars.as_str())
    {
        if unit.accepts_prefix() {
            return Ok((exp, Some(unit)));
        }
        return Err(QuantityErrorKind::PrefixNotAllowed {
            prefix: first,
            unit: unit.symbol(),
        });
    }
    if s.starts_with("Meg") {
        return Err(QuantityErrorKind::Meg);
    }
    if s == "e" || s == "E" {
        return Err(QuantityErrorKind::MissingExponentDigits);
    }
    Err(QuantityErrorKind::UnknownSuffix {
        suffix: s.to_string(),
        suggestions: suggestions(s),
    })
}

/// Decodes a `Quantity` token's text. Used by `check` (to report errors everywhere) and
/// by the parser (to get the value). Allocates only on errors and for very long numbers.
pub fn decode_quantity(text: &str) -> Result<QuantityLit, QuantityError> {
    let m = scan_mantissa(text.as_bytes());
    let suffix = &text[m.len..];
    let at = |kind, range: Range<usize>| QuantityError {
        kind,
        range: range.start as u32..range.end as u32,
    };

    let parts = if let Some(parts) = infix(text, &m)? {
        parts
    } else if suffix.is_empty() {
        Parts {
            frac: m.frac.clone(),
            prefix_exp: 0,
            unit: None,
        }
    } else {
        let (exp, unit) = split_suffix(suffix).map_err(|kind| {
            let range = match kind {
                // Point at the prefix: that's what has to go.
                QuantityErrorKind::PrefixNotAllowed { prefix, .. } => {
                    m.len..m.len + prefix.len_utf8()
                }
                QuantityErrorKind::Meg => m.len..m.len + 3,
                _ => m.len..text.len(),
            };
            at(kind, range)
        })?;
        Parts {
            frac: m.frac.clone(),
            prefix_exp: exp,
            unit,
        }
    };

    let written_exp = m.exponent.clone().map_or(0, |e| parse_exponent(&text[e]));
    let value = decimal_value(
        &text[..m.int_end],
        &text[parts.frac],
        written_exp.saturating_add(i64::from(parts.prefix_exp)),
    );
    if value.is_infinite() {
        return Err(at(QuantityErrorKind::TooLarge, 0..text.len()));
    }
    Ok(QuantityLit {
        value,
        unit: parts.unit,
        is_integer: !m.point() && m.exponent.is_none() && suffix.is_empty(),
    })
}

/// What the suffix contributes to the value.
struct Parts {
    /// The fraction digits: the mantissa's, or the ones after an infix prefix (`4k7`).
    frac: Range<usize>,
    prefix_exp: i32,
    unit: Option<UnitSym>,
}

/// The infix form, `4k7` = 4.7k: a prefix letter directly followed by digits, which
/// become the fraction. `None` if the suffix isn't in this form.
fn infix(text: &str, m: &Mantissa) -> Result<Option<Parts>, QuantityError> {
    let suffix = &text[m.len..];
    let Some(prefix) = suffix.chars().next() else {
        return Ok(None);
    };
    let Some(exp) = prefix_of(prefix) else {
        return Ok(None);
    };
    let after_prefix = m.len + prefix.len_utf8();
    if !text
        .as_bytes()
        .get(after_prefix)
        .is_some_and(u8::is_ascii_digit)
    {
        return Ok(None);
    }
    let at = |kind, range: Range<usize>| QuantityError {
        kind,
        range: range.start as u32..range.end as u32,
    };
    if m.point() || m.exponent.is_some() {
        return Err(at(QuantityErrorKind::DecimalAndInfix, 0..text.len()));
    }
    let digits_end = eat_digits(text.as_bytes(), after_prefix);
    let rest = &text[digits_end..];
    let unit = match unit_exact(rest) {
        _ if rest.is_empty() => None,
        Some(unit) if unit.accepts_prefix() => Some(unit),
        Some(unit) => {
            return Err(at(
                QuantityErrorKind::PrefixNotAllowed {
                    prefix,
                    unit: unit.symbol(),
                },
                m.len..after_prefix,
            ));
        }
        // `4k7k`, `4k7kΩ`: the infix letter already is the prefix.
        None if rest.chars().next().and_then(prefix_of).is_some() && split_suffix(rest).is_ok() => {
            let second = rest.chars().next().expect("rest is non-empty");
            return Err(at(
                QuantityErrorKind::SecondPrefix { prefix: second },
                digits_end..digits_end + second.len_utf8(),
            ));
        }
        None => {
            return Err(at(
                QuantityErrorKind::UnknownSuffix {
                    suffix: rest.to_string(),
                    suggestions: suggestions(rest),
                },
                digits_end..text.len(),
            ));
        }
    };
    Ok(Some(Parts {
        frac: after_prefix..digits_end,
        prefix_exp: exp,
        unit,
    }))
}

/// A written exponent (`-3`, `+12`, `1_0`), saturating: `1e99999999999999999999` must
/// still come out as too large, not wrap.
fn parse_exponent(s: &str) -> i64 {
    let (negative, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let magnitude = digits.bytes().filter(|&b| b != b'_').fold(0i64, |acc, b| {
        acc.saturating_mul(10).saturating_add(i64::from(b - b'0'))
    });
    if negative { -magnitude } else { magnitude }
}

/// Powers of ten that are exact in `f64`: up to 10^22 (5^22 < 2^53).
const POW10: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

/// `int.frac × 10^exp`, correctly rounded (decision L6). Digits may contain `_`.
///
/// Clinger's fast path, as in Rust's `dec2flt` (`can_use_fast_path`) and Zig's
/// `parse_float` (`isFastPath`): when the digits fit in 53 bits and the power of ten is
/// at most 22, both operands are exact, and one IEEE multiply or divide rounds
/// correctly. `100nF` is 100 / 1e9 = 1e-7 exactly. Multiplying by an inexact `1e-9`
/// instead gives 1.0000000000000001e-07. Anything else goes to std's parser, which
/// runs the full algorithm.
fn decimal_value(int: &str, frac: &str, exp: i64) -> f64 {
    let mut w: u64 = 0;
    let mut n_digits = 0u32;
    let mut frac_digits = 0i64;
    for (part, b) in int
        .bytes()
        .map(|b| (0, b))
        .chain(frac.bytes().map(|b| (1, b)))
    {
        if b == b'_' {
            continue;
        }
        n_digits += 1;
        frac_digits += part;
        if n_digits <= 19 {
            w = w * 10 + u64::from(b - b'0');
        }
    }
    let e = exp.saturating_sub(frac_digits);
    if n_digits <= 19 && w <= 1 << 53 && (-22..=22).contains(&e) {
        let w = w as f64;
        return if e >= 0 {
            w * POW10[e as usize]
        } else {
            w / POW10[(-e) as usize]
        };
    }
    let digits: String = int
        .chars()
        .chain(frac.chars())
        .filter(|&c| c != '_')
        .collect();
    format!("{digits}e{e}")
        .parse()
        .expect("digits and an exponent are valid float text")
}

/// Up to three valid suffixes close to `s`, best first:
/// 1. `s` with look-alike characters replaced (`ºC` → `°C`);
/// 2. case-insensitive matches (`KHz` → `kHz`, `mhz` → `mHz`, `MHz`);
/// 3. one edit away, for suffixes of two or more characters (`Hx` → `Hz`).
fn suggestions(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let push = |c: String, out: &mut Vec<String>| {
        if c != s && !out.contains(&c) && out.len() < 3 {
            out.push(c);
        }
    };

    let normalized: String = s
        .chars()
        .map(|c| if matches!(c, 'º' | '˚') { '°' } else { c })
        .collect();
    if normalized != s && split_suffix(&normalized).is_ok() {
        push(normalized, &mut out);
    }

    let candidates = candidates();
    let lower = s.to_lowercase();
    for c in &candidates {
        if c.to_lowercase() == lower {
            push(c.clone(), &mut out);
        }
    }
    if out.is_empty() && s.chars().count() >= 2 {
        // Same-length substitutions (`Hx` → `Hz`) before insertions and deletions.
        let len = s.chars().count();
        let mut close: Vec<&String> = candidates
            .iter()
            .filter(|c| edit_distance(c, s) == 1)
            .collect();
        close.sort_by_key(|c| c.chars().count().abs_diff(len));
        for c in close {
            push(c.clone(), &mut out);
        }
    }
    out
}

/// Every valid suffix, spelled canonically (`u` for micro, U+03A9 for ohm).
fn candidates() -> Vec<String> {
    let units = || {
        UNITS
            .iter()
            .filter(|(spelling, _)| *spelling != "\u{2126}")
            .map(|&(s, u)| (s, u))
    };
    let prefixes = || PREFIXES.iter().filter(|&&(p, _)| p == 'u' || p.is_ascii());
    let mut out: Vec<String> = units().map(|(s, _)| s.to_string()).collect();
    out.extend(prefixes().map(|&(p, _)| p.to_string()));
    for &(p, _) in prefixes() {
        for (s, u) in units() {
            if u.accepts_prefix() {
                out.push(format!("{p}{s}"));
            }
        }
    }
    out
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> (f64, Option<UnitSym>) {
        let q = decode_quantity(text).unwrap_or_else(|e| panic!("{text}: {e:?}"));
        (q.value, q.unit)
    }

    fn fails(text: &str) -> QuantityErrorKind {
        match decode_quantity(text) {
            Ok(q) => panic!("{text} decoded to {q:?}"),
            Err(e) => e.kind,
        }
    }

    /// Every row of grammar.md §5.2.
    #[test]
    fn grammar_table() {
        use UnitSym::*;
        assert_eq!(ok("47k"), (47_000.0, None));
        assert_eq!(ok("4k7"), (4_700.0, None));
        assert_eq!(ok("1kHz"), (1_000.0, Some(Hertz)));
        assert_eq!(ok("100nF"), (1e-7, Some(Farad)));
        assert_eq!(ok("1F"), (1.0, Some(Farad)));
        assert_eq!(ok("1f"), (1e-15, None));
        assert_eq!(ok("1m"), (1e-3, None));
        assert_eq!(ok("1M"), (1e6, None));
        assert_eq!(ok("1K"), (1.0, Some(Kelvin)));
        assert_eq!(ok("5%"), (5.0, Some(Percent)));
        assert_eq!(fails("1Meg"), QuantityErrorKind::Meg);
        assert_eq!(
            fails("1k°C"),
            QuantityErrorKind::PrefixNotAllowed {
                prefix: 'k',
                unit: "°C"
            }
        );
        assert!(matches!(
            fails("1m%"),
            QuantityErrorKind::PrefixNotAllowed { .. }
        ));
        assert!(matches!(
            fails("1kdB"),
            QuantityErrorKind::PrefixNotAllowed { .. }
        ));
    }

    #[test]
    fn spellings() {
        use UnitSym::*;
        assert_eq!(ok("1uF"), (1e-6, Some(Farad)));
        assert_eq!(ok("1\u{B5}F"), (1e-6, Some(Farad)));
        assert_eq!(ok("1\u{3BC}F"), (1e-6, Some(Farad)));
        assert_eq!(ok("10k\u{3A9}"), (10_000.0, Some(Ohm)));
        assert_eq!(ok("10k\u{2126}"), (10_000.0, Some(Ohm)));
        assert_eq!(ok("10kohm"), (10_000.0, Some(Ohm)));
        assert_eq!(ok("1_000"), (1000.0, None));
        assert_eq!(ok("1000"), (1000.0, None));
        assert_eq!(ok("4.7k"), (4_700.0, None));
        assert_eq!(ok("4k7\u{3A9}"), (4_700.0, Some(Ohm)));
        assert_eq!(ok("1.5e-3V"), (1.5e-3, Some(Volt)));
        assert_eq!(ok("1e3"), (1e3, None));
        assert_eq!(ok("1e3k"), (1e6, None));
        assert_eq!(ok("10°C"), (10.0, Some(Celsius)));
        assert_eq!(ok("3dB"), (3.0, Some(Decibel)));
        assert_eq!(ok("2ms"), (2e-3, Some(Second)));
        assert_eq!(ok("2.2mH"), (2.2e-3, Some(Henry)));
    }

    #[test]
    fn integers() {
        assert!(decode_quantity("3").unwrap().is_integer);
        assert!(decode_quantity("1_000").unwrap().is_integer);
        for text in ["3.0", "3e0", "3k", "3V", "4k7"] {
            assert!(!decode_quantity(text).unwrap().is_integer, "{text}");
        }
    }

    #[test]
    fn rejections() {
        assert_eq!(fails("4.7k7"), QuantityErrorKind::DecimalAndInfix);
        assert_eq!(fails("1e3k7"), QuantityErrorKind::DecimalAndInfix);
        assert_eq!(fails("1e"), QuantityErrorKind::MissingExponentDigits);
        assert_eq!(fails("1e400"), QuantityErrorKind::TooLarge);
        assert_eq!(
            fails("1e99999999999999999999999"),
            QuantityErrorKind::TooLarge
        );
        assert!(matches!(
            fails("47q"),
            QuantityErrorKind::UnknownSuffix { .. }
        ));
        assert_eq!(
            fails("4k7k"),
            QuantityErrorKind::SecondPrefix { prefix: 'k' }
        );
        assert_eq!(
            fails("4k7k\u{3A9}"),
            QuantityErrorKind::SecondPrefix { prefix: 'k' }
        );
        assert!(matches!(
            fails("4k7q"),
            QuantityErrorKind::UnknownSuffix { .. }
        ));
        assert!(matches!(
            fails("2k5°C"),
            QuantityErrorKind::PrefixNotAllowed { .. }
        ));
    }

    #[test]
    fn suggestion_order() {
        let suggest = |text: &str| match fails(text) {
            QuantityErrorKind::UnknownSuffix { suggestions, .. } => suggestions,
            other => panic!("{text}: {other:?}"),
        };
        assert_eq!(suggest("1KHz"), vec!["kHz"]);
        assert_eq!(suggest("1mhz"), vec!["mHz", "MHz"]);
        assert_eq!(suggest("10ºC"), vec!["°C"]);
        assert_eq!(suggest("1Hx"), vec!["Hz", "H"]);
        assert_eq!(suggest("1q"), Vec::<String>::new());
    }

    #[test]
    fn error_ranges_point_at_the_problem() {
        let range = |text: &str| decode_quantity(text).unwrap_err().range;
        assert_eq!(range("1Meg"), 1..4);
        assert_eq!(range("10k°C"), 2..3);
        assert_eq!(range("47q"), 2..3);
        assert_eq!(range("4k7q"), 3..4);
    }

    /// The fast path in `decimal_value` gives exactly what std's full parser gives, on
    /// both sides of every limit: 2^53, 19 vs 20 digits, exponents ±22 vs ±23.
    #[test]
    fn fast_path_matches_std() {
        let check = |int: &str, frac: &str, exp: i64| {
            let digits: String = format!("{int}{frac}").replace('_', "");
            let e = exp - frac.replace('_', "").len() as i64;
            let expected: f64 = format!("{digits}e{e}").parse().unwrap();
            let got = decimal_value(int, frac, exp);
            assert!(
                got == expected || (got.is_nan() && expected.is_nan()),
                "{int}.{frac}e{exp}: {got:e} vs {expected:e}"
            );
        };
        for int in [
            "0",
            "1",
            "47",
            "9007199254740992",
            "9007199254740993",
            "1_000",
        ] {
            for frac in ["", "7", "000_1", "1234567890123456789"] {
                for exp in [
                    -400, -308, -40, -23, -22, -9, -1, 0, 1, 9, 22, 23, 40, 308, 400,
                ] {
                    check(int, frac, exp);
                }
            }
        }
        // Many random cases, from a fixed seed.
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for _ in 0..200_000 {
            let int = (next() % 10u64.pow((next() % 17) as u32 + 1)).to_string();
            let frac_len = (next() % 6) as usize;
            let frac: String = (0..frac_len)
                .map(|_| char::from(b'0' + (next() % 10) as u8))
                .collect();
            let exp = (next() % 61) as i64 - 30;
            check(&int, &frac, exp);
        }
    }

    /// Decision L6: every E24 value × every prefix equals the correctly rounded parse of
    /// its decimal text. Multiplying instead would be off in the last bit for many of
    /// these (100.0 * 1e-9 != 1e-7).
    #[test]
    fn values_are_correctly_rounded() {
        const E24: [u32; 24] = [
            10, 11, 12, 13, 15, 16, 18, 20, 22, 24, 27, 30, 33, 36, 39, 43, 47, 51, 56, 62, 68, 75,
            82, 91,
        ];
        let mut multiplying_would_be_off = 0;
        for d in E24 {
            // One decade each way: 4.7, 47, 470.
            for mantissa in [
                format!("{}.{}", d / 10, d % 10),
                format!("{d}"),
                format!("{d}0"),
            ] {
                for &(prefix, exp) in PREFIXES {
                    let text = format!("{mantissa}{prefix}");
                    let expected: f64 = format!("{mantissa}e{exp}").parse().unwrap();
                    assert_eq!(decode_quantity(&text).unwrap().value, expected, "{text}");
                    let m: f64 = mantissa.parse().unwrap();
                    if m * 10f64.powi(exp) != expected {
                        multiplying_would_be_off += 1;
                    }
                }
            }
        }
        assert!(multiplying_would_be_off > 0);
    }
}
