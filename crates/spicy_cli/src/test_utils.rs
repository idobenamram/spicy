//! Helpers shared by the CLI's tests.

use std::fs;
use std::path::{Path, PathBuf};

use spicy_parser::{ParseOptions, instance_parser::Deck, netlist_types::Command, parse};
use spicy_simulate::{SimulationConfig, ac::simulate_ac};

/// A fresh temporary directory for one test, removed when dropped.
pub(crate) struct TestDir(PathBuf);

impl TestDir {
    pub(crate) fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("spicy_cli_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test dir");
        Self(dir)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn parse_deck(netlist: &str) -> Deck {
    let mut options = ParseOptions::new_with_source("inline.spicy", netlist.to_string());
    parse(&mut options).expect("parse")
}

/// AC results per frequency: (frequency, real parts, imaginary parts), in MNA order.
pub(crate) type AcPoints = Vec<(f64, Vec<f64>, Vec<f64>)>;

/// Run the deck's first analysis, which must be `.ac`.
pub(crate) fn simulate_ac_points(deck: &Deck) -> AcPoints {
    let Command::Ac(command) = &deck.commands[0] else {
        panic!("expected .ac")
    };
    simulate_ac(deck, command, &SimulationConfig::default())
        .into_iter()
        .map(|(freq, real, imag)| (freq, real.to_vec(), imag.to_vec()))
        .collect()
}

/// A raw file split into its header lines and its values in file order. The
/// `Date:` line is left out because it changes on every run.
#[derive(Debug, PartialEq)]
pub(crate) struct RawFile {
    pub(crate) header: Vec<String>,
    pub(crate) values: Vec<f64>,
}

impl RawFile {
    /// Expected contents, with the header written as text.
    pub(crate) fn new(header: &str, values: Vec<f64>) -> Self {
        Self {
            header: header.lines().map(String::from).collect(),
            values,
        }
    }

    pub(crate) fn read(path: &Path) -> Self {
        let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let marker = b"Binary:\n";
        let split = bytes
            .windows(marker.len())
            .position(|w| w == marker)
            .expect("`Binary:` line");
        let header: Vec<String> = std::str::from_utf8(&bytes[..split])
            .expect("utf-8 header")
            .lines()
            .filter(|line| !line.starts_with("Date: "))
            .map(String::from)
            .collect();
        let field = |name: &str| -> String {
            header
                .iter()
                .find_map(|line| line.strip_prefix(name))
                .unwrap_or_else(|| panic!("no `{name}` in header"))
                .trim()
                .to_string()
        };
        let plotname = field("Plotname:");
        let nvars: usize = field("No. Variables:").parse().unwrap();
        let npoints: usize = field("No. Points:").parse().unwrap();

        let mut data = &bytes[split + marker.len()..];
        let mut values = Vec::new();
        for _ in 0..npoints {
            match plotname.as_str() {
                // One f32 per variable.
                "Operation Point" => {
                    for _ in 0..nvars {
                        values.push(read_f32(&mut data));
                    }
                }
                // f64 frequency, then (re, im) f64 pairs.
                "AC Analysis" => {
                    values.push(read_f64(&mut data));
                    for _ in 1..nvars {
                        values.push(read_f64(&mut data));
                        values.push(read_f64(&mut data));
                    }
                }
                // DC sweep and transient: f64 sweep value / time, then one f32 per trace.
                _ => {
                    values.push(read_f64(&mut data));
                    for _ in 1..nvars {
                        values.push(read_f32(&mut data));
                    }
                }
            }
        }
        assert!(data.is_empty(), "{} trailing bytes", data.len());
        Self { header, values }
    }
}

fn read_f64(data: &mut &[u8]) -> f64 {
    let (head, rest) = data.split_at(8);
    *data = rest;
    f64::from_le_bytes(head.try_into().unwrap())
}

fn read_f32(data: &mut &[u8]) -> f64 {
    let (head, rest) = data.split_at(4);
    *data = rest;
    f32::from_le_bytes(head.try_into().unwrap()) as f64
}
