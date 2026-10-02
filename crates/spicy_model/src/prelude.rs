//! The standard part kinds, setup shapes, signal types and value types (model.md E8;
//! language.md §3, §5.1, §6.1, §8.2).
//!
//! Written in Rust for the MVP (roadmap §2.5): the language can't define parts or
//! signals yet. A part kind is a *schema*: its pins and its fields with the unit each
//! expects. It says nothing about simulation; which device a part becomes is decided
//! in lowering (M1e), so this crate stays simulator-free (roadmap §2.3).

use crate::units::{Dimension, QKind};

/// The part kinds of the MVP prelude.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartKind {
    Resistor,
    Capacitor,
    Electrolytic,
    Npn,
    Pnp,
}

impl PartKind {
    pub const ALL: [PartKind; 5] = [
        PartKind::Resistor,
        PartKind::Capacitor,
        PartKind::Electrolytic,
        PartKind::Npn,
        PartKind::Pnp,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PartKind::Resistor => "Resistor",
            PartKind::Capacitor => "Capacitor",
            PartKind::Electrolytic => "Electrolytic",
            PartKind::Npn => "Npn",
            PartKind::Pnp => "Pnp",
        }
    }

    pub fn from_name(name: &str) -> Option<PartKind> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// Pin names, in order. Polarity is in the names: `a`/`b` for non-polarized parts,
    /// `p`/`n` for polarized ones, `c`/`b`/`e` for transistors (language §6.1).
    pub fn pins(self) -> &'static [&'static str] {
        match self {
            PartKind::Resistor | PartKind::Capacitor => &["a", "b"],
            PartKind::Electrolytic => &["p", "n"],
            PartKind::Npn | PartKind::Pnp => &["c", "b", "e"],
        }
    }

    pub fn fields(self) -> &'static [FieldSchema] {
        const RESISTOR: &[FieldSchema] = &[FieldSchema::new(
            "value",
            FieldType::tol(Dimension::OHM),
            true,
        )];
        const CAPACITOR: &[FieldSchema] = &[
            FieldSchema::new("value", FieldType::tol(Dimension::FARAD), true),
            FieldSchema::new("rating", FieldType::exact(Dimension::VOLT), false),
        ];
        const BJT: &[FieldSchema] = &[FieldSchema::new(
            "beta",
            FieldType::tol(Dimension::NONE),
            false,
        )];
        match self {
            PartKind::Resistor => RESISTOR,
            PartKind::Capacitor | PartKind::Electrolytic => CAPACITOR,
            PartKind::Npn | PartKind::Pnp => BJT,
        }
    }

    pub fn field(self, name: &str) -> Option<(usize, &'static FieldSchema)> {
        field_named(self.fields(), name)
    }
}

/// What a setup puts on a port (language.md §8.2): a source that drives one of the
/// block's inputs, or a load on one of its outputs. A schema like a part kind's: its
/// fields, with the unit each expects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// A supply rail into a `Power<In>`: its voltage `v` and source impedance `z`.
    Supply,
    /// A signal source into an `Analog<In>`: its voltage `v` and source impedance `z`.
    Signal,
    /// What an output drives, each to ground: a resistance `r`, a capacitance `c` and a
    /// current `i`.
    Load,
}

impl Shape {
    pub const ALL: [Shape; 3] = [Shape::Supply, Shape::Signal, Shape::Load];

    pub fn name(self) -> &'static str {
        match self {
            Shape::Supply => "Supply",
            Shape::Signal => "Signal",
            Shape::Load => "Load",
        }
    }

    pub fn from_name(name: &str) -> Option<Shape> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// Its fields, in order. A source's voltage has no ideal default, so `v` must be
    /// given (v5 rule 1.3.1); an impedance or a load left out is ideal: a 0 Ω source,
    /// nothing on the output.
    pub fn fields(self) -> &'static [FieldSchema] {
        const SOURCE: &[FieldSchema] = &[
            FieldSchema::new("v", FieldType::tol(Dimension::VOLT), true),
            FieldSchema::new("z", FieldType::tol(Dimension::OHM), false),
        ];
        const LOAD: &[FieldSchema] = &[
            FieldSchema::new("r", FieldType::tol(Dimension::OHM), false),
            FieldSchema::new("c", FieldType::tol(Dimension::FARAD), false),
            FieldSchema::new("i", FieldType::tol(Dimension::AMPERE), false),
        ];
        match self {
            Shape::Supply | Shape::Signal => SOURCE,
            Shape::Load => LOAD,
        }
    }

    pub fn field(self, name: &str) -> Option<(usize, &'static FieldSchema)> {
        field_named(self.fields(), name)
    }

    /// The shape a port of type `signal` takes: an input's source, an output's load. A
    /// `Ground` is the reference and takes nothing; a `Pin` has no role.
    pub fn for_role(signal: SignalType) -> Option<Shape> {
        match signal {
            SignalType::Power(Role::In) => Some(Shape::Supply),
            SignalType::Analog(Role::In) => Some(Shape::Signal),
            SignalType::Power(Role::Out) | SignalType::Analog(Role::Out) => Some(Shape::Load),
            SignalType::Ground | SignalType::Pin => None,
        }
    }
}

