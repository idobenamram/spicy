//! Physical quantities: dimensions, values in SI, and spreads (model.md E10–E13).
//!
//! Everything is stored in coherent SI units (`47k` Ω is `47000.0`, `-10°C` is
//! `263.15` K); how a value was written stays in the source text. Dimensions are
//! checked while resolving, at run time, so errors can name units the way an engineer
//! writes them (`Ω`, not `kg·m²·s⁻³·A⁻²`).

use std::fmt;

/// Exponents of the base units s, m, kg, A, K and rad (model.md E11).
///
/// Every electrical unit is a product of the first four (V = kg·m²·s⁻³·A⁻¹); K is
/// for temperature; `rad` keeps a phase from being compared with a gain (atopile does
/// the same). Two quantities are compatible exactly when their dimensions are equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dimension([i8; 6]);

impl Dimension {
    pub const NONE: Dimension = Dimension([0, 0, 0, 0, 0, 0]);
    pub const SECOND: Dimension = Dimension([1, 0, 0, 0, 0, 0]);
    pub const METER: Dimension = Dimension([0, 1, 0, 0, 0, 0]);
    pub const KILOGRAM: Dimension = Dimension([0, 0, 1, 0, 0, 0]);
    pub const AMPERE: Dimension = Dimension([0, 0, 0, 1, 0, 0]);
    pub const KELVIN: Dimension = Dimension([0, 0, 0, 0, 1, 0]);
    pub const RADIAN: Dimension = Dimension([0, 0, 0, 0, 0, 1]);

    pub const HERTZ: Dimension = Dimension([-1, 0, 0, 0, 0, 0]);
    /// J/s = kg·m²·s⁻³
    pub const WATT: Dimension = Dimension([-3, 2, 1, 0, 0, 0]);
    /// W/A = kg·m²·s⁻³·A⁻¹
    pub const VOLT: Dimension = Dimension([-3, 2, 1, -1, 0, 0]);
    /// V/A = kg·m²·s⁻³·A⁻²
    pub const OHM: Dimension = Dimension([-3, 2, 1, -2, 0, 0]);
    /// A·s/V = kg⁻¹·m⁻²·s⁴·A²
    pub const FARAD: Dimension = Dimension([4, -2, -1, 2, 0, 0]);
    /// V·s/A = kg·m²·s⁻²·A⁻²
    pub const HENRY: Dimension = Dimension([-2, 2, 1, -2, 0, 0]);

    pub fn is_none(self) -> bool {
        self == Dimension::NONE
    }

    /// The name an engineer would use, for messages: `Ω`, `V`, a product of base
    /// units when there's no name (`V·s`, `kg·m`).
    pub fn symbol(self) -> String {
        if let Some((_, symbol, _)) = NAMED.iter().find(|(d, _, _)| *d == self) {
            return symbol.to_string();
        }
        const BASE: [&str; 6] = ["s", "m", "kg", "A", "K", "rad"];
        let parts: Vec<String> = BASE
            .iter()
            .zip(self.0)
            .filter(|&(_, e)| e != 0)
            .map(|(b, e)| {
                if e == 1 {
                    b.to_string()
                } else {
                    format!("{b}{}", superscript(e))
                }
            })
            .collect();
        parts.join("·")
    }

    /// The physical quantity, for messages ("a capacitance"), when there is a name.
    pub fn quantity_name(self) -> Option<&'static str> {
        NAMED
            .iter()
            .find(|(d, _, _)| *d == self)
            .map(|&(_, _, q)| q)
    }
}

/// Multiplying quantities adds exponents: V·A = W.
impl std::ops::Mul for Dimension {
    type Output = Dimension;
    fn mul(self, other: Dimension) -> Dimension {
        Dimension(std::array::from_fn(|i| self.0[i] + other.0[i]))
    }
}

/// Dividing quantities subtracts exponents: V/A = Ω, V/V = 1.
impl std::ops::Div for Dimension {
    type Output = Dimension;
    fn div(self, other: Dimension) -> Dimension {
        Dimension(std::array::from_fn(|i| self.0[i] - other.0[i]))
    }
}

