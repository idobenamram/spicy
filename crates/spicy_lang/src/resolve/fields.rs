//! Binding fields to a schema's slots, for parts (`body.rs`) and for the shapes a setup
//! puts on a port (`setup.rs`): one helper per rule, so both read the same way. Each
//! types the value itself, by its own rules (`value`, `setup_value`).
//! - The shorthand isn't a value ([`written_value`](Resolver::written_value)).
//! - A slot is given once ([`given_once`](Resolver::given_once)).
//! - A field that names no slot is reported with the slot probably meant
//!   ([`report_unknown`](Resolver::report_unknown)).
//! - A required field left out holds the proof it was reported ([`left_out_required`]).

use std::collections::HashSet;

use spicy_errors::Reported;
use spicy_model::design::FieldValue;
use spicy_model::prelude::FieldSchema;
use spicy_span::Span;

use super::{NameKind, ResolveError, ResolveErrorKind, Resolver, suggest};
use crate::parser::ast::{Expr, Field, Ident};

/// One `name: value` given to a slot: a field of `Kind { … }`, or the last segment of a
/// setup's path (`vcc.v: 12V`). `value` is `None` for the shorthand `C { gnd }`; `span`
/// is the whole binding, where a second one is reported.
#[derive(Clone, Copy)]
pub(super) struct Given<'a, 'src> {
    pub name: &'a Ident<'src>,
    pub value: Option<&'a Expr<'src>>,
    pub span: Span,
}

impl<'a, 'src> From<&'a Field<'src>> for Given<'a, 'src> {
    fn from(field: &'a Field<'src>) -> Self {
        Given {
            name: &field.name,
            value: field.value.as_ref(),
            span: field.span,
        }
    }
}

impl Resolver<'_, '_> {
    /// The expression `given` writes. The shorthand isn't a value (`Resistor { value }`).
    pub(super) fn written_value<'e>(
        &mut self,
        given: Given<'e, '_>,
    ) -> Result<&'e Expr<'e>, Reported> {
        given
            .value
            .ok_or_else(|| self.report(ResolveErrorKind::NotAValue, given.span))
    }

    /// Records that `given` gives `slot`, unless it was given already: a slot is given
    /// once, and a second binding is reported with the first (model.md E9). Slots, not
    /// names: the span is the whole binding. `true` if this one is the first.
    pub(super) fn given_once(&mut self, slot: &mut Option<Span>, given: Given) -> bool {
        let Some(first) = *slot else {
            *slot = Some(given.span);
            return true;
        };
        self.given_twice(given, first);
        false
    }

    /// Reports `given`, a second binding of the slot `first` gave. Out of line: every
    /// binding of every part and setup passes through [`given_once`](Self::given_once),
    /// and only a mistake gets here.
    #[cold]
    fn given_twice(&mut self, given: Given, first: Span) {
        let kind = ResolveErrorKind::Duplicate {
            name: given.name.text.to_string(),
            what: NameKind::Binding,
        };
        let error = ResolveError::new(kind, given.span).with_related(first);
        error.report(&mut self.errors);
    }

    /// Reports each of `unknown`, the fields that name no slot of `of` (whose slots are
    /// `what`: pins and fields, a shape's fields, a block's ports). One unknown name and
    /// one `missing` slot that fits it is one mistake, a misnamed field (`resistance:`
    /// for `value:`): reported once, with the rename, and its index is returned, so the
    /// caller doesn't report the slot as missing too. Otherwise each gets a close
    /// spelling among the slots still open that fit it. `fits(u, i)` says whether
    /// `unknown[u]` could go in slot `i`: by its kind of value, not its spelling, so with
    /// one hole and one peg, `Supply { foo: 5V }` is `v` misnamed. `is_net` says whether
    /// a shorthand's name is a net, so `C { gnd }` renamed to pin `b` can become
    /// `b: gnd`. Returns the proof, if it reported any, and the rename.
    pub(super) fn report_unknown(
        &mut self,
        of: &str,
        what: NameKind,
        unknown: &[Given],
        slots: &UnboundSlots,
        fits: impl Fn(usize, usize) -> bool,
        is_net: impl Fn(&str) -> bool,
    ) -> (Option<Reported>, Option<usize>) {
        let mut reported = None;
        // Each unknown name once: `valu: 1k, valu: 2k` is one misspelling written twice.
        let mut seen: HashSet<&str> = HashSet::new();
        let first: Vec<bool> = unknown.iter().map(|g| seen.insert(g.name.text)).collect();
        let renamed = match (seen.len(), slots.missing) {
            (1, &[i]) if fits(0, i) => Some(i),
            _ => None,
        };
        for (u, (given, &first)) in unknown.iter().zip(&first).enumerate() {
            let name = given.name.text;
            let suggestion = match renamed {
                Some(i) => Some(slots.names[i].clone()),
                None => {
                    let open = (0..slots.names.len())
                        .filter(|&i| slots.given[i].is_none() && fits(u, i))
                        .map(|i| slots.names[i].as_str());
                    suggest(&mut self.suggestions_left, name, open)
                }
            };
            // A repeat of an unknown name gets no fix: renaming both would give the slot
            // twice. A shorthand renamed is `b: gnd` only if `gnd` is a net; otherwise
            // there's no single fix.
            let fix = match (&suggestion, given.value) {
                _ if !first => None,
                (Some(s), Some(_)) => Some(s.clone()),
                (Some(s), None) if is_net(name) => Some(format!("{s}: {name}")),
                _ => None,
            };
            let kind = ResolveErrorKind::UnknownField {
                field: name.to_string(),
                of: of.to_string(),
                what,
                valid: slots.names.to_vec(),
                suggestion,
            };
            let mut error = ResolveError::new(kind, given.name.span);
            if let Some(fix) = fix {
                error = error.with_fix(given.name.span, fix);
            }
            reported = Some(error.report(&mut self.errors));
        }
        (reported, renamed)
    }
}

/// Each of `values` still `Unset` once every field is bound wasn't given: a required
/// one holds `missing()`, the proof it was reported (model.md E7), so later steps skip
/// it; an optional one stays `Unset`, ideal (`Resistor { a, b }` has no `value`;
/// `Load {}` has no `r`, `c` or `i`). `values` is in the order of `schemas`.
pub(super) fn left_out_required(
    values: &mut [FieldValue],
    schemas: &[FieldSchema],
    missing: impl Fn() -> Reported,
) {
    for (value, schema) in values.iter_mut().zip(schemas) {
        if schema.required && *value == FieldValue::Unset {
            *value = FieldValue::Invalid(missing());
        }
    }
}

/// What [`report_unknown`](Resolver::report_unknown) reads of a kind's slots: their
/// names, in slot order, where each was given, and the ones that must be given and
/// weren't.
pub(super) struct UnboundSlots<'a> {
    pub names: &'a [String],
    pub given: &'a [Option<Span>],
    pub missing: &'a [usize],
}
