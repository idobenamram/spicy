//! The netlist writer: a [`Lowered`] circuit as a SPICE netlist that ngspice
//! runs and the reader reads back to the same circuit
//! (docs/ecad/netlist_writer.md).

mod names;

use std::f64::consts::PI;

use spicy_circuit::{
    AcSpacing, Analysis, CircuitNames, DeviceTemperature, Lowered, NodeId, Phasor, Polarity,
    SolverOptions, SourceRef, TwoTerminal, Waveform,
};
use thiserror::Error;

use crate::CELSIUS_TO_KELVIN;
use names::{SpiceNames, spice_names};

/// A written netlist.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    pub text: String,
    /// Each node's and device's name in `text`, indexed like the circuit.
    /// ngspice reports names in lowercase, so match them ignoring case.
    pub names: CircuitNames,
}

#[derive(Debug, Error)]
pub enum WriteError {
    #[error("'{name}' can't be written as a SPICE name")]
    UnreadableName { name: String },

    #[error(
        "{device}: {function} gives an argument after one left for SPICE to fill in, which a netlist can't express"
    )]
    Inexpressible {
        device: String,
        function: &'static str,
    },
}

pub fn write(lowered: &Lowered) -> Result<Written, WriteError> {
    let spice = spice_names(lowered, title(&lowered.names.title))?;
    let names = &spice.names;

    let mut lines = vec![names.title.clone(), temperature_line(lowered.params.temp)];
    lines.extend(options_line(&lowered.options));
    lines.extend(source_lines(lowered, names)?);
    lines.extend(resistor_lines(lowered, &spice));
    lines.extend(capacitor_lines(lowered, &spice));
    lines.extend(inductor_lines(lowered, &spice));
    lines.extend(diode_lines(lowered, names));
    lines.extend(bjt_lines(lowered, names));
    lines.extend(model_cards(lowered, &spice));
    lines.extend(lowered.analyses.iter().map(|a| analysis_line(a, names)));
    lines.push(".end".to_string());

    Ok(Written {
        text: lines.join("\n") + "\n",
        names: spice.names,
    })
}

/// The first line, which SPICE always reads as the title, so it must be one
/// non-empty line.
fn title(title: &str) -> String {
    let first = title.lines().next().unwrap_or_default();
    if first.trim().is_empty() {
        "untitled".to_string()
    } else {
        first.to_string()
    }
}

fn temperature_line(temp: f64) -> String {
    format!(".temp {}", celsius(temp))
}

/// The tolerances the source asked for; none of them, no line.
fn options_line(options: &SolverOptions) -> Option<String> {
    let set: Vec<String> = [
        ("reltol", options.reltol),
        ("vntol", options.vntol),
        ("abstol", options.abstol),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|v| format!("{key}={}", number(v))))
    .collect();
    (!set.is_empty()).then(|| format!(".options {}", set.join(" ")))
}

fn source_lines(lowered: &Lowered, names: &CircuitNames) -> Result<Vec<String>, WriteError> {
    let circuit = &lowered.circuit;
    let params = &lowered.params;
    let voltage = circuit.vsources.iter().zip(&params.vsources);
    let current = circuit.isources.iter().zip(&params.isources);
    voltage
        .zip(&names.vsources)
        .chain(current.zip(&names.isources))
        .map(|((terminals, source), name)| {
            let mut fields = two_terminal(name, terminals, names);
            fields.push(waveform(name, &source.waveform)?);
            if source.ac != Phasor::default() {
                let phase = degrees(source.ac.phase);
                fields.push(format!("AC {} {phase}", number(source.ac.magnitude)));
            }
            Ok(fields.join(" "))
        })
        .collect()
}

// Values are written as named parameters (`r=1k`), so nothing depends on
// position. The model name has no named form: it comes right after the nodes.

