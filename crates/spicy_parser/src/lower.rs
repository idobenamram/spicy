//! Lowering: a parsed [`Deck`] to [`Lowered`], the simulator-ready form.
//!
//! Every SPICE-specific rule ends here (docs/ecad/circuit.md):
//! - instance values override `.model` values, and dialect defaults fill in
//!   the rest;
//! - `scale` is folded into R, C and L values, while `m` and `area` stay
//!   parameters for the simulator to apply;
//! - identical model cards are stored once;
//! - names are resolved to ids and moved to [`CircuitNames`].

use std::collections::HashMap;
use std::hash::Hash;

use spicy_circuit::{
    AcSpacing, AcSweep, Analysis, Bjt, BjtModel, BjtModelId, BjtParams, Capacitor, CapacitorModel,
    CapacitorModelId, CapacitorParams, Circuit, CircuitNames, DcSweep, DeviceTemperature, Diode,
    DiodeModel, DiodeModelId, DiodeParams, Inductor, InductorModel, InductorModelId,
    InductorParams, IsourceId, Lowered, NodeId, Params, Phasor, Polarity, Resistor, ResistorModel,
    ResistorModelId, ResistorParams, SourceParams, SourceRef, Transient, TwoTerminal, VsourceId,
    Waveform,
};

use crate::BjtPolarity;
use crate::devices::{
    BjtSpec, CapacitorSpec, DiodeSpec, IndependentSourceSpec, InductorSpec, ResistorSpec,
};
use crate::error::{LowerError, SpicyError};
use crate::expr::Value;
use crate::instance_parser::Deck;
use crate::netlist_types::{AcSweepType, Command, DcCommand, NodeIndex, NodeName};
use crate::netlist_waveform::WaveForm;

/// ngspice's resistance when neither the instance nor its model gives one
/// (`restemp.c`).
const DEFAULT_RESISTANCE: f64 = 1e-3;

/// SPICE gives temperatures in °C; `spicy_circuit` uses kelvin.
const CELSIUS_TO_KELVIN: f64 = 273.15;

pub fn lower(deck: &Deck) -> Result<Lowered, SpicyError> {
    let devices = &deck.devices;
    let mut circuit = Circuit {
        node_count: deck.node_mapping.nodes_len() + 1,
        ..Circuit::default()
    };
    let mut params = Params::default();
    let mut names = CircuitNames {
        title: deck.title.clone(),
        nodes: node_names(deck),
        ..CircuitNames::default()
    };

    let mut resistor_models = ModelTable::default();
    for r in &devices.resistors {
        let model = resistor_model(r);
        let model = ResistorModelId::new(resistor_models.insert(resistor_model_key(&model), model));
        circuit.resistors.push(Resistor {
            positive: node(r.positive),
            negative: node(r.negative),
            model,
        });
        params.resistors.push(resistor_params(r));
        names.resistors.push(r.name.clone());
    }
    params.resistor_models = resistor_models.models;

    let mut capacitor_models = ModelTable::default();
    for c in &devices.capacitors {
        let model = capacitor_model(c);
        let model = CapacitorModelId::new(
            capacitor_models.insert(tc_model_key(model.tc1, model.tc2), model),
        );
        circuit.capacitors.push(Capacitor {
            positive: node(c.positive),
            negative: node(c.negative),
            model,
        });
        params.capacitors.push(capacitor_params(c));
        names.capacitors.push(c.name.clone());
    }
    params.capacitor_models = capacitor_models.models;

    let mut inductor_models = ModelTable::default();
    for l in &devices.inductors {
        let model = inductor_model(l);
        let model =
            InductorModelId::new(inductor_models.insert(tc_model_key(model.tc1, model.tc2), model));
        circuit.inductors.push(Inductor {
            positive: node(l.positive),
            negative: node(l.negative),
            model,
        });
        params.inductors.push(inductor_params(l));
        names.inductors.push(l.name.clone());
    }
    params.inductor_models = inductor_models.models;

    let mut diode_models = ModelTable::default();
    for d in &devices.diodes {
        let model = diode_model(d);
        let model = DiodeModelId::new(diode_models.insert(diode_model_key(&model), model));
        circuit.diodes.push(Diode {
            positive: node(d.positive),
            negative: node(d.negative),
            model,
        });
        params.diodes.push(diode_params(d));
        names.diodes.push(d.name.clone());
    }
    params.diode_models = diode_models.models;

    let mut bjt_models = ModelTable::default();
    for q in &devices.bjts {
        let model = bjt_model(q);
        let model = BjtModelId::new(bjt_models.insert(bjt_model_key(&model), model));
        circuit.bjts.push(Bjt {
            collector: node(q.collector),
            base: node(q.base),
            emitter: node(q.emitter),
            model,
        });
        params.bjts.push(bjt_params(q));
        names.bjts.push(q.name.clone());
    }
    params.bjt_models = bjt_models.models;

    for v in &devices.voltage_sources {
        circuit.vsources.push(two_terminal(v.positive, v.negative));
        params.vsources.push(source_params(v));
        names.vsources.push(v.name.clone());
    }
    for i in &devices.current_sources {
        circuit.isources.push(two_terminal(i.positive, i.negative));
        params.isources.push(source_params(i));
        names.isources.push(i.name.clone());
    }

    let mut analyses = Vec::with_capacity(deck.commands.len());
    for command in &deck.commands {
        analyses.push(match command {
            Command::Op(_) => Analysis::Op,
            Command::Dc(dc) => Analysis::Dc(DcSweep {
                source: sweep_source(deck, dc)?,
                start: dc.vstart.get_value(),
                stop: dc.vstop.get_value(),
                step: dc.vincr.get_value(),
            }),
            Command::Ac(ac) => Analysis::Ac(AcSweep {
                spacing: match ac.ac_sweep_type {
                    AcSweepType::Dec(n) => AcSpacing::Decade(n),
                    AcSweepType::Oct(n) => AcSpacing::Octave(n),
                    AcSweepType::Lin(n) => AcSpacing::Linear(n),
                },
                start: ac.fstart.get_value(),
                stop: ac.fstop.get_value(),
            }),
            Command::Tran(tran) => Analysis::Tran(Transient {
                step: tran.tstep.get_value(),
                stop: tran.tstop.get_value(),
                uic: tran.uic,
            }),
            Command::End => break,
        });
    }

    Ok(Lowered {
        circuit,
        params,
        names,
        analyses,
    })
}

