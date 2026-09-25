//! Non-interactive mode: run every analysis in the netlist, print AC results,
//! and write raw files when asked.

use std::f64::consts::PI;
use std::io::Write;
use std::path::Path;

use spicy_parser::instance_parser::Deck;
use spicy_parser::netlist_types::Command;
use spicy_simulate::{
    AcResult, SimulationConfig, SimulationError,
    ac::simulate_ac,
    dc::{simulate_dc, simulate_op},
    trans::simulate_trans,
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
    deck: &Deck,
    config: &SimulationConfig,
    out: &mut impl Write,
    raw: Option<&RawOutput>,
) -> Result<(), SimulationError> {
    let raw_path = raw.map(|raw| raw::raw_file_path(raw.dir, raw.stem));
    for command in &deck.commands {
        match command {
            Command::Op(_) => {
                let op = simulate_op(deck, config)?;
                if let Some(path) = &raw_path {
                    let _ = raw::write_operating_point_raw(deck, &op, path);
                }
            }
            Command::Dc(params) => {
                let dc = simulate_dc(deck, params, config);
                if let Some(path) = &raw_path {
                    // The sweep variable is a voltage when it names a voltage source.
                    let is_voltage = deck
                        .devices
                        .voltage_sources
                        .iter()
                        .any(|v| v.name == params.srcnam);
                    let _ = raw::write_dc_raw(deck, &dc, path, &params.srcnam, is_voltage);
                }
            }
            Command::Ac(params) => {
                let ac = simulate_ac(deck, params, config);
                print_ac(out, deck, &ac);
                if let Some(path) = &raw_path {
                    let _ = raw::write_ac_raw(deck, &ac, path);
                }
            }
            Command::Tran(params) => {
                let tran = simulate_trans(deck, params, config)?;
                if let Some(path) = &raw_path {
                    let _ = raw::write_transient_raw(deck, &tran, path);
                }
            }
            Command::End => break,
        }
    }
    Ok(())
}

/// Print every node's phasor at every frequency. Panics if `out` can't be
/// written, like `println!`.
fn print_ac(out: &mut impl Write, deck: &Deck, ac: &AcResult) {
    let node_names = deck.node_mapping.node_names_mna_order();
    for (f, xr, xi) in ac {
        for (i, name) in node_names.iter().enumerate() {
            let (vr, vi) = (xr[i], xi[i]);
            let mag = (vr * vr + vi * vi).sqrt();
            let phase = vi.atan2(vr) * 180.0 / PI;
            writeln!(out, "f={:.6} Hz  {}: {:.6} ∠ {:.3}°", f, name, mag, phase)
                .expect("write AC results");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{RawFile, TestDir, parse_deck, simulate_ac_points};

    const DIVIDER_OP: &str = "divider\nV1 in 0 DC 5\nR1 in out 1k\nR2 out 0 2k\n.op\n.end\n";
    const DIVIDER_DC: &str =
        "divider\nV1 in 0 DC 5\nR1 in out 1k\nR2 out 0 2k\n.dc V1 0 2 1\n.end\n";
    const RC_TRAN: &str = "rc step\nV1 in 0 PULSE(0 1 0 1u 1u 5u 10u)\nR1 in out 1k\nC1 out 0 1n\n.tran 1u 4u\n.end\n";
    const RC_AC: &str =
        "rc filter\nV1 in 0 AC 1\nR1 in out 1k\nC1 out 0 1u\n.ac dec 1 10 1k\n.end\n";

    /// Run `deck` with raw output into a fresh directory and read the raw file back.
    fn run_to_raw_file(name: &str, deck: &Deck) -> RawFile {
        let dir = TestDir::new(name);
        let raw = RawOutput {
            dir: dir.path(),
            stem: name,
        };
        run(
            deck,
            &SimulationConfig::default(),
            &mut std::io::sink(),
            Some(&raw),
        )
        .unwrap();
        RawFile::read(&dir.path().join(format!("{name}.raw")))
    }

    #[test]
    fn op_raw_file() {
        let deck = parse_deck(DIVIDER_OP);
        let op = simulate_op(&deck, &SimulationConfig::default()).unwrap();
        let values = op
            .voltages
            .iter()
            .chain(&op.currents)
            .map(|&(_, v)| v as f32 as f64)
            .collect();
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
        assert_eq!(run_to_raw_file("divider_op", &deck), expected);
    }

    #[test]
    fn dc_raw_file() {
        let deck = parse_deck(DIVIDER_DC);
        let Command::Dc(params) = &deck.commands[0] else {
            panic!("expected .dc")
        };
        let sweep = simulate_dc(&deck, params, &SimulationConfig::default());
        let mut values = Vec::new();
        for (op, sweep_value) in &sweep.results {
            values.push(*sweep_value);
            values.extend(
                op.voltages
                    .iter()
                    .chain(&op.currents)
                    .map(|&(_, v)| v as f32 as f64),
            );
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
        assert_eq!(run_to_raw_file("divider_dc", &deck), expected);
    }

    #[test]
    fn transient_raw_file() {
        let deck = parse_deck(RC_TRAN);
        let Command::Tran(params) = &deck.commands[0] else {
            panic!("expected .tran")
        };
        let tran = simulate_trans(&deck, params, &SimulationConfig::default()).unwrap();
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
        assert_eq!(run_to_raw_file("rc_tran", &deck), expected);
    }

    #[test]
    fn ac_raw_file() {
        let deck = parse_deck(RC_AC);
        let mut values = Vec::new();
        for (freq, real, imag) in simulate_ac_points(&deck) {
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
        assert_eq!(run_to_raw_file("rc_ac", &deck), expected);
    }

    #[test]
    fn ac_results_are_printed() {
        let deck = parse_deck(RC_AC);
        let mut out = Vec::new();
        run(&deck, &SimulationConfig::default(), &mut out, None).unwrap();

        let names = deck.node_mapping.node_names_mna_order();
        let mut expected = String::new();
        for (freq, real, imag) in simulate_ac_points(&deck) {
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