fn resistor_lines<'a>(
    lowered: &'a Lowered,
    spice: &'a SpiceNames,
) -> impl Iterator<Item = String> + 'a {
    let names = &spice.names;
    let params = &lowered.params.resistors;
    lowered
        .circuit
        .resistors
        .iter()
        .zip(params)
        .zip(&names.resistors)
        .map(move |((resistor, p), name)| {
            let mut fields = vec![
                name.clone(),
                node(names, resistor.positive),
                node(names, resistor.negative),
            ];
            fields.extend(spice.resistor_models[resistor.model.index()].clone());
            fields.push(format!("r={}", number(p.r)));
            fields.extend(p.r_ac.map(|ac| format!("ac={}", number(ac))));
            fields.extend(param("m", p.m, 1.0));
            fields.extend(temperature(p.temperature));
            if !p.noisy {
                fields.push("noisy=0".to_string());
            }
            fields.join(" ")
        })
}

fn capacitor_lines<'a>(
    lowered: &'a Lowered,
    spice: &'a SpiceNames,
) -> impl Iterator<Item = String> + 'a {
    let names = &spice.names;
    let params = &lowered.params.capacitors;
    lowered
        .circuit
        .capacitors
        .iter()
        .zip(params)
        .zip(&names.capacitors)
        .map(move |((capacitor, p), name)| {
            let mut fields = vec![
                name.clone(),
                node(names, capacitor.positive),
                node(names, capacitor.negative),
            ];
            fields.extend(spice.capacitor_models[capacitor.model.index()].clone());
            fields.push(format!("c={}", number(p.c)));
            fields.extend(param("m", p.m, 1.0));
            fields.extend(temperature(p.temperature));
            fields.extend(param("ic", p.ic, 0.0));
            fields.join(" ")
        })
}

fn inductor_lines<'a>(
    lowered: &'a Lowered,
    spice: &'a SpiceNames,
) -> impl Iterator<Item = String> + 'a {
    let names = &spice.names;
    let params = &lowered.params.inductors;
    lowered
        .circuit
        .inductors
        .iter()
        .zip(params)
        .zip(&names.inductors)
        .map(move |((inductor, p), name)| {
            let mut fields = vec![
                name.clone(),
                node(names, inductor.positive),
                node(names, inductor.negative),
            ];
            fields.extend(spice.inductor_models[inductor.model.index()].clone());
            // ngspice has no `l=` on an inductor line (ind.c).
            fields.push(format!("inductance={}", number(p.l)));
            fields.extend(param("nt", p.nt, 0.0));
            fields.extend(param("m", p.m, 1.0));
            fields.extend(temperature(p.temperature));
            fields.extend(param("ic", p.ic, 0.0));
            fields.join(" ")
        })
}

fn diode_lines<'a>(
    lowered: &'a Lowered,
    names: &'a CircuitNames,
) -> impl Iterator<Item = String> + 'a {
    let params = &lowered.params.diodes;
    lowered
        .circuit
        .diodes
        .iter()
        .zip(params)
        .zip(&names.diodes)
        .map(move |((diode, p), name)| {
            let mut fields = vec![
                name.clone(),
                node(names, diode.positive),
                node(names, diode.negative),
                names.diode_models[diode.model.index()].clone(),
            ];
            fields.extend(param("area", p.area, 1.0));
            fields.extend(param("m", p.m, 1.0));
            fields.extend(param("pj", p.pj, 0.0));
            fields.extend(param("ic", p.ic, 0.0));
            fields.extend(temperature(p.temperature));
            fields.extend(param("lm", p.lm, 0.0));
            fields.extend(param("wm", p.wm, 0.0));
            fields.extend(param("lp", p.lp, 0.0));
            fields.extend(param("wp", p.wp, 0.0));
            if p.off {
                fields.push("off".to_string());
            }
            fields.join(" ")
        })
}

