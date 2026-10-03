//! The names a written netlist uses (docs/ecad/netlist_writer.md#decisions).

use std::collections::HashSet;

use spicy_circuit::{CircuitNames, Lowered};

use super::WriteError;

/// Every name the netlist uses, indexed like the circuit, plus the cards the
/// writer makes up for resistor, capacitor and inductor models: `None` for a
/// model equal to the default one, which needs no card.
pub(super) struct SpiceNames {
    pub names: CircuitNames,
    pub resistor_models: Vec<Option<String>>,
    pub capacitor_models: Vec<Option<String>>,
    pub inductor_models: Vec<Option<String>>,
}

pub(super) fn spice_names(lowered: &Lowered, title: String) -> Result<SpiceNames, WriteError> {
    let source = &lowered.names;
    let params = &lowered.params;

    let mut elements = Namespace::default();
    let mut devices = |letter: char, names: &[String]| -> Result<Vec<String>, WriteError> {
        names
            .iter()
            .map(|name| elements.claim(&element_name(letter, name)))
            .collect()
    };
    let resistors = devices('R', &source.resistors)?;
    let capacitors = devices('C', &source.capacitors)?;
    let inductors = devices('L', &source.inductors)?;
    let diodes = devices('D', &source.diodes)?;
    let bjts = devices('Q', &source.bjts)?;
    let vsources = devices('V', &source.vsources)?;
    let isources = devices('I', &source.isources)?;

    // One namespace for every card: the cards the source named keep their
    // names, and the writer's own cards take what's left.
    let mut models = Namespace::default();
    let diode_models = card_names(
        &mut models,
        &source.diode_models,
        params.diode_models.len(),
        "DM",
    )?;
    let bjt_models = card_names(
        &mut models,
        &source.bjt_models,
        params.bjt_models.len(),
        "QM",
    )?;
    let resistor_models = generated_cards(&mut models, &params.resistor_models, "RM")?;
    let capacitor_models = generated_cards(&mut models, &params.capacitor_models, "CM")?;
    let inductor_models = generated_cards(&mut models, &params.inductor_models, "LM")?;

    Ok(SpiceNames {
        names: CircuitNames {
            title,
            nodes: node_names(&source.nodes)?,
            resistors,
            capacitors,
            inductors,
            diodes,
            bjts,
            vsources,
            isources,
            diode_models,
            bjt_models,
        },
        resistor_models,
        capacitor_models,
        inductor_models,
    })
}

/// A device's name in SPICE, where the first letter is the kind: its own
/// name when that already starts with the kind's letter (`R1`), else the
/// letter and a dot in front (`X1.R1` → `R.X1.R1`), as ngspice names
/// flattened devices (`subckt.c:1131-1143`).
fn element_name(letter: char, name: &str) -> String {
    if name.starts_with(|c: char| c.eq_ignore_ascii_case(&letter)) {
        name.to_string()
    } else {
        format!("{letter}.{name}")
    }
}

/// Node names as they are, with node 0 written `0`. `gnd` is taken too:
/// ngspice turns a node named `gnd` into ground (checked on ngspice-42).
fn node_names(nodes: &[String]) -> Result<Vec<String>, WriteError> {
    let mut namespace = Namespace::default();
    namespace.claim("gnd")?;
    let ground = namespace.claim("0")?;
    let others = nodes.iter().skip(1).map(|name| namespace.claim(name));
    std::iter::once(Ok(ground)).chain(others).collect()
}

/// The names of the cards the source named, one per model; a model without
/// one gets `<prefix><index>`.
fn card_names(
    namespace: &mut Namespace,
    names: &[String],
    count: usize,
    prefix: &str,
) -> Result<Vec<String>, WriteError> {
    (0..count)
        .map(|i| match names.get(i) {
            Some(name) => namespace.claim(name),
            None => namespace.claim(&format!("{prefix}{i}")),
        })
        .collect()
}

/// A card named `<prefix><index>` for each model that differs from the
/// default one.
fn generated_cards<M: Default + PartialEq>(
    namespace: &mut Namespace,
    models: &[M],
    prefix: &str,
) -> Result<Vec<Option<String>>, WriteError> {
    models
        .iter()
        .enumerate()
        .map(|(i, model)| {
            if *model == M::default() {
                Ok(None)
            } else {
                namespace.claim(&format!("{prefix}{i}")).map(Some)
            }
        })
        .collect()
}

/// Names handed out so far. SPICE compares names ignoring case, so two names
/// that differ only in case are the same name.
#[derive(Default)]
struct Namespace {
    taken: HashSet<String>,
}

impl Namespace {
    /// `name` when no name taken so far equals it ignoring case, else the
    /// first free `name_2`, `name_3`, …
    fn claim(&mut self, name: &str) -> Result<String, WriteError> {
        if !readable(name) {
            return Err(WriteError::UnreadableName {
                name: name.to_string(),
            });
        }
        let mut candidate = name.to_string();
        let mut n = 1;
        while !self.taken.insert(candidate.to_ascii_lowercase()) {
            n += 1;
            candidate = format!("{name}_{n}");
        }
        Ok(candidate)
    }
}

/// Whether the reader reads `name` back as one name: digits (a node like `1`),
/// or a letter followed by letters, digits, `_` and `.`.
fn readable(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_alphabetic() => {
            chars.all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        }
        Some(_) => name.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_keep_their_names_or_get_the_kind_letter() {
        assert_eq!(element_name('R', "R1"), "R1");
        assert_eq!(element_name('R', "rload"), "rload");
        assert_eq!(element_name('R', "X1.R1"), "R.X1.R1");
        assert_eq!(element_name('R', "amp.r1"), "R.amp.r1");
        assert_eq!(element_name('Q', "q1"), "q1");
        assert_eq!(element_name('V', "input"), "V.input");
    }

    #[test]
    fn a_name_equal_ignoring_case_gets_a_suffix() {
        let mut namespace = Namespace::default();
        let claimed: Vec<String> = ["R.amp.r1", "R.amp.R1", "r.AMP.r1", "R.amp.r1_2"]
            .iter()
            .map(|name| namespace.claim(name).unwrap())
            .collect();
        assert_eq!(
            claimed,
            ["R.amp.r1", "R.amp.R1_2", "r.AMP.r1_3", "R.amp.r1_2_2"]
        );
    }

    #[test]
    fn a_node_named_gnd_is_renamed() {
        let nodes = ["0", "in", "GND", "out"].map(String::from);
        assert_eq!(node_names(&nodes).unwrap(), ["0", "in", "GND_2", "out"]);
    }

    #[test]
    fn names_the_reader_cant_read_are_refused() {
        let mut namespace = Namespace::default();
        for name in ["", "1a", "a-b", "a b", "/VCC"] {
            assert!(namespace.claim(name).is_err(), "{name:?}");
        }
        for name in ["1", "in", "X1.mid", "Q2N3904_ON"] {
            assert!(namespace.claim(name).is_ok(), "{name:?}");
        }
    }
}
