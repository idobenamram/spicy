pub use crate::devices::{
    capacitor::CapacitorSpec, diode::DiodeSpec, inductor::InductorSpec, resistor::ResistorSpec,
    sources::IndependentSourceSpec,
};

mod capacitor;
mod diode;
mod inductor;
mod resistor;
mod sources;

#[derive(Debug)]
pub struct Devices {
    pub resistors: Vec<ResistorSpec>,
    pub capacitors: Vec<CapacitorSpec>,
    pub inductors: Vec<InductorSpec>,
    pub diodes: Vec<DiodeSpec>,
    pub voltage_sources: Vec<IndependentSourceSpec>,
    pub current_sources: Vec<IndependentSourceSpec>,
}

impl Devices {
    pub fn new() -> Self {
        Self {
            resistors: Vec::new(),
            capacitors: Vec::new(),
            inductors: Vec::new(),
            diodes: Vec::new(),
            voltage_sources: Vec::new(),
            current_sources: Vec::new(),
        }
    }
}

impl Default for Devices {
    fn default() -> Self {
        Self::new()
    }
}