fn bjt_lines<'a>(
    lowered: &'a Lowered,
    names: &'a CircuitNames,
) -> impl Iterator<Item = String> + 'a {
    let params = &lowered.params.bjts;
    lowered
        .circuit
        .bjts
        .iter()
        .zip(params)
        .zip(&names.bjts)
        .map(move |((bjt, p), name)| {
            let mut fields = vec![
                name.clone(),
                node(names, bjt.collector),
                node(names, bjt.base),
                node(names, bjt.emitter),
                names.bjt_models[bjt.model.index()].clone(),
            ];
            fields.extend(param("area", p.area, 1.0));
            fields.extend(param("m", p.m, 1.0));
            if p.ic_vbe != 0.0 || p.ic_vce != 0.0 {
                fields.push(format!("ic={},{}", number(p.ic_vbe), number(p.ic_vce)));
            }
            fields.extend(temperature(p.temperature));
            if p.off {
                fields.push("off".to_string());
            }
            fields.join(" ")
        })
}

/// The `.model` cards: every parameter of the diode and BJT cards, and the
/// writer's own cards for resistor, capacitor and inductor models that differ
/// from the default one.
fn model_cards(lowered: &Lowered, spice: &SpiceNames) -> Vec<String> {
    let params = &lowered.params;
    let names = &spice.names;
    let mut cards = Vec::new();
    for (m, name) in params.resistor_models.iter().zip(&spice.resistor_models) {
        if let Some(name) = name {
            cards.push(format!(
                ".model {name} R tc1={} tc2={} w={} l={} tnom={}",
                number(m.tc1),
                number(m.tc2),
                number(m.default_width),
                number(m.default_length),
                celsius(m.tnom),
            ));
        }
    }
    for (m, name) in params.capacitor_models.iter().zip(&spice.capacitor_models) {
        if let Some(name) = name {
            let (tc1, tc2, tnom) = (number(m.tc1), number(m.tc2), celsius(m.tnom));
            cards.push(format!(".model {name} C tc1={tc1} tc2={tc2} tnom={tnom}"));
        }
    }
    for (m, name) in params.inductor_models.iter().zip(&spice.inductor_models) {
        if let Some(name) = name {
            let (tc1, tc2, tnom) = (number(m.tc1), number(m.tc2), celsius(m.tnom));
            cards.push(format!(".model {name} L tc1={tc1} tc2={tc2} tnom={tnom}"));
        }
    }
    for (m, name) in params.diode_models.iter().zip(&names.diode_models) {
        cards.push(format!(
            ".model {name} D is={} n={} rs={} eg={} xti={} tnom={}",
            number(m.is),
            number(m.n),
            number(m.rs),
            number(m.eg),
            number(m.xti),
            celsius(m.tnom),
        ));
    }
    for (m, name) in params.bjt_models.iter().zip(&names.bjt_models) {
        let polarity = match m.polarity {
            Polarity::Npn => "NPN",
            Polarity::Pnp => "PNP",
        };
        cards.push(format!(
            ".model {name} {polarity} is={} bf={} br={} nf={} nr={} xtb={} xti={} eg={} tnom={}",
            number(m.is),
            number(m.bf),
            number(m.br),
            number(m.nf),
            number(m.nr),
            number(m.xtb),
            number(m.xti),
            number(m.eg),
            celsius(m.tnom),
        ));
    }
    cards
}

fn analysis_line(analysis: &Analysis, names: &CircuitNames) -> String {
    match analysis {
        Analysis::Op => ".op".to_string(),
        Analysis::Dc(dc) => {
            let source = match dc.source {
                SourceRef::Voltage(id) => &names.vsources[id.index()],
                SourceRef::Current(id) => &names.isources[id.index()],
            };
            let (start, stop, step) = (number(dc.start), number(dc.stop), number(dc.step));
            format!(".dc {source} {start} {stop} {step}")
        }
        Analysis::Ac(ac) => {
            let (spacing, points) = match ac.spacing {
                AcSpacing::Decade(n) => ("dec", n),
                AcSpacing::Octave(n) => ("oct", n),
                AcSpacing::Linear(n) => ("lin", n),
            };
            let (start, stop) = (number(ac.start), number(ac.stop));
            format!(".ac {spacing} {points} {start} {stop}")
        }
        Analysis::Tran(tran) => {
            let uic = if tran.uic { " uic" } else { "" };
            format!(".tran {} {}{uic}", number(tran.step), number(tran.stop))
        }
    }
}

