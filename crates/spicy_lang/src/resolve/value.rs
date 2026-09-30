//! Typing values: an expression in a value position becomes a [`Value`], its units
//! checked against what the position expects (model.md E10–E13).
//!
//! No solver: every value position has a declared type, and a bare number takes it
//! (`value: 47k` is 47 kΩ). Inside `*` and `/` a bare number is a plain factor.
//!
//! The flow, top down: [`value`](Resolver::value) takes a spread apart (`±`, `..=`, or
//! one of those scaled) into numbers; [`term`](Resolver::term) computes each number,
//! tracking whether it is still unitless; [`typed`](Resolver::typed) gives a unitless
//! one the expected unit and checks it.

use spicy_errors::Reported;
use spicy_model::prelude::FieldType;
use spicy_model::units::{QKind, Quantity, Spread, Unit, Value};

use super::Resolver;
use super::error::{ResolveError, ResolveErrorKind, describe};
use spicy_span::Span;

use crate::lexer::QuantityLit;
use crate::parser::ast::{BinOp, Expr, ExprKind};

/// A number and whether it is still unitless: written without a unit, or computed
/// only from numbers written without one. That decides who gives it a unit: the
/// position, at the end (`typed`), or the other side of a `*` or `/` (`1V / 1mA` is
/// ohms, `2 * 1V` is volts).
struct Term {
    q: Quantity,
    unitless: bool,
}

impl Resolver<'_, '_> {
    /// A value for a field of type `expected`: a number, a tolerance (`47k ± 1%`), a
    /// range (`100..=300`), or one of those scaled (`2 * (1k ± 1%)`). `field` names the
    /// position, for messages. `Err` when it's wrong, and the error has been reported.
    pub(super) fn value(
        &mut self,
        e: &Expr,
        expected: FieldType,
        field: &str,
    ) -> Result<Value, Reported> {
        // The parser already reported a mistake in here, and what it built is a guess
        // (`1k +- 1%`, `a ± b ± c`): typing the guess would report the mistake twice.
        if let Some(reported) = self.syntax.inside(e.span) {
            return Err(reported);
        }
        self.spread_value(e, expected, field)
    }

    /// [`value`](Self::value) once the syntax is known to be sound. Recursive: a scaled
    /// spread is a spread inside a `*` or `/`.
    fn spread_value(
        &mut self,
        e: &Expr,
        expected: FieldType,
        field: &str,
    ) -> Result<Value, Reported> {
        let e = unparen(e);
        if is_spread(e) && !expected.spread_allowed {
            let kind = ResolveErrorKind::SpreadNotAllowed {
                field: field.to_string(),
            };
            return Err(self.report(kind, e.span));
        }
        if let ExprKind::Range { lo, hi } = &e.kind {
            return self.range(lo.as_deref(), hi.as_deref(), expected, e.span);
        }
        let ExprKind::Binary { op, lhs, rhs } = &e.kind else {
            return self.scalar(e, expected).map(Value::exact);
        };
        match (op, has_spread(lhs), has_spread(rhs)) {
            (BinOp::Tol, ..) => {
                let nominal = self.scalar(lhs, expected)?;
                let spread = self.tolerance(rhs, expected)?;
                Ok(Value { nominal, spread })
            }
            // Scaling a spread by a plain factor: `2 * (1k ± 1%)`, `(10k ± 5%) / 2`. The
            // parser's help for `2 * 1k ± 1%` suggests exactly this, so it must work.
            (BinOp::Mul | BinOp::Div, true, false) => {
                let k = self.scale_factor(rhs, *op == BinOp::Div, e.span)?;
                Ok(self.spread_value(lhs, expected, field)?.scaled(k))
            }
            (BinOp::Mul, false, true) => {
                let k = self.scale_factor(lhs, false, e.span)?;
                Ok(self.spread_value(rhs, expected, field)?.scaled(k))
            }
            // Dividing by a spread, or multiplying two, isn't scaling.
            (BinOp::Mul | BinOp::Div, true, true) | (BinOp::Div, false, true) => {
                Err(self.report(ResolveErrorKind::SpreadInArithmetic, e.span))
            }
            _ => self.scalar(e, expected).map(Value::exact),
        }
    }

