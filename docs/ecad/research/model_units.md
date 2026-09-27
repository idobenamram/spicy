# Research: Units, Dimensions and Quantity Types for the Elaboration Stage

> 2026-09-27 · Research report for `model.md` (M1d).
> Sources and revisions:
> - uom `a465bcc` (2026-04-04): `src/lib.rs:115`; `src/system.rs:252`; `src/si/{thermodynamic_temperature.rs:1–45, angle.rs:18, ratio.rs, mod.rs:187–230}`
> - Unitful.jl `550d5f1`: `docs/src/{temperature.md, logarithm.md}`; `test/runtests.jl:343–424`; `src/utils.jl:254`
> - pint `e4042bb`: `docs/user/{nonmult.rst, log_units.rst}`; `pint/default_en.txt:116,152,154,207,538`; `pint/errors.py:190`
> - F# units of measure docs (MS Learn, dotnet/docs `cf00b0b`); Kennedy, "Types for Units-of-Measure: Theory and Practice", CEFP'09 (§2.7, §3.2–3.3, Fig. 5)
> - OpenModelica master `41a658f`: `NFFrontEnd/NFUnit.mo`, `NFUnitCheck.mo`, `Util/Error.mo` msg 517
> - atopile `619eda7` (local): `src/faebryk/library/Units.py:30–45,102–208,2127,2295–2334`; `src/atopile/compiler/ast_visitor.py:527,1443–1540`; `src/faebryk/library/Literals.py:3305`
>
> **[V]** means verified in those sources; **[O]** means recommendation.

## 1. Representation

- **uom [V]:** a `Quantity<D, U, V>` holds zero-size compile-time markers for the dimension and the unit system (typenum exponents over the 7 ISQ base dimensions), plus the value. Values are **normalized to base units** (`lib.rs:115`): `1 cm` is stored as `0.01 m`, and the unit matters only when converting in or out.
- **pint, Unitful, OpenModelica, atopile [V]:** all check dimensions at run time, as an exponent vector plus a scale factor and an offset:
  - OpenModelica: `UNIT(s, m, g, A, K, mol, cd, factor, offset)`;
  - atopile: `BasisVector(ampere, second, meter, kilogram, kelvin, mole, candela, radian, steradian, bit)`, plus `multiplier` and `offset`.

  Units are compatible exactly when their vectors are equal; atopile calls this "commensurable".
- **Base dimensions needed:**
  - every electrical unit is built from **s, m, kg, A**: V = kg·m²·s⁻³·A⁻¹, Ω = V/A, F = A·s/V, H = V·s/A;
  - add **K** for temperature;
  - mole and candela aren't needed.
- **[O] Use SI (s, m, kg, A, K), even though {V, A, s, K} would be enough for electronics alone:**
  - every reference does;
  - length will be needed (trace width, clearance);
  - thermal resistance (K/W) works either way.
- **Angle:** add a sixth exponent, `rad`, as atopile does (see §3).
- **`%`, `ppm` and `dB` are not dimensions.**
  - `%` and `ppm` are dimensionless scale factors: uom `ratio.rs` (`percent: 1.0E-2`, `part_per_million: 1.0E-6`), pint (`percent = 0.01`), atopile `Percent` (multiplier 1e-2).
  - dB isn't multiplicative at all (§3).
- **[O] For us:**
  - dimensions are checked at run time, during elaboration;
  - values are `f64` in coherent SI (`47k` → 47000.0 Ω, `-10°C` → 263.15 K);
  - the written prefix is only a display hint (Modelica's `displayUnit`, atopile's `has_display_unit`).

## 2. Inference

**F# [V]**
- An unannotated literal is dimensionless. Only `0.0<_>` is allowed to have any unit, because zero is the only value that rescaling doesn't change (Kennedy §2.7).
- F# does have unit variables (`float<'u>`), solved by **Abelian group unification**.
  - In plain terms: units multiply like powers that commute and cancel. Solving "which unit α has α² = m²·s⁻⁴?" becomes integer arithmetic on the exponents: α = m·s⁻².
  - It always gives a single most general answer or a clear failure (Kennedy §3.2, Fig. 5, a variant of Gaussian elimination). That's why the check is decidable.

**Modelica / OpenModelica [V]**
- A variable declares `unit="V"`.
- The checker gives literals and unknown expressions the placeholder `MASTER` ("unknown") and fills units in along the equations.
- Inconsistencies are **warnings** (Error.mo msg 517), not errors.

**atopile [V]** doesn't infer units from context:
- a bare number is `Dimensionless` (`_decode_unit`);
- in a range, a bare endpoint takes its partner's unit;
- an arithmetic expression gets `MakeChild_DeferredUnit`, resolved later.

**[O] Recommended scheme: one bidirectional pass, no unit variables.** Types flow down from what's expected, and up from literals and ports.
1. Every field, port quantity, env knob and built-in function parameter has a declared dimension:
   - `Resistor.value: Tol<Ohm>`, `Npn.beta: Ratio`, `Power.v: Volt`;
   - `at(f: Hertz)`, `f_low(level: Db) -> Hertz`.