/// The field named `name` among `fields`, with its position.
fn field_named(
    fields: &'static [FieldSchema],
    name: &str,
) -> Option<(usize, &'static FieldSchema)> {
    fields.iter().enumerate().find(|(_, f)| f.name == name)
}

/// A field of a part kind or a setup's shape: its name, the value it expects, and
/// whether it must be given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldSchema {
    pub name: &'static str,
    pub ty: FieldType,
    pub required: bool,
}

impl FieldSchema {
    const fn new(name: &'static str, ty: FieldType, required: bool) -> Self {
        Self { name, ty, required }
    }
}

/// What a value position expects. A bare number in it takes this dimension
/// (model.md E12: `value: 47k` is 47 kΩ because `Resistor.value` expects ohms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldType {
    pub dim: Dimension,
    pub kind: QKind,
    /// Whether a tolerance or range may be given (`Tol<T>`, language §5.2), or only an
    /// exact value (a rating).
    pub spread_allowed: bool,
}

impl FieldType {
    pub const fn tol(dim: Dimension) -> Self {
        Self {
            dim,
            kind: QKind::Plain,
            spread_allowed: true,
        }
    }

    pub const fn exact(dim: Dimension) -> Self {
        Self {
            dim,
            kind: QKind::Plain,
            spread_allowed: false,
        }
    }

    /// An absolute temperature with a spread (`env ambient: Temperature in -10°C..=60°C;`).
    pub const fn temperature() -> Self {
        Self {
            dim: Dimension::KELVIN,
            kind: QKind::TempPoint,
            spread_allowed: true,
        }
    }
}

/// The type an `env` or a `const` declares (`env ambient: Temperature in …;`,
/// `const R_TOP: Ohm = 10k;`): a quantity type by its unit (language §5.1), a time or a
/// temperature by what it is, or a plain number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    Temperature,
    Duration,
    Volt,
    Amp,
    Ohm,
    Farad,
    Henry,
    Hertz,
    Watt,
    Number,
}

impl ValueType {
    pub const ALL: [ValueType; 10] = [
        ValueType::Temperature,
        ValueType::Duration,
        ValueType::Volt,
        ValueType::Amp,
        ValueType::Ohm,
        ValueType::Farad,
        ValueType::Henry,
        ValueType::Hertz,
        ValueType::Watt,
        ValueType::Number,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ValueType::Temperature => "Temperature",
            ValueType::Duration => "Duration",
            ValueType::Volt => "Volt",
            ValueType::Amp => "Amp",
            ValueType::Ohm => "Ohm",
            ValueType::Farad => "Farad",
            ValueType::Henry => "Henry",
            ValueType::Hertz => "Hertz",
            ValueType::Watt => "Watt",
            ValueType::Number => "f64",
        }
    }

    pub fn from_name(name: &str) -> Option<ValueType> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    /// What a value of this type is, with a spread allowed: an `env`'s is a range the
    /// engine searches (a `const` takes it exact).
    pub fn field_type(self) -> FieldType {
        let dim = match self {
            ValueType::Temperature => return FieldType::temperature(),
            ValueType::Duration => Dimension::SECOND,
            ValueType::Volt => Dimension::VOLT,
            ValueType::Amp => Dimension::AMPERE,
            ValueType::Ohm => Dimension::OHM,
            ValueType::Farad => Dimension::FARAD,
            ValueType::Henry => Dimension::HENRY,
            ValueType::Hertz => Dimension::HERTZ,
            ValueType::Watt => Dimension::WATT,
            ValueType::Number => Dimension::NONE,
        };
        FieldType::tol(dim)
    }
}

/// Direction of a port, seen from outside the block that owns it (language §3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    In,
    Out,
}

impl Role {
    pub fn from_name(name: &str) -> Option<Role> {
        match name {
            "In" => Some(Role::In),
            "Out" => Some(Role::Out),
            _ => None,
        }
    }
}

/// What a port carries (language §3). Only the MVP's signal types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SignalType {
    /// A plain conductor with no meaning attached.
    Pin,
    /// The reference node. Always bound explicitly: there are no global nets.
    Ground,
    /// A supply rail. `Power<Out>` is a source; `Power<In>` a sink, which inside its own
    /// block acts as the source for the parts there.
    Power(Role),
    /// An analog signal.
    Analog(Role),
}