/// A unit a value can be written in (`47kΩ`, `10°C`, `5%`): what a literal's suffix
/// means once the lexer has read its spelling (`ohm` and `Ω` are both `Ohm`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Unit {
    Volt,
    Amp,
    Ohm,
    Farad,
    Henry,
    Hertz,
    Second,
    /// 3600 s.
    Hour,
    /// A Julian year, 365.25 days.
    Year,
    Watt,
    Kelvin,
    /// A temperature point, offset from kelvin.
    Celsius,
    /// A ratio, scaled by 1/100.
    Percent,
    /// A level, logarithmic.
    Decibel,
}

impl Unit {
    /// `value` in this unit, in SI: `Unit::Ohm.quantity(47e3)` is 47 kΩ. The one place
    /// that knows each unit's dimension (a declared type's is
    /// [`ValueType::field_type`](crate::prelude::ValueType::field_type)).
    pub fn quantity(self, value: f64) -> Quantity {
        let dim = match self {
            Unit::Celsius => return Quantity::celsius(value),
            Unit::Percent => return Quantity::ratio(value / 100.0),
            Unit::Decibel => return Quantity::db(value),
            Unit::Volt => Dimension::VOLT,
            Unit::Amp => Dimension::AMPERE,
            Unit::Ohm => Dimension::OHM,
            Unit::Farad => Dimension::FARAD,
            Unit::Henry => Dimension::HENRY,
            Unit::Hertz => Dimension::HERTZ,
            Unit::Second => Dimension::SECOND,
            Unit::Hour => return Quantity::new(value * 3600.0, Dimension::SECOND),
            // One rounding: `0.9y` is exactly 28 401 840 s, which `value * 365.25 * 86400.0`
            // misses by a bit.
            Unit::Year => return Quantity::new(value * (365.25 * 86400.0), Dimension::SECOND),
            Unit::Watt => Dimension::WATT,
            Unit::Kelvin => Dimension::KELVIN,
        };
        Quantity::new(value, dim)
    }

    /// The canonical spelling, used in messages: the dimension's symbol, except for the
    /// units that aren't just a dimension.
    pub fn symbol(self) -> String {
        match self {
            Unit::Celsius => "°C".to_string(),
            Unit::Percent => "%".to_string(),
            Unit::Decibel => "dB".to_string(),
            Unit::Hour => "h".to_string(),
            Unit::Year => "y".to_string(),
            _ => self.quantity(1.0).dim.symbol(),
        }
    }

    /// `k°C`, `m%`, `kdB`, `mh` and `ky` make no sense, so these units take no prefix
    /// (and `10mh`, likely `10mH`, is an error rather than 36 seconds).
    pub fn accepts_prefix(self) -> bool {
        !matches!(
            self,
            Unit::Celsius | Unit::Percent | Unit::Decibel | Unit::Hour | Unit::Year
        )
    }
}

/// Dimensions with a name: the symbol and the quantity.
const NAMED: &[(Dimension, &str, &str)] = &[
    (Dimension::NONE, "1", "a ratio"),
    (Dimension::SECOND, "s", "a time"),
    (Dimension::HERTZ, "Hz", "a frequency"),
    (Dimension::AMPERE, "A", "a current"),
    (Dimension::VOLT, "V", "a voltage"),
    (Dimension::OHM, "Ω", "a resistance"),
    (Dimension::FARAD, "F", "a capacitance"),
    (Dimension::HENRY, "H", "an inductance"),
    (Dimension::WATT, "W", "a power"),
    (Dimension::KELVIN, "K", "a temperature difference"),
    (Dimension::RADIAN, "rad", "an angle"),
];

fn superscript(e: i8) -> String {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    let mut out = String::new();
    if e < 0 {
        out.push('⁻');
    }
    for d in e.unsigned_abs().to_string().bytes() {
        out.push(DIGITS[(d - b'0') as usize]);
    }
    out
}

/// What kind of value a quantity is, beyond its dimension (model.md E13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QKind {
    /// An ordinary quantity (a resistance, a voltage, a temperature *difference*).
    Plain,
    /// An absolute temperature: stored in K, may be written in `°C`. A point minus a
    /// point is a difference; adding two points or scaling one is an error (uom,
    /// pint, Unitful).
    TempPoint,
    /// A level in dB: 20·log₁₀ of an amplitude ratio. Never a dimension; accepted only
    /// where a level is expected (`f_low(-3dB)`).
    Db,
}

/// A number with its dimension, in SI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quantity {
    pub si: f64,
    pub dim: Dimension,
    pub kind: QKind,
}

/// 0 °C in kelvin.
pub const CELSIUS_OFFSET: f64 = 273.15;

