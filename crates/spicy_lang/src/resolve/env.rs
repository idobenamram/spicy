//! Pass 1 for the file's values: every `env` and `const`, declared in the file's value
//! namespace and typed against its declared type (contracts_plan.md step 1).
//!
//! The consts are typed first, all before any of them can be named: so a const's value
//! can't name another const yet, whatever their order (that needs typing on demand,
//! with a cycle check). Then the envs, whose values may name a const. An env and a const
//! share one namespace; a second definition of a name is typed for its own mistakes,
//! then dropped, and the first is broken (see [`Redefined`]).

use spicy_errors::Reported;
use spicy_model::design::{Const, ConstId, Env, EnvId};
use spicy_model::prelude::{FieldType, ValueType};
use spicy_model::units::{Quantity, Value};
use spicy_span::Span;

use super::{
    FileValue, NameKind, Namespace, Redefined, ResolveErrorKind, Resolver, suggest, unknown_name,
};
use crate::parser::ast::{self, ItemKind, ValueDecl};

/// v5's other value types, which are "not supported yet" rather than unknown.
const LATER_TYPES: &[&str] = &["Confidence"];

/// The file's envs and consts, declared: the first of each name, in source order (an
/// id is its place in `envs` or `consts`), each with the proof it was defined again if
/// it was (its value is then broken too, see [`Redefined`]), and the second definitions
/// of a name.
#[derive(Default)]
pub(super) struct ValueDecls<'p, 'src> {
    envs: Vec<(&'p ValueDecl<'src>, Option<Reported>)>,
    consts: Vec<(&'p ValueDecl<'src>, Option<Reported>)>,
    second_envs: Vec<&'p ValueDecl<'src>>,
    second_consts: Vec<&'p ValueDecl<'src>>,
}

impl<'p, 'src> Resolver<'p, 'src> {
    /// Every env's and const's name, in `values` (an env and a const share them).
    pub(super) fn declare_values(&mut self) -> ValueDecls<'p, 'src> {
        let mut decls = ValueDecls::default();
        for item in &self.parsed.file.items {
            let (decl, value, what, first, second) = match &item.kind {
                ItemKind::Env(decl) => (
                    decl,
                    FileValue::Env(EnvId::new(decls.envs.len())),
                    NameKind::Env,
                    &mut decls.envs,
                    &mut decls.second_envs,
                ),
                ItemKind::Const(decl) => (
                    decl,
                    FileValue::Const(ConstId::new(decls.consts.len())),
                    NameKind::Const,
                    &mut decls.consts,
                    &mut decls.second_consts,
                ),
                _ => continue,
            };
            match self
                .values
                .declare(&decl.name, value, what, &mut self.errors)
            {
                Ok(()) => first.push((decl, None)),
                Err(Redefined { first, reported }) => {
                    second.push(decl);
                    // The first may be of the other kind: `env T`, then `const T`.
                    let again = match first {
                        FileValue::Env(id) => &mut decls.envs[id.index()].1,
                        FileValue::Const(id) => &mut decls.consts[id.index()].1,
                    };
                    *again = Some(reported);
                }
            }
        }
        decls
    }

    /// Every const, typed, by `ConstId`, and its name's span. A second one of a name is
    /// typed for its own mistakes, then dropped, and the first is broken (see
    /// [`Redefined`]), as with a block defined twice.
    pub(super) fn const_values(&mut self, decls: &ValueDecls) -> (Vec<Const>, Vec<Span>) {
        let consts = decls.consts.iter().map(|&(decl, again)| {
            let value = self.const_value(decl);
            let value = again.map_or(value, Err);
            let name = decl.name.text.to_string();
            (Const { name, value }, decl.name.span)
        });
        let consts = consts.unzip();
        for decl in &decls.second_consts {
            let _ = self.const_value(decl);
        }
        consts
    }

    /// Every env, typed, by `EnvId`, and its name's span. A second one of a name is
    /// typed for its own mistakes, then dropped, and the first is broken.
    pub(super) fn env_values(&mut self, decls: &ValueDecls) -> (Vec<Env>, Vec<Span>) {
        let envs = decls.envs.iter().map(|&(decl, again)| {
            let value = self.env_value(decl);
            let value = again.map_or(value, Err);
            let name = decl.name.text.to_string();
            (Env { name, value }, decl.name.span)
        });
        let envs = envs.unzip();
        for decl in &decls.second_envs {
            let _ = self.env_value(decl);
        }
        envs
    }

