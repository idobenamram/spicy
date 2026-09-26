use crate::{
    devices::{Bjt, Capacitor, Devices, Diode, Inductor, Resistor, VoltageSource},
    error::SimulationError,
    solver::matrix::csc::CscMatrix,
};

use crate::solver::matrix::builder::MatrixBuilder;

fn dense_index(row: usize, col: usize, dim: usize) -> usize {
    row.checked_mul(dim)
        .and_then(|x| x.checked_add(col))
        .expect("overflow computing dense index")
}

fn setup_resistors(
    resistors: &mut [Resistor],
    builder: &mut MatrixBuilder,
) -> Result<(), SimulationError> {
    for r in resistors {
        let pos = r.positive;
        let neg = r.negative;
        r.stamp
            .set_temp_indices_from_nodes(pos, neg, |col, row| builder.push(col, row, 0.0))?;
    }
    Ok(())
}

fn setup_capacitors(
    capacitors: &mut [Capacitor],
    builder: &mut MatrixBuilder,
) -> Result<(), SimulationError> {
    for c in capacitors {
        let pos = c.positive;
        let neg = c.negative;
        c.stamp
            .set_temp_indices_from_nodes(pos, neg, |col, row| builder.push(col, row, 0.0))?;
    }
    Ok(())
}

fn setup_diodes(diodes: &mut [Diode], builder: &mut MatrixBuilder) -> Result<(), SimulationError> {
    for d in diodes {
        let pos = d.positive;
        let neg = d.negative;
        d.stamp
            .set_temp_indices_from_nodes(pos, neg, |col, row| builder.push(col, row, 0.0))?;
    }
    Ok(())
}

fn setup_bjts(bjts: &mut [Bjt], builder: &mut MatrixBuilder) -> Result<(), SimulationError> {
    for bjt in bjts {
        let b = bjt.base;
        let c = bjt.collector;
        let e = bjt.emitter;
        bjt.stamp
            .set_temp_indices_from_nodes(b, c, e, |row, col| builder.push(col, row, 0.0))?;
    }
    Ok(())
}

fn setup_inductors(
    inductors: &mut [Inductor],
    builder: &mut MatrixBuilder,
) -> Result<(), SimulationError> {
    for i in inductors {
        let pos = i.positive;
        let neg = i.negative;
        let branch_index = i.branch;
        i.stamp
            .set_temp_indices_from_nodes(pos, neg, branch_index, |col, row| {
                builder.push(col, row, 0.0)
            })?;
    }
    Ok(())
}

fn setup_voltage_sources(
    voltage_sources: &mut [VoltageSource],
    builder: &mut MatrixBuilder,
) -> Result<(), SimulationError> {
    for v in voltage_sources {
        let pos = v.positive;
        let neg = v.negative;
        let branch_index = v.branch;
        v.stamp
            .set_temp_indices_from_nodes(pos, neg, branch_index, |col, row| {
                builder.push(col, row, 0.0)
            })?;
    }
    Ok(())
}

pub fn setup_pattern(
    devices: &mut Devices,
    matrix_dim: usize,
) -> Result<CscMatrix, SimulationError> {
    let mut builder = MatrixBuilder::new(matrix_dim, matrix_dim);

    setup_resistors(&mut devices.resistors, &mut builder)?;
    setup_capacitors(&mut devices.capacitors, &mut builder)?;
    setup_inductors(&mut devices.inductors, &mut builder)?;
    setup_diodes(&mut devices.diodes, &mut builder)?;
    setup_bjts(&mut devices.bjts, &mut builder)?;
    setup_voltage_sources(&mut devices.voltage_sources, &mut builder)?;
    // we do not need to setup current sources as they don't effect the matrix structure (only the right hand side)

    let (matrix, mapping) = builder.build_csc_pattern()?;

    for r in &mut devices.resistors {
        r.stamp.set_final_indices(|i| mapping.get(i));
    }
    for c in &mut devices.capacitors {
        c.stamp.set_final_indices(|i| mapping.get(i));
    }
    for ind in &mut devices.inductors {
        ind.stamp.set_final_indices(|i| mapping.get(i));
    }
    for d in &mut devices.diodes {
        d.stamp.set_final_indices(|i| mapping.get(i));
    }
    for bjt in &mut devices.bjts {
        bjt.stamp.set_final_indices(|i| mapping.get(i));
    }
    for v in &mut devices.voltage_sources {
        v.stamp.set_final_indices(|i| mapping.get(i));
    }

    Ok(matrix)
}

