//! What a contract's measures mean (model.md E24, contracts_plan.md §2.5).
//!
//! A measure is an expression over a block's probes in an analysis: `dc(output.v)`,
//! `ac(output.v / input.v).at(1kHz).mag()`. Each has a [`MeasureType`], so a spec's
//! bound is typed in its measure's unit. The language reads the names and each
//! method's arguments (`spicy_lang::resolve`); what a method takes and gives is decided
//! here, one `match` arm each ([`Method::result`]). Zig splits its builtins the same
//! way: a table gives each one's arity (`BuiltinFn.zig`), and Sema types each one in
//! code of its own. The engine compiles the same [`MExpr`] (engine_types §7).

use crate::design::{MeasureId, NetId, PortId};
use crate::units::{Dimension, QKind, Quantity};

/// A measure, as resolved.
#[derive(Clone, Debug, PartialEq)]
pub enum MExpr {
    /// A number written in the measure: `1V` in `dc(vcc.v - 1V)`.
    Const(Quantity),
    /// A net's voltage, `output.v`: a signal, read only through an analysis.
    Voltage(NetId),
    /// A signal's operating point: `dc(output.v)`.
    Dc(Box<MExpr>),
    /// Net `out`'s small-signal response to the source on input port `input`:
    /// `ac(output.v / input.v)`. The only form an analysis can excite: the setup puts
    /// a source on every input, and that source gets the AC stimulus.
    Ac {
        out: NetId,
        input: PortId,
    },
    /// Another measure of the contract, by name: `h`.
    Measure(MeasureId),
    /// A method on a measure: `h.at(1kHz)`, `p.mag()`.
    Method {
        method: Method,
        of: Box<MExpr>,
    },
    Neg(Box<MExpr>),
    Binary {
        op: ArithOp,
        lhs: Box<MExpr>,
        rhs: Box<MExpr>,
    },
}

/// A method of the MVP's measure table, with its arguments as plain numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Method {
    /// `.at(f)`: a response's phasor at `hz` (> 0).
    At { hz: f64 },
    /// `.mag()`: a phasor's magnitude.
    Mag,
    /// `.db()`: a ratio as a level, 20·log₁₀|x| (ngspice's `db`).
    Db,
    /// `.f_low(L)`: the lowest frequency where a response is `db` (< 0) below its peak.
    FLow { db: f64 },
    /// `.f_high(L, ref: dc)`: the highest frequency where a response is `db` (< 0)
    /// below `reference`.
    FHigh { db: f64, reference: Reference },
}

/// What `.f_high` measures a response's fall from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reference {
    /// Its peak, as `.f_low` does (`ref` left out).
    Peak,
    /// Its gain at DC (`ref: dc`).
    Dc,
}

/// One of `+ - * /` in a measure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// What a measure is, which decides what it can be used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasureType {
    /// A probe (`output.v`, `a.v - b.v`), in its unit: only an analysis reads it
    /// (language.md §8.4).
    Probe(Dimension),
    /// A number: what a spec checks (`dc(output.v)`, `h.at(1kHz).mag()`).
    Number { dim: Dimension, kind: QKind },
    /// A response over frequency (`ac(o.v / i.v)`), in its unit (V/V is none).
    Response(Dimension),
    /// A response at one frequency (`h.at(1kHz)`): complex, in the response's unit.
    Phasor(Dimension),
}

impl Method {
    /// The name it's written with.
    pub fn name(self) -> &'static str {
        match self {
            Method::At { .. } => "at",
            Method::Mag => "mag",
            Method::Db => "db",
            Method::FLow { .. } => "f_low",
            Method::FHigh { .. } => "f_high",
        }
    }

    /// What it gives on a measure of type `on`, or `None` where it doesn't apply (a
    /// response has no magnitude until `.at(f)` picks a frequency).
    pub fn result(self, on: MeasureType) -> Option<MeasureType> {
        use MeasureType::*;
        let ratio = |dim: Dimension| dim.is_none();
        match (self, on) {
            (Method::At { .. }, Response(dim)) => Some(Phasor(dim)),
            (Method::Mag, Phasor(dim)) => Some(Number {
                dim,
                kind: QKind::Plain,
            }),
            (
                Method::Db,
                Phasor(dim)
                | Number {
                    dim,
                    kind: QKind::Plain,
                },
            ) if ratio(dim) => Some(Number {
                dim: Dimension::NONE,
                kind: QKind::Db,
            }),
            (Method::FLow { .. } | Method::FHigh { .. }, Response(dim)) if ratio(dim) => {
                Some(Number {
                    dim: Dimension::HERTZ,
                    kind: QKind::Plain,
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_methods_chain_as_the_table_says() {
        let gain = MeasureType::Response(Dimension::NONE);
        let at = Method::At { hz: 1e3 }.result(gain).unwrap();
        assert_eq!(at, MeasureType::Phasor(Dimension::NONE));
        let mag = Method::Mag.result(at).unwrap();
        assert_eq!(
            mag,
            MeasureType::Number {
                dim: Dimension::NONE,
                kind: QKind::Plain
            }
        );
        let level = MeasureType::Number {
            dim: Dimension::NONE,
            kind: QKind::Db,
        };
        assert_eq!(Method::Db.result(at), Some(level));
        assert_eq!(Method::Db.result(mag), Some(level));
        let hz = Method::FLow { db: -3.0 }.result(gain).unwrap();
        assert_eq!(
            hz,
            MeasureType::Number {
                dim: Dimension::HERTZ,
                kind: QKind::Plain
            }
        );
        // A response has no magnitude until a frequency is picked; volts aren't a ratio.
        assert_eq!(Method::Mag.result(gain), None);
        let volts = MeasureType::Number {
            dim: Dimension::VOLT,
            kind: QKind::Plain,
        };
        assert_eq!(Method::Db.result(volts), None);
        assert_eq!(Method::FLow { db: -3.0 }.result(volts), None);
        let impedance = MeasureType::Response(Dimension::OHM);
        assert_eq!(Method::FLow { db: -3.0 }.result(impedance), None);
    }
}
