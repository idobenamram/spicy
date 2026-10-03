//! Elaboration (docs/ecad/model.md#stage-names): the syntax tree becomes flat designs, in
//! two steps.
//! 1. **Resolve** ([`crate::resolve`]): each block once, names resolved, values typed.
//! 2. **Flatten** ([`spicy_model::flatten`]): every root with its placements expanded,
//!    its nets merged and named, its parts devices, and its whole-net checks.
//!
//! Each step always returns and reports its own problems; flatten skips what resolve
//! left broken, so nothing is reported twice.

use spicy_model::flat::Flat;
use spicy_model::flatten::{Flattened, flatten};

use crate::parser::Parsed;
use crate::resolve::{Resolved, resolve};

/// Everything elaborating produces.
#[derive(Clone, Debug)]
pub struct Elaborated {
    pub resolved: Resolved,
    pub flattened: Flattened,
}

impl Elaborated {
    /// Each root's flat design, read with the design it flattens.
    pub fn roots(&self) -> impl Iterator<Item = Flat<'_>> {
        let design = &self.resolved.design;
        let roots = self.flattened.roots.iter();
        roots.map(move |data| Flat { design, data })
    }
}

pub fn elaborate(parsed: &Parsed) -> Elaborated {
    // 1. Resolve: the `Design`, each block once.
    let resolved = resolve(parsed);
    // 2. Flatten: one flat design per root.
    let flattened = flatten(&resolved.design, &resolved.source_map);
    Elaborated {
        resolved,
        flattened,
    }
}