/// Initialize device stamp indices for a dense (BLAS) matrix.
///
/// For BLAS we store a *dense linear index* into the MNA matrix buffer in each stamp field:
/// `idx = row * dim + col` (row-major). This avoids building a sparse CSC pattern just to
/// compute per-device stamp locations.
pub fn setup_dense_stamps(devices: &mut Devices, dim: usize) {
    for r in &mut devices.resistors {
        let pos = r.positive;
        let neg = r.negative;
        let pos_pos = pos.map(|p| dense_index(p, p, dim));
        let neg_neg = neg.map(|n| dense_index(n, n, dim));
        let off = if let (Some(p), Some(n)) = (pos, neg) {
            Some((dense_index(p, n, dim), dense_index(n, p, dim)))
        } else {
            None
        };
        r.stamp.set_temp_indices(pos_pos, neg_neg, off);
    }

    for c in &mut devices.capacitors {
        let pos = c.positive;
        let neg = c.negative;
        let pos_pos = pos.map(|p| dense_index(p, p, dim));
        let neg_neg = neg.map(|n| dense_index(n, n, dim));
        let off = if let (Some(p), Some(n)) = (pos, neg) {
            Some((dense_index(p, n, dim), dense_index(n, p, dim)))
        } else {
            None
        };
        c.stamp.set_temp_indices(pos_pos, neg_neg, off);
    }

    for d in &mut devices.diodes {
        let pos = d.positive;
        let neg = d.negative;
        let pos_pos = pos.map(|p| dense_index(p, p, dim));
        let neg_neg = neg.map(|n| dense_index(n, n, dim));
        let off = if let (Some(p), Some(n)) = (pos, neg) {
            Some((dense_index(p, n, dim), dense_index(n, p, dim)))
        } else {
            None
        };
        d.stamp.set_temp_indices(pos_pos, neg_neg, off);
    }

    for bjt in &mut devices.bjts {
        let b = bjt.base;
        let c = bjt.collector;
        let e = bjt.emitter;

        let dense_entry = |row: Option<usize>, col: Option<usize>| match (row, col) {
            (Some(r), Some(c)) => Some(dense_index(r, c, dim)),
            _ => None,
        };

        let bb = dense_entry(b, b);
        let bc = dense_entry(b, c);
        let be = dense_entry(b, e);
        let cb = dense_entry(c, b);
        let cc = dense_entry(c, c);
        let ce = dense_entry(c, e);
        let eb = dense_entry(e, b);
        let ec = dense_entry(e, c);
        let ee = dense_entry(e, e);

        bjt.stamp
            .set_temp_indices(bb, bc, be, cb, cc, ce, eb, ec, ee);
    }

    for ind in &mut devices.inductors {
        let pos = ind.positive;
        let neg = ind.negative;
        let b = ind.branch;
        let pos_branch = pos.map(|p| (dense_index(p, b, dim), dense_index(b, p, dim)));
        let neg_branch = neg.map(|n| (dense_index(n, b, dim), dense_index(b, n, dim)));
        let bb = dense_index(b, b, dim);
        ind.stamp.set_temp_indices(pos_branch, neg_branch, bb);
    }

    for v in &mut devices.voltage_sources {
        let pos = v.positive;
        let neg = v.negative;
        let b = v.branch;
        let pos_branch = pos.map(|p| (dense_index(p, b, dim), dense_index(b, p, dim)));
        let neg_branch = neg.map(|n| (dense_index(n, b, dim), dense_index(b, n, dim)));
        v.stamp.set_temp_indices(pos_branch, neg_branch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::build_devices;

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
        let mut devices = build_devices(
            r#"setup_pattern stamp test
V1 in 0 1
R1 in out 2k
R2 out 0 3k
L1 out 0 1m
.op
.end
"#,
        );

        // Nodes `in` and `out`, plus the currents of V1 and L1.
        let dim = 4;
        let matrix = super::setup_pattern(&mut devices, dim).expect("setup_pattern");
        debug_assert!(matrix.check_invariants().is_ok());
        assert_eq!(matrix.dim.nrows, dim);
        assert_eq!(matrix.dim.ncols, dim);

        let [r1, r2] = &devices.resistors[..] else {
            panic!("expected R1 and R2");
        };
        let v1 = &devices.voltage_sources[0];
        let l1 = &devices.inductors[0];

        // Node-voltage unknown indices (ground excluded).
        let in_mna = r1.positive.expect("in is non-ground");
        let out_mna = r1.negative.expect("out is non-ground");

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
        let v1_pos = v1.positive.expect("V1 pos");
        let v1_branch = v1.branch;
        assert_eq!(v1_pos_branch, nnz_at(&matrix, v1_pos, v1_branch));
        assert_eq!(v1_branch_pos, nnz_at(&matrix, v1_branch, v1_pos));

        // --- L1: to ground => pos/branch entries plus branch-branch entry.
        let (l1_pos_branch, l1_branch_pos) = l1.stamp.pos_branch.expect("L1 pos_branch");
        assert_eq!(l1.stamp.neg_branch, None);
        assert_ne!(l1.stamp.branch_branch, usize::MAX);
        let l1_pos = l1.positive.expect("L1 pos");
        let l1_branch = l1.branch;
        assert_eq!(l1_pos_branch, nnz_at(&matrix, l1_pos, l1_branch));
        assert_eq!(l1_branch_pos, nnz_at(&matrix, l1_branch, l1_pos));
        assert_eq!(
            l1.stamp.branch_branch,
            nnz_at(&matrix, l1_branch, l1_branch)
        );
    }
}
