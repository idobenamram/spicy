//! Non-interactive mode: run every analysis in the netlist, print AC results,
//! and write raw files when asked.

use std::f64::consts::PI;
use std::io::Write;
use std::path::Path;

use spicy_circuit::{Analysis, Lowered};
use spicy_simulate::{
    AcResult, SimulationConfig, SimulationError, Unknown,
    ac::simulate_ac,
    dc::{simulate_dc, simulate_op},
    trans::simulate_trans,
    unknowns,
};

use crate::raw;

/// Where raw files go: `<dir>/<stem>.raw`.
pub struct RawOutput<'a> {
    pub dir: &'a Path,
    pub stem: &'a str,
}

/// Run the netlist's analyses in order, printing AC results to `out`.
///
/// With `raw`, each result is also written to the raw file. Every analysis
/// writes to the same file, so only the last one is kept (a known issue, see
/// docs/ecad/pipeline.md §11).
pub fn run(
    lowered: &Lowered,
    config: &SimulationConfig,
    out: &mut impl Write,
    raw: Option<&RawOutput>,
) -> Result<(), SimulationError> {
    let (circuit, params) = (&lowered.circuit, &lowered.params);
    let raw_path = raw.map(|raw| raw::raw_file_path(raw.dir, raw.stem));
    for analysis in &lowered.analyses {
        match analysis {
            Analysis::Op => {
                let op = simulate_op(circuit, params, config)?;
                if let Some(path) = &raw_path {
                    let _ = raw::write_operating_point_raw(lowered, &op, path);
                }
            }
            Analysis::Dc(sweep) => {
                let dc = simulate_dc(circuit, params, sweep, config);
                if let Some(path) = &raw_path {
                    let _ = raw::write_dc_raw(lowered, &dc, sweep.source, path);
                }
            }
            Analysis::Ac(sweep) => {
                let ac = simulate_ac(circuit, params, sweep, config);
                print_ac(out, lowered, &ac);
                if let Some(path) = &raw_path {
                    let _ = raw::write_ac_raw(lowered, &ac, path);
                }
            }
            Analysis::Tran(tran) => {
                let tran = simulate_trans(circuit, params, tran, config)?;
                if let Some(path) = &raw_path {
                    let _ = raw::write_transient_raw(lowered, &tran, path);
                }
            }
        }
    }
    Ok(())
}