/// Stores each distinct model once. The parser copies a `.model` card into
/// every instance that names it, so identical cards are merged here, by value.
struct ModelTable<K, M> {
    ids: HashMap<K, usize>,
    models: Vec<M>,
}

impl<K, M> Default for ModelTable<K, M> {
    fn default() -> Self {
        Self {
            ids: HashMap::new(),
            models: Vec::new(),
        }
    }
}

impl<K: Hash + Eq, M> ModelTable<K, M> {
    /// Returns the index of the model with this key, adding it if it's new.
    fn insert(&mut self, key: K, model: M) -> usize {
        *self.ids.entry(key).or_insert_with(|| {
            self.models.push(model);
            self.models.len() - 1
        })
    }
}

fn resistor_model_key(model: &ResistorModel) -> [u64; 4] {
    [
        model.tc1,
        model.tc2,
        model.default_width,
        model.default_length,
    ]
    .map(f64::to_bits)
}

fn tc_model_key(tc1: f64, tc2: f64) -> [u64; 2] {
    [tc1.to_bits(), tc2.to_bits()]
}

fn diode_model_key(model: &DiodeModel) -> [u64; 3] {
    [model.is, model.n, model.rs].map(f64::to_bits)
}

fn bjt_model_key(model: &BjtModel) -> (Polarity, [u64; 5]) {
    let numbers = [model.is, model.bf, model.br, model.nf, model.nr];
    (model.polarity, numbers.map(f64::to_bits))
}

fn node_names(deck: &Deck) -> Vec<String> {
    let mut names = vec![NodeName::GROUND.to_string()];
    names.extend(deck.node_mapping.node_names());
    names
}

fn node(index: NodeIndex) -> NodeId {
    NodeId::new(index.0)
}

/// A device's temperature: `temp` (°C) if given, else the run's plus `dtemp`.
/// ngspice ignores `dtemp` when `temp` is given (restemp.c).
fn device_temperature(temp: &Option<Value>, dtemp: &Option<Value>) -> DeviceTemperature {
    match value(temp) {
        Some(celsius) => DeviceTemperature::Fixed(celsius + CELSIUS_TO_KELVIN),
        None => DeviceTemperature::Offset(value_or(dtemp, 0.0)),
    }
}