/// The case-file suite for flatten (docs/ecad/model.md#flatten-plan), and the properties
/// of the whole stage.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use crate::testing::{
        Rng, assert_every_kind_has_a_case, check_parse_invariants, dump_elaborate, dump_flat,
        file_name, flatten_errors, read,
    };
    use spicy_errors::DiagKind;
    use spicy_model::flat::{FlatField, FlatInstanceId, FlatNetId, KnobId, KnobKind};
    use spicy_model::flatten::{FlattenErrorKind, check_simulation};
    use spicy_span::Span;

    /// `ok/` cases: no problems at any stage, and a snapshot of every root.
    #[test]
    fn ok_cases() {
        insta::glob!("../test_data/flatten", "ok/*.spl", |path| {
            let src = read(path);
            check_parse_invariants(&src);
            let parsed = parse(&src);
            let elaborated = elaborate(&parsed);
            assert!(
                !parsed.has_errors()
                    && elaborated.resolved.errors.is_empty()
                    && elaborated.flattened.errors.is_empty(),
                "{}: unexpected errors {:#?} {:#?}",
                path.display(),
                elaborated.resolved.errors,
                elaborated.flattened.errors
            );
            insta::assert_snapshot!(dump_elaborate(&file_name(path), &src));
        });
    }

    /// `err/` cases: at least one flatten problem, and a snapshot of roots and problems.
    #[test]
    fn err_cases() {
        insta::glob!("../test_data/flatten", "err/*.spl", |path| {
            let src = read(path);
            check_parse_invariants(&src);
            assert!(
                !flatten_errors(&src).is_empty(),
                "{}: expected flatten problems",
                path.display()
            );
            insta::assert_snapshot!(dump_elaborate(&file_name(path), &src));
        });
    }

    #[test]
    fn every_error_kind_has_a_case() {
        assert_every_kind_has_a_case("flatten/err", FlattenErrorKind::ALL_NAMES, |src| {
            flatten_errors(src).iter().map(|e| e.kind.name()).collect()
        });
    }

    /// The MVP design (docs/ecad/model.md#ce-amp-example): one root, 6 devices, 6 nets
    /// with `gnd` as ground, a knob for each of the 6 part fields with a spread, then
    /// the setup's `vcc.v` and the env `ambient` (contracts_plan.md §0.4), and its 3
    /// specs checkable.
    #[test]
    fn ce_amp() {
        let src = include_str!("../../../circuits/ce_amp.spl");
        let elaborated = elaborate(&parse(src));
        assert!(
            elaborated.flattened.errors.is_empty(),
            "{:#?}",
            elaborated.flattened.errors
        );
        let [flat] = elaborated.roots().collect::<Vec<_>>()[..] else {
            panic!("one root")
        };
        assert_eq!(flat.data.devices.len(), 6);
        let nets: Vec<String> = (0..flat.data.nets.len())
            .map(|n| flat.net_path(FlatNetId::new(n)).to_string())
            .collect();
        assert_eq!(nets.len(), 6, "{nets:?}");
        let map = &elaborated.resolved.source_map;
        let ground = check_simulation(flat, map, &mut Vec::new()).expect("one ground net");
        assert_eq!(flat.net_path(ground).to_string(), "gnd");
        let knobs: Vec<String> = (0..flat.data.knobs.len())
            .map(|k| flat.knob_path(KnobId::new(k)).to_string())
            .collect();
        let parts = [
            "c_in.value",
            "q1.beta",
            "r1.value",
            "r2.value",
            "rc.value",
            "re.value",
        ];
        assert_eq!(knobs, [&parts[..], &["vcc.v", "ambient"]].concat());
        let kinds: Vec<KnobKind> = flat.data.knobs.iter().map(|k| k.source.kind()).collect();
        let (part, range) = (KnobKind::Statistical, KnobKind::Range);
        let parts_then_setup = [part, part, part, part, part, part, range, range];
        assert_eq!(kinds, parts_then_setup);
        let specs: Vec<&str> = flat
            .checkable_specs()
            .map(|(_, s)| s.name.as_str())
            .collect();
        assert_eq!(specs, ["bias", "gain", "bass"]);
        insta::assert_snapshot!(dump_elaborate("ce_amp.spl", src));
    }

    /// A spec is checkable only when its root, contract and default setup are sound and
    /// it resolved itself (contracts_plan.md §0.4): no `setup = S;` leaves the root no
    /// flat setup, a setup, a circuit or a contract with an error inside leaves it
    /// broken, and a broken spec is left out on its own.
    #[test]
    fn only_sound_specs_are_checkable() {
        let head = "block A { vcc: Power<In>, gnd: Ground }\n\
                    circuit A { let r = Resistor { a: vcc, b: gnd, value: 1k }; }\n";
        let broken_head = "block A { vcc: Power<In>, gnd: Ground }\n\
                           circuit A { let r = Resistor { a: vcc, b: gnd, value: 1V }; }\n";
        let setup = "setup S for A { vcc: Supply { v: 1V }, temp: 25°C }\n";
        let broken_setup = "setup S for A { vcc: Supply { v: 1A }, temp: 25°C }\n";
        let specs = "spec ok: dc(vcc.v) <= 2V; spec bad: dc(vcc.v) <= 2Hz;";
        let cases = [
            (
                format!("{head}{setup}contract A {{ {specs} }}\n"),
                false,
                vec![],
            ),
            (
                format!("{head}{broken_setup}contract A {{ setup = S; {specs} }}\n"),
                true,
                vec![],
            ),
            (
                format!("{broken_head}{setup}contract A {{ setup = S; {specs} }}\n"),
                true,
                vec![],
            ),
            (
                format!("{head}{setup}contract A {{ setup = S; {specs} spec; }}\n"),
                true,
                vec![],
            ),
            // A lexer error in the contract: what was written may be missing from it.
            (
                format!("{head}{setup}contract A {{ setup = S; {specs} @ }}\n"),
                true,
                vec![],
            ),
            (
                format!("{head}{setup}contract A {{ setup = S; {specs} }}\n"),
                true,
                vec!["ok"],
            ),
        ];
        for (src, has_setup, checkable) in cases {
            let elaborated = elaborate(&parse(&src));
            let [flat] = elaborated.roots().collect::<Vec<_>>()[..] else {
                panic!("one root")
            };
            assert_eq!(flat.data.setup.is_some(), has_setup, "{src}");
            let names: Vec<&str> = flat
                .checkable_specs()
                .map(|(_, s)| s.name.as_str())
                .collect();
            assert_eq!(names, checkable, "{src}");
        }
    }

    /// Red team: the invariants check each measure once, however many specs and lets
    /// read it, as resolve's `pub` rule does. Followed through every reader, 4 specs on
    /// 22 lets that each read the next twice are 2²⁴ walks: seconds here, and out of
    /// memory a few lets deeper.
    #[test]
    fn the_invariants_check_a_shared_measure_once() {
        let mut src = String::from(
            "block A { vcc: Power<In>, gnd: Ground }\n\
             circuit A { let r = Resistor { a: vcc, b: gnd, value: 1k }; }\n\
             setup S for A { vcc: Supply { v: 1V }, temp: 25°C }\n\
             contract A {\n    setup = S;\n",
        );
        for i in 0..22 {
            src.push_str(&format!("    let m{i} = m{} + m{};\n", i + 1, i + 1));
        }
        src.push_str("    let m22 = dc(vcc.v);\n");
        for k in 0..4 {
            src.push_str(&format!("    spec s{k}: m0 <= 1V;\n"));
        }
        src.push_str("}\n");
        let start = std::time::Instant::now();
        check_parse_invariants(&src);
        let elapsed = start.elapsed();
        assert!(elapsed.as_secs_f64() < 1.0, "took {elapsed:?}");
    }

    /// The flat part of the dump: every root's nets, devices and knobs, by path.
    fn flat_dump(src: &str) -> String {
        elaborate(&parse(src)).roots().map(dump_flat).collect()
    }

    /// What a shuffle keeps: the flat part of the dump, then every flatten problem with
    /// the text it points at instead of where that text is.
    fn order_free_dump(src: &str) -> String {
        let text = |s: Span| &src[s.range()];
        let mut problems: Vec<String> = flatten_errors(src)
            .iter()
            .map(|e| {
                format!(
                    "{:?} at {:?} related {:?}",
                    e.kind,
                    text(e.span),
                    e.related.map(text)
                )
            })
            .collect();
        problems.sort();
        let mut out = flat_dump(src);
        for problem in problems {
            out.push_str(&problem);
            out.push('\n');
        }
        out
    }

    fn shuffle<T>(items: &mut [T], rng: &mut Rng) {
        for i in (1..items.len()).rev() {
            items.swap(i, rng.below(i + 1));
        }
    }

    /// `src` with its items in a random order, and the statements of every circuit and
    /// contract too. The comments between items are left out.
    fn shuffled(src: &str, rng: &mut Rng) -> String {
        let mut items: Vec<String> = Vec::new();
        let mut item: Option<String> = None;
        for line in src.lines() {
            let starts = [
                "block ",
                "pub block ",
                "circuit ",
                "setup ",
                "contract ",
                "env ",
                "const ",
            ]
            .iter()
            .any(|keyword| line.starts_with(keyword));
            if starts {
                item = Some(String::new());
            }
            if let Some(text) = &mut item {
                text.push_str(line);
                text.push('\n');
                // A `}` alone ends an item, and so does one on its first line
                // (`block A { p: Pin }`), or its `;` (`env a: T in r;`).
                if line == "}" || (starts && (line.ends_with('}') || line.ends_with(';'))) {
                    items.extend(item.take());
                }
            }
        }
        shuffle(&mut items, rng);
        let mut out = String::new();
        let mut body: Vec<&str> = Vec::new();
        for line in items.iter().flat_map(|item| item.lines()) {
            if line.starts_with("    ") && line.trim_end().ends_with(';') {
                body.push(line);
                continue;
            }
            shuffle(&mut body, rng);
            for l in body.drain(..) {
                out.push_str(l);
                out.push('\n');
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    /// The order of statements and blocks never changes a name, a device, a knob or a
    /// problem (model.md E4, E15, §9): KiCad and atopile both shipped net naming that
    /// depended on it.
    #[test]
    fn order_does_not_matter() {
        let mut sources = vec![include_str!("../../../circuits/ce_amp.spl").to_string()];
        for dir in ["ok", "err"] {
            let mut paths: Vec<_> = std::fs::read_dir(test_file(dir))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            paths.sort();
            sources.extend(paths.iter().map(|path| read(path)));
        }
        let mut rng = Rng(0xf1a7);
        for src in &sources {
            let reference = order_free_dump(src);
            for _ in 0..20 {
                let text = shuffled(src, &mut rng);
                assert_eq!(order_free_dump(&text), reference, "{text}");
            }
        }
    }

    /// Adding an unrelated part renames nothing: every net, device and knob keeps its
    /// path (docs/ecad/model.md#testing, stability).
    #[test]
    fn an_unrelated_part_renames_nothing() {
        let src = read(&test_file("ok/stereo.spl"));
        let with_extra = src.replace(
            "    let right = Amp",
            "    let extra = Resistor { a: v12, b: gnd, value: 1k ± 1% };\n    let right = Amp",
        );
        let lines =
            |src: &str| -> Vec<String> { flat_dump(src).lines().map(str::to_string).collect() };
        let after = lines(&with_extra);
        for line in lines(&src) {
            assert!(after.contains(&line), "lost `{line}`");
        }
        assert!(after.len() > lines(&src).len());
    }

    /// Flatten skips what resolve left broken, and reports nothing about it (model.md
    /// E7): an unknown kind, an unbound pin, a port with a wrong type, a wrong value.
    #[test]
    fn resolve_errors_add_no_flatten_errors() {
        let src = "block A { v: Power<In>, g: Ground, bad: Bus }\n\ncircuit A {\n    \
                   let x = Nothing { a: v };\n    let r = Resistor { a: v, value: 1k };\n    \
                   let c = Capacitor { a: v, b: g, value: 5V };\n    let s = B { i: bad };\n}\n\n\
                   block B { i: Power<In> }\n\ncircuit B {}\n";
        let elaborated = elaborate(&parse(src));
        assert!(!elaborated.resolved.errors.is_empty());
        assert_eq!(flatten_errors(src), vec![], "{src}");
    }

    /// A statement resolve drops, or one the parser can't read, leaves no placeholder,
    /// but its block has an error: the root isn't checked as a whole, so the damage (a
    /// net the dropped part was on, now isolated or unpowered) isn't reported on top of
    /// the cause (model.md E7).
    #[test]
    fn dropped_statements_add_no_flatten_errors() {
        let head = "block Load { vcc: Power<In>, gnd: Ground }\n\ncircuit Load {\n    \
                    let r = Resistor { a: vcc, b: gnd, value: 1k };\n}\n\n\
                    block T { v: Power<In>, g: Ground }\n\ncircuit T {\n    net x;\n";
        let bodies = [
            // A second `let r`, dropped: `x` would be isolated.
            "    let r = Resistor { a: v, b: g, value: 1k };\n    let r = Resistor { a: x, b: g, value: 1k };\n",
            // `a` given twice, the second dropped: `r` would be shorted, `x` isolated.
            "    let r = Resistor { a: g, a: x, b: g, value: 1k };\n",
            // A statement the parser can't read: `x` would be isolated.
            "    let r = Resistor { a: x b: g, value: 1k };\n",
            // A merge that isn't a list, dropped: `load` would be unpowered.
            "    net y = v;\n    let load = Load { vcc: y, gnd: g };\n",
            // A merged net that doesn't resolve: the same.
            "    net y = [vv];\n    let load = Load { vcc: y, gnd: g };\n",
        ];
        for body in bodies {
            let src = format!("{head}{body}}}\n");
            let parsed = parse(&src);
            let elaborated = elaborate(&parsed);
            assert!(
                parsed.has_errors() || !elaborated.resolved.errors.is_empty(),
                "{src}"
            );
            assert_eq!(flatten_errors(&src), vec![], "{src}");
        }
    }

    /// A syntax error breaks the block whose body reported it, not the block its span
    /// happens to be in (rustc taints the body that reported the error). A statement
    /// left open at the end of the file breaks its block, so `A`'s shorted, isolated `x`
    /// isn't reported on top of it. A missing `}` is found at the next item, `B`, but
    /// breaks `A`, whose body is left open: `B` is still checked (its shorted `r`).
    #[test]
    fn a_syntax_error_breaks_its_own_block() {
        let open_at_eof = "block A { g: Ground }\n\ncircuit A {\n    net x;\n    \
                           let r = Resistor { a: x, b: x, value: 1k };\n    \
                           let r2 = Resistor { a: g";
        let elaborated = elaborate(&parse(open_at_eof));
        assert!(elaborated.resolved.design.blocks[0].tainted.is_some());
        assert_eq!(flatten_errors(open_at_eof), vec![], "{open_at_eof}");

        let missing_brace = "block A { g: Ground }\n\ncircuit A {\n    net x;\n    \
                             let r = Resistor { a: x, b: x, value: 1k };\n\n\
                             block B { g: Ground }\n\ncircuit B {\n    \
                             let r = Resistor { a: g, b: g, value: 1k };\n}\n";
        let elaborated = elaborate(&parse(missing_brace));
        let [a, b] = &elaborated.resolved.design.blocks[..] else {
            panic!("two blocks")
        };
        assert!(a.tainted.is_some() && b.tainted.is_none());
        let errors = flatten_errors(missing_brace);
        let names: Vec<&str> = errors.iter().map(|e| e.kind.name()).collect();
        assert_eq!(names, ["ShortedPart"]);
        let b_starts = missing_brace.find("block B").unwrap() as u32;
        assert!(errors[0].span.start > b_starts, "the warning is `B`'s");
    }

    /// A block defined twice: which definition was meant isn't known, so the first,
    /// which the design keeps, is broken too, and a root that places it isn't checked
    /// as a whole (its shorted `r` and isolated `x` aren't reported).
    #[test]
    fn a_block_defined_twice_is_broken() {
        let src = "block A { g: Ground }\n\ncircuit A {\n    net x;\n    \
                   let r = Resistor { a: x, b: x, value: 1k };\n}\n\n\
                   block A { g: Ground }\n\n\
                   block T { g: Ground }\n\ncircuit T {\n    let a = A { g };\n}\n";
        let elaborated = elaborate(&parse(src));
        assert!(elaborated.resolved.design.blocks[0].tainted.is_some());
        assert_eq!(flatten_errors(src), vec![], "{src}");
    }

    /// A block with two circuits is broken the same way: which circuit was meant isn't
    /// known, so the one the design keeps isn't checked as a whole either (contracts_plan
    /// §3: the second-definition rule).
    #[test]
    fn a_block_with_two_circuits_is_broken() {
        let src = "block A { g: Ground }\n\ncircuit A {\n    net x;\n    \
                   let r = Resistor { a: x, b: x, value: 1k };\n}\n\n\
                   circuit A {}\n\n\
                   block T { g: Ground }\n\ncircuit T {\n    let a = A { g };\n}\n";
        let elaborated = elaborate(&parse(src));
        let kinds: Vec<_> = elaborated
            .resolved
            .errors
            .iter()
            .map(|e| e.kind.name())
            .collect();
        assert_eq!(kinds, ["Duplicate"]);
        assert!(elaborated.resolved.design.blocks[0].tainted.is_some());
        assert_eq!(flatten_errors(src), vec![], "{src}");
    }

    /// A spread of nothing varies nothing: `± 0%` and `200..=200` are exact values, not
    /// knobs (ngspice's `agauss` returns the nominal for one; Xyce still samples it, which
    /// shifts every later draw).
    #[test]
    fn a_zero_spread_is_exact() {
        let src = "block T { g: Ground, p: Pin }\n\ncircuit T {\n    \
                   let r = Resistor { a: p, b: g, value: 1k ± 0% };\n    \
                   let q = Npn { c: p, b: p, e: g, beta: 200..=200 };\n}\n";
        let elaborated = elaborate(&parse(src));
        let flat = elaborated.roots().next().expect("one root");
        assert_eq!(flat.data.knobs, []);
        let fields: Vec<_> = flat.data.devices.iter().map(|d| d.fields[0]).collect();
        assert!(
            fields.iter().all(|f| matches!(f, FlatField::Exact(_))),
            "{fields:?}"
        );
    }

    /// An error in a block the root doesn't place changes nothing for the root: it's
    /// still checked as a whole.
    #[test]
    fn an_error_elsewhere_still_checks_the_root() {
        let src = "block T { g: Ground }\n\ncircuit T {\n    net x;\n    \
                   let r = Resistor { a: x, b: x, value: 1k };\n}\n\n\
                   block Other { g: Ground }\n\ncircuit Other {\n    let n = Nothing { g };\n}\n";
        let mut names: Vec<&str> = flatten_errors(src).iter().map(|e| e.kind.name()).collect();
        names.sort();
        assert_eq!(names, ["IsolatedNets", "ShortedPart"]);
    }

    /// A placement's ancestors start at itself and run out to the root, and its path
    /// from a placement around it is the names below that one: what the checks' walks
    /// up the tree rely on.
    #[test]
    fn ancestors_and_paths_from_a_scope() {
        let src = "block C { p: Pin }\n\ncircuit C {}\n\nblock B { p: Pin }\n\ncircuit B {\n    \
                   let c = C { p };\n}\n\nblock A { p: Pin }\n\ncircuit A {\n    let b = B { p };\n}\n";
        let elaborated = elaborate(&parse(src));
        let [flat] = elaborated.roots().collect::<Vec<_>>()[..] else {
            panic!("one root")
        };
        let [a, b, c] = [0, 1, 2].map(FlatInstanceId::new);
        assert_eq!(flat.data.ancestors(c).collect::<Vec<_>>(), [c, b, a]);
        assert_eq!(flat.data.ancestors(a).collect::<Vec<_>>(), [a]);
        assert_eq!((flat.data.parent(c), flat.data.parent(a)), (Some(b), None));
        assert_eq!(flat.path(c).to_string(), "b.c");
        assert_eq!(flat.path_from(b, c).to_string(), "c");
        assert_eq!(flat.path_from(c, c).to_string(), "");
    }

    fn test_file(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test_data/flatten")
            .join(name)
    }

    /// A deep hierarchy stays linear: paths are built only to report something, and
    /// nets are named without building one (a first version stored every path and took
    /// 20 s for 10 000 levels).
    #[test]
    fn deep_hierarchy_stays_linear() {
        let mut src = String::from(
            "block L0 { p: Pin, g: Ground }\n\ncircuit L0 {\n    let r = Resistor { a: p, b: g, value: 1k ± 1% };\n}\n",
        );
        for i in 1..3000 {
            src.push_str(&format!(
                "block L{i} {{ p: Pin, g: Ground }}\n\ncircuit L{i} {{\n    let inner = L{} {{ p, g }};\n}}\n",
                i - 1
            ));
        }
        let parsed = parse(&src);
        let start = std::time::Instant::now();
        let elaborated = elaborate(&parsed);
        let elapsed = start.elapsed();
        assert_eq!(elaborated.flattened.roots.len(), 1);
        assert!(
            elaborated.flattened.errors.is_empty(),
            "{:?}",
            elaborated.flattened.errors
        );
        assert!(elapsed.as_secs_f64() < 2.0, "took {elapsed:?}");
    }
}
