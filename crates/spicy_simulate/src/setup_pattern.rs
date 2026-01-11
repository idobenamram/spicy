use crate::{
    devices::{Capacitor, Devices, IndependentSource, Inductor, Resistor},
    error::SimulationError,
    solver::matrix::{builder::EntryMapping, csc::CscMatrix},
};
use spicy_parser::node_mapping::NodeMapping;

use crate::solver::matrix::builder::MatrixBuilder;

fn setup_resistors(
    resistors: &mut [Resistor],
    node_mapping: &NodeMapping,
    builder: &mut MatrixBuilder,
) -> Result<(), SimulationError> {
    for r in resistors {
        let pos = node_mapping.mna_node_index(r.positive);
        let neg = node_mapping.mna_node_index(r.negative);
        let pos_pos = pos.map(|p| builder.push(p, p, 0.0)).transpose()?;
        let neg_neg = neg.map(|n| builder.push(n, n, 0.0)).transpose()?;
        let combination = if let (Some(pos), Some(neg)) = (pos, neg) {
            Some((builder.push(pos, neg, 0.0)?, builder.push(neg, pos, 0.0)?))
        } else {
            None
        };
        r.stamp.temp_entries(pos_pos, neg_neg, combination);
    }
    Ok(())
}

fn setup_capacitors(
    capacitors: &mut [Capacitor],
    node_mapping: &NodeMapping,
    builder: &mut MatrixBuilder,
) -> Result<(), SimulationError> {
    for c in capacitors {
        let pos = node_mapping.mna_node_index(c.positive);
        let neg = node_mapping.mna_node_index(c.negative);
        let pos_pos = pos.map(|p| builder.push(p, p, 0.0)).transpose()?;
        let neg_neg = neg.map(|n| builder.push(n, n, 0.0)).transpose()?;
        let combination = if let (Some(pos), Some(neg)) = (pos, neg) {
            Some((builder.push(pos, neg, 0.0)?, builder.push(neg, pos, 0.0)?))
        } else {
            None
        };
        c.stamp.temp_entries(pos_pos, neg_neg, combination);
    }
    Ok(())
}

fn setup_inductors(
    inductors: &mut [Inductor],
    node_mapping: &NodeMapping,
    builder: &mut MatrixBuilder,
) -> Result<(), SimulationError> {
    for i in inductors {
        let pos = node_mapping.mna_node_index(i.positive);
        let neg = node_mapping.mna_node_index(i.negative);
        let branch_index = node_mapping.mna_branch_index(i.current_branch);

        let pos_branch = if let Some(pos) = pos {
            Some((
                builder.push(pos, branch_index, 0.0)?,
                builder.push(branch_index, pos, 0.0)?,
            ))
        } else {
            None
        };

        let neg_branch = if let Some(neg) = neg {
            Some((
                builder.push(neg, branch_index, 0.0)?,
                builder.push(branch_index, neg, 0.0)?,
            ))
        } else {
            None
        };

        let branch_branch = builder.push(branch_index, branch_index, 0.0)?;
        i.stamp.temp_entries(pos_branch, neg_branch, branch_branch);
    }
    Ok(())
}

fn setup_voltage_sources(
    voltage_sources: &mut [IndependentSource],
    node_mapping: &NodeMapping,
    builder: &mut MatrixBuilder,
) -> Result<(), SimulationError> {
    for v in voltage_sources {
        let pos = node_mapping.mna_node_index(v.positive);
        let neg = node_mapping.mna_node_index(v.negative);
        let branch_index = node_mapping.mna_branch_index(v.current_branch);

        let pos_branch = if let Some(pos) = pos {
            Some((
                builder.push(pos, branch_index, 0.0)?,
                builder.push(branch_index, pos, 0.0)?,
            ))
        } else {
            None
        };

        let neg_branch = if let Some(neg) = neg {
            Some((
                builder.push(neg, branch_index, 0.0)?,
                builder.push(branch_index, neg, 0.0)?,
            ))
        } else {
            None
        };

        v.stamp.temp_entries(pos_branch, neg_branch);
    }
    Ok(())
}