/// The instance's value if given, else the model's.
fn instance_or_model(instance: &Option<Value>, model: Option<&Option<Value>>) -> Option<f64> {
    value(instance).or_else(|| model.and_then(value))
}

fn two_terminal(positive: NodeIndex, negative: NodeIndex) -> TwoTerminal {
    TwoTerminal {
        positive: node(positive),
        negative: node(negative),
    }
}

fn value(value: &Option<Value>) -> Option<f64> {
    value.as_ref().map(Value::get_value)
}

fn value_or(value: &Option<Value>, default: f64) -> f64 {
    value.as_ref().map_or(default, Value::get_value)
}

/// A resistor's model. Temperature coefficients set on the instance override
/// the card's, which gives the instance a model of its own.
fn resistor_model(spec: &ResistorSpec) -> ResistorModel {
    let model = spec.model.as_ref();
    let defaults = ResistorModel::default();
    ResistorModel {
        tc1: instance_or_model(&spec.tc1, model.map(|m| &m.tc1)).unwrap_or(defaults.tc1),
        tc2: instance_or_model(&spec.tc2, model.map(|m| &m.tc2)).unwrap_or(defaults.tc2),
        default_width: model
            .and_then(|m| value(&m.w))
            .unwrap_or(defaults.default_width),
        default_length: model
            .and_then(|m| value(&m.l))
            .unwrap_or(defaults.default_length),
    }
}

fn resistor_params(spec: &ResistorSpec) -> ResistorParams {
    let model = spec.model.as_ref();
    let r = instance_or_model(&spec.resistance, model.map(|m| &m.resistance))
        .unwrap_or(DEFAULT_RESISTANCE);
    let scale = value_or(&spec.scale, 1.0);
    ResistorParams {
        r: r * scale,
        r_ac: value(&spec.ac).map(|r_ac| r_ac * scale),
        m: value_or(&spec.m, 1.0),
        temperature: device_temperature(&spec.temp, &spec.dtemp),
        noisy: spec.noisy.unwrap_or(true),
    }
}

fn capacitor_model(spec: &CapacitorSpec) -> CapacitorModel {
    let model = spec.model.as_ref();
    CapacitorModel {
        tc1: instance_or_model(&spec.tc1, model.map(|m| &m.tc1)).unwrap_or(0.0),
        tc2: instance_or_model(&spec.tc2, model.map(|m| &m.tc2)).unwrap_or(0.0),
    }
}

fn capacitor_params(spec: &CapacitorSpec) -> CapacitorParams {
    let model = spec.model.as_ref();
    let c = instance_or_model(&spec.capacitance, model.map(|m| &m.cap)).unwrap_or(0.0);
    CapacitorParams {
        c: c * value_or(&spec.scale, 1.0),
        m: value_or(&spec.m, 1.0),
        ic: value_or(&spec.ic, 0.0),
        temperature: device_temperature(&spec.temp, &spec.dtemp),
    }
}

fn inductor_model(spec: &InductorSpec) -> InductorModel {
    let model = spec.model.as_ref();
    InductorModel {
        tc1: instance_or_model(&spec.tc1, model.map(|m| &m.tc1)).unwrap_or(0.0),
        tc2: instance_or_model(&spec.tc2, model.map(|m| &m.tc2)).unwrap_or(0.0),
    }
}

fn inductor_params(spec: &InductorSpec) -> InductorParams {
    let model = spec.model.as_ref();
    let l = instance_or_model(&spec.inductance, model.map(|m| &m.inductance)).unwrap_or(0.0);
    InductorParams {
        l: l * value_or(&spec.scale, 1.0),
        m: value_or(&spec.m, 1.0),
        ic: value_or(&spec.ic, 0.0),
        temperature: device_temperature(&spec.temp, &spec.dtemp),
        // ngspice's default number of turns (indtemp.c).
        nt: value_or(&spec.nt, 0.0),
    }
}

fn diode_model(spec: &DiodeSpec) -> DiodeModel {
    let defaults = DiodeModel::default();
    DiodeModel {
        is: value_or(&spec.model.is, defaults.is),
        n: value_or(&spec.model.n, defaults.n),
        rs: value_or(&spec.model.rs, defaults.rs),
    }
}