/// Print every node's phasor at every frequency. Panics if `out` can't be
/// written, like `println!`.
fn print_ac(out: &mut impl Write, lowered: &Lowered, ac: &AcResult) {
    let unknowns = unknowns(&lowered.circuit);
    for (f, xr, xi) in ac {
        for (i, unknown) in unknowns.iter().enumerate() {
            if !matches!(unknown, Unknown::Voltage(_)) {
                continue;
            }
            let (vr, vi) = (xr[i], xi[i]);
            let mag = (vr * vr + vi * vi).sqrt();
            let phase = vi.atan2(vr) * 180.0 / PI;
            let name = unknown.name(&lowered.names);
            writeln!(out, "f={:.6} Hz  {}: {:.6} ∠ {:.3}°", f, name, mag, phase)
                .expect("write AC results");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{RawFile, TestDir, lower_netlist, simulate_ac_points};

    const DIVIDER_OP: &str = "divider\nV1 in 0 DC 5\nR1 in out 1k\nR2 out 0 2k\n.op\n.end\n";
    const DIVIDER_DC: &str =
        "divider\nV1 in 0 DC 5\nR1 in out 1k\nR2 out 0 2k\n.dc V1 0 2 1\n.end\n";
    const RC_TRAN: &str = "rc step\nV1 in 0 PULSE(0 1 0 1u 1u 5u 10u)\nR1 in out 1k\nC1 out 0 1n\n.tran 1u 4u\n.end\n";
    const RC_AC: &str =
        "rc filter\nV1 in 0 AC 1\nR1 in out 1k\nC1 out 0 1u\n.ac dec 1 10 1k\n.end\n";

    /// Run `deck` with raw output into a fresh directory and read the raw file back.
    fn run_to_raw_file(name: &str, lowered: &Lowered) -> RawFile {
        let dir = TestDir::new(name);
        let raw = RawOutput {
            dir: dir.path(),
            stem: name,
        };
        run(
            lowered,
            &SimulationConfig::default(),
            &mut std::io::sink(),
            Some(&raw),
        )
        .unwrap();
        RawFile::read(&dir.path().join(format!("{name}.raw")))
    }

    #[test]
    fn op_raw_file() {
        let lowered = lower_netlist(DIVIDER_OP);
        let (circuit, params) = (&lowered.circuit, &lowered.params);
        let op = simulate_op(circuit, params, &SimulationConfig::default()).unwrap();
        let values = op.solution.iter().map(|&v| v as f32 as f64).collect();
        let expected = RawFile::new(
            "Title: *divider\n\
             Plotname: Operation Point\n\
             Flags: real\n\
             No. Variables: 3\n\
             No. Points: 1\n\
             Command: spicy\n\
             Variables:\n\
             \t0\tV(in)\tvoltage\n\
             \t1\tV(out)\tvoltage\n\
             \t2\tI(V1)\tdevice_current",
            values,
        );
        assert_eq!(run_to_raw_file("divider_op", &lowered), expected);
    }

    #[test]
    fn dc_raw_file() {
        let lowered = lower_netlist(DIVIDER_DC);
        let Analysis::Dc(sweep) = lowered.analyses[0] else {
            panic!("expected .dc")
        };
        let (circuit, params) = (&lowered.circuit, &lowered.params);
        let dc = simulate_dc(circuit, params, &sweep, &SimulationConfig::default());
        let mut values = Vec::new();
        for (op, sweep_value) in &dc.results {
            values.push(*sweep_value);
            values.extend(op.solution.iter().map(|&v| v as f32 as f64));
        }
        let expected = RawFile::new(
            "Title: *divider\n\
             Plotname: DC transfer characteristic\n\
             Flags: real forward\n\
             No. Variables: 4\n\
             No. Points: 3\n\
             Command: spicy\n\
             Variables:\n\
             \t0\tV1\tvoltage\n\
             \t1\tV(in)\tvoltage\n\
             \t2\tV(out)\tvoltage\n\
             \t3\tI(V1)\tdevice_current",
            values,
        );
        assert_eq!(run_to_raw_file("divider_dc", &lowered), expected);
    }

    #[test]
    fn transient_raw_file() {
        let lowered = lower_netlist(RC_TRAN);
        let Analysis::Tran(tran) = lowered.analyses[0] else {
            panic!("expected .tran")
        };
        let (circuit, params) = (&lowered.circuit, &lowered.params);
        let tran = simulate_trans(circuit, params, &tran, &SimulationConfig::default()).unwrap();
        let mut values = Vec::new();
        for (time, sample) in tran.times.iter().zip(&tran.samples) {
            values.push(*time);
            values.extend(sample.iter().map(|&v| v as f32 as f64));
        }
        let expected = RawFile::new(
            "Title: *rc step\n\
             Plotname: Transient Analysis\n\
             Flags: real forward\n\
             No. Variables: 4\n\
             No. Points: 5\n\
             Command: spicy\n\
             Variables:\n\
             \t0\ttime\ttime\n\
             \t1\tV(in)\tvoltage\n\
             \t2\tV(out)\tvoltage\n\
             \t3\tI(V1)\tdevice_current",
            values,
        );
        assert_eq!(run_to_raw_file("rc_tran", &lowered), expected);
    }

    #[test]
    fn ac_raw_file() {
        let lowered = lower_netlist(RC_AC);
        let mut values = Vec::new();
        for (freq, real, imag) in simulate_ac_points(&lowered) {
            values.push(freq);
            for (re, im) in real.into_iter().zip(imag) {
                values.push(re);
                values.push(im);
            }
        }
        let expected = RawFile::new(
            "Title: *rc filter\n\
             Plotname: AC Analysis\n\
             Flags: complex forward\n\
             No. Variables: 4\n\
             No. Points: 3\n\
             Command: spicy\n\
             Variables:\n\
             \t0\tfrequency\tfrequency\n\
             \t1\tV(in)\tvoltage\n\
             \t2\tV(out)\tvoltage\n\
             \t3\tI(V1)\tdevice_current",
            values,
        );
        assert_eq!(run_to_raw_file("rc_ac", &lowered), expected);
    }

    #[test]
    fn ac_results_are_printed() {
        let lowered = lower_netlist(RC_AC);
        let mut out = Vec::new();
        run(&lowered, &SimulationConfig::default(), &mut out, None).unwrap();

        // Node voltages come first in a solution, in node order (0 is ground).
        let names = &lowered.names.nodes[1..];
        let mut expected = String::new();
        for (freq, real, imag) in simulate_ac_points(&lowered) {
            for (i, name) in names.iter().enumerate() {
                let (re, im) = (real[i], imag[i]);
                let mag = (re * re + im * im).sqrt();
                let phase = im.atan2(re) * 180.0 / PI;
                expected += &format!("f={freq:.6} Hz  {name}: {mag:.6} ∠ {phase:.3}°\n");
            }
        }
        assert_eq!(String::from_utf8(out).unwrap(), expected);
    }
}