    /// `lo..=hi`. Its nominal is the midpoint (model.md E16; agreed 2026-09-27):
    /// `beta: 100..=300` is 200, as in the walkthrough. A range with an end left out
    /// (`..=hi`, `lo..`) isn't a value yet.
    fn range(
        &mut self,
        lo: Option<&Expr>,
        hi: Option<&Expr>,
        expected: FieldType,
        at: Span,
    ) -> Result<Value, Reported> {
        let (Some(lo), Some(hi)) = (lo, hi) else {
            return Err(self.report(ResolveErrorKind::NotAValue, at));
        };
        let lo = self.scalar(lo, expected)?;
        let hi = self.scalar(hi, expected)?;
        if lo.si > hi.si {
            return Err(self.report(ResolveErrorKind::RangeReversed, at));
        }
        let nominal = Quantity {
            si: (lo.si + hi.si) / 2.0,
            ..lo
        };
        let spread = Spread::Range {
            lo: lo.si,
            hi: hi.si,
        };
        Ok(Value { nominal, spread })
    }

    /// What a spread is multiplied by when scaled by `e`: `e`, or `1 / e` if `divide`.
    /// `at` is the whole product, reported when `e` isn't a plain factor.
    fn scale_factor(&mut self, e: &Expr, divide: bool, at: Span) -> Result<f64, Reported> {
        let k = self.term(e)?.q;
        if !k.is_ratio() {
            return Err(self.report(ResolveErrorKind::SpreadInArithmetic, at));
        }
        if !divide {
            return Ok(k.si);
        }
        if k.si == 0.0 {
            return Err(self.report(ResolveErrorKind::DivisionByZero, e.span));
        }
        Ok(1.0 / k.si)
    }

    /// The spread after `±`. A ratio written with `%` (`1%`, `2 * 0.5%`) is relative
    /// (model.md E13); anything else is absolute, in the nominal's unit (`47k ± 500`,
    /// `5V ± 0.1V`). A ratio without `%`, like `± (1V / 1V)`, is absolute too: `±` reads
    /// as relative only where it's written so.
    /// (Temperature spreads, which are differences in K, arrive with setups and `env`s
    /// in M1d-5.)
    fn tolerance(&mut self, e: &Expr, expected: FieldType) -> Result<Spread, Reported> {
        let t = self.term(e)?;
        let relative = t.q.is_ratio() && has_percent(e);
        let size = if relative {
            t.q.si
        } else {
            self.typed(t, expected, e)?.si
        };
        if size < 0.0 {
            return Err(self.report(ResolveErrorKind::NegativeTolerance, e.span));
        }
        Ok(if relative {
            Spread::Rel(size)
        } else {
            Spread::Abs(size)
        })
    }

    /// One number, possibly computed (`2 * 4.7k`), checked against `expected`.
    fn scalar(&mut self, e: &Expr, expected: FieldType) -> Result<Quantity, Reported> {
        let t = self.term(e)?;
        self.typed(t, expected, e)
    }

    /// `t`, the term `e` computes to, as a quantity of type `expected`: given its unit
    /// if it's unitless (model.md E12: `47k`, `2 * 4.7k` and `1k + 2k` are all ohms in a
    /// `value:`), then checked.
    fn typed(&mut self, t: Term, expected: FieldType, e: &Expr) -> Result<Quantity, Reported> {
        let q = if t.unitless {
            Quantity::new(t.q.si, expected.dim)
        } else {
            t.q
        };
        if q.dim == expected.dim && q.kind == expected.kind {
            return Ok(q);
        }
        Err(self.unit_mismatch(q, expected, e))
    }

    /// Reports that `e` is `q`, not `expected`. A value in kelvin gets a note that `K`
    /// isn't kilo, and a lone `47K` the fix `47k`.
    fn unit_mismatch(&mut self, q: Quantity, expected: FieldType, e: &Expr) -> Reported {
        let kind = ResolveErrorKind::UnitMismatch { expected, found: q };
        let kelvin_for_kilo = kind.kelvin_for_kilo();
        let mut error = ResolveError::new(kind, e.span);
        // Fixed only for a lone literal whose whole suffix is `K`, where a unit is
        // expected: `5mK` or `1K + 1K` could mean several things, so they get the note
        // but no fix.
        let written = &self.src()[e.span.range()];
        if kelvin_for_kilo
            && literal(e).is_some()
            && expected.kind == QKind::Plain
            && !expected.dim.is_none()
            && let Some(digits) = written.strip_suffix('K')
            && digits.ends_with(|c: char| c.is_ascii_digit())
        {
            error = error.with_fix(e.span, format!("{digits}k"));
        }
        error.report(&mut self.errors)
    }

