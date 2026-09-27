//! Reading a `Quantity` token: `47k`, `4k7`, `1uF`, `10°C`, `5%` (grammar.md §2.5, §5).
//!
//! Everything the lexer knows about numbers and units is in this file, each rule once:
//! - **Where the token ends:** [`quantity_len`], the number ([`scan_mantissa`]) and then
//!   every suffix character glued to it ([`suffix_len`]). The scanner cuts a `Quantity`
//!   with it and `check` finds a spaced-out unit (`10 kΩ`) with it, so the two can't
//!   disagree about where a number ends.
//! - **What it means:** [`decode_quantity`], the same number split, then the suffix split
//!   into a prefix and a unit by the tables below. `check` calls it to report errors, the
//!   parser to get the value. Every error range is an offset into the token's text, as
//!   in Zig's `parseNumberLiteral`.
//! - **Help when it doesn't decode:** [`suffix_suggestions`] and [`SUFFIX_NOTE`].
//!
//! A token's shape next to its meaning, as Spade declares a literal's regex and the
//! callback that decodes it together (`spade-ast/src/token.rs`).

use std::ops::Range;

use spicy_model::span::Span;
use spicy_model::units::Unit;

use crate::edit_distance::edit_distance;

/// Every accepted unit spelling. `Ω` appears twice: U+03A9 (Greek capital omega) and
/// U+2126 (ohm sign) look identical, so both are accepted.
const UNITS: &[(&str, Unit)] = &[
    ("V", Unit::Volt),
    ("A", Unit::Amp),
    ("\u{3A9}", Unit::Ohm),
    ("\u{2126}", Unit::Ohm),
    ("ohm", Unit::Ohm),
    ("F", Unit::Farad),
    ("H", Unit::Henry),
    ("Hz", Unit::Hertz),
    ("s", Unit::Second),
    ("W", Unit::Watt),
    ("K", Unit::Kelvin),
    ("°C", Unit::Celsius),
    ("%", Unit::Percent),
    ("dB", Unit::Decibel),
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

/// Characters that look like the `°` of `°C`: `º` (U+00BA, masculine ordinal) and `˚`
/// (U+02DA, ring above). A suffix may contain them, so `10ºC` is one token, and
/// [`suffix_suggestions`] offers `°C` for it. (Alone, `˚` is in the look-alike table.)
const DEGREE_LOOKALIKES: [char; 2] = ['\u{BA}', '\u{2DA}'];

/// A decoded quantity literal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuantityLit {
    /// The value with the prefix applied: `1kHz` → 1000.0, `47k` → 47000.0.
    pub value: f64,
    /// `None` for a bare number (`47k`, `3`); the unit then comes from context.
    pub unit: Option<Unit>,
}

/// Why a quantity literal doesn't decode, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct QuantityError {
    pub kind: QuantityErrorKind,
    /// Where in the token's text (relative to the token's start).
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QuantityErrorKind {
    /// `47q`, `KHz`. `after_infix` when it follows an infix prefix (`4k7q`), where only
    /// a bare unit may come. Suggestions are computed only when asked for
    /// ([`suffix_suggestions`]), so decoding stays cheap.
    UnknownSuffix { suffix: String, after_infix: bool },
    /// `1Meg`: SPICE's mega.
    Meg,
    /// `k°C`, `m%`. `infix` for `4k7%`, where the letter stands for the decimal point.
    PrefixNotAllowed {
        prefix: char,
        unit: Unit,
        infix: bool,
    },
    /// `4k7k`: the infix letter is already the prefix.
    SecondPrefix { prefix: char },
    /// `4.7k7`, `1e3k7`: an infix prefix needs a plain integer before it.
    DecimalAndInfix,
    /// `1e`: an exponent marker with no digits.
    MissingExponentDigits,
    /// `1e400`: beyond `f64`.
    TooLarge,
    /// `1e-400`: a non-zero number that rounds to zero.
    TooSmall,
}

/// Where a `Quantity` token that starts `s` ends: the number, then every suffix
/// character glued to it (lexer.md L5). `s` starts with an ASCII digit.
#[inline]
pub(super) fn quantity_len(s: &str) -> usize {
    let number = scan_mantissa(s.as_bytes()).len;
    number + suffix_len(&s[number..])
}