    /// A const's value, typed as its declared type: exact, since a range the engine
    /// searches is an `env` (a spread gets `SpreadNotAllowed`).
    fn const_value(&mut self, decl: &ValueDecl) -> Result<Quantity, Reported> {
        let ty = self.value_type(&decl.ty)?.field_type();
        let exact = FieldType {
            spread_allowed: false,
            ..ty
        };
        Ok(self.value(&decl.value, exact, decl.name.text)?.nominal)
    }

    /// An env's value, typed as its declared type: a range or a tolerance, since a
    /// fixed value is a `const`.
    fn env_value(&mut self, decl: &ValueDecl) -> Result<Value, Reported> {
        let value = self
            .value_type(&decl.ty)
            .and_then(|ty| self.value(&decl.value, ty.field_type(), decl.name.text));
        // `12V ± 0%` and `5V..=5V` don't vary either. One written without a spread
        // can't, whatever its type and its own mistakes, so it's reported with them:
        // fixing `25` to `25°C` mustn't uncover this error.
        let varies = match &value {
            Ok(value) => value.varies(),
            Err(_) => !self.is_one_number(&decl.value),
        };
        if !varies {
            let kind = ResolveErrorKind::EnvNeedsRange {
                name: decl.name.text.to_string(),
            };
            return Err(self.report(kind, decl.value.span));
        }
        value
    }

    /// An env's or const's declared type: `Temperature`, `Ohm`, `f64`.
    fn value_type(&mut self, ty: &ast::Type) -> Result<ValueType, Reported> {
        let (path, args) = match &ty.kind {
            ast::TypeKind::Path { path, args } => (path, args),
            ast::TypeKind::Error(reported) => return Err(*reported),
        };
        let name = self.path_text(path);
        if !args.is_empty() {
            let what = "type arguments on a value type";
            return Err(self.report(ResolveErrorKind::Unsupported { what }, ty.span));
        }
        if let Some(ty) = ValueType::from_name(name) {
            return Ok(ty);
        }
        if LATER_TYPES.contains(&name) {
            let what = "this type";
            return Err(self.report(ResolveErrorKind::Unsupported { what }, path.span));
        }
        let names = ValueType::ALL.iter().map(|t| t.name());
        let suggestion = suggest(&mut self.suggestions_left, name, names);
        let error = unknown_name(
            name,
            Namespace::Type,
            suggestion.clone(),
            suggestion,
            path.span,
        );
        Err(error.report(&mut self.errors))
    }
}

#[cfg(test)]
mod tests {
    use spicy_errors::DiagKind;
    use spicy_model::design::FieldValue;
    use spicy_model::units::{Dimension, Quantity, Spread, Value};

    use crate::parser::parse;
    use crate::resolve::resolve;

    /// A plain-number const can't say whether it's relative (`TOL = 1%` is the number
    /// 0.01 once named, so `1k ± TOL` would be ±0.01 Ω), so it isn't a tolerance yet
    /// (decided 2026-10-02). A const with a unit is one.
    #[test]
    fn a_plain_number_const_is_no_tolerance_yet() {
        let src = "const TOL: f64 = 1%;\nconst R_TOL: Ohm = 10;\n\nblock X { p: Pin, n: Pin }\n\n\
                   circuit X {\n    let r1 = Resistor { a: p, b: n, value: 1k ± TOL };\n    \
                   let r2 = Resistor { a: p, b: n, value: 1k ± R_TOL };\n}\n";
        let resolved = resolve(&parse(src));
        let kinds: Vec<_> = resolved.errors.iter().map(|e| e.kind.name()).collect();
        assert_eq!(kinds, ["Unsupported"]);
        let r2 = &resolved.design.blocks[0].instances[1];
        let value = Value {
            nominal: Quantity::new(1000.0, Dimension::OHM),
            spread: Spread::Abs(10.0),
        };
        assert_eq!(r2.fields, [FieldValue::Given(value)]);
    }

