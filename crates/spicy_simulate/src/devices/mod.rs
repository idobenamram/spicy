pub(crate) mod bjt;
pub(crate) mod capacitor;
pub(crate) mod diode;
pub(crate) mod inductor;
pub(crate) mod resistor;
pub(crate) mod sources;
pub(crate) mod stamp;
pub(crate) mod waveform;

use spicy_circuit::{Circuit, Params};

pub(crate) use bjt::Bjt;
pub(crate) use capacitor::Capacitor;
pub(crate) use diode::Diode;
pub(crate) use inductor::Inductor;
pub(crate) use resistor::Resistor;
pub(crate) use sources::{CurrentSource, VoltageSource};

use crate::unknowns::Layout;

/// The simulator's view of each device: effective values and matrix
/// positions, computed from a circuit and its parameters.
#[derive(Debug, Clone)]
pub(crate) struct Devices {
    pub resistors: Vec<Resistor>,
    pub capacitors: Vec<Capacitor>,
    pub inductors: Vec<Inductor>,
    pub diodes: Vec<Diode>,
    pub bjts: Vec<Bjt>,
    pub voltage_sources: Vec<VoltageSource>,
    pub current_sources: Vec<CurrentSource>,
}

impl Devices {
    pub fn new(circuit: &Circuit, params: &Params, layout: &Layout) -> Self {
        Self {
            resistors: circuit
                .resistors
                .iter()
                .zip(&params.resistors)
                .map(|(pins, p)| Resistor::new(pins, p, layout))
                .collect(),
            capacitors: circuit
                .capacitors
                .iter()
                .zip(&params.capacitors)
                .map(|(pins, p)| Capacitor::new(pins, p, layout))
                .collect(),
            inductors: circuit
                .inductors
                .iter()
                .zip(&params.inductors)
                .enumerate()
                .map(|(i, (pins, p))| Inductor::new(pins, p, layout.inductor(i), layout))
                .collect(),
            diodes: circuit
                .diodes
                .iter()
                .zip(&params.diodes)
                .map(|(pins, p)| {
                    Diode::new(pins, &params.diode_models[pins.model.index()], p, layout)
                })
                .collect(),
            bjts: circuit
                .bjts
                .iter()
                .zip(&params.bjts)
                .map(|(pins, p)| Bjt::new(pins, &params.bjt_models[pins.model.index()], p, layout))
                .collect(),
            voltage_sources: circuit
                .vsources
                .iter()
                .zip(&params.vsources)
                .enumerate()
                .map(|(i, (pins, p))| VoltageSource::new(pins, p, layout.vsource(i), layout))
                .collect(),
            current_sources: circuit
                .isources
                .iter()
                .zip(&params.isources)
                .map(|(pins, p)| CurrentSource::new(pins, p, layout))
                .collect(),
        }
    }
}