/// A source's value over time: `DC 5`, or a function like `SIN(0 1 1000)`.
fn waveform(device: &str, waveform: &Waveform) -> Result<String, WriteError> {
    let function = |name, args: &[Arg]| function(device, name, args);
    match *waveform {
        Waveform::Dc(value) => Ok(format!("DC {}", number(value))),
        Waveform::Pulse {
            initial,
            pulsed,
            delay,
            rise,
            fall,
            width,
            period,
            count,
        } => function(
            "PULSE",
            &[
                Arg::given(initial),
                Arg::given(pulsed),
                Arg::or_zero(delay),
                Arg::optional(rise),
                Arg::optional(fall),
                Arg::optional(width),
                Arg::optional(period),
                Arg::count(count),
            ],
        ),
        Waveform::Sine {
            offset,
            amplitude,
            frequency,
            delay,
            damping,
            phase,
        } => function(
            "SIN",
            &[
                Arg::given(offset),
                Arg::given(amplitude),
                Arg::optional(frequency),
                Arg::or_zero(delay),
                Arg::or_zero(damping),
                Arg::phase(phase),
            ],
        ),
        Waveform::Exp {
            initial,
            pulsed,
            rise_delay,
            rise_tau,
            fall_delay,
            fall_tau,
        } => function(
            "EXP",
            &[
                Arg::given(initial),
                Arg::given(pulsed),
                Arg::or_zero(rise_delay),
                Arg::optional(rise_tau),
                Arg::optional(fall_delay),
                Arg::optional(fall_tau),
            ],
        ),
    }
}

/// `NAME(a b c)`, leaving out the trailing arguments the source didn't give.
fn function(device: &str, name: &'static str, args: &[Arg]) -> Result<String, WriteError> {
    let used = args
        .iter()
        .rposition(|a| a.text.is_some())
        .map_or(0, |i| i + 1);
    let texts = args[..used]
        .iter()
        .map(|a| a.text.as_deref().or(a.filler))
        .collect::<Option<Vec<&str>>>()
        .ok_or_else(|| WriteError::Inexpressible {
            device: device.to_string(),
            function: name,
        })?;
    Ok(format!("{name}({})", texts.join(" ")))
}

/// One positional argument of a waveform function: its text if the source
/// gave it, and what stands in for it when a later argument is given. There's
/// no stand-in for an argument SPICE fills in from the time step or stop time.
struct Arg {
    text: Option<String>,
    filler: Option<&'static str>,
}

impl Arg {
    fn given(value: f64) -> Self {
        Self {
            text: Some(number(value)),
            filler: None,
        }
    }

    /// An argument SPICE fills in itself when it's left out.
    fn optional(value: Option<f64>) -> Self {
        Self {
            text: value.map(number),
            filler: None,
        }
    }

    /// An argument that defaults to 0.
    fn or_zero(value: f64) -> Self {
        Self {
            text: (value != 0.0).then(|| number(value)),
            filler: Some("0"),
        }
    }

    /// A phase in radians, written in degrees.
    fn phase(radians: f64) -> Self {
        Self {
            text: (radians != 0.0).then(|| degrees(radians)),
            filler: Some("0"),
        }
    }

    /// A number of pulses; 0 repeats forever.
    fn count(count: u64) -> Self {
        Self {
            text: (count != 0).then(|| count.to_string()),
            filler: Some("0"),
        }
    }
}

fn two_terminal(name: &str, terminals: &TwoTerminal, names: &CircuitNames) -> Vec<String> {
    vec![
        name.to_string(),
        node(names, terminals.positive),
        node(names, terminals.negative),
    ]
}

fn node(names: &CircuitNames, id: NodeId) -> String {
    names.nodes[id.index()].clone()
}

