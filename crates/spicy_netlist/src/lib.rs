//! SPICE netlists. The reader turns a netlist into a
//! [`spicy_circuit::Lowered`]; the writer turns one back into a netlist.

pub mod reader;
pub mod writer;

/// SPICE gives temperatures in °C; `spicy_circuit` uses kelvin.
pub(crate) const CELSIUS_TO_KELVIN: f64 = 273.15;