/// A temperature in K, read in °C without the float noise of the offset: `263.15` is
/// `-10`, not `-9.999999999999972`. For messages and fixes; values stay in K. It has
/// the fewest decimals, up to 9, that read back as `kelvin`: a huge value needs none,
/// so scaling by 10⁹ never overflows (`1e300` K once read as `inf` °C), and one no
/// number of °C reads back as exactly (`0.1` K) gets 9.
pub fn to_celsius(kelvin: f64) -> f64 {
    let c = kelvin - CELSIUS_OFFSET;
    let rounded = |decimals: i32| {
        let scale = 10f64.powi(decimals);
        (c * scale).round() / scale
    };
    (0..9)
        .map(rounded)
        .find(|&celsius| celsius + CELSIUS_OFFSET == kelvin)
        .unwrap_or_else(|| rounded(9))
}

impl Quantity {
    pub fn new(si: f64, dim: Dimension) -> Self {
        Self {
            si,
            dim,
            kind: QKind::Plain,
        }
    }

    pub fn ratio(value: f64) -> Self {
        Self::new(value, Dimension::NONE)
    }

    /// An absolute temperature from a value in °C.
    pub fn celsius(c: f64) -> Self {
        Self::kelvin_point(c + CELSIUS_OFFSET)
    }

    /// An absolute temperature from a value in K.
    pub fn kelvin_point(k: f64) -> Self {
        Self {
            si: k,
            dim: Dimension::KELVIN,
            kind: QKind::TempPoint,
        }
    }

    pub fn db(level: f64) -> Self {
        Self {
            si: level,
            dim: Dimension::NONE,
            kind: QKind::Db,
        }
    }

    /// A plain dimensionless number: a ratio (`5%`) or a factor (`2`), never a dB level.
    pub fn is_ratio(self) -> bool {
        self.kind == QKind::Plain && self.dim.is_none()
    }
}

impl fmt::Display for Quantity {
    /// `47000 Ω`, `263.15 K (-10 °C)`, `-3 dB`, `200`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            QKind::TempPoint => {
                write!(f, "{} K ({} °C)", shown(self.si), to_celsius(self.si))
            }
            QKind::Db => write!(f, "{} dB", shown(self.si)),
            QKind::Plain if self.dim.is_none() => write!(f, "{}", shown(self.si)),
            QKind::Plain => write!(f, "{} {}", shown(self.si), self.dim.symbol()),
        }
    }
}

/// A value as shown: 15 significant digits, so the float noise of one rounding
/// doesn't show (a range's midpoint, `2.1 V`, not `2.0999999999999996 V`; `7%`, not
/// `7.000000000000001%`). The noise of a difference of close values stays (`5V - 4.9V`
/// shows `0.0999999999999996 V`).
fn shown(x: f64) -> f64 {
    format!("{x:.14e}").parse().unwrap_or(x)
}

/// How a value may vary (model.md E10). Kept as written: the budget check ("a ±5% part
/// is looser than a ±1% budget") needs the relative form, which atopile loses by
/// turning tolerances into intervals right away.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Spread {
    /// An exact value, like `4.7k` with no tolerance.
    Exact,
    /// `± 1%`: relative to the nominal (0.01).
    Rel(f64),
    /// `± 50mV`: in the nominal's dimension, in SI.
    Abs(f64),
    /// `100..=300`: the bounds, in SI. The nominal is the midpoint (model.md E16).
    Range { lo: f64, hi: f64 },
}

/// A value as the design states it: a nominal and how far it may be from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Value {
    pub nominal: Quantity,
    pub spread: Spread,
}

impl Value {
    pub fn exact(q: Quantity) -> Self {
        Self {
            nominal: q,
            spread: Spread::Exact,
        }
    }

    /// `lo..=hi`, whose nominal is the midpoint (model.md E16): `100..=300` is 200. The
    /// caller checks that `lo` isn't above `hi`.
    pub fn range(lo: Quantity, hi: Quantity) -> Self {
        Self {
            nominal: Quantity {
                si: (lo.si + hi.si) / 2.0,
                ..lo
            },
            spread: Spread::Range {
                lo: lo.si,
                hi: hi.si,
            },
        }
    }

