pub(crate) mod capacitor;
pub(crate) mod inductor;
pub(crate) mod resistor;
pub(crate) mod sources;
pub(crate) mod stamp;

use spicy_parser::devices::Devices as DevicesSpec;

pub(crate) use capacitor::Capacitor;
pub(crate) use inductor::Inductor;
pub(crate) use resistor::Resistor;
pub(crate) use sources::IndependentSource;

#[derive(Debug, Clone)]
pub(crate) struct Devices {
    pub resistors: Vec<Resistor>,
    pub capacitors: Vec<Capacitor>,
    pub inductors: Vec<Inductor>,
    pub voltage_sources: Vec<IndependentSource>,
    pub current_sources: Vec<IndependentSource>,
}

impl Devices {
    pub fn from_spec(spec: &DevicesSpec) -> Self {
        Self {
            resistors: spec.resistors.iter().map(Resistor::from_spec).collect(),
            capacitors: spec
                .capacitors
                .iter()
                .map(Capacitor::from_spec)
                .collect(),
            inductors: spec.inductors.iter().map(Inductor::from_spec).collect(),
            voltage_sources: spec
                .voltage_sources
                .iter()
                .map(IndependentSource::from_spec)
                .collect(),
            current_sources: spec
                .current_sources
                .iter()
                .map(IndependentSource::from_spec)
                .collect(),
        }
    }
}