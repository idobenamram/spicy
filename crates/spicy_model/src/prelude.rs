//! The standard part kinds and signal types (model.md E8; language.md §3, §6.1).
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
        self.fields()
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == name)
    }
}

/// A field of a part kind: its name, the value it expects, and whether it must be
/// given.
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

    /// An absolute temperature with a spread (`assume temp in -10°C..=60°C`).
    pub const fn temperature() -> Self {
        Self {
            dim: Dimension::KELVIN,
            kind: QKind::TempPoint,
            spread_allowed: true,
        }
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