    /// The number `e` computes to, and whether it's still unitless (see [`Term`]).
    fn term(&mut self, e: &Expr) -> Result<Term, Reported> {
        if let Some((lit, negative)) = literal(e) {
            return Ok(Term::literal(lit, negative));
        }
        match &e.kind {
            ExprKind::Paren(inner) => self.term(inner),
            ExprKind::Neg(inner) => {
                let t = self.term(inner)?;
                self.linear(&t, e.span)?;
                Ok(Term {
                    q: Quantity { si: -t.q.si, ..t.q },
                    ..t
                })
            }
            _ if is_spread(e) => Err(self.report(ResolveErrorKind::SpreadInArithmetic, e.span)),
            ExprKind::Binary {
                op: op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div),
                lhs,
                rhs,
            } => self.arithmetic(*op, lhs, rhs, e.span),
            // A number the lexer rejected: already reported.
            ExprKind::Error(reported) => Err(*reported),
            _ => Err(self.report(ResolveErrorKind::NotAValue, e.span)),
        }
    }

    /// `lhs op rhs` for one of `+ - * /`; `at` is the whole expression.
    fn arithmetic(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        at: Span,
    ) -> Result<Term, Reported> {
        let a = self.term(lhs)?;
        let b = self.term(rhs)?;
        self.linear(&a, at)?;
        self.linear(&b, at)?;
        let (si, dim) = match op {
            BinOp::Add | BinOp::Sub => {
                // Both unitless, or both the same unit. A unitless number next to one
                // with a unit is an error, as in F#: `r + 5` where `5k` was meant.
                if a.unitless != b.unitless || a.q.dim != b.q.dim {
                    let kind = ResolveErrorKind::MixedUnits {
                        left: a.describe(),
                        right: b.describe(),
                    };
                    return Err(self.report(kind, at));
                }
                let si = if op == BinOp::Add {
                    a.q.si + b.q.si
                } else {
                    a.q.si - b.q.si
                };
                (si, a.q.dim)
            }
            BinOp::Mul => (a.q.si * b.q.si, a.q.dim * b.q.dim),
            _ => {
                if b.q.si == 0.0 {
                    return Err(self.report(ResolveErrorKind::DivisionByZero, rhs.span));
                }
                (a.q.si / b.q.si, a.q.dim / b.q.dim)
            }
        };
        Ok(Term {
            q: Quantity::new(si, dim),
            unitless: a.unitless && b.unitless,
        })
    }

    /// `Ok` if `t` may be used in arithmetic. Temperatures and dB levels aren't linear
    /// quantities: `2 * 10°C` and `3dB + 3dB` have no single meaning, so they're only
    /// written as they are. `at` is the arithmetic, for the error.
    fn linear(&mut self, t: &Term, at: Span) -> Result<(), Reported> {
        let what = match t.q.kind {
            QKind::Plain => return Ok(()),
            QKind::TempPoint => "temperatures",
            QKind::Db => "dB levels",
        };
        Err(self.report(ResolveErrorKind::NotArithmetic { what }, at))
    }
}

impl Term {
    /// A literal's value with its written unit. A literal without one is dimensionless
    /// and unitless: `typed` gives it the expected unit.
    fn literal(lit: QuantityLit, negative: bool) -> Self {
        let v = if negative { -lit.value } else { lit.value };
        let q = lit.unit.map_or(Quantity::ratio(v), |unit| unit.quantity(v));
        Term {
            q,
            unitless: lit.unit.is_none(),
        }
    }

    /// For `MixedUnits`: `100 + 5%` is "a plain number and a ratio".
    fn describe(&self) -> String {
        if self.unitless {
            "a plain number".to_string()
        } else if self.q.is_ratio() {
            "a ratio".to_string()
        } else {
            describe(self.q.kind, self.q.dim)
        }
    }
}

/// A literal, possibly negated: `47k`, `-10°C`, `-(3dB)`. A negated literal is still
/// one, so that `-10°C` is a temperature, not the negation of one (an error).
fn literal(e: &Expr) -> Option<(QuantityLit, bool)> {
    match &e.kind {
        ExprKind::Quantity(q) => Some((*q, false)),
        ExprKind::Neg(inner) => match &unparen(inner).kind {
            ExprKind::Quantity(q) => Some((*q, true)),
            _ => None,
        },
        _ => None,
    }
}

/// Whether `e` contains a `%` literal.
fn has_percent(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Quantity(q) => q.unit == Some(Unit::Percent),
        ExprKind::Paren(inner) | ExprKind::Neg(inner) => has_percent(inner),
        ExprKind::Binary { lhs, rhs, .. } => has_percent(lhs) || has_percent(rhs),
        _ => false,
    }
}

