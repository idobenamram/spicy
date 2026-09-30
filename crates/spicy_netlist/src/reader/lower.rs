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
    CapacitorModelId, CapacitorParams, Circuit, CircuitNames, DEFAULT_TEMPERATURE, DcSweep,
    DeviceTemperature, Diode, DiodeModel, DiodeModelId, DiodeParams, Inductor, InductorModel,
    InductorModelId, InductorParams, IsourceId, Lowered, NodeId, Params, Phasor, Polarity,
    Resistor, ResistorModel, ResistorModelId, ResistorParams, SolverOptions, SourceParams,
    SourceRef, Transient, TwoTerminal, VsourceId, Waveform,
};

use crate::CELSIUS_TO_KELVIN;
use crate::reader::BjtPolarity;
use crate::reader::devices::{
    BjtSpec, CapacitorSpec, DiodeSpec, IndependentSourceSpec, InductorSpec, ResistorSpec,
};
use crate::reader::error::{LowerError, SpicyError};
use crate::reader::expr::Value;
use crate::reader::instance_parser::Deck;
use crate::reader::netlist_types::{AcSweepType, Command, DcCommand, NodeIndex, NodeName};
use crate::reader::netlist_waveform::WaveForm;

/// ngspice's resistance for a line whose model card gives no value
/// (`restemp.c:61-74`). ngspice drops a line with neither a value nor a model,
/// with a warning; this reader still gives it this value.
const DEFAULT_RESISTANCE: f64 = 1e-3;

pub fn lower(deck: &Deck) -> Result<Lowered, SpicyError> {
    let devices = &deck.devices;
    let settings = run_settings(&deck.commands);
    let mut circuit = Circuit {
        node_count: deck.node_mapping.nodes_len() + 1,
        ..Circuit::default()
    };
    let mut params = Params {
        temp: settings.temp,
        ..Params::default()
    };
    let mut names = CircuitNames {
        title: deck.title.clone(),
        nodes: node_names(deck),
        ..CircuitNames::default()
    };

    let mut resistor_models = ModelTable::default();
    for r in &devices.resistors {
        let model = resistor_model(r, settings.tnom);
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
        let model = capacitor_model(c, settings.tnom);
        let model = CapacitorModelId::new(
            capacitor_models.insert(tc_model_key(model.tc1, model.tc2, model.tnom), model),
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
        let model = inductor_model(l, settings.tnom);
        let model = InductorModelId::new(
            inductor_models.insert(tc_model_key(model.tc1, model.tc2, model.tnom), model),
        );
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
        let model = diode_model(d, settings.tnom);
        let model = DiodeModelId::new(diode_models.insert(d.model_name.clone(), model));
        circuit.diodes.push(Diode {
            positive: node(d.positive),
            negative: node(d.negative),
            model,
        });
        params.diodes.push(diode_params(d));
        names.diodes.push(d.name.clone());
    }
    params.diode_models = diode_models.models;
    names.diode_models = diode_models.keys;

    let mut bjt_models = ModelTable::default();
    for q in &devices.bjts {
        let model = bjt_model(q, settings.tnom);
        let model = BjtModelId::new(bjt_models.insert(q.model_name.clone(), model));
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
    names.bjt_models = bjt_models.keys;

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
            // Settings, read by `run_settings`.
            Command::Temp(_) | Command::Options(_) => continue,
            Command::End => break,
        });
    }

    Ok(Lowered {
        circuit,
        params,
        names,
        analyses,
        options: settings.solver,
    })
}

/// What `.temp` and `.options` set. ngspice applies them before any analysis
/// runs, wherever they sit in the deck, and a later line overrides an earlier
/// one (`inp.c`, `inpdoopt.c`).
struct RunSettings {
    /// The circuit temperature, in kelvin.
    temp: f64,
    /// The TNOM of a model whose card gives none, in kelvin.
    tnom: f64,
    solver: SolverOptions,
}

fn run_settings(commands: &[Command]) -> RunSettings {
    let mut settings = RunSettings {
        temp: DEFAULT_TEMPERATURE,
        tnom: DEFAULT_TEMPERATURE,
        solver: SolverOptions::default(),
    };
    for command in commands {
        match command {
            Command::Temp(temp) => settings.temp = temp.celsius.get_value() + CELSIUS_TO_KELVIN,
            Command::Options(options) => {
                settings.tnom = tnom(options.tnom.as_ref(), settings.tnom);
                let solver = &mut settings.solver;
                solver.reltol = value(&options.reltol).or(solver.reltol);
                solver.vntol = value(&options.vntol).or(solver.vntol);
                solver.abstol = value(&options.abstol).or(solver.abstol);
            }
            _ => {}
        }
    }
    settings
}