fn diode_params(spec: &DiodeSpec) -> DiodeParams {
    let defaults = DiodeParams::default();
    DiodeParams {
        area: value_or(&spec.area, defaults.area),
        m: value_or(&spec.m, defaults.m),
        pj: value_or(&spec.pj, defaults.pj),
        lm: value_or(&spec.lm, defaults.lm),
        wm: value_or(&spec.wm, defaults.wm),
        lp: value_or(&spec.lp, defaults.lp),
        wp: value_or(&spec.wp, defaults.wp),
        off: spec.off.unwrap_or(defaults.off),
        ic: value_or(&spec.ic, defaults.ic),
        temperature: device_temperature(&spec.temp, &spec.dtemp),
    }
}

fn bjt_params(spec: &BjtSpec) -> BjtParams {
    let defaults = BjtParams::default();
    BjtParams {
        area: value_or(&spec.area, defaults.area),
        m: value_or(&spec.m, defaults.m),
        off: spec.off.unwrap_or(defaults.off),
        ic_vbe: value_or(&spec.ic_vbe, defaults.ic_vbe),
        ic_vce: value_or(&spec.ic_vce, defaults.ic_vce),
    }
}

fn bjt_model(spec: &BjtSpec) -> BjtModel {
    let model = &spec.model;
    let defaults = BjtModel::default();
    BjtModel {
        polarity: match model.polarity {
            BjtPolarity::Npn => Polarity::Npn,
            BjtPolarity::Pnp => Polarity::Pnp,
        },
        is: value_or(&model.is, defaults.is),
        bf: value_or(&model.bf, defaults.bf),
        br: value_or(&model.br, defaults.br),
        nf: value_or(&model.nf, defaults.nf),
        nr: value_or(&model.nr, defaults.nr),
    }
}

fn source_params(spec: &IndependentSourceSpec) -> SourceParams {
    SourceParams {
        waveform: spec.dc.as_ref().map_or(Waveform::Dc(0.0), waveform),
        ac: spec.ac.as_ref().map_or(Phasor::default(), |ac| Phasor {
            magnitude: ac.mag.get_value(),
            phase: angle(&ac.phase),
        }),
    }
}

/// An angle in radians. SPICE reads angles in degrees unless marked `rad`.
fn angle(value: &Option<Value>) -> f64 {
    value.as_ref().map_or(0.0, |v| v.angle_radians(true))
}

fn waveform(waveform: &WaveForm) -> Waveform {
    match waveform {
        WaveForm::Constant(v) => Waveform::Dc(v.get_value()),
        WaveForm::Pulse {
            voltage1,
            voltage2,
            delay,
            rise_time,
            fall_time,
            pulse_width,
            period,
            number_of_pulses,
        } => Waveform::Pulse {
            initial: voltage1.get_value(),
            pulsed: voltage2.get_value(),
            delay: value_or(delay, 0.0),
            rise: value(rise_time),
            fall: value(fall_time),
            width: value(pulse_width),
            period: value(period),
            count: number_of_pulses.unwrap_or(0),
        },
        WaveForm::Sinusoidal {
            offset,
            amplitude,
            frequency,
            delay,
            damping_factor,
            phase,
        } => Waveform::Sine {
            offset: offset.get_value(),
            amplitude: amplitude.get_value(),
            frequency: value(frequency),
            delay: value_or(delay, 0.0),
            damping: value_or(damping_factor, 0.0),
            phase: angle(phase),
        },
        WaveForm::Exponential {
            initial_value,
            pulsed_value,
            rise_delay_time,
            rise_time_constant,
            fall_delay_time,
            fall_time_constant,
        } => Waveform::Exp {
            initial: initial_value.get_value(),
            pulsed: pulsed_value.get_value(),
            rise_delay: value_or(rise_delay_time, 0.0),
            rise_tau: value(rise_time_constant),
            fall_delay: value(fall_delay_time),
            fall_tau: value(fall_time_constant),
        },
    }
}

