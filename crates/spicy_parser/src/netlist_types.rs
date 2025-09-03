// https://ngspice.sourceforge.io/docs/ngspice-manual.pdf

use crate::{expr::Value, lexer::Span};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Node {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandType {
    AC,
    DC,
    Op,
    Subcircuit,
    Ends,
    Param,
    End,
}

impl CommandType {
    pub fn from_str(s: &str) -> Option<CommandType> {
        match s {
            "AC" | "ac" => Some(CommandType::AC),
            "DC" | "dc" => Some(CommandType::DC),
            "OP" | "op" => Some(CommandType::Op),
            "SUBCKT" | "subckt" => Some(CommandType::Subcircuit),
            "ENDS" | "ends" => Some(CommandType::Ends),
            "PARAM" | "param" => Some(CommandType::Param),
            "END" | "end" => Some(CommandType::End),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpCommand {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct DcCommand {
    pub span: Span,
    pub srcnam: String,
    pub vstart: Value,
    pub vstop: Value,
    pub vincr: Value,
}

#[derive(Debug, Clone)]
pub enum Command {
    Op(OpCommand),
    Dc(DcCommand),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceType {
    Resistor,
    Capacitor,
    Inductor,
    VoltageSource,
    CurrentSource,
    Subcircuit,
}

impl DeviceType {
    pub fn from_str(s: &str) -> Option<DeviceType> {
        match s.to_uppercase().to_string().as_str() {
            "R" => Some(DeviceType::Resistor),
            "C" => Some(DeviceType::Capacitor),
            "L" => Some(DeviceType::Inductor),
            "V" => Some(DeviceType::VoltageSource),
            "I" => Some(DeviceType::CurrentSource),
            "X" => Some(DeviceType::Subcircuit),
            _ => None,
        }
    }

    pub fn to_char(&self) -> char {
        match self {
            DeviceType::Resistor => 'R',
            DeviceType::Capacitor => 'C',
            DeviceType::Inductor => 'L',
            DeviceType::VoltageSource => 'V',
            DeviceType::CurrentSource => 'I',
            DeviceType::Subcircuit => 'X',
        }
    }
}

#[derive(Debug, Clone)]
pub struct Resistor {
    pub name: String,
    pub span: Span,
    pub positive: Node,
    pub negative: Node,
    pub resistance: Value,
    pub ac: Option<Value>,
    pub m: Option<Value>,
    pub scale: Option<Value>,
    pub temp: Option<Value>,
    pub dtemp: Option<Value>,
    pub tc1: Option<Value>,
    pub tc2: Option<Value>,
    pub noisy: Option<bool>,
}

impl Resistor {
    pub fn new(
        name: String,
        span: Span,
        positive: Node,
        negative: Node,
        resistance: Value,
    ) -> Self {
        Self {
            name,
            span,
            positive,
            negative,
            resistance,
            ac: None,
            m: None,
            scale: None,
            temp: None,
            dtemp: None,
            tc1: None,
            tc2: None,
            noisy: None,
        }
    }

    pub fn set_ac(&mut self, value: Value) {
        self.ac = Some(value);
    }
    pub fn set_m(&mut self, value: Value) {
        self.m = Some(value);
    }

    pub fn set_scale(&mut self, value: Value) {
        self.scale = Some(value);
    }
    pub fn set_temp(&mut self, value: Value) {
        self.temp = Some(value);
    }

    pub fn set_dtemp(&mut self, value: Value) {
        self.dtemp = Some(value);
    }
    pub fn set_tc1(&mut self, value: Value) {
        self.tc1 = Some(value);
    }

    pub fn set_tc2(&mut self, value: Value) {
        self.tc2 = Some(value);
    }
    pub fn set_noisy(&mut self, value: bool) {
        self.noisy = Some(value);
    }
}

#[derive(Debug, Clone)]
pub struct Capacitor {
    pub name: String,
    pub span: Span,
    pub positive: Node,
    pub negative: Node,
    pub capacitance: Value,
    pub mname: Option<String>,
    pub m: Option<Value>,
    pub scale: Option<Value>,
    pub temp: Option<Value>,
    pub dtemp: Option<Value>,
    pub tc1: Option<Value>,
    pub tc2: Option<Value>,
    pub ic: Option<Value>,
}

impl Capacitor {
    pub fn new(
        name: String,
        span: Span,
        positive: Node,
        negative: Node,
        capacitance: Value,
    ) -> Self {
        Self {
            name,
            span,
            positive,
            negative,
            capacitance,
            mname: None,
            m: None,
            scale: None,
            temp: None,
            dtemp: None,
            tc1: None,
            tc2: None,
            ic: None,
        }
    }

    pub fn set_m(&mut self, value: Value) {
        self.m = Some(value);
    }

    pub fn set_scale(&mut self, value: Value) {
        self.scale = Some(value);
    }
    pub fn set_temp(&mut self, value: Value) {
        self.temp = Some(value);
    }

    pub fn set_dtemp(&mut self, value: Value) {
        self.dtemp = Some(value);
    }
    pub fn set_tc1(&mut self, value: Value) {
        self.tc1 = Some(value);
    }

    pub fn set_tc2(&mut self, value: Value) {
        self.tc2 = Some(value);
    }
    pub fn set_ic(&mut self, value: Value) {
        self.ic = Some(value);
    }
}

#[derive(Debug, Clone)]
pub struct Inductor {
    pub name: String,
    pub span: Span,
    pub positive: Node,
    pub negative: Node,
    pub inductance: Value,
    pub nt: Option<Value>,
    pub m: Option<Value>,
    pub scale: Option<Value>,
    pub temp: Option<Value>,
    pub dtemp: Option<Value>,
    pub tc1: Option<Value>,
    pub tc2: Option<Value>,
    pub ic: Option<Value>,
}


impl Inductor {
    pub fn new(
        name: String,
        span: Span,
        positive: Node,
        negative: Node,
        inductance: Value,
    ) -> Self {
        Self {
            name,
            span,
            positive,
            negative,
            inductance,
            nt: None,
            m: None,
            scale: None,
            temp: None,
            dtemp: None,
            tc1: None,
            tc2: None,
            ic: None,
        }
    }

    pub fn set_nt(&mut self, value: Value) {
        self.nt = Some(value);
    }

    pub fn set_m(&mut self, value: Value) {
        self.m = Some(value);
    }

    pub fn set_scale(&mut self, value: Value) {
        self.scale = Some(value);
    }
    pub fn set_temp(&mut self, value: Value) {
        self.temp = Some(value);
    }

    pub fn set_dtemp(&mut self, value: Value) {
        self.dtemp = Some(value);
    }
    pub fn set_tc1(&mut self, value: Value) {
        self.tc1 = Some(value);
    }

    pub fn set_tc2(&mut self, value: Value) {
        self.tc2 = Some(value);
    }
    pub fn set_ic(&mut self, value: Value) {
        self.ic = Some(value);
    }
}

#[derive(Debug, Clone)]
pub enum IndependentSourceMode {
    DC { value: Value },
    // TODO: support AC
}

#[derive(Debug, Clone)]
pub struct IndependentSource {
    pub name: String,
    pub positive: Node,
    pub negative: Node,
    pub mode: IndependentSourceMode,
}

#[derive(Debug, Clone)]
pub enum Device {
    Resistor(Resistor),
    Capacitor(Capacitor),
    Inductor(Inductor),
    VoltageSource(IndependentSource),
    CurrentSource(IndependentSource),
}

impl Device {

    pub fn name(&self) -> &str {
        match self {
            Device::Resistor(r) => &r.name,
            Device::Capacitor(c) => &c.name,
            Device::Inductor(l) => &l.name,
            Device::VoltageSource(v) => &v.name,
            Device::CurrentSource(i) => &i.name,
        }
    }
    pub fn nodes(&self) -> Vec<&Node> {
        match self {
            Device::Resistor(r) => vec![&r.positive, &r.negative],
            Device::Capacitor(c) => vec![&c.positive, &c.negative],
            Device::Inductor(l) => vec![&l.positive, &l.negative],
            Device::VoltageSource(v) => vec![&v.positive, &v.negative],
            Device::CurrentSource(i) => vec![&i.positive, &i.negative],
        }
    }
}


#[derive(Debug, Clone, PartialEq)]
pub enum ValueSuffix {
    Tera,
    Giga,
    Mega,
    Kilo,
    Milli,
    Micro,
    Nano,
    Pico,
    Femto,
    Atto,
}

impl ValueSuffix {
    pub fn from_str(s: &str) -> Option<ValueSuffix> {
        match s {
            s if s.starts_with("T") => Some(ValueSuffix::Tera),
            s if s.starts_with("G") => Some(ValueSuffix::Giga),
            s if s.starts_with("Meg") => Some(ValueSuffix::Mega),
            s if s.starts_with("K") || s.starts_with("k") => Some(ValueSuffix::Kilo),
            s if s.starts_with("m") || s.starts_with("M") => Some(ValueSuffix::Milli),
            s if s.starts_with("u") || s.starts_with("U") => Some(ValueSuffix::Micro),
            s if s.starts_with("n") => Some(ValueSuffix::Nano),
            s if s.starts_with("p") => Some(ValueSuffix::Pico),
            s if s.starts_with("f") => Some(ValueSuffix::Femto),
            s if s.starts_with("a") => Some(ValueSuffix::Atto),
            // TODO: should probalby panic?
            _ => None,
        }
    }
    pub fn scale(&self) -> f64 {
        match self {
            ValueSuffix::Tera => 1e12,
            ValueSuffix::Giga => 1e9,
            ValueSuffix::Mega => 1e6,
            ValueSuffix::Kilo => 1e3,
            ValueSuffix::Milli => 1e-3,
            ValueSuffix::Micro => 1e-6,
            ValueSuffix::Nano => 1e-9,
            ValueSuffix::Pico => 1e-12,
            ValueSuffix::Femto => 1e-15,
            ValueSuffix::Atto => 1e-18,
        }
    }
}