    /// An env written without a spread needs one whatever else is wrong: an unknown type,
    /// or a broken const it names. Fixing either mustn't uncover `EnvNeedsRange`. Red
    /// team: only the other error was reported.
    #[test]
    fn an_env_without_a_spread_needs_one_whatever_else_is_wrong() {
        let sources = [
            ("env t: Temperatur in 25°C;\n", "UnknownName"),
            (
                "const T: Temperature = 25;\nenv t: Temperature in T;\n",
                "TemperaturePoint",
            ),
        ];
        for (src, other) in sources {
            let resolved = resolve(&parse(src));
            let kinds: Vec<_> = resolved.errors.iter().map(|e| e.kind.name()).collect();
            assert_eq!(kinds, [other, "EnvNeedsRange"], "{src}");
        }
    }

    /// A name defined twice: which definition was meant isn't known, so the first is
    /// broken too, as a block defined twice is, and a part reading it taints its
    /// block. Only the duplicates are reported.
    #[test]
    fn a_name_defined_twice_breaks_the_first() {
        let src = "const R: Ohm = 1k;\nconst R: Ohm = 2k;\nenv T: Volt in 1V..=2V;\n\
                   const T: Ohm = 1;\n\nblock X { p: Pin, n: Pin }\n\n\
                   circuit X {\n    let r = Resistor { a: p, b: n, value: R };\n}\n";
        let resolved = resolve(&parse(src));
        let kinds: Vec<_> = resolved.errors.iter().map(|e| e.kind.name()).collect();
        assert_eq!(kinds, ["Duplicate", "Duplicate"]);
        assert!(resolved.design.consts[0].value.is_err());
        assert!(resolved.design.envs[0].value.is_err());
        let x = &resolved.design.blocks[0];
        assert!(x.tainted.is_some());
        assert!(matches!(x.instances[0].fields[0], FieldValue::Invalid(_)));
    }

    /// A file whose names are all defined twice stays linear: each first holds its own
    /// "defined again" slot (a first version searched a list: 3.8 s for these 40 000
    /// names in a debug build).
    #[test]
    fn names_defined_twice_stay_linear() {
        let mut src = String::new();
        for _ in 0..2 {
            for i in 0..20_000 {
                src.push_str(&format!(
                    "const C{i}: Ohm = 1k;\nenv E{i}: Volt in 1V..=2V;\n"
                ));
            }
        }
        let parsed = parse(&src);
        let start = std::time::Instant::now();
        let resolved = resolve(&parsed);
        let elapsed = start.elapsed();
        assert_eq!(resolved.errors.len(), 40_000);
        assert!(resolved.design.consts.iter().all(|c| c.value.is_err()));
        assert!(resolved.design.envs.iter().all(|e| e.value.is_err()));
        assert!(elapsed.as_secs_f64() < 2.0, "took {elapsed:?}");
    }

    /// A second definition of a name is dropped without shifting the consts after it:
    /// `B` is the second const, and a part's value reads its own value (a first version
    /// numbered the dropped `A`, so `B` read past the table).
    #[test]
    fn a_second_definition_shifts_no_const() {
        let src = "const A: Ohm = 1k;\nconst A: Ohm = 2k;\nenv A: Volt in 1V..=2V;\n\
                   const B: Ohm = 3k;\n\nblock X { p: Pin, n: Pin }\n\n\
                   circuit X {\n    let r = Resistor { a: p, b: n, value: B };\n}\n";
        let resolved = resolve(&parse(src));
        let duplicates = resolved
            .errors
            .iter()
            .filter(|e| e.kind.name() == "Duplicate");
        assert_eq!(duplicates.count(), 2, "{:#?}", resolved.errors);
        assert_eq!(resolved.errors.len(), 2, "{:#?}", resolved.errors);
        let consts: Vec<_> = resolved
            .design
            .consts
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(consts, ["A", "B"]);
        assert!(resolved.design.envs.is_empty());
        let ohms = Value::exact(Quantity::new(3000.0, Dimension::OHM));
        let r = &resolved.design.blocks[0].instances[0];
        assert_eq!(r.fields, [FieldValue::Given(ohms)]);
    }
}