/// A TNOM in kelvin: the one given (°C), else `default` (kelvin).
fn tnom(celsius: Option<&Value>, default: f64) -> f64 {
    celsius.map_or(default, |t| t.get_value() + CELSIUS_TO_KELVIN)
}

/// Stores each model once, under a key. Diode and BJT models are keyed by
/// their `.model` card's name: one entry per card, as ngspice keeps one model
/// per card, so two cards stay two even when their numbers match. Resistor,
/// capacitor and inductor models are built per instance (inline `tc1=` plus an
/// optional card), so they are keyed, and merged, by value.
struct ModelTable<K, M> {
    ids: HashMap<K, usize>,
    /// Each model's key, in model order.
    keys: Vec<K>,
    models: Vec<M>,
}

impl<K, M> Default for ModelTable<K, M> {
    fn default() -> Self {
        Self {
            ids: HashMap::new(),
            keys: Vec::new(),
            models: Vec::new(),
        }
    }
}

impl<K: Hash + Eq + Clone, M> ModelTable<K, M> {
    /// Returns the index of the model with this key, adding it if it's new.
    fn insert(&mut self, key: K, model: M) -> usize {
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let id = self.models.len();
        self.ids.insert(key.clone(), id);
        self.keys.push(key);
        self.models.push(model);
        id
    }
}

fn resistor_model_key(model: &ResistorModel) -> [u64; 5] {
    [
        model.tc1,
        model.tc2,
        model.default_width,
        model.default_length,
        model.tnom,
    ]
    .map(f64::to_bits)
}