/// `key=value`, left out when the value is the default.
fn param(key: &str, value: f64, default: f64) -> Option<String> {
    (value != default).then(|| format!("{key}={}", number(value)))
}

/// A device's own temperature (`temp`, °C), or its offset from the circuit's
/// (`dtemp`, left out when 0).
fn temperature(temperature: DeviceTemperature) -> Option<String> {
    match temperature {
        DeviceTemperature::Fixed(kelvin) => Some(format!("temp={}", celsius(kelvin))),
        DeviceTemperature::Offset(delta) => param("dtemp", delta, 0.0),
    }
}

/// A temperature in kelvin, written in °C as SPICE reads it.
fn celsius(kelvin: f64) -> String {
    converted(kelvin - CELSIUS_TO_KELVIN)
}

/// An angle in radians, written in degrees as SPICE reads it.
fn degrees(radians: f64) -> String {
    converted(radians * 180.0 / PI)
}

/// The shortest text that reads back to `x`: plain for everyday magnitudes,
/// exponent form for the very small and very large (`2000`, `0.01`, `4.7e-9`).
/// No SPICE suffixes: `1M` would be 0.001.
fn number(x: f64) -> String {
    debug_assert!(x.is_finite(), "a lowered circuit holds finite numbers");
    if x == 0.0 || (1e-3..1e6).contains(&x.abs()) {
        format!("{x}")
    } else {
        format!("{x:e}")
    }
}