/// Whether `e` is a tolerance or a closed range, possibly scaled: what `spread_value`
/// handles.
fn has_spread(e: &Expr) -> bool {
    let e = unparen(e);
    match &e.kind {
        ExprKind::Binary {
            op: BinOp::Mul | BinOp::Div,
            lhs,
            rhs,
        } => has_spread(lhs) || has_spread(rhs),
        _ => is_spread(e),
    }
}

/// A tolerance or a closed range. An open range (`..=hi`, `lo..`) isn't a value yet, so
/// it isn't a spread either: it gets `NotAValue`.
fn is_spread(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Binary { op: BinOp::Tol, .. }
            | ExprKind::Range {
                lo: Some(_),
                hi: Some(_)
            }
    )
}

fn unparen<'e, 'src>(mut e: &'e Expr<'src>) -> &'e Expr<'src> {
    while let ExprKind::Paren(inner) = &e.kind {
        e = inner;
    }
    e
}

#[cfg(test)]
mod tests {
    use spicy_model::design::FieldValue;

    use crate::parser::parse;
    use crate::resolve::resolve;
    use spicy_errors::DiagKind;

    /// Resolves the part `part` (`Resistor { a: p, b: n, value: 47k }`, with ports `p`
    /// and `n` in scope) and returns its last field's value, or its errors as
    /// `Name "text"`, with ` fix "…"` when there is one.
    fn resolved(part: &str) -> String {
        let src =
            format!("block A {{ p: Pin, n: Pin }}\n\ncircuit A {{\n    let x = {part};\n}}\n");
        let parsed = parse(&src);
        assert!(!parsed.has_errors(), "{part}: {:?}", parsed.errors);
        let resolved = resolve(&parsed);
        if !resolved.errors.is_empty() {
            let errors: Vec<String> = resolved
                .errors
                .iter()
                .map(|e| {
                    let fix = e
                        .fix
                        .as_ref()
                        .map_or(String::new(), |f| format!(" fix {:?}", f.replacement));
                    format!("{} {:?}{fix}", e.kind.name(), &src[e.span.range()])
                })
                .collect();
            return errors.join("; ");
        }
        let fields = &resolved.design.blocks[0].instances[0].fields;
        match fields
            .iter()
            .rev()
            .find(|f| !matches!(f, FieldValue::Unset))
        {
            Some(FieldValue::Given(v)) => v.to_string(),
            other => panic!("{part}: {other:?}"),
        }
    }

    fn value(v: &str) -> String {
        resolved(&format!("Resistor {{ a: p, b: n, value: {v} }}"))
    }

    fn beta(v: &str) -> String {
        resolved(&format!("Npn {{ c: p, b: n, e: n, beta: {v} }}"))
    }

    fn rating(v: &str) -> String {
        resolved(&format!(
            "Capacitor {{ a: p, b: n, value: 1u, rating: {v} }}"
        ))
    }

