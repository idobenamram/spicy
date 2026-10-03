//! Inputs for flatten's scaling pairs, one shape each: a deep hierarchy ([`deep`]), one
//! wide circuit ([`wide_circuit`]), a problem in every part of a placed block
//! ([`shorted`]), and a root whose default setup has many ports
//! ([`wide_default_setup`]). The fifth pair, many roots, flattens
//! [`crate::typical::library`]. The tests also check the two typical inputs as flatten
//! reads them.

use std::fmt::Write;

/// A hierarchy `depth` levels deep: `L00000` holds one resistor and each level places
/// the one below, so the last level is the one root, and it flattens to `depth`
/// placements on two nets. The shape of `elaborate::tests::deep_hierarchy_stays_linear`
/// and of the earlier `deep_3000.spl`. `deep(2)`:
///
/// ```text
/// block L00000 { p: Pin, g: Ground }
///
/// circuit L00000 {
///     let r = Resistor { a: p, b: g, value: 1k ± 1% };
/// }
///
/// block L00001 { p: Pin, g: Ground }
///
/// circuit L00001 {
///     let inner = L00000 { p, g };
/// }
/// ```
pub fn deep(depth: usize) -> String {
    let mut src = String::new();
    for level in 0..depth {
        let body = match level {
            0 => "let r = Resistor { a: p, b: g, value: 1k ± 1% };".to_string(),
            _ => format!("let inner = L{:05} {{ p, g }};", level - 1),
        };
        writeln!(
            src,
            "block L{level:05} {{ p: Pin, g: Ground }}\n\n\
             circuit L{level:05} {{\n    {body}\n}}\n"
        )
        .unwrap();
    }
    src
}

/// One circuit, an RC ladder of `n` rungs: rung `i` is a resistor from the net before
/// it (the port `input`, for the first) to its own net, and a capacitor from that net
/// to ground. One placement with `2n` parts and `n + 2` nets; the ground net holds `n`
/// pins. Resistors and capacitors take turns, so the statements aren't in name order
/// and flatten's sorts by name do real work, as on a written design. `wide_circuit(2)`:
///
/// ```text
/// block Ladder { input: Pin, g: Ground }
///
/// circuit Ladder {
///     net n00000;
///     net n00001;
///     let r00000 = Resistor { a: input, b: n00000, value: 1k ± 1% };
///     let c00000 = Capacitor { a: n00000, b: g, value: 1n };
///     let r00001 = Resistor { a: n00000, b: n00001, value: 1k ± 1% };
///     let c00001 = Capacitor { a: n00001, b: g, value: 1n };
/// }
/// ```
pub fn wide_circuit(n: usize) -> String {
    let mut src = String::from("block Ladder { input: Pin, g: Ground }\n\ncircuit Ladder {\n");
    for i in 0..n {
        writeln!(src, "    net n{i:05};").unwrap();
    }
    let mut before = "input".to_string();
    for i in 0..n {
        let net = format!("n{i:05}");
        writeln!(
            src,
            "    let r{i:05} = Resistor {{ a: {before}, b: {net}, value: 1k ± 1% }};\n    \
             let c{i:05} = Capacitor {{ a: {net}, b: g, value: 1n }};"
        )
        .unwrap();
        before = net;
    }
    src.push_str("}\n");
    src
}

/// A block of `n` shorted resistors (both pins on its net `x`), placed twice: `n`
/// flatten problems, each found in both placements and reported once, with the path of
/// the first (model.md E23). Flatten builds a path only to report a problem, so this is
/// the shape that pays for them. The resistor `tie` grounds `x`. `shorted(2)`:
///
/// ```text
/// block Shorted { g: Ground }
///
/// circuit Shorted {
///     net x;
///     let r00000 = Resistor { a: x, b: x, value: 1k };
///     let r00001 = Resistor { a: x, b: x, value: 1k };
///     let tie = Resistor { a: x, b: g, value: 1k };
/// }
///
/// block Board { g: Ground }
///
/// circuit Board {
///     let left = Shorted { g };
///     let right = Shorted { g };
/// }
/// ```
pub fn shorted(n: usize) -> String {
    let mut src = String::from("block Shorted { g: Ground }\n\ncircuit Shorted {\n    net x;\n");
    for i in 0..n {
        writeln!(
            src,
            "    let r{i:05} = Resistor {{ a: x, b: x, value: 1k }};"
        )
        .unwrap();
    }
    src.push_str(
        "    let tie = Resistor { a: x, b: g, value: 1k };\n}\n\n\
         block Board { g: Ground }\n\n\
         circuit Board {\n    let left = Shorted { g };\n    let right = Shorted { g };\n}\n",
    );
    src
}