/// How much of `s` is suffix characters: what glues onto a number. ASCII is read byte
/// by byte; only non-ASCII is decoded (quantities are common, and `chars()` on every
/// suffix made quantity-heavy files ~15% slower to parse).
#[inline]
pub(super) fn suffix_len(s: &str) -> usize {
    let mut len = 0;
    while let Some(&b) = s.as_bytes().get(len) {
        let c = if b.is_ascii() {
            b as char
        } else {
            s[len..].chars().next().expect("len is on a char boundary")
        };
        if !is_suffix_char(c) {
            break;
        }
        len += c.len_utf8();
    }
    len
}

/// Characters a quantity's suffix may contain. Deliberately broad (lexer.md L5): `47q`
/// and `4.7k7` stay one token each and get one precise error each. Besides ASCII, every
/// character of a spelling in [`UNITS`] and [`PREFIXES`], and the [`DEGREE_LOOKALIKES`]
/// (a test checks the tables).
fn is_suffix_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '_' | '%' | '\u{B5}' | '\u{3BC}' | '\u{3A9}' | '\u{2126}' | '°'
        )
        || DEGREE_LOOKALIKES.contains(&c)
}

/// Where the parts of a quantity's number end. Byte offsets into the token text,
/// which must start with an ASCII digit: `4.7e-3V` is int `4`, fraction `7`, exponent
/// `-3`, then the suffix `V`.
struct Mantissa {
    /// End of the integer digits.
    int_end: usize,
    /// The fraction digits, after the `.` (empty without a decimal point).
    frac: Range<usize>,
    /// The exponent's sign and digits, after the `e`/`E`, if there is one.
    exponent: Option<Range<usize>>,
    /// End of the whole number; the suffix starts here.
    len: usize,
}

impl Mantissa {
    fn has_point(&self) -> bool {
        self.frac.end > self.int_end
    }
}

/// Digits and `_`; then `.` only if a digit follows (so `100..=300` and `0..N` work);
/// then an exponent only if a digit follows the `e` or its sign.
fn scan_mantissa(b: &[u8]) -> Mantissa {
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

fn eat_digits(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
        i += 1;
    }
    i
}

/// Decodes a `Quantity` token's text. Used by `check` (to report errors everywhere) and
/// by the parser (to get the value). Allocates only on errors and for very long numbers.
pub fn decode_quantity(text: &str) -> Result<QuantityLit, QuantityError> {
    let m = scan_mantissa(text.as_bytes());
    let (frac, suffix) = match infix_prefix(&text[m.len..]) {
        Some(prefix) => decode_infix(text, &m, prefix)?,
        None => {
            let suffix = split_suffix(text, m.len)?;
            (m.frac.clone(), suffix)
        }
    };

    let int = &text[..m.int_end];
    let frac = &text[frac];
    let written_exp = m.exponent.clone().map_or(0, |e| parse_exponent(&text[e]));
    let prefix_exp = suffix.prefix.map_or(0, |(_, exp)| exp);
    let value = decimal_value(int, frac, written_exp.saturating_add(i64::from(prefix_exp)));
    if value.is_infinite() {
        return Err(error_at(QuantityErrorKind::TooLarge, 0..text.len()));
    }
    let nonzero = |s: &str| s.bytes().any(|b| matches!(b, b'1'..=b'9'));
    if value == 0.0 && (nonzero(int) || nonzero(frac)) {
        return Err(error_at(QuantityErrorKind::TooSmall, 0..text.len()));
    }
    Ok(QuantityLit {
        value,
        unit: suffix.unit,
    })
}

/// A suffix split into its parts (grammar.md §5.2): `kHz` is the prefix `k` and the
/// unit `Hz`.
struct Suffix {
    /// The prefix letter and its power of ten.
    prefix: Option<(char, i32)>,
    unit: Option<Unit>,
}

/// The prefix of the infix form `4k7`: a prefix letter directly followed by a digit.
fn infix_prefix(suffix: &str) -> Option<(char, i32)> {
    let letter = suffix.chars().next()?;
    let exp = prefix_of(letter)?;
    let digit_follows = suffix
        .as_bytes()
        .get(letter.len_utf8())
        .is_some_and(u8::is_ascii_digit);
    digit_follows.then_some((letter, exp))
}

