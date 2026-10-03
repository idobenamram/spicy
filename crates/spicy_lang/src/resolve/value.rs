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
use spicy_model::units::{Dimension, QKind, Quantity, Spread, Unit, Value, to_celsius};

use super::error::{ResolveError, ResolveErrorKind, describe};
use super::{FileValue, Resolver};
use spicy_span::Span;

use crate::lexer::QuantityLit;
use crate::parser::ast::{self, BinOp, Expr, ExprKind};

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

    /// Whether `e` is written as one number: a literal, a name, or arithmetic on them
    /// (`25`, `T_LAB`, `2 * 4.7k`). Whatever it computes to, it can't vary, as an env
    /// must. Not when the parser reported a mistake in it, since what it built is a
    /// guess.
    pub(super) fn is_one_number(&self, e: &Expr) -> bool {
        self.syntax.inside(e.span).is_none() && one_number(e)
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
            // Both sides are typed before either error stops the value, as a range's
            // ends are, so `25 ± 5%` in a temperature reports both, and fixing `25`
            // doesn't uncover the other.
            (BinOp::Tol, ..) => {
                let nominal = self.scalar(lhs, expected);
                let spread = self.tolerance(rhs, expected, nominal.as_ref().ok());
                Ok(Value {
                    nominal: nominal?,
                    spread: spread?,
                })
            }
            // Scaling a spread by a plain factor: `2 * (1k ± 1%)`, `(10k ± 5%) / 2`. The
            // parser's help for `2 * 1k ± 1%` suggests exactly this, so it must work.
            (BinOp::Mul | BinOp::Div, true, false) => {
                let k = self.scale_factor(rhs, *op == BinOp::Div, e.span)?;
                self.scalable(expected, e.span)?;
                Ok(self.spread_value(lhs, expected, field)?.scaled(k))
            }
            (BinOp::Mul, false, true) => {
                let k = self.scale_factor(lhs, false, e.span)?;
                self.scalable(expected, e.span)?;
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
        // Both ends are typed before either error stops the range, so `25..=30` in a
        // temperature reports both numbers, and fixing one doesn't uncover the other.
        let (lo, hi) = (self.scalar(lo, expected), self.scalar(hi, expected));
        let (lo, hi) = (lo?, hi?);
        if lo.si > hi.si {
            return Err(self.report(ResolveErrorKind::RangeReversed, at));
        }
        Ok(Value::range(lo, hi))
    }

    /// `Ok` if a spread of type `expected` may be scaled; `at` is the scaled spread. A
    /// temperature never is, a spread of one included (`2 * (20°C..=30°C)`, model.md
    /// E13). The position decides, so it's checked before the spread is typed, as the
    /// factor is: a fix inside (`25°C` for `2 * (25 ± 5K)`'s `25`) would uncover it.
    fn scalable(&mut self, expected: FieldType, at: Span) -> Result<(), Reported> {
        self.linear(expected.kind, at)
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
    /// as relative only where it's written so. A temperature's spread is a difference
    /// (see [`temperature_difference`](Self::temperature_difference)).
    /// `nominal` is the value's, if it typed, for the message of a percentage that
    /// isn't allowed.
    fn tolerance(
        &mut self,
        e: &Expr,
        expected: FieldType,
        nominal: Option<&Quantity>,
    ) -> Result<Spread, Reported> {
        let t = self.term(e)?;
        let relative = t.q.is_ratio() && has_percent(e);
        // A plain-number const can't show whether it's relative (`TOL = 1%` is the
        // number 0.01 once named), so it isn't a tolerance yet (decided 2026-10-02).
        if t.q.is_ratio() && !relative && names_a_const(e) {
            let what = "a plain-number const after `±` (write the tolerance with `%`)";
            return Err(self.unsupported(what, e.span));
        }
        let size = match expected.kind {
            // A percentage of a temperature or a level depends on where its zero is, so
            // which one is meant isn't written (model.md E13).
            QKind::TempPoint | QKind::Db if relative => {
                let kind = ResolveErrorKind::PercentSpread {
                    kind: expected.kind,
                    nominal: nominal.map(|q| q.si),
                    percent: t.q.si * 100.0,
                };
                return Err(self.report(kind, e.span));
            }
            QKind::TempPoint => self.temperature_difference(t, e)?,
            _ if relative => t.q.si,
            _ => self.typed(t, expected, e)?.si,
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

    /// A temperature's absolute spread `t`, written `e`, in K: a difference, in K or °C
    /// (`25°C ± 5K` and `25°C ± 5°C` are both ± 5 K), never a temperature, which is a
    /// point (`25°C ± T_ROOM`, model.md E13).
    fn temperature_difference(&mut self, t: Term, e: &Expr) -> Result<f64, Reported> {
        // `± 5°C` is 5 K: after `±` it's a difference, not the point 278.15 K.
        if let Some(kelvin) = celsius_difference(e) {
            return Ok(kelvin);
        }
        if t.q.kind == QKind::TempPoint {
            let kind = ResolveErrorKind::TemperatureSpread;
            return Err(self.report(kind, e.span));
        }
        // In K, as a plain quantity: a difference, not a point.
        Ok(self.typed(t, FieldType::tol(Dimension::KELVIN), e)?.si)
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
        let q = match (t.unitless, expected.kind) {
            // A temperature is written in °C (E13): `25` stays a plain number, reported
            // below.
            (true, QKind::TempPoint) => Quantity::new(t.q.si, expected.dim),
            // Any other position gives it its unit (E12), a level's dB too (`.db() >= 12`).
            (true, kind) => Quantity {
                kind,
                ..Quantity::new(t.q.si, expected.dim)
            },
            (false, _) => t.q,
        };
        if q.dim == expected.dim && q.kind == expected.kind {
            return Ok(q);
        }
        // A temperature as a plain number (`25`) or in K (`300K`, a difference).
        if expected.kind == QKind::TempPoint && q.kind == QKind::Plain && q.dim == Dimension::KELVIN
        {
            return Err(self.temperature_point(&t, e));
        }
        Err(self.unit_mismatch(q, expected, e))
    }

    /// Reports a temperature written as a plain number or in K, `t`. A lone literal gets
    /// the same temperature in `°C` as the fix: `25` is `25°C`, `300K` is `26.85°C`.
    fn temperature_point(&mut self, t: &Term, e: &Expr) -> Reported {
        let kind = ResolveErrorKind::TemperaturePoint {
            kelvin: !t.unitless,
        };
        let mut error = ResolveError::new(kind, e.span);
        if literal(e).is_some() {
            // A plain number is already in °C: it is written back exactly.
            let celsius = if t.unitless {
                t.q.si
            } else {
                to_celsius(t.q.si)
            };
            error = error.with_fix(e.span, format!("{celsius}°C"));
        }
        error.report(&mut self.errors)
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
        if let Some(lit) = literal(e) {
            return Ok(Term::literal(lit));
        }
        match &e.kind {
            ExprKind::Paren(inner) => self.term(inner),
            ExprKind::Neg(inner) => {
                let t = self.term(inner)?;
                self.linear(t.q.kind, e.span)?;
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
            ExprKind::Path(path) => self.named_value(path, e.span),
            // A number the lexer rejected: already reported.
            ExprKind::Error(reported) => Err(*reported),
            _ => Err(self.report(ResolveErrorKind::NotAValue, e.span)),
        }
    }

    /// A name where a number is expected: a const's value. A net, a port or an instance
    /// is no value, even one named like a const, so a const is the only reading here.
    fn named_value(&mut self, path: &ast::Path, at: Span) -> Result<Term, Reported> {
        let Some(value) = self.values.get(self.path_text(path)) else {
            return Err(self.report(ResolveErrorKind::NotAValue, at));
        };
        // A plain-number const acts like a plain number: `GAIN * 1k` is in Ω.
        let q = self.file_value(value, at)?;
        Ok(Term {
            q,
            unitless: q.is_ratio(),
        })
    }

    /// The number a file value names where one is expected (`at`), in a part's value or
    /// a measure: a const's. A broken one was reported at its definition: its proof is
    /// the reader's. An env is a range the engine searches, named only by a setup's
    /// `temp:`.
    pub(super) fn file_value(&mut self, value: FileValue, at: Span) -> Result<Quantity, Reported> {
        let what = match value {
            FileValue::Const(c) => match &self.consts {
                Some(consts) => return consts[c.index()],
                // The consts aren't typed yet: this is another const's value (see
                // `const_values`).
                None => "a const in another const's value",
            },
            FileValue::Env(_) => "an env's name anywhere but a setup's `temp:`",
        };
        Err(self.unsupported(what, at))
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
        self.linear(a.q.kind, at)?;
        self.linear(b.q.kind, at)?;
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

    /// `Ok` if a value of this kind may be used in arithmetic. Temperatures and dB
    /// levels aren't linear quantities: `2 * 10°C` and `3dB + 3dB` have no single
    /// meaning, so they're only written as they are. `at` is the arithmetic, for the
    /// error.
    pub(super) fn linear(&mut self, kind: QKind, at: Span) -> Result<(), Reported> {
        let what = match kind {
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
    fn literal(lit: QuantityLit) -> Self {
        Term {
            q: literal_quantity(lit),
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

/// A literal's value in its written unit; one written without a unit is a ratio.
pub(super) fn literal_quantity(QuantityLit { value, unit }: QuantityLit) -> Quantity {
    unit.map_or(Quantity::ratio(value), |unit| unit.quantity(value))
}

/// A literal, possibly negated, with its sign: `47k`, `-10°C` (the number -10 in °C),
/// `-(3dB)`. A negated literal is still one, so that `-10°C` is a temperature, not the
/// negation of one (an error).
fn literal(e: &Expr) -> Option<QuantityLit> {
    match &e.kind {
        ExprKind::Quantity(q) => Some(*q),
        ExprKind::Neg(inner) => match &unparen(inner).kind {
            ExprKind::Quantity(q) => Some(QuantityLit {
                value: -q.value,
                ..*q
            }),
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

/// Whether `e` names a value. Once `e` is typed, every name in it is a const's.
fn names_a_const(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Path(_) => true,
        ExprKind::Paren(inner) | ExprKind::Neg(inner) => names_a_const(inner),
        ExprKind::Binary { lhs, rhs, .. } => names_a_const(lhs) || names_a_const(rhs),
        _ => false,
    }
}

/// A spread written as one `°C` literal (`5°C`, `-5°C`, `(5°C)`), as the difference it
/// means: its written number, in K, since a step of 1 °C is 1 K (model.md E13).
fn celsius_difference(e: &Expr) -> Option<f64> {
    let lit = literal(unparen(e))?;
    (lit.unit == Some(Unit::Celsius)).then_some(lit.value)
}

/// See [`Resolver::is_one_number`]. A spread, an open range, a call and an operand the
/// parser couldn't read (`25°C +`) aren't.
fn one_number(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Quantity(_) | ExprKind::Path(_) => true,
        ExprKind::Paren(inner) | ExprKind::Neg(inner) => one_number(inner),
        ExprKind::Binary {
            op: BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div,
            lhs,
            rhs,
        } => one_number(lhs) && one_number(rhs),
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

pub(super) fn unparen<'e, 'src>(mut e: &'e Expr<'src>) -> &'e Expr<'src> {
    while let ExprKind::Paren(inner) = &e.kind {
        e = inner;
    }
    e
}

#[cfg(test)]
mod tests {
    use spicy_model::design::FieldValue;
    use spicy_model::units::{Quantity, Spread, Value};

    use crate::parser::parse;
    use crate::resolve::{ResolveError, resolve};
    use crate::testing::resolve_errors;
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
            return errors(&src, &resolved.errors);
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

    /// `errors` as `Name "text"`, with ` fix "…"` when there is one, joined by `; `.
    fn errors(src: &str, errors: &[ResolveError]) -> String {
        let errors: Vec<String> = errors
            .iter()
            .map(|e| {
                let fix = e
                    .fix
                    .as_ref()
                    .map_or(String::new(), |f| format!(" fix {:?}", f.replacement));
                format!("{} {:?}{fix}", e.kind.name(), &src[e.span.range()])
            })
            .collect();
        errors.join("; ")
    }

    /// A file with the env `t: Temperature in {v}`, after the const `T_LAB` (25 °C).
    fn temperature_src(v: &str) -> String {
        format!("const T_LAB: Temperature = 25°C;\nenv t: Temperature in {v};\n")
    }

    /// The env `t: Temperature in {v}`'s value, or its errors as in [`resolved`].
    fn temperature(v: &str) -> String {
        let src = temperature_src(v);
        let parsed = parse(&src);
        assert!(!parsed.has_errors(), "{v}: {:?}", parsed.errors);
        let resolved = resolve(&parsed);
        match &resolved.design.envs[0].value {
            Ok(value) => value.to_string(),
            Err(_) => errors(&src, &resolved.errors),
        }
    }

    /// The env `t: Temperature in {v}`'s value once every fix is applied, which must
    /// leave no problem.
    fn temperature_fixed(v: &str) -> Value {
        let mut src = temperature_src(v);
        let mut fixes: Vec<_> = resolve_errors(&src)
            .into_iter()
            .filter_map(|e| e.fix)
            .collect();
        fixes.sort_by_key(|f| std::cmp::Reverse(f.span.start));
        for fix in fixes {
            src.replace_range(fix.span.range(), &fix.replacement);
        }
        let parsed = parse(&src);
        let resolved = resolve(&parsed);
        assert!(
            !parsed.has_errors() && resolved.errors.is_empty(),
            "{v} fixed to {src:?}: {:?} {:?}",
            parsed.lex_errors,
            resolved.errors
        );
        resolved.design.envs[0].value.unwrap()
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
        assert_eq!(value("1V / 1mA"), "1 kΩ");
        assert_eq!(value("2 * 1k / 4"), "500 Ω");
        assert_eq!(value("1k - 3k"), "-2 kΩ");
        assert_eq!(value("2 * 1V"), r#"UnitMismatch "2 * 1V""#);
        assert_eq!(value("1V / 1mA - 1V / 1mA"), "0 Ω");
        // `%` is a plain factor outside a tolerance.
        assert_eq!(beta("50% * 400"), "200");
        assert_eq!(beta("1V / 1V"), "1");
    }

    #[test]
    fn tolerances_relative_or_absolute() {
        assert_eq!(value("1k ± 2"), "1 kΩ ± 2 Ω");
        assert_eq!(value("1k ± 2Ω"), "1 kΩ ± 2 Ω");
        assert_eq!(value("1k ± (1V / 1A)"), "1 kΩ ± 1 Ω");
        assert_eq!(value("1k ± 1V"), r#"UnitMismatch "1V""#);
        assert_eq!(value("1k ± 3dB"), r#"UnitMismatch "3dB""#);
        // Relative only when written with `%`: units that cancel are a plain number, so
        // `± (1V / 1V)` is ±1 on a ratio field and a mismatch where ohms are expected.
        assert_eq!(beta("200 ± 10"), "200 ± 10");
        assert_eq!(beta("200 ± 10%"), "200 ± 10%");
        assert_eq!(beta("200 ± (2 * 5%)"), "200 ± 10%");
        assert_eq!(beta("200 ± (1V / 1V)"), "200 ± 1");
        assert_eq!(value("1k ± (1V / 1V)"), r#"UnitMismatch "(1V / 1V)""#);
        assert_eq!(value("1k ± 0"), "1 kΩ ± 0 Ω");
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
        assert_eq!(value("2 * 3 * (1k ± 1%)"), "6 kΩ ± 1%");
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

    /// A temperature is never scaled, a spread of one included (E13: point × anything
    /// is an error). Red team: `2 * (20°C..=30°C)` was accepted as 596.3 K.
    #[test]
    fn scaled_temperatures() {
        assert_eq!(
            temperature("(25°C ± 5K) * 2"),
            r#"NotArithmetic "(25°C ± 5K) * 2""#
        );
        assert_eq!(
            temperature("2 * (20°C..=30°C)"),
            r#"NotArithmetic "2 * (20°C..=30°C)""#
        );
        assert_eq!(
            temperature("(20°C..=30°C) / 2"),
            r#"NotArithmetic "(20°C..=30°C) / 2""#
        );
        assert_eq!(
            temperature("(T_LAB ± 5K) * 1"),
            r#"NotArithmetic "(T_LAB ± 5K) * 1""#
        );
        assert_eq!(temperature("T_LAB ± 5K"), "298.15 K (25 °C) ± 5 K");
        // Checked before the spread inside is typed, as a bad factor is.
        assert_eq!(
            temperature("2 * (25°C ± 5V)"),
            r#"NotArithmetic "2 * (25°C ± 5V)""#
        );
    }

    /// A temperature's fix means what was written: a plain number is that many °C, a
    /// value in K that many kelvin. Each is applied and read back. Red team: the fixes
    /// rounded through `x * 1e9`, so `1e300` became `inf°C` and `1e-12` became `0°C`.
    #[test]
    fn temperature_fixes_mean_what_was_written() {
        let points = [
            ("25", Quantity::celsius(25.0).si),
            ("1e-12", Quantity::celsius(1e-12).si),
            ("1e300", 1e300),
            ("300K", 300.0),
            ("263.15K", 263.15),
            ("1e9K", 1e9),
            ("1e20K", 1e20),
            ("1e300K", 1e300),
        ];
        // The spread is as large as the point, so the env still varies at `1e300`.
        for (point, kelvin) in points {
            let fixed = temperature_fixed(&format!("{point} ± {point}"));
            assert_eq!(fixed.nominal.si, kelvin, "{point}");
        }
        // A fix that can't be exact is as close as `°C` gets (0.1 K is no f64 of °C).
        let fixed = temperature_fixed("0.1K ± 5K");
        assert!((fixed.nominal.si - 0.1).abs() < 1e-12, "{fixed}");
        assert_eq!(
            temperature("0.1K ± 5K"),
            r#"TemperaturePoint "0.1K" fix "-273.05°C""#
        );
    }

    /// After `±` a temperature's spread is a difference, in K or °C alike: a `°C`
    /// literal is its written number of kelvin, at any size (decided 2026-10-02: the SI
    /// and datasheets write `±0.5°C`). A percentage or a named temperature, a point,
    /// isn't a difference, arithmetic on a point is still an error, and a negative
    /// spread is wrong as anywhere.
    #[test]
    fn celsius_spreads_are_differences() {
        assert_eq!(temperature("25°C ± 5°C"), "298.15 K (25 °C) ± 5 K");
        assert_eq!(temperature("25°C ± (5°C)"), "298.15 K (25 °C) ± 5 K");
        assert_eq!(temperature("25°C ± 5K"), "298.15 K (25 °C) ± 5 K");
        for (spread, kelvin) in [("1e-12°C", 1e-12), ("1e20°C", 1e20), ("1e300°C", 1e300)] {
            let resolved = resolve(&parse(&temperature_src(&format!("25°C ± {spread}"))));
            let value = resolved.design.envs[0].value.unwrap();
            assert_eq!(value.spread, Spread::Abs(kelvin), "{spread}");
        }
        assert_eq!(temperature("25°C ± 5%"), r#"PercentSpread "5%""#);
        assert_eq!(temperature("25°C ± T_LAB"), r#"TemperatureSpread "T_LAB""#);
        assert_eq!(
            temperature("25°C ± (2 * 5°C)"),
            r#"NotArithmetic "2 * 5°C""#
        );
        assert_eq!(temperature("25°C ± -5°C"), r#"NegativeTolerance "-5°C""#);
        assert_eq!(
            temperature("25°C ± -(5°C)"),
            r#"NegativeTolerance "-(5°C)""#
        );
    }

    /// A mistake doesn't hide another one that its fix would uncover: a tolerance types
    /// both sides, as a range types both ends; an env written without a spread needs
    /// one whatever else is wrong; and a temperature isn't scaled whatever is inside.
    /// Red team: each of these reported only the first error, whose fix (`25°C`,
    /// `47k`) then uncovered the second.
    #[test]
    fn a_fix_uncovers_no_error() {
        assert_eq!(
            temperature("25 ± 5%"),
            r#"TemperaturePoint "25" fix "25°C"; PercentSpread "5%""#
        );
        assert_eq!(
            value("47K ± 1V"),
            r#"UnitMismatch "47K" fix "47k"; UnitMismatch "1V""#
        );
        assert_eq!(
            temperature("25"),
            r#"TemperaturePoint "25" fix "25°C"; EnvNeedsRange "25""#
        );
        assert_eq!(
            temperature("2 * (25 ± 5K)"),
            r#"NotArithmetic "2 * (25 ± 5K)""#
        );
        // A value the parser couldn't read is a guess, not taken for one number.
        let src = "env t: Temperature in 25°C +;\n";
        assert!(resolve(&parse(src)).errors.is_empty());
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