    /// `self` times `k`: the nominal and an absolute spread scale, a relative one
    /// doesn't. A negative `k` flips a range, so `lo` stays the lower bound.
    pub fn scaled(self, k: f64) -> Self {
        let spread = match self.spread {
            Spread::Exact => Spread::Exact,
            Spread::Rel(r) => Spread::Rel(r),
            Spread::Abs(a) => Spread::Abs(a * k.abs()),
            Spread::Range { lo, hi } => {
                let (lo, hi) = (lo * k, hi * k);
                Spread::Range {
                    lo: lo.min(hi),
                    hi: lo.max(hi),
                }
            }
        };
        let nominal = Quantity {
            si: self.nominal.si * k,
            ..self.nominal
        };
        Self { nominal, spread }
    }

    /// The lowest and highest value the spread allows, in SI.
    pub fn bounds(&self) -> (f64, f64) {
        let n = self.nominal.si;
        match self.spread {
            Spread::Exact => (n, n),
            Spread::Rel(r) => {
                let d = (n * r).abs();
                (n - d, n + d)
            }
            Spread::Abs(a) => (n - a.abs(), n + a.abs()),
            Spread::Range { lo, hi } => (lo, hi),
        }
    }

    /// Whether it can be anything but its nominal: a spread of nothing (`± 0%`, `± 0V`,
    /// `200..=200`) doesn't vary, as ngspice's `agauss` returns the nominal for one.
    pub fn varies(&self) -> bool {
        let (lo, hi) = self.bounds();
        lo != hi
    }
}