fn tc_model_key(tc1: f64, tc2: f64, tnom: f64) -> [u64; 3] {
    [tc1, tc2, tnom].map(f64::to_bits)
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
fn resistor_model(spec: &ResistorSpec, default_tnom: f64) -> ResistorModel {
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
        tnom: tnom(model.and_then(|m| m.tnom.as_ref()), default_tnom),
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

fn capacitor_model(spec: &CapacitorSpec, default_tnom: f64) -> CapacitorModel {
    let model = spec.model.as_ref();
    let defaults = CapacitorModel::default();
    CapacitorModel {
        tc1: instance_or_model(&spec.tc1, model.map(|m| &m.tc1)).unwrap_or(defaults.tc1),
        tc2: instance_or_model(&spec.tc2, model.map(|m| &m.tc2)).unwrap_or(defaults.tc2),
        tnom: tnom(model.and_then(|m| m.tnom.as_ref()), default_tnom),
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

fn inductor_model(spec: &InductorSpec, default_tnom: f64) -> InductorModel {
    let model = spec.model.as_ref();
    let defaults = InductorModel::default();
    InductorModel {
        tc1: instance_or_model(&spec.tc1, model.map(|m| &m.tc1)).unwrap_or(defaults.tc1),
        tc2: instance_or_model(&spec.tc2, model.map(|m| &m.tc2)).unwrap_or(defaults.tc2),
        tnom: tnom(model.and_then(|m| m.tnom.as_ref()), default_tnom),
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

fn diode_model(spec: &DiodeSpec, default_tnom: f64) -> DiodeModel {
    let model = &spec.model;
    let defaults = DiodeModel::default();
    DiodeModel {
        is: value_or(&model.is, defaults.is),
        n: value_or(&model.n, defaults.n),
        rs: value_or(&model.rs, defaults.rs),
        eg: value_or(&model.eg, defaults.eg),
        xti: value_or(&model.xti, defaults.xti),
        tnom: tnom(model.tnom.as_ref(), default_tnom),
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
        temperature: device_temperature(&spec.temp, &spec.dtemp),
    }
}

fn bjt_model(spec: &BjtSpec, default_tnom: f64) -> BjtModel {
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
        xtb: value_or(&model.xtb, defaults.xtb),
        xti: value_or(&model.xti, defaults.xti),
        eg: value_or(&model.eg, defaults.eg),
        tnom: tnom(model.tnom.as_ref(), default_tnom),
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

/// An angle in radians. SPICE writes angles in degrees.
fn angle(value: &Option<Value>) -> f64 {
    value.as_ref().map_or(0.0, Value::angle_radians)
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
    use crate::reader::test_utils::parse_netlist;
    use rstest::rstest;
    use std::f64::consts::FRAC_PI_2;
    use std::path::PathBuf;

    /// Lower a netlist body (the lines between the title and `.end`).
    fn lower_body(body: &str) -> Lowered {
        lower(&parse_netlist(&format!("test\n{body}\n.end\n"))).expect("lower")
    }

    /// The error a netlist body fails with, from parsing or from lowering.
    fn body_error(body: &str) -> SpicyError {
        let netlist = format!("test\n{body}\n.end\n");
        let mut options = crate::reader::ParseOptions::new_with_source("inline.spicy", netlist);
        match crate::reader::parse(&mut options) {
            Err(error) => error,
            Ok(deck) => lower(&deck).expect_err("the body should be rejected"),
        }
    }

    #[test]
    fn temp_sets_the_circuit_temperature() {
        assert_eq!(lower_body("R1 a 0 1k").params.temp, DEFAULT_TEMPERATURE);
        let hot = lower_body(".temp 60\nR1 a 0 1k");
        assert_eq!(hot.params.temp, 60.0 + CELSIUS_TO_KELVIN);
        let from_param = lower_body(".param t=25\n.temp {t}\nR1 a 0 1k");
        assert_eq!(from_param.params.temp, 25.0 + CELSIUS_TO_KELVIN);
    }

    /// ngspice runs a list of temperatures as several runs; a lowered circuit is one run.
    #[test]
    fn temp_takes_one_value() {
        let error = body_error(".temp 25 60\nR1 a 0 1k");
        assert!(error.to_string().contains("one value"), "{error}");
    }

    #[test]
    fn options_set_tnom_and_solver_tolerances() {
        let lowered = lower_body(
            ".options tnom=25 reltol=1e-6\n.option vntol=1e-9 abstol=1e-15\n\
             .model QA NPN bf=200\n.model QB NPN tnom=50\nQ1 c b 0 QA\nQ2 c b 0 QB\nR1 a 0 1k",
        );
        let asked = SolverOptions {
            reltol: Some(1e-6),
            vntol: Some(1e-9),
            abstol: Some(1e-15),
        };
        assert_eq!(lowered.options, asked);
        let tnom: Vec<f64> = lowered.params.bjt_models.iter().map(|m| m.tnom).collect();
        assert_eq!(
            tnom,
            [25.0 + CELSIUS_TO_KELVIN, 50.0 + CELSIUS_TO_KELVIN],
            "`.options tnom` fills a card without one; a card's own TNOM wins"
        );
        assert_eq!(
            lowered.params.resistor_models[0].tnom,
            25.0 + CELSIUS_TO_KELVIN
        );
    }

    #[test]
    fn an_unknown_option_lists_the_accepted_ones() {
        let error = body_error(".options gmin=1e-12\nR1 a 0 1k");
        let message = error.to_string();
        assert!(
            message.contains("gmin") && message.contains("tnom, reltol, vntol, abstol"),
            "{message}"
        );
    }

    #[test]
    fn model_cards_carry_temperature_parameters() {
        let lowered = lower_body(
            ".model Q NPN xtb=1.5 xti=3.5 eg=1.2 tnom=25\nQ1 c b 0 Q\n\
             .model DX D eg=0.69 xti=2 tnom=30\nD1 a 0 DX\n\
             .model RM R tnom=50\nR1 a 0 1k RM\n\
             .model CM C tnom=40\nC1 a 0 1u CM\n\
             .model LM L tnom=35\nL1 a 0 1m LM",
        );
        let q = lowered.params.bjt_models[0];
        assert_eq!(
            (q.xtb, q.xti, q.eg, q.tnom),
            (1.5, 3.5, 1.2, 25.0 + CELSIUS_TO_KELVIN)
        );
        let d = lowered.params.diode_models[0];
        assert_eq!((d.eg, d.xti, d.tnom), (0.69, 2.0, 30.0 + CELSIUS_TO_KELVIN));
        assert_eq!(
            lowered.params.resistor_models[0].tnom,
            50.0 + CELSIUS_TO_KELVIN
        );
        assert_eq!(
            lowered.params.capacitor_models[0].tnom,
            40.0 + CELSIUS_TO_KELVIN
        );
        assert_eq!(
            lowered.params.inductor_models[0].tnom,
            35.0 + CELSIUS_TO_KELVIN
        );
    }

    #[test]
    fn bjt_instances_take_temp_and_dtemp() {
        let lowered = lower_body(".model Q NPN\nQ1 c b 0 Q temp=85\nQ2 c b 0 Q dtemp=5");
        let temperature: Vec<DeviceTemperature> =
            lowered.params.bjts.iter().map(|q| q.temperature).collect();
        assert_eq!(
            temperature,
            [
                DeviceTemperature::Fixed(85.0 + CELSIUS_TO_KELVIN),
                DeviceTemperature::Offset(5.0)
            ]
        );
    }

    #[test]
    fn instance_value_overrides_model_value() {
        let lowered = lower_body(".model RM R resistance=2k\nR1 a 0 mname=RM\nR2 a 0 5k RM");
        let r: Vec<f64> = lowered.params.resistors.iter().map(|p| p.r).collect();
        assert_eq!(r, [2e3, 5e3]);
    }

    #[test]
    fn values_are_read_correctly_rounded() {
        let lowered =
            lower_body("R1 a 0 4.69207e3\nR2 a 0 4.7k\nC1 a 0 100n\nC2 a 0 1.23456789e-11");
        let expected = |text: &str| text.parse::<f64>().unwrap();
        assert_eq!(lowered.params.resistors[0].r, expected("4692.07"));
        assert_eq!(lowered.params.resistors[1].r, expected("4700"));
        assert_eq!(lowered.params.capacitors[0].c, expected("100e-9"));
        assert_eq!(lowered.params.capacitors[1].c, expected("1.23456789e-11"));
    }

    /// Any value printed in Rust's shortest round-trip form (`{:e}`, what an
    /// exporter writes) must read back as the same `f64`, bit for bit.
    #[test]
    fn printed_values_read_back_bit_exact() {
        let mut state: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let values: Vec<f64> = (0..2000)
            .map(|_| {
                let mantissa = 1.0 + (next() >> 11) as f64 / (1u64 << 53) as f64;
                let exponent = (next() % 22) as i32 - 15;
                mantissa * 10f64.powi(exponent)
            })
            .collect();
        let body: String = values
            .iter()
            .enumerate()
            .map(|(i, v)| format!("R{i} a 0 {v:e}\n"))
            .collect();
        let lowered = lower_body(&body);
        for (i, v) in values.iter().enumerate() {
            assert_eq!(
                lowered.params.resistors[i].r.to_bits(),
                v.to_bits(),
                "R{i}: {v:e}"
            );
        }
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

    /// One model per `.model` card, as ngspice keeps: instances naming the same card
    /// share it (whatever case they write it in), and two cards stay two even when
    /// their numbers match, so a knob on one card never moves the other.
    #[test]
    fn each_model_card_is_one_model() {
        let lowered = lower_body(
            ".model QA NPN bf=200\n.model QB NPN bf=200\n.model QC NPN bf=50\n\
             Q1 c b 0 QA\nQ2 c b 0 qa\nQ3 c b 0 QB\nQ4 c b 0 QC",
        );
        let models: Vec<usize> = lowered
            .circuit
            .bjts
            .iter()
            .map(|q| q.model.index())
            .collect();
        assert_eq!(models, [0, 0, 1, 2]);
        let bf: Vec<f64> = lowered.params.bjt_models.iter().map(|m| m.bf).collect();
        assert_eq!(bf, [200.0, 200.0, 50.0]);
        assert_eq!(
            lowered.names.bjt_models,
            ["QA", "QB", "QC"],
            "each card's own spelling"
        );
    }

    /// Names may contain `_` and `.` after the first letter, as in ngspice:
    /// vendor names like `Q2N3904_ON`, and flattened names like `R.X1.R1`
    /// with its node `X1.mid`, which the writer produces.
    #[test]
    fn names_may_contain_underscores_and_dots() {
        let lowered = lower_body(
            ".model Q2N3904_ON NPN bf=150\nV_in in 0 1\nR_amp_r1 in n_1 1k\n\
             R.X1.R1 n_1 X1.mid 2.5k\nQ.X1.Q1 X1.mid n_1 0 Q2N3904_ON",
        );
        let names = &lowered.names;
        assert_eq!(names.vsources, ["V_in"]);
        assert_eq!(names.resistors, ["R_amp_r1", "R.X1.R1"]);
        assert_eq!(names.bjts, ["Q.X1.Q1"]);
        assert_eq!(names.bjt_models, ["Q2N3904_ON"]);
        assert_eq!(names.nodes, ["0", "in", "n_1", "X1.mid"]);
        let r: Vec<f64> = lowered.params.resistors.iter().map(|r| r.r).collect();
        assert_eq!(r, [1e3, 2.5e3]);
    }

    /// Every spelling ngspice accepts for a value (res.c, cap.c, ind.c). On a
    /// card that includes the instance's name, which ngspice takes as a
    /// default for the card's instances (inpgmod.c).
    #[test]
    fn value_spellings_match_ngspice() {
        let lowered = lower_body(
            ".model RA R r=1k\n.model RB R res=2k\n.model RC R resistance=3k\n\
             R1 a 0 RA\nR2 a 0 RB\nR3 a 0 RC\nR4 a 0 resistance=4k\nR5 a 0 r=5k\n\
             .model CA C cap=1n\n.model CB C capacitance=2n\n\
             C1 a 0 CA\nC2 a 0 CB\nC3 a 0 capacitance=3n\nC4 a 0 cap=4n\nC5 a 0 c=5n\n\
             .model LA L ind=1u\n.model LB L inductance=2u\n\
             L1 a 0 LA\nL2 a 0 LB\nL3 a 0 inductance=3u",
        );
        let params = &lowered.params;
        let r: Vec<f64> = params.resistors.iter().map(|r| r.r).collect();
        assert_eq!(r, [1e3, 2e3, 3e3, 4e3, 5e3]);
        let c: Vec<f64> = params.capacitors.iter().map(|c| c.c).collect();
        assert_eq!(c, [1e-9, 2e-9, 3e-9, 4e-9, 5e-9]);
        let l: Vec<f64> = params.inductors.iter().map(|l| l.l).collect();
        assert_eq!(l, [1e-6, 2e-6, 3e-6]);
    }

    /// A flag like `off` counts wherever it stands, as in ngspice, including
    /// right after the model name, before any value.
    #[test]
    fn flags_count_anywhere_on_the_line() {
        let lowered = lower_body(
            ".model DM D\n.model QM NPN\nD1 a 0 DM off\nD2 a 0 DM 2 off\nD3 a 0 DM\nQ1 c b 0 QM off",
        );
        let diodes = &lowered.params.diodes;
        let off: Vec<bool> = diodes.iter().map(|d| d.off).collect();
        let area: Vec<f64> = diodes.iter().map(|d| d.area).collect();
        assert_eq!(off, [true, true, false]);
        assert_eq!(area, [1.0, 2.0, 1.0]);
        assert!(lowered.params.bjts[0].off);
    }

    /// The model name may come before the value; for capacitors and inductors
    /// it's the only order ngspice-42 reads (`C1 a 0 1n CM` is an error there).
    #[test]
    fn the_model_may_come_before_the_value() {
        let lowered = lower_body(
            ".model RM R tc1=1m\n.model CM C tc1=2m\n.model LM L tc1=3m\n\
             R1 a 0 RM 1k\nC1 a 0 CM 1n\nL1 a 0 LM 1u m=2",
        );
        let params = &lowered.params;
        assert_eq!(
            (
                params.resistors[0].r,
                params.capacitors[0].c,
                params.inductors[0].l
            ),
            (1e3, 1e-9, 1e-6)
        );
        assert_eq!(params.inductors[0].m, 2.0);
        let tc1 = (
            params.resistor_models[0].tc1,
            params.capacitor_models[0].tc1,
            params.inductor_models[0].tc1,
        );
        assert_eq!(tc1, (1e-3, 2e-3, 3e-3));
    }

    /// `res` names the value only on a card; ngspice rejects it on an instance.
    #[test]
    fn res_on_an_instance_is_an_error() {
        let error = body_error("R1 a 0 res=2k");
        assert!(error.to_string().contains("res"), "{error}");
    }

    /// On a card, ngspice reads `c` as the flag for the model's type and drops
    /// the value, so the capacitor gets 0 F with no warning.
    #[test]
    fn c_on_a_capacitor_card_is_an_error() {
        let error = body_error(".model CM C c=3n\nC1 a 0 CM");
        assert!(error.to_string().contains("c"), "{error}");
    }

    /// ngspice reads `1.5rad` as 1.5 degrees, so the suffix is refused rather
    /// than read differently from ngspice.
    #[test]
    fn the_rad_suffix_is_an_error() {
        let error = body_error("V1 a 0 AC 1 1.5rad\nR1 a 0 1k");
        assert!(error.to_string().contains("degrees"), "{error}");
    }

    #[test]
    fn equal_diode_cards_stay_two_models() {
        let lowered =
            lower_body(".model DA D is=1e-14\n.model DB D is=1e-14\nD1 a 0 DA\nD2 a 0 DB");
        assert_eq!(lowered.params.diode_models.len(), 2);
        assert_eq!(lowered.names.diode_models, ["DA", "DB"]);
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