/// The infix form, `4k7` = 4.7k: the prefix letter stands for the decimal point, and the
/// digits after it are the fraction. Returns where those digits are, and the suffix.
/// Only a unit with no prefix of its own may follow (`4k7Ω`).
fn decode_infix(
    text: &str,
    m: &Mantissa,
    (letter, exp): (char, i32),
) -> Result<(Range<usize>, Suffix), QuantityError> {
    if m.has_point() || m.exponent.is_some() {
        return Err(error_at(QuantityErrorKind::DecimalAndInfix, 0..text.len()));
    }
    let frac_start = m.len + letter.len_utf8();
    let frac_end = eat_digits(text.as_bytes(), frac_start);
    let unit = match split_suffix(text, frac_end) {
        Ok(Suffix { prefix: None, unit }) => unit,
        // `4k7k`, `4k7kΩ`: the infix letter already is the prefix.
        Ok(Suffix {
            prefix: Some((second, _)),
            ..
        }) => {
            return Err(error_at(
                QuantityErrorKind::SecondPrefix { prefix: second },
                frac_end..frac_end + second.len_utf8(),
            ));
        }
        Err(_) => {
            return Err(error_at(
                QuantityErrorKind::UnknownSuffix {
                    suffix: text[frac_end..].to_string(),
                    after_infix: true,
                },
                frac_end..text.len(),
            ));
        }
    };
    if let Some(unit) = unit
        && !unit.accepts_prefix()
    {
        // `4k7%`: point at the letter, which should have been a `.`.
        return Err(error_at(
            QuantityErrorKind::PrefixNotAllowed {
                prefix: letter,
                unit,
                infix: true,
            },
            m.len..frac_start,
        ));
    }
    let prefix = Some((letter, exp));
    Ok((frac_start..frac_end, Suffix { prefix, unit }))
}

/// Splits the suffix `text[start..]` by the rules of grammar.md §5.2, in order: the
/// whole suffix as a unit, then as a prefix, then prefix + unit. An empty suffix has
/// neither.
fn split_suffix(text: &str, start: usize) -> Result<Suffix, QuantityError> {
    let s = &text[start..];
    if s.is_empty() {
        return Ok(Suffix {
            prefix: None,
            unit: None,
        });
    }
    if let Some(unit) = unit_exact(s) {
        return Ok(Suffix {
            prefix: None,
            unit: Some(unit),
        });
    }
    let mut chars = s.chars();
    if let Some(letter) = chars.next()
        && let Some(exp) = prefix_of(letter)
    {
        let prefix = Some((letter, exp));
        let rest = chars.as_str();
        if rest.is_empty() {
            return Ok(Suffix { prefix, unit: None });
        }
        if let Some(unit) = unit_exact(rest) {
            if unit.accepts_prefix() {
                return Ok(Suffix {
                    prefix,
                    unit: Some(unit),
                });
            }
            // Point at the prefix: that's what has to go.
            return Err(error_at(
                QuantityErrorKind::PrefixNotAllowed {
                    prefix: letter,
                    unit,
                    infix: false,
                },
                start..start + letter.len_utf8(),
            ));
        }
    }
    if s.starts_with("Meg") {
        return Err(error_at(QuantityErrorKind::Meg, start..start + "Meg".len()));
    }
    let kind = if s == "e" || s == "E" {
        QuantityErrorKind::MissingExponentDigits
    } else {
        QuantityErrorKind::UnknownSuffix {
            suffix: s.to_string(),
            after_infix: false,
        }
    };
    Err(error_at(kind, start..text.len()))
}

fn unit_exact(s: &str) -> Option<Unit> {
    UNITS
        .iter()
        .find(|(spelling, _)| *spelling == s)
        .map(|&(_, u)| u)
}

fn prefix_of(c: char) -> Option<i32> {
    PREFIXES.iter().find(|&&(p, _)| p == c).map(|&(_, e)| e)
}

/// A decoding error at `range`, byte offsets into the token's text.
fn error_at(kind: QuantityErrorKind, range: Range<usize>) -> QuantityError {
    QuantityError {
        kind,
        span: Span::new(range.start as u32, range.end as u32),
    }
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
    let digit_bytes = || int.bytes().chain(frac.bytes()).filter(|&b| b != b'_');
    // The digits as one integer `w`, while they fit in a `u64` (any 19 digits do).
    let mut w: u64 = 0;
    let mut n_digits = 0u32;
    for b in digit_bytes() {
        n_digits += 1;
        if n_digits <= 19 {
            w = w * 10 + u64::from(b - b'0');
        }
    }
    // `4.7e3` is 47 × 10^2: each fraction digit moves the power down by one.
    let frac_digits = frac.bytes().filter(|&b| b != b'_').count() as i64;
    let e = exp.saturating_sub(frac_digits);
    if n_digits <= 19 && w <= 1 << 53 && (-22..=22).contains(&e) {
        let w = w as f64;
        return if e >= 0 {
            w * POW10[e as usize]
        } else {
            w / POW10[(-e) as usize]
        };
    }
    let digits: String = digit_bytes().map(char::from).collect();
    format!("{digits}e{e}")
        .parse()
        .expect("digits and an exponent are valid float text")
}