    /// A unitless expression takes the field's unit at the end; a unit on one side of
    /// `*` or `/` makes the other side a plain factor (model.md E12).
    #[test]
    fn unit_inference() {
        assert_eq!(value("1V / 1mA"), "1000 Ω");
        assert_eq!(value("2 * 1k / 4"), "500 Ω");
        assert_eq!(value("1k - 3k"), "-2000 Ω");
        assert_eq!(value("2 * 1V"), r#"UnitMismatch "2 * 1V""#);
        assert_eq!(value("1V / 1mA - 1V / 1mA"), "0 Ω");
        // `%` is a plain factor outside a tolerance.
        assert_eq!(beta("50% * 400"), "200");
        assert_eq!(beta("1V / 1V"), "1");
    }

    #[test]
    fn tolerances_relative_or_absolute() {
        assert_eq!(value("1k ± 2"), "1000 Ω ± 2 Ω");
        assert_eq!(value("1k ± 2Ω"), "1000 Ω ± 2 Ω");
        assert_eq!(value("1k ± (1V / 1A)"), "1000 Ω ± 1 Ω");
        assert_eq!(value("1k ± 1V"), r#"UnitMismatch "1V""#);
        assert_eq!(value("1k ± 3dB"), r#"UnitMismatch "3dB""#);
        // Relative only when written with `%`: units that cancel are a plain number, so
        // `± (1V / 1V)` is ±1 on a ratio field and a mismatch where ohms are expected.
        assert_eq!(beta("200 ± 10"), "200 ± 10");
        assert_eq!(beta("200 ± 10%"), "200 ± 10%");
        assert_eq!(beta("200 ± (2 * 5%)"), "200 ± 10%");
        assert_eq!(beta("200 ± (1V / 1V)"), "200 ± 1");
        assert_eq!(value("1k ± (1V / 1V)"), r#"UnitMismatch "(1V / 1V)""#);
        assert_eq!(value("1k ± 0"), "1000 Ω ± 0 Ω");
        assert_eq!(value("1k ± (1% - 2%)"), r#"NegativeTolerance "(1% - 2%)""#);
    }

    /// Scaling: the factor is checked before the spread, so a bad factor is the only
    /// error; a negative factor flips a range; multiplying by zero is fine.
    #[test]
    fn scaled_spreads() {
        assert_eq!(beta("-2 * (50..=150)"), "-200 (-300..=-100)");
        assert_eq!(beta("(50..=150) / -2"), "-50 (-75..=-25)");
        assert_eq!(value("0 * (1k ± 1%)"), "0 Ω ± 1%");
        assert_eq!(value("50% * (1k ± 10)"), "500 Ω ± 5 Ω");
        assert_eq!(value("2 * 3 * (1k ± 1%)"), "6000 Ω ± 1%");
        assert_eq!(value("2 * ((1k ± 1%) / 4)"), "500 Ω ± 1%");
        assert_eq!(
            value("(1k ± -1%) * 1V"),
            r#"SpreadInArithmetic "(1k ± -1%) * 1V""#
        );
        assert_eq!(
            value("(1k ± 1%) * (2 ± 1%)"),
            r#"SpreadInArithmetic "(1k ± 1%) * (2 ± 1%)""#
        );
        assert_eq!(value("(1k ± 1%) / 0"), r#"DivisionByZero "0""#);
        assert_eq!(
            value("1k * (2 ± 1%) + 1k"),
            r#"SpreadInArithmetic "2 ± 1%""#
        );
    }

    /// A spread on an exact field is reported where it's written, even scaled.
    #[test]
    fn spread_on_an_exact_field() {
        assert_eq!(rating("2 * (25V ± 1V)"), r#"SpreadNotAllowed "25V ± 1V""#);
        assert_eq!(rating("2 * 25V"), "50 V");
        assert_eq!(
            rating("1V * (25V ± 1V)"),
            r#"SpreadInArithmetic "1V * (25V ± 1V)""#
        );
    }

    /// Temperatures and dB levels are only written as they are; a negated literal is
    /// still a literal.
    #[test]
    fn temperatures_and_levels() {
        assert_eq!(value("-10°C"), r#"UnitMismatch "-10°C""#);
        assert_eq!(value("-(-(10°C))"), r#"NotArithmetic "-(-(10°C))""#);
        assert_eq!(value("10°C - 5°C"), r#"NotArithmetic "10°C - 5°C""#);
        assert_eq!(value("1k + 3dB"), r#"NotArithmetic "1k + 3dB""#);
        assert_eq!(value("-3dB"), r#"UnitMismatch "-3dB""#);
    }

    #[test]
    fn not_a_value() {
        assert_eq!(value("1k < 2k"), r#"NotAValue "1k < 2k""#);
        assert_eq!(value("2 * p"), r#"NotAValue "p""#);
    }

    /// `K` is kelvin: every value in kelvin gets the note, and only a lone literal
    /// whose whole suffix is `K`, where a unit is expected, gets the fix.
    #[test]
    fn kelvin_for_kilo() {
        let with_note = |v: &str| {
            let src = format!(
                "block A {{ p: Pin, n: Pin }}\n\ncircuit A {{\n    let x = Resistor {{ a: p, b: n, value: {v} }};\n}}\n"
            );
            let errors = crate::testing::resolve_errors(&src);
            assert_eq!(errors.len(), 1, "{v}");
            let kind = &errors[0].kind;
            assert_eq!(kind.name(), "UnitMismatch", "{v}");
            kind.kelvin_for_kilo()
        };
        assert_eq!(value("47K"), r#"UnitMismatch "47K" fix "47k""#);
        assert_eq!(value("-47K"), r#"UnitMismatch "-47K" fix "-47k""#);
        assert_eq!(value("1_000K"), r#"UnitMismatch "1_000K" fix "1_000k""#);
        // The parentheses around a whole value are dropped first.
        assert_eq!(value("(47K)"), r#"UnitMismatch "47K" fix "47k""#);
        assert_eq!(value("2 * 47K"), r#"UnitMismatch "2 * 47K""#);
        assert_eq!(beta("47K"), r#"UnitMismatch "47K""#);
        for v in ["47K", "5mK", "1K + 1K", "(47K)", "2 * 47K", "47K * 2"] {
            assert!(with_note(v), "{v}");
        }
        for v in ["10°C", "10nF", "-(10°C)"] {
            assert!(!with_note(v), "{v}");
        }
    }
}
