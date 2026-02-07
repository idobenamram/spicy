pub(crate) mod capacitor;
pub(crate) mod diode;
pub(crate) mod inductor;
pub(crate) mod resistor;
pub(crate) mod sources;
pub(crate) mod stamp;
pub(crate) mod bjt;

use spicy_parser::devices::Devices as DevicesSpec;

pub(crate) use capacitor::Capacitor;
pub(crate) use diode::Diode;
pub(crate) use inductor::Inductor;
pub(crate) use resistor::Resistor;
pub(crate) use sources::IndependentSource;
pub(crate) use bjt::Bjt;

#[derive(Debug, Clone)]
pub(crate) struct Devices {
    pub resistors: Vec<Resistor>,
    pub capacitors: Vec<Capacitor>,
    pub inductors: Vec<Inductor>,
    pub diodes: Vec<Diode>,
    pub bjts: Vec<Bjt>,
    pub voltage_sources: Vec<IndependentSource>,
    pub current_sources: Vec<IndependentSource>,
}

impl Devices {
    pub fn from_spec(spec: &DevicesSpec) -> Self {
        Self {
            resistors: spec.resistors.iter().map(Resistor::from_spec).collect(),
            capacitors: spec.capacitors.iter().map(Capacitor::from_spec).collect(),
            inductors: spec.inductors.iter().map(Inductor::from_spec).collect(),
            diodes: spec.diodes.iter().map(Diode::from_spec).collect(),
            bjts: spec.bjts.iter().map(Bjt::from_spec).collect(),
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