/// The source a `.dc` sweep names: a voltage source first, then a current source.
fn sweep_source(deck: &Deck, command: &DcCommand) -> Result<SourceRef, LowerError> {
    let devices = &deck.devices;
    let name = command.srcnam.as_str();
    if let Some(i) = devices.voltage_sources.iter().position(|v| v.name == name) {
        return Ok(SourceRef::Voltage(VsourceId::new(i)));
    }
    if let Some(i) = devices.current_sources.iter().position(|s| s.name == name) {
        return Ok(SourceRef::Current(IsourceId::new(i)));
    }
    Err(LowerError::UnknownSweepSource {
        name: command.srcnam.clone(),
        span: command.span,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::parse_netlist;
    use rstest::rstest;
    use std::f64::consts::FRAC_PI_2;
    use std::path::PathBuf;

    /// Lower a netlist body (the lines between the title and `.end`).
    fn lower_body(body: &str) -> Lowered {
        lower(&parse_netlist(&format!("test\n{body}\n.end\n"))).expect("lower")
    }

    #[test]
    fn instance_value_overrides_model_value() {
        let lowered = lower_body(".model RM R resistance=2k\nR1 a 0 mname=RM\nR2 a 0 5k RM");
        let r: Vec<f64> = lowered.params.resistors.iter().map(|p| p.r).collect();
        assert_eq!(r, [2e3, 5e3]);
    }

    #[test]
    fn missing_resistance_defaults_to_one_milliohm() {
        let lowered = lower_body(".model RM R tc1=1m\nR1 a 0 mname=RM");
        assert_eq!(lowered.params.resistors[0].r, 1e-3);
    }

    #[test]
    fn scale_is_folded_and_m_is_kept() {
        let lowered = lower_body(
            "R1 a 0 1k ac=500 m=2 scale=3\nC1 a 0 1u m=2 scale=3\nL1 a 0 1m m=2 scale=3",
        );
        let params = &lowered.params;
        assert_eq!(params.resistors[0].r, 3e3);
        assert_eq!(params.resistors[0].r_ac, Some(1.5e3));
        assert_eq!(params.resistors[0].m, 2.0);
        assert_eq!(params.capacitors[0].c, 1e-6 * 3.0);
        assert_eq!(params.capacitors[0].m, 2.0);
        assert_eq!(params.inductors[0].l, 1e-3 * 3.0);
        assert_eq!(params.inductors[0].m, 2.0);
    }

    #[test]
    fn ac_resistance_is_unset_unless_given() {
        let lowered = lower_body("R1 a 0 1k");
        assert_eq!(lowered.params.resistors[0].r_ac, None);
    }

    #[test]
    fn area_and_m_are_kept() {
        let lowered = lower_body(
            ".model DMOD D is=1e-14\n.model QN NPN is=1e-16\nD1 a 0 DMOD area=2 m=3\nQ1 c b 0 QN area=2 m=3",
        );
        let params = &lowered.params;
        assert_eq!((params.diodes[0].area, params.diodes[0].m), (2.0, 3.0));
        assert_eq!((params.bjts[0].area, params.bjts[0].m), (2.0, 3.0));
        assert_eq!(params.diode_models[0].is, 1e-14);
        assert_eq!(params.bjt_models[0].is, 1e-16);
    }

    #[test]
    fn identical_model_cards_are_stored_once() {
        let lowered = lower_body(
            ".model QA NPN bf=200\n.model QB NPN bf=200\n.model QC NPN bf=50\n\
             Q1 c b 0 QA\nQ2 c b 0 QA\nQ3 c b 0 QB\nQ4 c b 0 QC",
        );
        let models: Vec<usize> = lowered
            .circuit
            .bjts
            .iter()
            .map(|q| q.model.index())
            .collect();
        assert_eq!(models, [0, 0, 0, 1]);
        let bf: Vec<f64> = lowered.params.bjt_models.iter().map(|m| m.bf).collect();
        assert_eq!(bf, [200.0, 50.0]);
    }

    #[test]
    fn empty_model_cards_get_ngspice_defaults() {
        let lowered = lower_body(".model DMOD D\n.model QP PNP\nD1 a 0 DMOD\nQ1 c b 0 QP");
        let params = &lowered.params;
        assert_eq!(params.diode_models, [DiodeModel::default()]);
        let pnp = BjtModel {
            polarity: Polarity::Pnp,
            ..BjtModel::default()
        };
        assert_eq!(params.bjt_models, [pnp]);
    }

    #[test]
    fn instance_temperature_coefficients_override_the_model() {
        let lowered = lower_body(
            ".model RM R tc1=1m\n\
             R1 a 0 1k RM\nR2 a 0 1k RM tc1=2m\nR3 a 0 1k RM tc1=1m\nR4 a 0 1k",
        );
        let models: Vec<usize> = lowered
            .circuit
            .resistors
            .iter()
            .map(|r| r.model.index())
            .collect();
        // R3 restates the card's value, so it shares R1's model. R4 has no card.
        assert_eq!(models, [0, 1, 0, 2]);
        let tc1: Vec<f64> = lowered
            .params
            .resistor_models
            .iter()
            .map(|m| m.tc1)
            .collect();
        assert_eq!(tc1, [1e-3, 2e-3, 0.0]);
    }

    #[test]
    fn temp_is_fixed_in_kelvin_and_wins_over_dtemp() {
        let lowered = lower_body("R1 a 0 1k temp=50 dtemp=10\nR2 a 0 1k dtemp=10\nR3 a 0 1k");
        let temperatures: Vec<DeviceTemperature> = lowered
            .params
            .resistors
            .iter()
            .map(|r| r.temperature)
            .collect();
        assert_eq!(
            temperatures,
            [
                DeviceTemperature::Fixed(50.0 + CELSIUS_TO_KELVIN),
                DeviceTemperature::Offset(10.0),
                DeviceTemperature::Offset(0.0),
            ]
        );
    }

    #[test]
    fn parameters_not_simulated_yet_are_carried() {
        let lowered = lower_body(
            ".model DMOD D rs=10\n.model QN NPN\n\
             D1 a 0 DMOD pj=2 off ic=0.6\nQ1 c b 0 QN area=1 off ic=0.7,1.2\n\
             L1 a 0 1m nt=5\nR1 a 0 1k noisy=0",
        );
        let params = &lowered.params;
        assert_eq!(params.diode_models[0].rs, 10.0);
        let d = &params.diodes[0];
        assert_eq!((d.pj, d.off, d.ic), (2.0, true, 0.6));
        let q = &params.bjts[0];
        assert_eq!((q.off, q.ic_vbe, q.ic_vce), (true, 0.7, 1.2));
        assert_eq!(params.inductors[0].nt, 5.0);
        assert!(!params.resistors[0].noisy);
    }

    #[test]
    fn ac_phase_is_in_radians() {
        let lowered = lower_body("V1 a 0 DC 1 AC 2 90\nV2 b 0 DC 1");
        let sources = &lowered.params.vsources;
        assert_eq!(
            sources[0].ac,
            Phasor {
                magnitude: 2.0,
                phase: FRAC_PI_2
            }
        );
        assert_eq!(sources[1].ac, Phasor::default());
    }

    #[test]
    fn waveform_defaults_that_depend_on_the_analysis_stay_unset() {
        let lowered = lower_body("V1 a 0 PULSE(0 5)");
        let pulse = Waveform::Pulse {
            initial: 0.0,
            pulsed: 5.0,
            delay: 0.0,
            rise: None,
            fall: None,
            width: None,
            period: None,
            count: 0,
        };
        assert_eq!(lowered.params.vsources[0].waveform, pulse);
    }

    #[test]
    fn dc_sweep_names_a_source() {
        let lowered = lower_body("V1 a 0 1\nI1 a 0 1m\n.dc I1 0 1m 0.1m\n.dc V1 0 1 0.5");
        let sources: Vec<SourceRef> = lowered
            .analyses
            .iter()
            .map(|analysis| match analysis {
                Analysis::Dc(sweep) => sweep.source,
                other => panic!("expected .dc, got {other:?}"),
            })
            .collect();
        assert_eq!(
            sources,
            [
                SourceRef::Current(IsourceId::new(0)),
                SourceRef::Voltage(VsourceId::new(0))
            ]
        );
    }

    #[test]
    fn dc_sweep_of_an_unknown_source_is_an_error() {
        let deck = parse_netlist("test\nV1 a 0 1\nR1 a 0 1k\n.dc R1 0 1 0.5\n.end\n");
        let error = lower(&deck).expect_err("R1 isn't a source");
        assert!(matches!(
            error,
            SpicyError::Lower(LowerError::UnknownSweepSource { ref name, .. }) if name == "R1"
        ));
    }

    #[rstest]
    fn lower_parser_inputs(#[files("tests/parser_inputs/*.spicy")] input: PathBuf) {
        let netlist = std::fs::read_to_string(&input).expect("read input");
        let lowered = lower(&parse_netlist(&netlist)).expect("lower");
        let name = format!("lower-{}", input.file_stem().unwrap().to_string_lossy());
        insta::assert_debug_snapshot!(name, lowered);
    }
}
