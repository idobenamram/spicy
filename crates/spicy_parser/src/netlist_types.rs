// https://ngspice.sourceforge.io/docs/ngspice-manual.pdf

use serde::Serialize;

use crate::{
    error::{ParserError, SpicyError},
    expr::Value,
    lexer::Span,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct NodeName(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeIndex(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentBranchIndex(pub usize);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandType {
    AC,
    DC,
    Op,
    Tran,
    Lib,
    Endl,
    Include,
    Model,
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
            "TRAN" | "tran" => Some(CommandType::Tran),
            "LIB" | "lib" => Some(CommandType::Lib),
            "ENDL" | "endl" => Some(CommandType::Endl),
            "INCLUDE" | "include" => Some(CommandType::Include),
            "MODEL" | "model" => Some(CommandType::Model),
            "SUBCKT" | "subckt" => Some(CommandType::Subcircuit),
            "ENDS" | "ends" => Some(CommandType::Ends),
            "PARAM" | "param" => Some(CommandType::Param),
            "END" | "end" => Some(CommandType::End),
            _ => None,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            CommandType::AC => "AC",
            CommandType::DC => "DC",
            CommandType::Op => "OP",
            CommandType::Tran => "TRAN",
            CommandType::Lib => "LIB",
            CommandType::Endl => "ENDL",
            CommandType::Include => "INCLUDE",
            CommandType::Model => "MODEL",
            CommandType::Subcircuit => "SUBCKT",
            CommandType::Ends => "ENDS",
            CommandType::Param => "PARAM",
            CommandType::End => "END",
        }
        .to_string()
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
pub enum AcSweepType {
    Dec(usize),
    Oct(usize),
    Lin(usize),
}

#[derive(Debug, Clone)]
pub struct AcCommand {
    pub span: Span,
    pub ac_sweep_type: AcSweepType,
    pub fstart: Value,
    pub fstop: Value,
}

#[derive(Debug, Clone)]
pub struct TranCommand {
    pub span: Span,
    /// printing or plotting increment for line-printer output.
    /// it is also the suggest computing increment.
    pub tstep: Value,
    /// the final time for the simulation
    pub tstop: Value,
    /// use initial conditions
    pub uic: bool,
}

#[derive(Debug, Clone)]
pub enum Command {
    Op(OpCommand),
    Dc(DcCommand),
    Ac(AcCommand),
    Tran(TranCommand),
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceType {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Bjt,
    VoltageSource,
    CurrentSource,
    Subcircuit,
}

impl DeviceType {
    pub fn from_char(c: char) -> Result<DeviceType, SpicyError> {
        match c.to_ascii_uppercase() {
            'R' => Ok(DeviceType::Resistor),
            'C' => Ok(DeviceType::Capacitor),
            'L' => Ok(DeviceType::Inductor),
            'D' => Ok(DeviceType::Diode),
            'Q' => Ok(DeviceType::Bjt),
            'V' => Ok(DeviceType::VoltageSource),
            'I' => Ok(DeviceType::CurrentSource),
            'X' => Ok(DeviceType::Subcircuit),
            _ => Err(ParserError::InvalidDeviceType { s: c.to_string() }.into()),
        }
    }

    pub fn from_str(s: &str) -> Result<DeviceType, SpicyError> {
        let mut chars = s.chars();
        let Some(first) = chars.next() else {
            return Err(ParserError::InvalidDeviceType { s: s.to_string() }.into());
        };
        // Device types are a single letter; reject multi-character strings.
        if chars.next().is_some() {
            return Err(ParserError::InvalidDeviceType { s: s.to_string() }.into());
        }
        Self::from_char(first)
    }

    pub fn to_char(&self) -> char {
        match self {
            DeviceType::Resistor => 'R',
            DeviceType::Capacitor => 'C',
            DeviceType::Inductor => 'L',
            DeviceType::Diode => 'D',
            DeviceType::Bjt => 'Q',
            DeviceType::VoltageSource => 'V',
            DeviceType::CurrentSource => 'I',
            DeviceType::Subcircuit => 'X',
        }
    }
}

#[derive(Debug, Clone)]
pub struct Phasor {
    pub mag: Value,
    pub phase: Option<Value>,
}

impl Phasor {
    pub fn new(mag: Value) -> Self {
        Self { mag, phase: None }
    }

    pub fn set_phase(&mut self, phase: Value) {
        self.phase = Some(phase);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    Degree,
    Radian,
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
            s if s.eq_ignore_ascii_case("deg") => Some(ValueSuffix::Degree),
            s if s.eq_ignore_ascii_case("rad") => Some(ValueSuffix::Radian),
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
            ValueSuffix::Degree => 1.0,
            ValueSuffix::Radian => 1.0,
        }
    }
}