impl fmt::Display for Value {
    /// `47000 Ω ± 1%`, `12 V ± 0.05 V`, `200 (100..=300)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.nominal)?;
        let unit = |v: f64| match self.nominal.kind {
            QKind::TempPoint => format!("{} K", shown(v)),
            QKind::Db => format!("{} dB", shown(v)),
            _ if self.nominal.dim.is_none() => format!("{}", shown(v)),
            _ => format!("{} {}", shown(v), self.nominal.dim.symbol()),
        };
        match self.spread {
            Spread::Exact => Ok(()),
            Spread::Rel(r) => write!(f, " ± {}%", shown(r * 100.0)),
            Spread::Abs(a) => write!(f, " ± {}", unit(a)),
            Spread::Range { lo, hi } => write!(f, " ({}..={})", unit(lo), unit(hi)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_know_their_dimension_and_symbol() {
        assert_eq!(
            Unit::Ohm.quantity(47e3),
            Quantity::new(47e3, Dimension::OHM)
        );
        assert_eq!(Unit::Percent.quantity(5.0), Quantity::ratio(0.05));
        assert_eq!(Unit::Celsius.quantity(0.0), Quantity::celsius(0.0));
        let seconds = |si| Quantity::new(si, Dimension::SECOND);
        assert_eq!(Unit::Hour.quantity(0.5), seconds(1800.0));
        assert_eq!(Unit::Year.quantity(10.0), seconds(315_576_000.0));
        assert_eq!(Unit::Year.quantity(0.9), seconds(28_401_840.0));
        let symbols = [
            Unit::Volt,
            Unit::Ohm,
            Unit::Hertz,
            Unit::Hour,
            Unit::Year,
            Unit::Kelvin,
            Unit::Celsius,
            Unit::Percent,
            Unit::Decibel,
        ]
        .map(Unit::symbol);
        assert_eq!(symbols, ["V", "Ω", "Hz", "h", "y", "K", "°C", "%", "dB"]);
    }

    #[test]
    fn derived_units_follow_from_the_base() {
        let volt = Dimension::WATT / Dimension::AMPERE;
        assert_eq!(volt, Dimension::VOLT);
        assert_eq!(Dimension::VOLT / Dimension::AMPERE, Dimension::OHM);
        // Ω·F = s: an RC time constant.
        assert_eq!(Dimension::OHM * Dimension::FARAD, Dimension::SECOND);
        // H/Ω = s: an L/R time constant.
        assert_eq!(Dimension::HENRY / Dimension::OHM, Dimension::SECOND);
        // V/V is dimensionless.
        assert!((Dimension::VOLT / Dimension::VOLT).is_none());
    }

    #[test]
    fn symbols_name_units_the_way_engineers_write_them() {
        assert_eq!(Dimension::OHM.symbol(), "Ω");
        assert_eq!(Dimension::FARAD.quantity_name(), Some("a capacitance"));
        assert_eq!(
            (Dimension::VOLT * Dimension::SECOND).symbol(),
            "s⁻²·m²·kg·A⁻¹"
        );
        assert_eq!(
            (Dimension::KELVIN / Dimension::WATT).symbol(),
            "s³·m⁻²·kg⁻¹·K"
        );
    }

    #[test]
    fn temperatures_are_points_in_kelvin() {
        let t = Quantity::celsius(-10.0);
        assert_eq!(t.kind, QKind::TempPoint);
        assert!((t.si - 263.15).abs() < 1e-12);
        assert_eq!(t.to_string(), "263.15 K (-10 °C)");
        // Read back in °C without the offset's float noise (`300.0 - 273.15` is
        // `26.850000000000023`).
        assert_eq!(to_celsius(300.0), 26.85);
        assert_eq!(to_celsius(t.si), -10.0);
    }

    #[test]
    fn values_display_and_bounds() {
        let r = Value {
            nominal: Quantity::new(47_000.0, Dimension::OHM),
            spread: Spread::Rel(0.01),
        };
        assert_eq!(r.to_string(), "47000 Ω ± 1%");
        assert_eq!(r.bounds(), (46_530.0, 47_470.0));
        let beta = Value {
            nominal: Quantity::ratio(200.0),
            spread: Spread::Range {
                lo: 100.0,
                hi: 300.0,
            },
        };
        assert_eq!(beta.to_string(), "200 (100..=300)");
        let v = Value {
            nominal: Quantity::new(3.3, Dimension::VOLT),
            spread: Spread::Abs(0.05),
        };
        assert_eq!(v.to_string(), "3.3 V ± 0.05 V");
        // A percentage as shown: no float noise (`0.07 * 100` is 7.000000000000001),
        // and a tiny or huge one keeps its value (9 decimals showed `0%` and `inf%`).
        let rel = |r| Value {
            nominal: Quantity::ratio(1.0),
            spread: Spread::Rel(r),
        };
        assert_eq!(rel(0.07).to_string(), "1 ± 7%");
        assert_eq!(rel(1e-14).to_string(), "1 ± 0.000000000001%");
        assert!(!rel(1e300).to_string().contains("inf"));
        // A level's spread is in dB too, as a spec's bound on `.db()` is.
        let level = Value::range(Quantity::db(10.0), Quantity::db(20.0));
        assert_eq!(level.to_string(), "15 dB (10 dB..=20 dB)");
    }

    /// Scaling keeps a relative spread, scales an absolute one by `|k|`, and keeps a
    /// range's bounds in order when `k` is negative.
    #[test]
    fn scaling_a_value() {
        let ohms = |si| Quantity::new(si, Dimension::OHM);
        let v = |spread| Value {
            nominal: ohms(1000.0),
            spread,
        };
        assert_eq!(
            v(Spread::Rel(0.01)).scaled(-2.0),
            Value {
                nominal: ohms(-2000.0),
                spread: Spread::Rel(0.01),
            }
        );
        assert_eq!(v(Spread::Abs(50.0)).scaled(-2.0).spread, Spread::Abs(100.0));
        assert_eq!(v(Spread::Exact).scaled(0.5), Value::exact(ohms(500.0)));
        let range = v(Spread::Range {
            lo: 500.0,
            hi: 1500.0,
        })
        .scaled(-2.0);
        assert_eq!(
            range.spread,
            Spread::Range {
                lo: -3000.0,
                hi: -1000.0,
            }
        );
        assert_eq!(range.bounds(), (-3000.0, -1000.0));
    }

    #[test]
    fn a_spread_of_nothing_does_not_vary() {
        let v = |spread| Value {
            nominal: Quantity::new(1000.0, Dimension::OHM),
            spread,
        };
        assert!(!v(Spread::Exact).varies());
        assert!(!v(Spread::Rel(0.0)).varies());
        assert!(!v(Spread::Abs(0.0)).varies());
        let point = Spread::Range {
            lo: 1000.0,
            hi: 1000.0,
        };
        assert!(!v(point).varies());
        assert!(v(Spread::Rel(0.01)).varies());
        assert!(v(Spread::Abs(5.0)).varies());
    }

    #[test]
    fn ratios_are_plain_and_dimensionless() {
        assert!(Quantity::ratio(0.05).is_ratio());
        assert!((Quantity::new(1.0, Dimension::VOLT / Dimension::VOLT)).is_ratio());
        assert!(!Quantity::db(-3.0).is_ratio());
        assert!(!Quantity::new(1.0, Dimension::OHM).is_ratio());
        assert!(!Quantity::celsius(25.0).is_ratio());
    }
}
