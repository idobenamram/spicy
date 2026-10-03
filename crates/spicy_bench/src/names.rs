//! Inputs for resolve: many names, many values, many mistakes about names, setups of a
//! block with many ports, and a contract with many measures. Each is one shape at any
//! size, for a scaling pair (`n` and `4n`); its doc says the problems resolve finds in
//! it, and a test pins them.

use std::fmt::Write;

/// `n` consts and `n` envs, and `n` blocks whose two resistors read the consts (the
/// earlier `values_*.spl` and `reads_*.spl`). Clean.
pub fn named_values(n: usize) -> String {
    let mut src = String::new();
    for i in 0..n {
        writeln!(src, "const C{i:05}: Ohm = {}k;", i % 97 + 1).unwrap();
        writeln!(src, "env E{i:05}: Volt in {}V ± 5%;", i % 13 + 1).unwrap();
    }
    for i in 0..n {
        let other = (i * 7) % n;
        writeln!(
            src,
            "block B{i:05} {{ p: Pin, g: Ground }}\n\ncircuit B{i:05} {{\n    \
             let r1 = Resistor {{ a: p, b: g, value: C{i:05} }};\n    \
             let r2 = Resistor {{ a: p, b: g, value: C{other:05} ± 1% }};\n}}"
        )
        .unwrap();
    }
    src
}

/// Every const, env, block, circuit and setup defined twice, the second definitions
/// after all the first ones (the earlier `dups_*.spl` and `dupsetups_*.spl`): `5n`
/// `Duplicate`s, `n` for each kind of item.
pub fn duplicates(n: usize) -> String {
    let mut src = String::new();
    for _ in 0..2 {
        for i in 0..n {
            writeln!(
                src,
                "const C{i:05}: Ohm = 1k;\nenv E{i:05}: Volt in 1V..=2V;\n\
                 block B{i:05} {{ v: Power<In>, g: Ground }}\ncircuit B{i:05} {{}}\n\
                 setup S{i:05} for B{i:05} {{ v: Supply {{ v: 1V }}, temp: 25°C }}"
            )
            .unwrap();
        }
    }
    src
}

/// `n` broken consts (a voltage where a resistance is declared), each read by six parts
/// of `n` blocks (the earlier `broken_*.spl`): `n` `UnitMismatch`es, one per const. The
/// `6n` reads are tainted, not reported.
pub fn broken_reads(n: usize) -> String {
    let mut src = String::new();
    for i in 0..n {
        writeln!(src, "const K{i:05}: Ohm = 1V;").unwrap();
    }
    for b in 0..n {
        writeln!(
            src,
            "block B{b:05} {{ p: Pin, n: Pin }}\n\ncircuit B{b:05} {{"
        )
        .unwrap();
        for j in 0..6 {
            let k = (b * 6 + j) % n;
            writeln!(
                src,
                "    let r{j} = Resistor {{ a: p, b: n, value: K{k:05} }};"
            )
            .unwrap();
        }
        src.push_str("}\n");
    }
    src
}

/// One circuit whose `n` parts each name an unknown net, among `n` nets
/// (`resolve::tests::many_unknown_names_stay_linear`): `n` `UnknownName`s. Each is
/// close to a net (`nope00042`, `n00042`), so a "did you mean" search compares it with
/// every net, but only the first 64 searches run.
pub fn unknown_names(n: usize) -> String {
    let mut src = String::from("block B { v: Pin }\n\ncircuit B {\n");
    for i in 0..n {
        writeln!(src, "    net n{i:05};").unwrap();
        writeln!(
            src,
            "    let r{i:05} = Resistor {{ a: nope{i:05}, b: v, value: 1k }};"
        )
        .unwrap();
    }
    src.push_str("}\n");
    src
}

/// A block of `ports` supply inputs, and a setup that sets each one in two entries far
/// apart: its shape in the first half (`p00042: Supply {}`), its voltage in the second
/// (`p00042.v: 1V`), so the setup's grouping of entries by port does real work (the
/// earlier `wide_*.spl` and `paths_*.spl`). Clean.
pub fn wide_setup(ports: usize) -> String {
    wide_block_setup(ports, 0)
}

/// [`wide_setup`] of `n` ports, then `n` keys that name nothing (`q00042: Supply { v:
/// 1V }`): `n` `UnknownField`s and nothing else. Every port and `temp` is set, so no key
/// is renamed to a slot left out, none gets a "did you mean", and nothing is left out:
/// all this adds to `wide_setup(n)` is reporting the keys. Each report lists the
/// block's `n + 2` slots today, so it's quadratic (the earlier `unknown_*.spl`, without
/// their ports left out).
pub fn unknown_setup_keys(n: usize) -> String {
    wide_block_setup(n, n)
}

