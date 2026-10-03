//! The typical design: copies of the canonical example, `circuits/ce_amp.spl`.

use std::fmt::Write;

use spicy_lang::parser::ast::ItemKind;
use spicy_lang::parser::parse;

/// The canonical example (walkthrough §1): an env, a block, its circuit, a setup and a
/// contract.
pub const CE_AMP: &str = include_str!("../../../circuits/ce_amp.spl");

/// Whether the copies keep `ce_amp.spl`'s env, setup and contract.
#[derive(Clone, Copy, Debug)]
pub enum Contracts {
    With,
    Without,
}

/// A board: `n` copies of `ce_amp.spl`, renamed apart (`CeAmp00000`, `ambient00000`,
/// `Operating00000`, …), and a `Board` that places each copy once, in a chain (each
/// copy's output is the next one's input). One root, `n` placements, `n` definitions.
pub fn board(n: usize, contracts: Contracts) -> String {
    let mut src = library(n, contracts);
    src.push_str(
        "block Board { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }\n\n\
         circuit Board {\n",
    );
    for i in 1..n {
        writeln!(src, "    net s{i:05};").unwrap();
    }
    for i in 0..n {
        let input = if i == 0 {
            "input".to_string()
        } else {
            format!("input: s{i:05}")
        };
        let output = match i + 1 {
            last if last == n => "output".to_string(),
            next => format!("output: s{next:05}"),
        };
        writeln!(
            src,
            "    let a{i:05} = CeAmp{i:05} {{ vcc, gnd, {input}, {output} }};"
        )
        .unwrap();
    }
    src.push_str("}\n");
    src
}

/// A library: the `n` renamed copies alone, so each is a root of its own (the shape of
/// the earlier `contracts_10m.spl`).
pub fn library(n: usize, contracts: Contracts) -> String {
    let one = items(contracts);
    let mut src = String::with_capacity(n * (one.len() + 40));
    for i in 0..n {
        let suffix = format!("{i:05}");
        let copy = one
            .replace("ambient", &format!("ambient{suffix}"))
            .replace("CeAmp", &format!("CeAmp{suffix}"))
            .replace("Operating", &format!("Operating{suffix}"));
        src.push_str(&copy);
    }
    src
}

/// `ce_amp.spl`'s items, cut out by the parser's spans (doc comments included), so the
/// example can change without this changing; only the block and circuit for
/// [`Contracts::Without`].
fn items(contracts: Contracts) -> String {
    let parsed = parse(CE_AMP);
    assert!(!parsed.has_errors(), "ce_amp.spl parses");
    let keep = |kind: &ItemKind| match contracts {
        Contracts::With => true,
        Contracts::Without => matches!(kind, ItemKind::Block(_) | ItemKind::Circuit(_)),
    };
    let mut out = String::new();
    for item in parsed.file.items.iter().filter(|item| keep(&item.kind)) {
        out.push_str(&CE_AMP[item.span.range()]);
        out.push_str("\n\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Problems;
    use spicy_lang::elaborate::elaborate;
    use spicy_lang::resolve::resolve;
    use spicy_model::design::Design;

    #[test]
    fn a_board_is_clean_with_one_root() {
        for contracts in [Contracts::With, Contracts::Without] {
            let src = board(3, contracts);
            assert_eq!(Problems::of(&src), Problems::default(), "{src}");
            let elaborated = elaborate(&parse(&src));
            assert_eq!(elaborated.flattened.roots.len(), 1);
            assert_eq!(elaborated.resolved.design.blocks.len(), 4);
        }
    }

    #[test]
    fn a_library_is_clean_with_a_root_per_copy() {
        let src = library(3, Contracts::With);
        assert_eq!(Problems::of(&src), Problems::default(), "{src}");
        assert_eq!(elaborate(&parse(&src)).flattened.roots.len(), 3);
    }

    #[test]
    fn without_contracts_keeps_only_blocks_and_circuits() {
        let kinds: Vec<_> = parse(&items(Contracts::Without))
            .file
            .items
            .iter()
            .map(|item| matches!(item.kind, ItemKind::Block(_) | ItemKind::Circuit(_)))
            .collect();
        assert_eq!(kinds, [true, true]);
    }

    /// The typical pair (`flatten_design`'s two inputs, `resolve_file`'s too) has the
    /// same blocks, and differs only in the env, setup and contract of each copy.
    /// Flatten reads a setup only on a root, and the board's root has none (the copies
    /// are placed), so it does the same work on both.
    #[test]
    fn the_typical_pair_differs_only_in_env_setup_and_contract() {
        let with = resolve(&parse(&board(3, Contracts::With))).design;
        let without = resolve(&parse(&board(3, Contracts::Without))).design;
        assert_eq!(with.blocks, without.blocks);
        let extras = |d: &Design| {
            let contracts = d.contracts.iter().flatten().count();
            (d.envs.len(), d.setups.len(), contracts, d.consts.len())
        };
        assert_eq!(extras(&with), (3, 3, 3, 0));
        assert_eq!(extras(&without), (0, 0, 0, 0));
    }
}