/// A value that went through a unit conversion, rounded to 15 significant
/// digits. Every decimal of up to 15 digits survives a trip through `f64`
/// (C's `DBL_DIG`), so a typed 30° comes back as `30`, not
/// `29.999999999999996`.
fn converted(x: f64) -> String {
    let rounded: f64 = format!("{x:.14e}")
        .parse()
        .expect("a formatted f64 reads back");
    number(rounded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::{ParseOptions, lower, parse};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Temperatures, cards and every instance parameter the writer writes.
    const DEVICES: &str = "devices
.temp 60
.options tnom=25 reltol=1e-4 abstol=1e-15
.model RM R tc1=1m tc2=2u tnom=50
.model CM C tc1=3m
.model LM L tnom=35
.model DX D is=2e-14 n=1.5 eg=0.69 xti=2
.model QP PNP bf=80 xtb=1.5 tnom=30
V1 in 0 DC 5
R1 in a 1k RM temp=85
R2 a 0 4.7k tc1=1m dtemp=10 m=2 ac=5k noisy=0
C1 a 0 10u CM ic=1.5 m=3
L1 a b 1m LM nt=10 ic=1m
R3 b 0 1k
D1 a 0 DX area=2 pj=1u off ic=0.6 dtemp=5
Q1 c b 0 QP area=3 ic=0.7,0.3 off temp=100
R4 in c 1k
.op
.end
";

    /// Every waveform, with arguments left out, and phases that don't survive
    /// a plain trip through radians (30°, 75°, 150°, 300°).
    const WAVEFORMS: &str = "waveforms
V1 a 0 PULSE(0 5 1n 2n 3n 1u 2u 4)
V2 b 0 PULSE(0 5)
V3 c 0 PULSE(0 5 0 1n)
V4 d 0 SIN(1.2 0.01 1k 0 0 30) AC 1 75
V5 e 0 SIN(0 1 1k 1m 100 150)
I1 f 0 EXP(0 1m 1u)
I2 g 0 EXP(0 1m 1u 2u 3u 4u) AC 2 300
R1 a 0 1k
R2 b 0 1k
R3 c 0 1k
R4 d 0 1k
R5 e 0 1k
R6 f 0 1k
R7 g 0 1k
.tran 1n 10u uic
.ac oct 5 1 1meg
.dc I1 0 1m 0.1m
.end
";

    fn read(path: &str, text: &str) -> Lowered {
        let mut options = ParseOptions::new_with_source(path, text.to_string());
        lower(&parse(&mut options).expect("parse")).expect("lower")
    }

    /// Writes `original`, reads it back and checks it's the same circuit.
    /// Nodes are matched by name, since the reader numbers them in the order
    /// the written netlist first uses them; everything else must be equal.
    fn assert_round_trip(original: &Lowered) -> Written {
        let written = write(original).expect("write");
        let mut back = read("written.spicy", &written.text);
        renumber_nodes(&mut back, &written.names.nodes);
        let text = &written.text;
        assert_eq!(back.names, written.names, "{text}");
        assert_eq!(back.circuit, original.circuit, "{text}");
        assert_eq!(back.params, original.params, "{text}");
        assert_eq!(back.analyses, original.analyses, "{text}");
        assert_eq!(back.options, original.options, "{text}");
        written
    }

    /// Puts `lowered`'s nodes in the order of `names`.
    fn renumber_nodes(lowered: &mut Lowered, names: &[String]) {
        let ids: Vec<NodeId> = lowered
            .names
            .nodes
            .iter()
            .map(|name| {
                let index = names.iter().position(|n| n == name);
                NodeId::new(index.expect("every node read back was written"))
            })
            .collect();
        let renumber = |node: &mut NodeId| *node = ids[node.index()];
        let circuit = &mut lowered.circuit;
        for r in &mut circuit.resistors {
            renumber(&mut r.positive);
            renumber(&mut r.negative);
        }
        for c in &mut circuit.capacitors {
            renumber(&mut c.positive);
            renumber(&mut c.negative);
        }
        for l in &mut circuit.inductors {
            renumber(&mut l.positive);
            renumber(&mut l.negative);
        }
        for d in &mut circuit.diodes {
            renumber(&mut d.positive);
            renumber(&mut d.negative);
        }
        for q in &mut circuit.bjts {
            renumber(&mut q.collector);
            renumber(&mut q.base);
            renumber(&mut q.emitter);
        }
        for s in circuit.vsources.iter_mut().chain(&mut circuit.isources) {
            renumber(&mut s.positive);
            renumber(&mut s.negative);
        }
        lowered.names.nodes = names.to_vec();
    }

    /// The netlists in `circuits/` and the reader's test inputs.
    fn netlist_files() -> Vec<PathBuf> {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files: Vec<PathBuf> = [
            crate_dir.join("../../circuits"),
            crate_dir.join("tests/parser_inputs"),
        ]
        .iter()
        .flat_map(|dir| std::fs::read_dir(dir).expect("netlist directory"))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "spicy"))
        .collect();
        files.sort();
        files
    }

    /// Every netlist the tests write: the files, then the inline cases.
    fn netlists() -> Vec<(String, String)> {
        let files = netlist_files().into_iter().map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read_to_string(&path).expect("read netlist"))
        });
        let inline = [("inline_devices", DEVICES), ("inline_waveforms", WAVEFORMS)]
            .map(|(name, text)| (name.to_string(), text.to_string()));
        files.chain(inline).collect()
    }

    #[test]
    fn netlists_read_back_the_same() {
        for (name, text) in netlists() {
            let written = assert_round_trip(&read(&format!("{name}.spicy"), &text));
            insta::assert_snapshot!(format!("written-{name}"), written.text);
        }
    }

    #[test]
    fn names_that_clash_in_spice_are_renamed() {
        let mut lowered = read(
            "names.spicy",
            "names\nV1 in 0 1\nR1 in gnd 1k\nR2 gnd 0 1k\n.op\n.end\n",
        );
        lowered.names.resistors = vec!["amp.r1".into(), "amp.R1".into()];
        let written = assert_round_trip(&lowered);
        assert_eq!(written.names.resistors, ["R.amp.r1", "R.amp.R1_2"]);
        assert_eq!(written.names.nodes, ["0", "in", "gnd_2"]);
    }

    #[test]
    fn subcircuit_devices_get_ngspice_names() {
        let lowered = read(
            "subckt.spicy",
            "subckt\n.subckt DIV a b\nR1 a mid 1k\nR2 mid b 1k\n.ends\nV1 in 0 1\nX1 in 0 DIV\n.op\n.end\n",
        );
        let written = assert_round_trip(&lowered);
        assert_eq!(written.names.resistors, ["R.X1.R1", "R.X1.R2"]);
        assert!(written.names.nodes.contains(&"X1.mid".to_string()));
    }

    #[test]
    fn a_waveform_a_netlist_cant_express_is_an_error() {
        let mut lowered = read(
            "pulse.spicy",
            "pulse\nV1 a 0 PULSE(0 5 0 1n 1n)\nR1 a 0 1k\n.end\n",
        );
        if let Waveform::Pulse { rise, .. } = &mut lowered.params.vsources[0].waveform {
            *rise = None;
        }
        let error = write(&lowered).expect_err("rise left out, fall given");
        assert!(matches!(error, WriteError::Inexpressible { .. }), "{error}");
    }

    #[test]
    fn a_name_the_reader_cant_read_is_an_error() {
        let mut lowered = read("name.spicy", "name\nV1 a 0 1\nR1 a 0 1k\n.end\n");
        lowered.names.nodes[1] = "a-b".to_string();
        let error = write(&lowered).expect_err("a dash in a node name");
        assert!(
            matches!(error, WriteError::UnreadableName { .. }),
            "{error}"
        );
    }

    #[test]
    fn numbers_are_plain_for_everyday_sizes() {
        let values = [2000.0, 0.01, 0.001, 4.7e-9, 1e-16, 1e6, 999999.0, 0.0, -5.0];
        let written = values.map(number);
        let expected = [
            "2000", "0.01", "0.001", "4.7e-9", "1e-16", "1e6", "999999", "0", "-5",
        ];
        assert_eq!(written, expected);
    }

    /// A typed angle or temperature is written back as typed, although the
    /// circuit holds it in radians or kelvin.
    #[test]
    fn converted_values_are_written_as_typed() {
        for d in -360..=360 {
            assert_eq!(degrees(f64::from(d) * PI / 180.0), d.to_string());
        }
        for c in -55..=200 {
            assert_eq!(celsius(f64::from(c) + CELSIUS_TO_KELVIN), c.to_string());
        }
        assert_eq!(celsius(25.5 + CELSIUS_TO_KELVIN), "25.5");
    }

    /// Every written netlist loads in ngspice without a complaint. The
    /// analyses are left out, so ngspice only reads the circuit. Skipped
    /// where ngspice isn't installed.
    #[test]
    fn ngspice_reads_written_netlists() {
        if Command::new("ngspice").arg("--version").output().is_err() {
            eprintln!("ngspice not found: skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!("spicy_writer_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        for (name, text) in netlists() {
            let written = write(&read(&format!("{name}.spicy"), &text)).expect("write");
            let is_analysis = |line: &&str| {
                [".op", ".dc ", ".ac ", ".tran "]
                    .iter()
                    .any(|a| line.starts_with(a))
            };
            let deck: String = written
                .text
                .lines()
                .filter(|l| !is_analysis(l))
                .map(|l| format!("{l}\n"))
                .collect();
            let path = dir.join(format!("{name}.cir"));
            std::fs::write(&path, &deck).expect("write deck");
            let output = Command::new("ngspice")
                .arg("-b")
                .arg(&path)
                .output()
                .expect("run ngspice");
            let log = String::from_utf8_lossy(&output.stdout).into_owned()
                + &String::from_utf8_lossy(&output.stderr);
            let complaints: Vec<&str> = log
                .lines()
                .filter(|line| {
                    let line = line.to_ascii_lowercase();
                    ["error", "warning", "unrecognized", "unknown"]
                        .iter()
                        .any(|w| line.contains(w))
                })
                .collect();
            assert!(complaints.is_empty(), "{name}: {complaints:#?}\n{deck}");
        }
        std::fs::remove_dir_all(&dir).expect("remove temp dir");
    }
}
