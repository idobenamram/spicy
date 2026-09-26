// https://ngspice.sourceforge.io/docs/ngspice-manual.pdf

use serde::Serialize;
use std::{fmt, str::FromStr};

use crate::{
    error::{ParserError, SpicyError},
    expr::Value,
    lexer::Span,
};

// SPICE ignores case in keywords and names: ngspice lowercases its whole input
// before parsing (inpcom.c, `inp_casefix`). We keep the spelling for display
// and fold case wherever text is matched: keywords through `keyword`, names
// through `NameKey`.

/// A keyword in the one spelling it's matched against: lowercase.
pub(crate) fn keyword(text: &str) -> String {
    fold_case(text)
}

/// A name as a table key, so `QN`, `qn` and `Qn` name the same model. Tables
/// that show names keep the original spelling separately.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct NameKey(String);

impl NameKey {
    pub fn new(name: &str) -> Self {
        Self(fold_case(name))
    }
}

impl fmt::Debug for NameKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// ASCII case folding, as ngspice does; other characters are kept as written.
fn fold_case(text: &str) -> String {
    text.to_ascii_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct NodeName(pub String);

impl NodeName {
    /// Name of the ground (reference) node.
    pub const GROUND: &'static str = "0";

    pub fn is_ground(&self) -> bool {
        self.0 == Self::GROUND
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeIndex(pub usize);

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
        match keyword(s).as_str() {
            "ac" => Ok(CommandType::AC),
            "dc" => Ok(CommandType::DC),
            "op" => Ok(CommandType::Op),
            "tran" => Ok(CommandType::Tran),
            "lib" => Ok(CommandType::Lib),
            "endl" => Ok(CommandType::Endl),
            "include" => Ok(CommandType::Include),
            "model" => Ok(CommandType::Model),
            "subckt" => Ok(CommandType::Subcircuit),
            "ends" => Ok(CommandType::Ends),
            "param" => Ok(CommandType::Param),
            "end" => Ok(CommandType::End),
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
        // A scale factor may be followed by a unit (`10kOhm`, `1uF`), so match
        // prefixes. `meg` is checked before `m` (milli).
        let s = keyword(s);
        match s.as_str() {
            "deg" => Ok(ValueSuffix::Degree),
            "rad" => Ok(ValueSuffix::Radian),
            s if s.starts_with("meg") => Ok(ValueSuffix::Mega),
            s if s.starts_with('t') => Ok(ValueSuffix::Tera),
            s if s.starts_with('g') => Ok(ValueSuffix::Giga),
            s if s.starts_with('k') => Ok(ValueSuffix::Kilo),
            s if s.starts_with('m') => Ok(ValueSuffix::Milli),
            s if s.starts_with('u') => Ok(ValueSuffix::Micro),
            s if s.starts_with('n') => Ok(ValueSuffix::Nano),
            s if s.starts_with('p') => Ok(ValueSuffix::Pico),
            s if s.starts_with('f') => Ok(ValueSuffix::Femto),
            s if s.starts_with('a') => Ok(ValueSuffix::Atto),
            _ => Err(()),
        }
    }
}