/// Up to three valid suffixes close to `s`, best first:
/// 1. `s` with look-alike characters replaced (`ºC` → `°C`);
/// 2. case-insensitive matches (`KHz` → `kHz`, `mhz` → `mHz`, `MHz`);
/// 3. one edit away, for suffixes of two to five characters (`Hx` → `Hz`).
///
/// After an infix prefix (`after_infix`, as in `4k7q`) only a unit that takes a prefix
/// is offered: the infix letter is its prefix, so `4k7dB` would be a new error.
/// Only called for a quantity that failed to decode, so it never slows a clean file.
pub fn suffix_suggestions(s: &str, after_infix: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let push = |c: &str, out: &mut Vec<String>| {
        if c != s && !out.iter().any(|o| o == c) && out.len() < 3 {
            out.push(c.to_string());
        }
    };
    // Whether `c`, written after the number, is a valid suffix.
    let fits = |c: &str| match split_suffix(c, 0) {
        Ok(Suffix {
            prefix: None,
            unit: Some(unit),
        }) if after_infix => unit.accepts_prefix(),
        Ok(_) => !after_infix,
        Err(_) => false,
    };

    let normalized: String = s
        .chars()
        .map(|c| {
            if DEGREE_LOOKALIKES.contains(&c) {
                '°'
            } else {
                c
            }
        })
        .collect();
    if normalized != s && fits(&normalized) {
        push(&normalized, &mut out);
    }

    // Bounds keep a pathological suffix (a kilobyte of letters) cheap.
    if s.len() <= 16 {
        let lower = s.to_lowercase();
        for c in candidates()
            .iter()
            .filter(|c| c.to_lowercase() == lower && fits(c))
        {
            push(c, &mut out);
        }
    }
    let len = s.chars().count();
    if out.is_empty() && (2..=5).contains(&len) {
        // Same-length substitutions (`Hx` → `Hz`) before insertions and deletions.
        let mut close: Vec<&String> = candidates()
            .iter()
            .filter(|c| edit_distance(c, s) == 1 && fits(c))
            .collect();
        close.sort_by_key(|c| c.chars().count().abs_diff(len));
        for c in close {
            push(c, &mut out);
        }
    }
    out
}

/// The note under an unknown suffix: every unit and prefix, one spelling each. A test
/// checks it against [`UNITS`] and [`PREFIXES`].
pub(super) const SUFFIX_NOTE: &str = "note: units are V A Ω F H Hz s W K °C % dB, optionally after a \
                                      prefix f p n u µ m k M G T (case matters)";