/// A root `W` of `n` supply inputs and a ground, with no parts, whose contract's
/// default setup gives each input a voltage with a spread: flatten's pass 8 makes a
/// flat setup of `n + 1` ports and `n` Range knobs, and the other passes see one
/// placement on `n + 1` nets. `wide_default_setup(2)`:
///
/// ```text
/// block W { p00000: Power<In>, p00001: Power<In>, gnd: Ground }
///
/// circuit W {}
///
/// setup S for W {
///     p00000: Supply { v: 1V ± 5% },
///     p00001: Supply { v: 1V ± 5% },
///     temp: 25°C,
/// }
///
/// contract W {
///     setup = S;
/// }
/// ```
pub fn wide_default_setup(n: usize) -> String {
    let mut src = String::from("block W { ");
    for i in 0..n {
        write!(src, "p{i:05}: Power<In>, ").unwrap();
    }
    src.push_str("gnd: Ground }\n\ncircuit W {}\n\nsetup S for W {\n");
    for i in 0..n {
        writeln!(src, "    p{i:05}: Supply {{ v: 1V ± 5% }},").unwrap();
    }
    src.push_str("    temp: 25°C,\n}\n\ncontract W {\n    setup = S;\n}\n");
    src
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Problems;
    use spicy_lang::elaborate::elaborate;
    use spicy_lang::parser::parse;
    use spicy_model::flat::KnobKind;

    /// Each root's placements, devices and nets: the size of what flatten builds.
    fn flat_sizes(src: &str) -> Vec<(usize, usize, usize)> {
        let elaborated = elaborate(&parse(src));
        let roots = elaborated.flattened.roots.iter();
        let sizes = roots.map(|root| (root.instances.len(), root.devices.len(), root.nets.len()));
        sizes.collect()
    }

    /// One root of 4 placements, with the one resistor, on the nets `p` and `g`.
    #[test]
    fn a_deep_design_is_one_clean_chain() {
        let src = deep(4);
        assert_eq!(Problems::of(&src), Problems::default(), "{src}");
        assert_eq!(flat_sizes(&src), [(4, 1, 2)]);
    }

    /// One placement of 8 parts, on `input`, `g` and a net per rung.
    #[test]
    fn a_wide_circuit_is_one_clean_placement() {
        let src = wide_circuit(4);
        assert_eq!(Problems::of(&src), Problems::default(), "{src}");
        assert_eq!(flat_sizes(&src), [(1, 8, 6)]);
    }

    /// One problem per shorted part, found in both placements, and nothing else: the
    /// root, its two placements, and their 12 parts on 3 nets (`g`, and each `x`).
    #[test]
    fn every_shorted_part_is_one_problem_in_both_placements() {
        let src = shorted(5);
        let problems = Problems::of(&src);
        assert_eq!(problems.flatten, ["ShortedPart"; 5], "{problems:?}");
        assert_eq!(problems.total(), 5, "{problems:?}");
        for error in elaborate(&parse(&src)).flattened.errors {
            let in_block = error.kind.in_block.expect("inside a placed block");
            assert_eq!((in_block.found, in_block.placements), (2, 2));
        }
        assert_eq!(flat_sizes(&src), [(3, 12, 3)]);
    }

    /// One placement on `n + 1` nets, no parts, and a flat setup with a shape on each
    /// input (none on `gnd`) and a Range knob for each input's voltage.
    #[test]
    fn a_wide_default_setup_is_clean_with_a_knob_per_input() {
        let src = wide_default_setup(4);
        assert_eq!(Problems::of(&src), Problems::default(), "{src}");
        assert_eq!(flat_sizes(&src), [(1, 0, 5)]);
        let elaborated = elaborate(&parse(&src));
        let root = &elaborated.flattened.roots[0];
        let setup = root.setup.as_ref().expect("a flat default setup");
        let shaped: Vec<bool> = setup.ports.iter().map(Option::is_some).collect();
        assert_eq!(shaped, [true, true, true, true, false]);
        let kinds: Vec<KnobKind> = root.knobs.iter().map(|knob| knob.source.kind()).collect();
        assert_eq!(kinds, [KnobKind::Range; 4]);
    }
}