2. A bare literal takes the expected dimension, but only in "value positions":
   - a field value;
   - a function argument;
   - the nominal of `±`;
   - an endpoint of `..=`;
   - the bound of `in` and the other comparisons, where the expected unit is the measured side's.

   Anywhere else it's dimensionless.
3. In `*` and `/`, a bare number is a dimensionless factor. In `+` and `-` next to a dimensioned value it's an error with a fix ("write `5k`"), so `r + 5` is rejected, as F# does.
4. A `let` takes the dimension of its right-hand side. With no unit polymorphism a single pass decides everything. If generic `fn<U>` ever arrives, Kennedy's algorithm is about 50 lines.

**`ce_amp.spl`, worked through:**

| Expression | Expected | Result |
|---|---|---|
| `value: 47k ± 1%` | `Tol<Ohm>` | `47k` takes Ω; `1%` → `Rel(0.01)` |
| `value: 1uF ± 20%` | `Tol<Farad>` | written F = expected F ✓ |
| `beta: 100..=300` | `Ratio` | `Range(100, 300)`, dimensionless |
| `temp in -10°C..=60°C` | `Celsius` (a point) | 263.15 K ..= 333.15 K |
| `vcc.v in 12V ± 5%` | `Volt` (from `Power.v`) | ✓ |
| `output.v / input.v` | — | V·V⁻¹: the exponents subtract to 0, so dimensionless; `ac()` gives a transfer function |
| `h.at(1kHz).mag() in 4.6 ± 5%` | `at` takes Hz ✓; `mag()` returns `Ratio` | `4.6` takes `Ratio` ✓ |
| `h.f_low(-3dB) <= 30Hz` | `f_low` takes a dB level, returns Hz | Hz ≤ Hz ✓ |

## 3. Hard cases

**Temperature**
- [V] What the tools do:
  - uom has two types with the same dimension, `ThermodynamicTemperature` and `TemperatureInterval`. Temperature + temperature doesn't compile; temperature + interval does.
  - pint: °C − °C = `delta_degC`; °C + delta = °C; °C + °C and °C × anything raise `OffsetUnitCalculusError`, and °C + kelvin is an error too.
  - Unitful: `100°C + 1K` = 374.15 K (absolute), `100°C − 1°C` = `99K`, and `°C·K` is an `AffineError`.
  - OpenModelica keeps the offset but ignores it when comparing units (`isEqual`), and drops it when combining.
  - atopile: "Cannot use unit expression with non-zero offset" when multiplying offset units.
- [O] Our rule: a value's kind is either a **temperature point** or a plain quantity (a difference is a plain quantity in K).
  - `-10°C..=60°C` is absolute: both ends are points.
  - point − point = a difference; point ± difference = a point.
  - point + point, and point × or ÷ anything, are errors.
  - `temp + 15K` is a point 15 K higher. K has no offset, so `15K` is the same whether read as a point or a difference; only `°C` needs a decision.
  - A `°C` literal is always a point. Where a difference is needed (`25°C ± 5°C`, `temp + 5°C`), it's an error with the fix "write `5K` for a temperature difference".
  - `ppm/°C` means `ppm/K`, as pint's parser treats it.
  - The ngspice exporter converts back to °C for `.temp`.

**`%` and `ppm`**
- [V] In atopile, a tolerance written in `%` is always relative (`rel = True`) and becomes an interval right away (`MakeChild_FromCenterRel`).
- [O] Our rule:
  - after `±`, `%` and `ppm` are **always relative** to the nominal: `4.6 ± 5%` = ±0.23. For an absolute spread on a dimensionless value, write a bare number: `0.5 ± 0.05`.
  - Elsewhere they're dimensionless factors (×0.01, ×1e-6): `aging: -20%`, `tc: ±100ppm/K` (dimension K⁻¹).
  - `ppm` isn't in the MVP unit table yet.

**dB**
- [V] What the tools do:
  - pint defines `decibel` as a power ratio (`20 dB → 100`) and its docs warn against using it with voltages;
  - Unitful refuses to convert a dB gain to a plain number automatically, because 10·log (power) vs 20·log (amplitude) is ambiguous; it offers `uconvertp`/`uconvertrp` for the two readings;
  - atopile has no dB yet (a TODO in `Units.py`).
- [O] Our rule: a separate value kind, `Db(f64)`, never a dimension, accepted only where a dB level is expected.
  - In our language, dB is **20·log₁₀ of the magnitude of an amplitude ratio** (voltages or currents), the Bode-plot and SPICE `vdb` convention (standard knowledge, not re-verified here).
  - So `f_low(-3dB)` = the frequency where |h| falls to 10^(−3/20) = 0.708 of its midband value.
  - Because `output.v / input.v` cancels to dimensionless, the dimension can't tell a voltage ratio from a power ratio. `ac()` should record that its argument is an amplitude ratio. A power reading can come later as an explicit form.

**Dimensionless ratios:** V/V is the zero vector. [O] One `Ratio` type, with no "ratio of what" tag, except the amplitude flag in transfer functions.