/// A block `W` of `ports` supply inputs `p00000…` and a ground, and a setup `S` that
/// sets each port and `temp`, with `unknown` keys `q00000…` that name nothing.
fn wide_block_setup(ports: usize, unknown: usize) -> String {
    let mut src = String::from("block W { ");
    for i in 0..ports {
        write!(src, "p{i:05}: Power<In>, ").unwrap();
    }
    src.push_str("gnd: Ground }\n\ncircuit W {}\n\nsetup S for W {\n");
    for i in 0..ports {
        writeln!(src, "    p{i:05}: Supply {{}},").unwrap();
    }
    for i in 0..ports {
        writeln!(src, "    p{i:05}.v: 1V,").unwrap();
    }
    for i in 0..unknown {
        writeln!(src, "    q{i:05}: Supply {{ v: 1V }},").unwrap();
    }
    src.push_str("    temp: 25°C,\n}\n");
    src
}

/// A contract of `n` measures, each reading the next one (`let m00000 = m00001 * 2;`),
/// the last `dc(output.v)`: typing `m00000`, the first in name order, types the whole
/// chain on demand, `n` measures deep. Clean.
pub fn measure_chain(n: usize) -> String {
    let mut src = String::from(
        "block A { input: Analog<In>, output: Analog<Out>, gnd: Ground }\n\n\
         circuit A {\n    \
         let r1 = Resistor { a: input, b: output, value: 1k };\n    \
         let r2 = Resistor { a: output, b: gnd, value: 1k };\n}\n\n\
         setup S for A { input: Signal { v: 0V }, output: Load {}, temp: 25°C }\n\n\
         contract A {\n    setup = S;\n",
    );
    for i in 0..n - 1 {
        writeln!(src, "    let m{i:05} = m{:05} * 2;", i + 1).unwrap();
    }
    writeln!(src, "    let m{:05} = dc(output.v);\n}}", n - 1).unwrap();
    src
}

#[cfg(test)]
mod tests {
    use spicy_lang::parser::parse;
    use spicy_lang::resolve::{NameKind, ResolveErrorKind, resolve};

    use super::*;
    use crate::Problems;

    /// `kinds` from resolve, and no problem at any other stage.
    fn only_resolve(kinds: Vec<&'static str>) -> Problems {
        Problems {
            resolve: kinds,
            ..Problems::default()
        }
    }

    #[test]
    fn named_values_wide_setups_and_measure_chains_are_clean() {
        for src in [named_values(5), wide_setup(5), measure_chain(5)] {
            assert_eq!(Problems::of(&src), Problems::default(), "{src}");
        }
    }

    #[test]
    fn every_measure_in_a_chain_is_typed() {
        let resolved = resolve(&parse(&measure_chain(5)));
        let contract = resolved.design.contracts[0].as_ref().unwrap();
        let typed: Vec<bool> = contract.measures.iter().map(|m| m.value.is_ok()).collect();
        assert_eq!(typed, [true; 5]);
    }

    #[test]
    fn every_item_in_duplicates_is_defined_twice() {
        let src = duplicates(3);
        assert_eq!(Problems::of(&src), only_resolve(vec!["Duplicate"; 15]));
        let errors = resolve(&parse(&src)).errors;
        let defined_twice: Vec<NameKind> = errors
            .into_iter()
            .filter_map(|e| match e.kind {
                ResolveErrorKind::Duplicate { what, .. } => Some(what),
                _ => None,
            })
            .collect();
        for kind in [
            NameKind::Const,
            NameKind::Env,
            NameKind::Block,
            NameKind::Circuit,
            NameKind::Setup,
        ] {
            let count = defined_twice.iter().filter(|&&what| what == kind).count();
            assert_eq!(count, 3, "{kind:?}");
        }
    }

    #[test]
    fn a_broken_const_is_reported_once_however_often_it_is_read() {
        let problems = Problems::of(&broken_reads(4));
        assert_eq!(problems, only_resolve(vec!["UnitMismatch"; 4]));
    }

    #[test]
    fn every_unknown_name_is_reported_with_its_net() {
        let src = unknown_names(5);
        assert_eq!(Problems::of(&src), only_resolve(vec!["UnknownName"; 5]));
        for (i, error) in resolve(&parse(&src)).errors.iter().enumerate() {
            let ResolveErrorKind::UnknownName { suggestion, .. } = &error.kind else {
                unreachable!();
            };
            assert_eq!(suggestion.as_deref(), Some(format!("n{i:05}").as_str()));
        }
    }

    #[test]
    fn unknown_setup_keys_are_reported_and_nothing_else() {
        let src = unknown_setup_keys(5);
        assert_eq!(Problems::of(&src), only_resolve(vec!["UnknownField"; 5]));
        for error in resolve(&parse(&src)).errors {
            let ResolveErrorKind::UnknownField { suggestion, .. } = error.kind else {
                unreachable!();
            };
            // Every slot is set: no slot left out to rename a key to, none to suggest.
            assert_eq!(suggestion, None);
        }
    }
}
