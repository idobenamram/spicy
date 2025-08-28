// https://ngspice.sourceforge.io/docs/ngspice-manual.pdf


#[derive(Debug, Clone, PartialEq)]
pub enum CommandType {
    AC,
    DC,
    Op,
    Subcircuit,
    Ends,
    End,
}

impl CommandType {
    pub fn from_str(s: &str) -> Option<CommandType> {
        match s {
            "AC" => Some(CommandType::AC),
            "DC" => Some(CommandType::DC),
            "OP" => Some(CommandType::Op),
            "SUBCKT" => Some(CommandType::Subcircuit),
            "END" => Some(CommandType::End),
            "ENDS" => Some(CommandType::Ends),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ElementType {
    Resistor,
    Capacitor,
    Inductor,
    VoltageSource,
    CurrentSource,
}

impl ElementType {
    pub fn from_str(s: &str) -> Option<ElementType> {
        match s.to_uppercase().to_string().as_str() {
            "R" => Some(ElementType::Resistor),
            "C" => Some(ElementType::Capacitor),
            "L" => Some(ElementType::Inductor),
            "V" => Some(ElementType::VoltageSource),
            "I" => Some(ElementType::CurrentSource),
            _ => None,
        }
    }

    pub fn to_char(&self) -> char {
        match self {
            ElementType::Resistor => 'R',
            ElementType::Capacitor => 'C',
            ElementType::Inductor => 'L',
            ElementType::VoltageSource => 'V',
            ElementType::CurrentSource => 'I',
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