**Angles**
- [V] What the tools do:
  - SI and uom treat angle as dimensionless; uom distinguishes it only with an `AngleKind` marker;
  - pint: `radian = []`; OpenModelica: `rad` = 1;
  - atopile makes radian a base dimension "to permit distinction between e.g. torque and energy", and its `is_angular` requires radians explicitly.
- [O] Use a `rad` exponent, as atopile does:
  - `phase()` returns rad, and `45°` converts via π/180;
  - `phase() in 4.6` then fails, and so does comparing a gain with a phase.
  - The cost: 2π·f gives s⁻¹, not rad/s. Allow an explicit conversion (`.rad_per_s()`), or treat `rad` as cancellable only in ω = 2πf.
  - `°` alone isn't in the lexer's unit table today.

**A spread in a different unit than the nominal**
- [V] In atopile, when both have units the tolerance must be "commensurable" and is converted; otherwise it's a type error.
- [O] Our rule:
  - `12V ± 5%` → `Rel(0.05)`;
  - `3.3V ± 50mV` → `Abs(0.05)`, after checking that the spread has the same dimension as the nominal;
  - a bare spread takes the nominal's dimension;
  - `0V ± 1%` gets a zero-width lint.
  - Keep `Rel` as written, not as an interval: budget checks ("part ±5% is looser than budget ±1%") and `r_nom ± 1%` need the relative form. atopile's immediate `[min, max]` conversion loses it.

## 4. Errors

[V] What the tools actually print:
- F#: `error FS0001: The unit of measure 'm' does not match the unit of measure 'm/s ^ 2'`, with carets under the span.
- pint: `Cannot convert from 'meter' ([length]) to 'second' ([time])`.
- Unitful: `DimensionError: 1 m and 2 s are not dimensionally compatible.` and `AffineError: an invalid operation was attempted with affine quantities: 32 °F + 1 °F`.
- OpenModelica (a warning): `The system of units is inconsistent in term %s with the units %s and %s respectively.`
- atopile: ``Tolerance unit `mA` is not commensurable with quantity unit `V` ``, and `Parameter constraints have incompatible units`.
- uom (compile time): rustc E0308 with typenum types. [O] Very hard to read, one reason not to use uom for user-facing checks.

**[O] A good message:**
- names both units (Ω, not kg·m²·s⁻³·A⁻²), plus the physical quantity and where the expectation came from (which field);
- points at the exact span;
- has a fix for the known confusions (`K` vs `k`, `°C` vs `K`, a missing unit).

To print a unit, look its vector up in a table of named units, falling back to a product of named units:

```
error[E-unit]: expected `Ohm`, found `Kelvin`
  |  value: 1K ± 1%
  |         ^^ `K` is kelvin; kilo is lowercase `k`
error[E-unit]: tolerance `50mA` (current) doesn't match `12V` (voltage)
error[E-unit]: `°C` is a temperature point; a difference is written in K
  |  assume temp in 25°C ± 5°C
  |                        ^^^ help: `± 5K`
error[E-unit]: `f_low` expects a level in dB, found a bare number
  |  h.f_low(-3)       help: `-3dB`
error[E-unit]: can't add `Ohm` and a bare number
  |  value: r_top + 5  help: `5k`, or `5` with a unit
```

## 5. Recommended design [O]

No dependencies; checked during elaboration.

```rust
/// Exponents of s, m, kg, A, K, rad.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Dimension([i8; 6]);        // Mul/Div add/subtract; powi scales
pub enum QKind { Plain, TempPoint, Db }
pub struct Quantity { si: f64, dim: Dimension, kind: QKind }
pub enum Spread { Exact, Rel(f64), Abs(f64 /* same dim */), Range { lo: f64, hi: f64 } }
pub struct Value { nominal: Quantity, spread: Spread, display: Option<Prefix> }
```

Why each part:
- **SI vector compared for equality:** every reference does it, and comparing vectors is decidable (Kennedy's normal form).
- **Stored in coherent SI,** as uom normalizes: the engine and the exporter never convert units.
- **Offsets only at the edges:** only `°C` input and output converts (±273.15). Internally a point is in K, and `QKind::TempPoint` enforces pint's and Unitful's point/difference rules.
- **`%` and `ppm` only as literal scale factors,** relative after `±` (atopile's rule).
- **`Db` as its own kind** (Unitful's refusal to guess), with amplitude ratios as our one convention.
- **Bidirectional inference,** where a bare literal takes the expected dimension only in value positions. No generics means no unit variables.
- **Named types map to `(Dimension, QKind)`:** `Ohm`, `Volt`, `Ratio`, `Hertz`, `Celsius` (a point, displayed in °C). `Tol<T>` is `T` with a non-`Exact` spread.

**Open questions:**
- the nominal of a `Range` value (`beta: 100..=300`);
- how `100ppm/K` is written when the unit sits outside a literal;
- whether to accept `± 5°C` as a convenience;
- whether Hz and rad/s convert automatically.