impl SignalType {
    /// The signal type named `name` with the given role arguments, if it exists and
    /// the number of arguments fits.
    pub fn from_name(name: &str, role: Option<Role>) -> Result<SignalType, SignalTypeError> {
        match (name, role) {
            ("Pin", None) => Ok(SignalType::Pin),
            ("Ground", None) => Ok(SignalType::Ground),
            ("Power", Some(r)) => Ok(SignalType::Power(r)),
            ("Analog", Some(r)) => Ok(SignalType::Analog(r)),
            ("Pin" | "Ground", Some(_)) => Err(SignalTypeError::TakesNoRole),
            ("Power" | "Analog", None) => Err(SignalTypeError::NeedsRole),
            _ => Err(SignalTypeError::Unknown),
        }
    }

    /// Whether `name` is a signal type's name, with or without its role (`Power`).
    pub fn is_name(name: &str) -> bool {
        Self::from_name(name, None) != Err(SignalTypeError::Unknown)
    }
}

/// As written in a port declaration: `Power<In>`.
impl std::fmt::Display for SignalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalType::Pin => write!(f, "Pin"),
            SignalType::Ground => write!(f, "Ground"),
            SignalType::Power(r) => write!(f, "Power<{r:?}>"),
            SignalType::Analog(r) => write!(f, "Analog<{r:?}>"),
        }
    }
}

/// Why [`SignalType::from_name`] found no signal type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalTypeError {
    Unknown,
    NeedsRole,
    TakesNoRole,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_kind_round_trips_by_name() {
        for kind in PartKind::ALL {
            assert_eq!(PartKind::from_name(kind.name()), Some(kind));
            assert!(!kind.pins().is_empty());
        }
        assert_eq!(
            PartKind::from_name("resistor"),
            None,
            "names are case-sensitive"
        );
    }

    #[test]
    fn schema_for_ce_amp_parts() {
        let (_, value) = PartKind::Resistor.field("value").unwrap();
        assert_eq!(value.ty.dim, Dimension::OHM);
        assert!(value.required && value.ty.spread_allowed);
        assert_eq!(PartKind::Electrolytic.pins(), ["p", "n"]);
        let (_, beta) = PartKind::Npn.field("beta").unwrap();
        assert!(beta.ty.dim.is_none() && !beta.required);
        assert!(PartKind::Resistor.field("beta").is_none());
    }

    #[test]
    fn shapes_fit_roles() {
        for shape in Shape::ALL {
            assert_eq!(Shape::from_name(shape.name()), Some(shape));
        }
        let supply = Shape::for_role(SignalType::Power(Role::In));
        assert_eq!(supply, Some(Shape::Supply));
        assert_eq!(
            Shape::for_role(SignalType::Analog(Role::In)),
            Some(Shape::Signal)
        );
        assert_eq!(
            Shape::for_role(SignalType::Power(Role::Out)),
            Some(Shape::Load)
        );
        assert_eq!(Shape::for_role(SignalType::Ground), None);
        assert_eq!(Shape::for_role(SignalType::Pin), None);
        // A source's voltage must be written; everything else is ideal when left out.
        let (_, v) = Shape::Supply.field("v").unwrap();
        assert!(v.required && v.ty.dim == Dimension::VOLT);
        assert!(Shape::Load.fields().iter().all(|f| !f.required));
        assert!(Shape::Load.field("v").is_none());
    }

    #[test]
    fn value_types_round_trip_by_name() {
        for ty in ValueType::ALL {
            assert_eq!(ValueType::from_name(ty.name()), Some(ty));
        }
        let temperature = ValueType::Temperature.field_type();
        assert_eq!(temperature, FieldType::temperature());
        let ohms = ValueType::Ohm.field_type();
        assert_eq!(ohms, FieldType::tol(Dimension::OHM));
    }

    #[test]
    fn signal_types_check_their_role_argument() {
        assert_eq!(
            SignalType::from_name("Power", Some(Role::In)),
            Ok(SignalType::Power(Role::In))
        );
        assert_eq!(
            SignalType::from_name("Ground", None),
            Ok(SignalType::Ground)
        );
        assert_eq!(
            SignalType::from_name("Power", None),
            Err(SignalTypeError::NeedsRole)
        );
        assert_eq!(
            SignalType::from_name("Ground", Some(Role::In)),
            Err(SignalTypeError::TakesNoRole)
        );
        assert_eq!(
            SignalType::from_name("Bus", None),
            Err(SignalTypeError::Unknown)
        );
        assert_eq!(SignalType::Power(Role::Out).to_string(), "Power<Out>");
        assert!(SignalType::is_name("Power") && SignalType::is_name("Pin"));
        assert!(!SignalType::is_name("Bus") && !SignalType::is_name("power"));
    }
}
