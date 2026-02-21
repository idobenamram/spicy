// https://ngspice.sourceforge.io/docs/ngspice-manual.pdf

use serde::Serialize;
use std::{fmt, str::FromStr};

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

impl fmt::Display for CommandType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let command = match self {
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
        };
        f.write_str(command)
    }
}

impl FromStr for CommandType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "AC" | "ac" => Ok(CommandType::AC),
            "DC" | "dc" => Ok(CommandType::DC),
            "OP" | "op" => Ok(CommandType::Op),
            "TRAN" | "tran" => Ok(CommandType::Tran),
            "LIB" | "lib" => Ok(CommandType::Lib),
            "ENDL" | "endl" => Ok(CommandType::Endl),
            "INCLUDE" | "include" => Ok(CommandType::Include),
            "MODEL" | "model" => Ok(CommandType::Model),
            "SUBCKT" | "subckt" => Ok(CommandType::Subcircuit),
            "ENDS" | "ends" => Ok(CommandType::Ends),
            "PARAM" | "param" => Ok(CommandType::Param),
            "END" | "end" => Ok(CommandType::End),
            _ => Err(()),
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

impl FromStr for DeviceType {
    type Err = SpicyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
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

impl FromStr for ValueSuffix {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            s if s.starts_with("T") => Ok(ValueSuffix::Tera),
            s if s.starts_with("G") => Ok(ValueSuffix::Giga),
            s if s.starts_with("Meg") => Ok(ValueSuffix::Mega),
            s if s.starts_with("K") || s.starts_with("k") => Ok(ValueSuffix::Kilo),
            s if s.starts_with("m") || s.starts_with("M") => Ok(ValueSuffix::Milli),
            s if s.starts_with("u") || s.starts_with("U") => Ok(ValueSuffix::Micro),
            s if s.starts_with("n") => Ok(ValueSuffix::Nano),
            s if s.starts_with("p") => Ok(ValueSuffix::Pico),
            s if s.starts_with("f") => Ok(ValueSuffix::Femto),
            s if s.starts_with("a") => Ok(ValueSuffix::Atto),
            s if s.eq_ignore_ascii_case("deg") => Ok(ValueSuffix::Degree),
            s if s.eq_ignore_ascii_case("rad") => Ok(ValueSuffix::Radian),
            _ => Err(()),
        }
    }
}
