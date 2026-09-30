//! SPICE netlists. The reader turns a netlist into a
//! [`spicy_circuit::Lowered`].

pub mod reader;

/// SPICE gives temperatures in °C; `spicy_circuit` uses kelvin.
pub(crate) const CELSIUS_TO_KELVIN: f64 = 273.15;