fn finialize_resistors(resistors: &mut [Resistor], entry_mapping: &EntryMapping) {
    for r in resistors {
        let pos_pos = r.stamp.pos_pos.map(|i| entry_mapping.get(i));
        let neg_neg = r.stamp.neg_neg.map(|i| entry_mapping.get(i));
        let combination = r
            .stamp
            .off_diagonals
            .map(|(pos_neg, neg_pos)| (entry_mapping.get(pos_neg), entry_mapping.get(neg_pos)));
        r.stamp.finialize(pos_pos, neg_neg, combination);
    }
}

fn finialize_capacitors(capacitors: &mut [Capacitor], entry_mapping: &EntryMapping) {
    for c in capacitors {
        let pos_pos = c.stamp.pos_pos.map(|i| entry_mapping.get(i));

        let neg_neg = c.stamp.neg_neg.map(|i| entry_mapping.get(i));

        let combination = c
            .stamp
            .off_diagonals
            .map(|(pos_neg, neg_pos)| (entry_mapping.get(pos_neg), entry_mapping.get(neg_pos)));

        c.stamp.finialize(pos_pos, neg_neg, combination);
    }
}

fn finialize_inductors(inductors: &mut [Inductor], entry_mapping: &EntryMapping) {
    for ind in inductors {
        let pos_branch = ind.stamp.pos_branch.map(|(pos_branch, branch_pos)| {
            (entry_mapping.get(pos_branch), entry_mapping.get(branch_pos))
        });

        let neg_branch = ind.stamp.neg_branch.map(|(neg_branch, branch_neg)| {
            (entry_mapping.get(neg_branch), entry_mapping.get(branch_neg))
        });

        let branch_branch = entry_mapping.get(ind.stamp.branch_branch);

        ind.stamp.finialize(pos_branch, neg_branch, branch_branch);
    }
}

fn finialize_voltage_sources(
    voltage_sources: &mut [IndependentSource],
    entry_mapping: &EntryMapping,
) {
    for v in voltage_sources {
        let pos_branch = v.stamp.pos_branch.map(|(pos_branch, branch_pos)| {
            (entry_mapping.get(pos_branch), entry_mapping.get(branch_pos))
        });

        let neg_branch = v.stamp.neg_branch.map(|(neg_branch, branch_neg)| {
            (entry_mapping.get(neg_branch), entry_mapping.get(branch_neg))
        });

        v.stamp.finialize(pos_branch, neg_branch);
    }
}