/// Every valid suffix, spelled canonically (`u` for micro, U+03A9 for ohm). Built once.
fn candidates() -> &'static [String] {
    static CANDIDATES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    CANDIDATES.get_or_init(|| {
        let units = || UNITS.iter().filter(|(spelling, _)| *spelling != "\u{2126}");
        let prefixes = || PREFIXES.iter().filter(|&&(p, _)| p.is_ascii());
        let mut out: Vec<String> = units().map(|&(s, _)| s.to_string()).collect();
        out.extend(prefixes().map(|&(p, _)| p.to_string()));
        for &(p, _) in prefixes() {
            for &(s, u) in units() {
                if u.accepts_prefix() {
                    out.push(format!("{p}{s}"));
                }
            }
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Rng;

    fn ok(text: &str) -> (f64, Option<Unit>) {
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
        use Unit::*;
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
                unit: Unit::Celsius,
                infix: false,
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
        use Unit::*;
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
    fn rejections() {
        assert_eq!(fails("4.7k7"), QuantityErrorKind::DecimalAndInfix);
        assert_eq!(fails("1e3k7"), QuantityErrorKind::DecimalAndInfix);
        assert_eq!(fails("1e"), QuantityErrorKind::MissingExponentDigits);
        assert_eq!(fails("1e400"), QuantityErrorKind::TooLarge);
        assert_eq!(fails("1e-400"), QuantityErrorKind::TooSmall);
        assert_eq!(ok("0e-400").0, 0.0);
        assert_eq!(ok("0.000"), (0.0, None));
        // `4k7%`: the letter stands for the decimal point, so the fix is `4.7%`.
        assert_eq!(
            fails("4k7%"),
            QuantityErrorKind::PrefixNotAllowed {
                prefix: 'k',
                unit: Unit::Percent,
                infix: true,
            }
        );
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
            QuantityErrorKind::UnknownSuffix {
                suffix,
                after_infix,
            } => suffix_suggestions(&suffix, after_infix),
            other => panic!("{text}: {other:?}"),
        };
        assert_eq!(suggest("1KHz"), vec!["kHz"]);
        assert_eq!(suggest("1mhz"), vec!["mHz", "MHz"]);
        assert_eq!(suggest("10ºC"), vec!["°C"]);
        assert_eq!(suggest("10˚C"), vec!["°C"]);
        // After an infix prefix, only units that take one: `4k7°C` would be an error.
        assert_eq!(suggest("4k7ºC"), Vec::<String>::new());
        assert_eq!(suggest("4k7DB"), Vec::<String>::new());
        assert_eq!(suggest("4k7ohms"), vec!["ohm"]);
        assert_eq!(suggest("4k7hz"), vec!["Hz"]);
        // At most three, same-length substitutions first, in table order.
        assert_eq!(suggest("1xF"), vec!["fF", "pF", "nF"]);
        assert_eq!(suggest("1Hx"), vec!["Hz", "H"]);
        assert_eq!(suggest("1q"), Vec::<String>::new());
        // After an infix prefix only a bare unit fits (`4k7in` must not suggest `n`).
        assert!(suggest("4k7in").iter().all(|s| s != "n"));
        assert_eq!(suggest("4k7v"), vec!["V"]);
        // A pathological suffix is cheap and gets nothing.
        assert_eq!(
            suggest(&format!("1{}", "q".repeat(1000))),
            Vec::<String>::new()
        );
    }

    #[test]
    fn error_ranges_point_at_the_problem() {
        let range = |text: &str| decode_quantity(text).unwrap_err().span.range();
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
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for _ in 0..200_000 {
            let int = (rng.next() % 10u64.pow(rng.below(17) as u32 + 1)).to_string();
            let frac: String = (0..rng.below(6))
                .map(|_| char::from(b'0' + rng.below(10) as u8))
                .collect();
            let exp = rng.below(61) as i64 - 30;
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

    /// The scanner glues every spelling in the tables onto a number, so each one reaches
    /// the decoder whole: every character of every unit and prefix is a suffix character.
    #[test]
    fn every_spelling_is_made_of_suffix_chars() {
        let spellings = UNITS.iter().map(|&(s, _)| s.to_string());
        let prefixes = PREFIXES.iter().map(|&(p, _)| p.to_string());
        for s in spellings.chain(prefixes) {
            assert_eq!(suffix_len(&s), s.len(), "{s}");
            assert_eq!(quantity_len(&format!("1{s} ")), 1 + s.len(), "{s}");
        }
        for c in DEGREE_LOOKALIKES {
            assert!(is_suffix_char(c), "{c}");
        }
    }

    /// Where a quantity token ends: the number, then the glued suffix, and nothing that
    /// can't be in one (`..`, an operator, a space, a non-ASCII letter off the list).
    #[test]
    fn quantity_ends() {
        let len = |s: &str| quantity_len(s);
        assert_eq!(len("4.7e-3kΩ+x"), "4.7e-3kΩ".len());
        assert_eq!(len("10ºC;"), "10ºC".len());
        assert_eq!(len("10˚C"), "10˚C".len());
        assert_eq!(len("1..=3"), 1);
        assert_eq!(len("1.x"), 1);
        assert_eq!(len("4k7_x%y"), "4k7_x%y".len());
        assert_eq!(len("1kα"), 2);
        assert_eq!(len("1 k"), 1);
    }

    /// The note names every unit (by its symbol) and every ASCII prefix, plus `µ`.
    #[test]
    fn suffix_note_lists_the_tables() {
        let (units, prefixes) = SUFFIX_NOTE
            .strip_prefix("note: units are ")
            .and_then(|s| s.strip_suffix(" (case matters)"))
            .and_then(|s| s.split_once(", optionally after a prefix "))
            .expect("the note's shape");
        let units: Vec<&str> = units.split(' ').collect();
        let prefixes: Vec<&str> = prefixes.split(' ').collect();
        for &(_, unit) in UNITS {
            assert!(units.contains(&unit.symbol().as_str()), "{unit:?}");
        }
        for &(p, _) in PREFIXES.iter().filter(|(p, _)| p.is_ascii()) {
            assert!(prefixes.contains(&p.to_string().as_str()), "{p}");
        }
        assert!(prefixes.contains(&"\u{B5}"));
        assert_eq!(units.len(), 12);
        assert_eq!(prefixes.len(), PREFIXES.len() - 1);
    }

    #[test]
    fn mantissa_boundaries() {
        let parts = |text: &str| {
            let m = scan_mantissa(text.as_bytes());
            (m.int_end, m.frac, m.exponent, m.len)
        };
        assert_eq!(parts("4.7e-3V"), (1, 2..3, Some(4..6), 6));
        assert_eq!(parts("1_000"), (5, 5..5, None, 5));
        // A `.` needs a digit after it: ranges and `1.` stay out of the number.
        assert_eq!(parts("100..=300"), (3, 3..3, None, 3));
        assert_eq!(parts("1.e3"), (1, 1..1, None, 1));
        // An `e` needs a digit after it (or after its sign): `1e+` ends at `1`.
        assert_eq!(parts("1e+"), (1, 1..1, None, 1));
        assert_eq!(parts("1E+5"), (1, 1..1, Some(2..4), 4));
        assert_eq!(parts("2e1_0k"), (1, 1..1, Some(2..5), 5));
    }

    #[test]
    fn infix_form() {
        use Unit::*;
        assert_eq!(ok("1M5Hz"), (1.5e6, Some(Hertz)));
        assert_eq!(ok("4\u{B5}7F"), (4.7e-6, Some(Farad)));
        assert_eq!(ok("0k47"), (470.0, None));
        assert_eq!(ok("2_2k1_0"), (22_100.0, None));
        assert_eq!(ok("4k7ohm"), (4_700.0, Some(Ohm)));
        // A letter that isn't a prefix, or a prefix not followed by a digit, isn't infix.
        assert!(matches!(
            fails("4q7"),
            QuantityErrorKind::UnknownSuffix {
                after_infix: false,
                ..
            }
        ));
        // After the infix digits only a bare unit fits: anything else is unknown,
        // including what would be a `Meg` or an `e` error on its own.
        for text in ["4k7Meg", "4k7e", "4k7m%", "4k7q"] {
            assert!(
                matches!(
                    fails(text),
                    QuantityErrorKind::UnknownSuffix {
                        after_infix: true,
                        ..
                    }
                ),
                "{text}"
            );
        }
    }

    #[test]
    fn exponents() {
        assert_eq!(ok("1E5"), (1e5, None));
        assert_eq!(ok("1e+3"), (1e3, None));
        assert_eq!(ok("2e1_0"), (2e10, None));
        // The written exponent and the prefix add up before rounding.
        assert_eq!(ok("1e-3k"), (1.0, None));
        assert_eq!(ok("1e300k"), (1e303, None));
        assert_eq!(fails("1e306k"), QuantityErrorKind::TooLarge);
        assert_eq!(fails("1e-310f"), QuantityErrorKind::TooSmall);
    }

    #[test]
    fn more_error_ranges() {
        let range = |text: &str| decode_quantity(text).unwrap_err().span.range();
        // `Meg` is three bytes, even when more follows.
        assert_eq!(range("1Megohm"), 1..4);
        // A multi-byte prefix is covered whole.
        assert_eq!(range("1\u{B5}°C"), 1..3);
        // `4k7%`: the letter that should be a `.`.
        assert_eq!(range("4k7%"), 1..2);
        assert_eq!(range("4\u{B5}7%"), 1..3);
        // `4k7kΩ`: just the second prefix.
        assert_eq!(range("4k7k\u{3A9}"), 3..4);
        assert_eq!(range("22k47\u{B5}F"), 5..7);
        // The whole token when the number itself is the problem.
        assert_eq!(range("4.7k7"), 0..5);
        assert_eq!(range("1e3k7"), 0..5);
        assert_eq!(range("1e400V"), 0..6);
        assert_eq!(range("1e"), 1..2);
        assert_eq!(range("10mhz"), 2..5);
    }
}
