// https://ngspice.sourceforge.io/docs/ngspice-manual.pdf

use serde::Serialize;
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::{fmt, str::FromStr};

use crate::{
    error::{ParserError, SpicyError},
    expr::Value,
    lexer::Span,
};

// SPICE ignores case in keywords and names. ngspice lowercases its whole
// input (inpcom.c, `inp_casefix`); like Xyce (`HashNoCase`, `EqualNoCase`) we
// keep the spelling and compare without regard to case instead, without
// copying: keywords through `keyword`, names through `Name` and `NoCase`.

/// A word as keywords are matched: lowercased, in a buffer on the stack.
pub(crate) struct Keyword {
    bytes: [u8; Keyword::CAPACITY],
    len: usize,
}

impl Keyword {
    /// More than the longest keyword (`capacitance`). A longer word can't be
    /// a keyword, so it folds to the empty string, which matches none.
    // TODO: nothing checks this limit: a keyword longer than `CAPACITY` added
    // later would fold to "" and never match. Replace this buffer with keyword
    // tables (name → meaning, matched with `eq_ignore_ascii_case`), the way
    // ngspice declares device parameters as data (`bjt.c`, `BJTmPTable`).
    // Tracked in docs/ecad/roadmap.md ("Parser follow-ups").
    const CAPACITY: usize = 16;

    pub(crate) fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).expect("ASCII folding keeps UTF-8 valid")
    }
}

pub(crate) fn keyword(text: &str) -> Keyword {
    let mut bytes = [0; Keyword::CAPACITY];
    let len = if text.len() <= Keyword::CAPACITY {
        bytes[..text.len()].copy_from_slice(text.as_bytes());
        bytes[..text.len()].make_ascii_lowercase();
        text.len()
    } else {
        0
    };
    Keyword { bytes, len }
}

/// A name, compared, hashed and ordered without regard to (ASCII) case.
#[repr(transparent)]
pub struct NoCase(str);

impl NoCase {
    pub fn new(name: &str) -> &NoCase {
        // SAFETY: `NoCase` is `repr(transparent)` over `str`, so a `&str` is a
        // valid `&NoCase`. std's `Path::new` casts `&OsStr` to `&Path` this way.
        unsafe { &*(name as *const str as *const NoCase) }
    }

    fn folded_bytes(&self) -> impl Iterator<Item = u8> + '_ {
        self.0.bytes().map(|b| b.to_ascii_lowercase())
    }
}

impl PartialEq for NoCase {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for NoCase {}

impl Hash for NoCase {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the lowercased bytes a chunk at a time, without allocating.
        let mut chunk = [0; 32];
        for bytes in self.0.as_bytes().chunks(chunk.len()) {
            let folded = &mut chunk[..bytes.len()];
            folded.copy_from_slice(bytes);
            folded.make_ascii_lowercase();
            state.write(folded);
        }
        state.write_u8(0xff);
    }
}

impl PartialOrd for NoCase {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NoCase {
    fn cmp(&self, other: &Self) -> Ordering {
        self.folded_bytes().cmp(other.folded_bytes())
    }
}

/// An owned name, as a table key: keeps its spelling, and compares, hashes
/// and orders like [`NoCase`], so a table can be searched with a `&NoCase`
/// without allocating.
#[derive(Clone, Serialize)]
pub struct Name(String);

impl Name {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    fn no_case(&self) -> &NoCase {
        NoCase::new(&self.0)
    }
}

impl Borrow<NoCase> for Name {
    fn borrow(&self) -> &NoCase {
        self.no_case()
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        self.no_case() == other.no_case()
    }
}

impl Eq for Name {}

impl Hash for Name {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.no_case().hash(state);
    }
}

impl PartialOrd for Name {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Name {
    fn cmp(&self, other: &Self) -> Ordering {
        self.no_case().cmp(other.no_case())
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
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
        // A scale factor may be followed by a unit (`10kOhm`, `1uF`), so only
        // its first letters count. `meg` is checked before `m` (milli).
        if s.eq_ignore_ascii_case("deg") {
            return Ok(ValueSuffix::Degree);
        }
        if s.eq_ignore_ascii_case("rad") {
            return Ok(ValueSuffix::Radian);
        }
        if s.get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("meg"))
        {
            return Ok(ValueSuffix::Mega);
        }
        match s.as_bytes().first().map(u8::to_ascii_lowercase) {
            Some(b't') => Ok(ValueSuffix::Tera),
            Some(b'g') => Ok(ValueSuffix::Giga),
            Some(b'k') => Ok(ValueSuffix::Kilo),
            Some(b'm') => Ok(ValueSuffix::Milli),
            Some(b'u') => Ok(ValueSuffix::Micro),
            Some(b'n') => Ok(ValueSuffix::Nano),
            Some(b'p') => Ok(ValueSuffix::Pico),
            Some(b'f') => Ok(ValueSuffix::Femto),
            Some(b'a') => Ok(ValueSuffix::Atto),
            _ => Err(()),
        }
    }
}