pub fn setup_pattern(
    devices: &mut Devices,
    node_mapping: &NodeMapping,
) -> Result<CscMatrix, SimulationError> {
    let matrix_dim = node_mapping.mna_matrix_dim();
    let mut builder = MatrixBuilder::new(matrix_dim, matrix_dim);

    setup_resistors(&mut devices.resistors, node_mapping, &mut builder)?;
    setup_capacitors(&mut devices.capacitors, node_mapping, &mut builder)?;
    setup_inductors(&mut devices.inductors, node_mapping, &mut builder)?;
    setup_voltage_sources(&mut devices.voltage_sources, node_mapping, &mut builder)?;
    // setup_current_sources(&devices.current_sources, node_mapping, &mut builder)?;

    let (matrix, mapping) = builder.build_csc_pattern()?;

    finialize_resistors(&mut devices.resistors, &mapping);
    finialize_capacitors(&mut devices.capacitors, &mapping);
    finialize_inductors(&mut devices.inductors, &mapping);
    finialize_voltage_sources(&mut devices.voltage_sources, &mapping);

    Ok(matrix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::Devices as SimDevices;
    use spicy_parser::{ParseOptions, SourceMap, parse};
    use std::path::PathBuf;

    fn parse_inline_deck(netlist: &str) -> spicy_parser::instance_parser::Deck {
        let source_map = SourceMap::new(
            PathBuf::from("inline_setup_pattern.spicy"),
            netlist.to_string(),
        );
        let mut options = ParseOptions {
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            source_map,
            max_include_depth: 10,
        };
        parse(&mut options).expect("parse")
    }

    /// Return the CSC nnz index for a given (col, row) coordinate.
    ///
    /// This is fast and deterministic because row indices are strictly increasing within each column.
    fn nnz_at(matrix: &CscMatrix, col: usize, row: usize) -> usize {
        let (rows, _vals) = matrix.col(col);
        let k = rows
            .binary_search(&row)
            .unwrap_or_else(|_| panic!("missing sparsity-pattern entry at (row={row}, col={col})"));
        matrix.col_start(col) + k
    }

    #[test]
    fn setup_pattern_populates_device_stamps_with_final_nnz_indices() {
        let deck = parse_inline_deck(
            r#"setup_pattern stamp test
V1 in 0 1
R1 in out 2k
R2 out 0 3k
L1 out 0 1m
.op
.end
"#,
        );

        let mut sim_devices = SimDevices::from_spec(&deck.devices);
        let matrix =
            super::setup_pattern(&mut sim_devices, &deck.node_mapping).expect("setup_pattern");
        debug_assert!(matrix.check_invariants().is_ok());

        let dim = deck.node_mapping.mna_matrix_dim();
        assert_eq!(matrix.dim.nrows, dim);
        assert_eq!(matrix.dim.ncols, dim);

        let r1 = sim_devices
            .resistors
            .iter()
            .find(|r| r.name == "R1")
            .expect("R1");
        let r2 = sim_devices
            .resistors
            .iter()
            .find(|r| r.name == "R2")
            .expect("R2");
        let v1 = sim_devices
            .voltage_sources
            .iter()
            .find(|v| v.name == "V1")
            .expect("V1");
        let l1 = sim_devices
            .inductors
            .iter()
            .find(|l| l.name == "L1")
            .expect("L1");

        // Node-voltage unknown indices (ground excluded).
        let in_mna = deck
            .node_mapping
            .mna_node_index(r1.positive)
            .expect("in is non-ground");
        let out_mna = deck
            .node_mapping
            .mna_node_index(r1.negative)
            .expect("out is non-ground");

        // --- R1: between two non-ground nodes => full stamp (diag + off-diagonals).
        let r1_pos_pos = r1.stamp.pos_pos.expect("R1 pos_pos");
        let r1_neg_neg = r1.stamp.neg_neg.expect("R1 neg_neg");
        let (r1_pos_neg, r1_neg_pos) = r1.stamp.off_diagonals.expect("R1 off-diagonals");
        assert_eq!(r1_pos_pos, nnz_at(&matrix, in_mna, in_mna));
        assert_eq!(r1_neg_neg, nnz_at(&matrix, out_mna, out_mna));
        // Note: stamp index pairs are ordered and follow `MatrixBuilder::push(column, row, ..)`.
        assert_eq!(r1_pos_neg, nnz_at(&matrix, in_mna, out_mna));
        assert_eq!(r1_neg_pos, nnz_at(&matrix, out_mna, in_mna));

        // --- R2: to ground => diagonal only.
        let r2_pos_pos = r2.stamp.pos_pos.expect("R2 pos_pos");
        assert_eq!(r2.stamp.neg_neg, None);
        assert_eq!(r2.stamp.off_diagonals, None);
        assert_eq!(r2_pos_pos, nnz_at(&matrix, out_mna, out_mna));
        // Shares the (out,out) diagonal with R1.
        assert_eq!(r2_pos_pos, r1_neg_neg);

        // --- V1: to ground => only pos/branch entries.
        let (v1_pos_branch, v1_branch_pos) = v1.stamp.pos_branch.expect("V1 pos_branch");
        assert_eq!(v1.stamp.neg_branch, None);
        let v1_pos = deck
            .node_mapping
            .mna_node_index(v1.positive)
            .expect("V1 pos");
        let v1_branch = deck.node_mapping.mna_branch_index(v1.current_branch);
        assert_eq!(v1_pos_branch, nnz_at(&matrix, v1_pos, v1_branch));
        assert_eq!(v1_branch_pos, nnz_at(&matrix, v1_branch, v1_pos));

        // --- L1: to ground => pos/branch entries plus branch-branch entry.
        let (l1_pos_branch, l1_branch_pos) = l1.stamp.pos_branch.expect("L1 pos_branch");
        assert_eq!(l1.stamp.neg_branch, None);
        assert_ne!(l1.stamp.branch_branch, usize::MAX);
        let l1_pos = deck
            .node_mapping
            .mna_node_index(l1.positive)
            .expect("L1 pos");
        let l1_branch = deck.node_mapping.mna_branch_index(l1.current_branch);
        assert_eq!(l1_pos_branch, nnz_at(&matrix, l1_pos, l1_branch));
        assert_eq!(l1_branch_pos, nnz_at(&matrix, l1_branch, l1_pos));
        assert_eq!(
            l1.stamp.branch_branch,
            nnz_at(&matrix, l1_branch, l1_branch)
        );
    }
}
