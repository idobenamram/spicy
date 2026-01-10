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
        let combination = r.stamp.off_diagonals.map(|(pos_neg, neg_pos)| {
            (entry_mapping.get(pos_neg), entry_mapping.get(neg_pos))
        });
        r.stamp.finialize(pos_pos, neg_neg, combination);
    }
}

fn finialize_capacitors(capacitors: &mut [Capacitor], entry_mapping: &EntryMapping) {
    for c in capacitors {
        let pos_pos = c.stamp.pos_pos.map(|i| entry_mapping.get(i));

        let neg_neg = c.stamp.neg_neg.map(|i| entry_mapping.get(i));

        let combination = c.stamp.off_diagonals.map(|(pos_neg, neg_pos)| {
            (entry_mapping.get(pos_neg), entry_mapping.get(neg_pos))
        });

        c.stamp.finialize(pos_pos, neg_neg, combination);
    }
}

fn finialize_inductors(inductors: &mut [Inductor], entry_mapping: &EntryMapping) {
    for ind in inductors {
        let pos_branch =
            ind.stamp.pos_branch.map(|(pos_branch, branch_pos)| {
                    (entry_mapping.get(pos_branch), entry_mapping.get(branch_pos))
                });

        let neg_branch =
            ind.stamp.neg_branch.map(|(neg_branch, branch_neg)| {
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
        let pos_branch =
            v.stamp.pos_branch.map(|(pos_branch, branch_pos)| {
                    (entry_mapping.get(pos_branch), entry_mapping.get(branch_pos))
                });

        let neg_branch =
            v.stamp.neg_branch.map(|(neg_branch, branch_neg)| {
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
