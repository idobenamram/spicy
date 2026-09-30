# The netlist writer

> **Status:** decided 2026-09-30 (the user). Step 1, the crate rename, is done; the rest is to build. This is the numeric export of roadmap M1f. The engine deck (`engine_plan.md` §5.1) comes later, in `spicy_backends`, and reuses this writer's line functions.

## 1. What it does

The writer is the reader's reverse. The reader turns a SPICE netlist into a `spicy_circuit::Lowered` (`spicy_netlist::reader::{parse, lower}`); the writer turns a `Lowered` back into a netlist that ngspice runs and our reader reads back.

```rust
// spicy_netlist::writer
pub struct Written {
    pub text: String,
    /// Each node's and device's name in `text`, indexed like the Circuit.
    /// ngspice reports names in lowercase, so match them ignoring case.
    pub names: CircuitNames,
}
pub fn write(lowered: &Lowered) -> Result<Written, WriteError>;
```

`circuits/differential_amplifier.spicy`, read, lowered and written:

```
simple differential amplifier
.temp 27
VCC vcc 0 DC 5
V1 inp 0 SIN(1.2 0.01 1000)
V2 inn 0 SIN(1.2 0.01 1000 0 0 180)
Rc1 vcc outp 2000
Rc2 vcc outn 2000
Re tail 0 1000
Q1 outp inp tail QNPN
Q2 outn inn tail QNPN
.model QNPN NPN is=1e-16 bf=100 br=1 nf=1 nr=1 xtb=0 xti=3 eg=1.11 tnom=27
.tran 1e-5 0.005
.end
```

## 2. Decisions

1. **One crate, a reader and a writer.** `spicy_parser` is renamed `spicy_netlist`, with the existing code in `reader` and the new code in `writer`. Gnucap keeps each language's reader and writer in one module (`apps/lang_spice.cc`: `parse_instance` … `print_instance`, lines 222–788). The reader's `lower` already goes from SPICE to `spicy_circuit`; the writer goes back, next to it. A change to one is reviewed with the other, and the round trip is tested inside the crate.

2. **Names.** In SPICE, a device's first letter is its kind (R, C, L, D, Q, V, I; X places a subcircuit).
   - **The reader accepts `_` and `.` inside names** (after the first letter), as ngspice and vendor libraries use them (`Q2N3904_ON`). Today it rejects both: `R_amp_r1`, `n_1` and `x1.mid` are errors.
   - **The writer uses ngspice's scheme for flattened names.** A device keeps its name when it starts with its kind's letter; otherwise the letter and a dot go in front (`subckt.c:1131-1143`). Nodes are written as they are (`subckt.c:1107-1128`):

     | Name in `CircuitNames` | Written |
     |---|---|
     | `R1` | `R1` |
     | `X1.R1` (R1 inside the placement X1) | `R.X1.R1` |
     | `X1.mid` (a node inside X1) | `X1.mid` |
     | `amp.r1` (from the language) | `R.amp.r1` |
     | `r1` (from the language) | `r1` |

     This replaces the engine plan's first choice, `R_<path>`: `_` would merge `a.b` and `a_b`, and our reader didn't accept it either.
   - **Collisions are resolved by the writer, not by restricting the language.** SPICE names ignore case, so `amp.R1` and `amp.r1` would be the same device to ngspice. A name equal, ignoring case, to one already written gets the first free suffix: `R.amp.r1_2`, then `_3`. `Written::names` records the name each device got. Model names share one namespace: the `.model` cards of diodes and BJTs keep their names, and the writer's own resistor, capacitor and inductor cards follow the same rule.

3. **Phases stay in radians.** `spicy_circuit` keeps radians, and the round-trip test allows a tolerance on phases. Degrees → radians → degrees isn't exact in floating point: of the 721 whole degrees from −360 to 360, 156 come back with junk digits (30 → `29.999999999999996`), and 16 don't read back to the same stored number. So the writer prints values that went through a unit conversion (phases from radians to degrees, temperatures from kelvin to °C) at 15 significant digits. Any decimal of up to 15 digits survives a trip through `f64` (C's `DBL_DIG`), so a typed `30` comes back as `30`, and it reads back to the stored number whenever the source typed it.

4. **Numbers: plain for everyday sizes, exponent form outside.** Everything else is written as the shortest text that reads back to the same `f64` in our reader, which is correctly rounded: plain for 1e-3 ≤ |x| < 1e6, exponent form outside (`2000`, `0.01`, `4.7e-9`, `1e-16`). No SPICE suffixes: in SPICE, `M` means milli and `MEG` mega, so `1M` is 0.001. ngspice's reader isn't correctly rounded: it adds up the digits and multiplies by a power of ten (`inpeval.c:202`). So a value can reach ngspice one unit in the last place off, and checks against ngspice use a tolerance of a few units in the last place.

5. **What gets written.**
   - Model cards in full: the defaults that differ between simulators (TNOM, EG, IS) live in models, and decision D-A already requires the default Npn model to be written out.
   - Instance lines only where a value differs from its default: `m=1`, `area=1` and not `off` mean the same everywhere.
   - Resistor, capacitor and inductor models aren't cards the source wrote; lowering merges them from the instance lines. So one gets a card only when it differs from the default model (a TNOM, a temperature coefficient or a default geometry). A plain resistor gets none.
   - `.temp` always; `.options` only for the tolerances that were set; never `.options tnom`, since every card carries its own TNOM.

## 3. The round-trip test

Write, read back, lower: the result equals the original. Nodes are matched by name, because the writer groups devices by kind and the reader numbers nodes in order of first appearance. Phases are compared with a tolerance; everything else must be exact. The inputs are `circuits/*.spicy` and the reader's test netlists.

Every written netlist must also load in ngspice-42 without a warning. That catches the places where our reader and ngspice read the same text differently.

## 4. Plan

Each step comes with its own tests and is reviewed on its own.

1. Rename `spicy_parser` to `spicy_netlist`, with the existing code in `reader`. **Done.**
2. Fix the reader:
   - names may contain `_` and `.`;
   - resistor cards accept ngspice's `r`/`res` for the default value (`res.c:62-63`); today only `resistance` is accepted, which in ngspice is an instance parameter;
   - the `rad` suffix becomes an error, because ngspice reads `1.5rad` as 1.5 degrees.
3. The writer, with the round-trip tests and the ngspice load check.
4. `spicy export` in the CLI.

## 5. Not now

- **Vendor models shipped as `.subckt`** (op-amps, regulators): `spicy_circuit` has no subcircuit devices, so they can't be written yet. A hierarchical export is a possible later mode.
- **PWL sources:** `Waveform` has none.
- **KiCad-style node names** (`/VCC`, `Net-(R1-Pad2)`): `-`, `/` and `(` inside names need a context-aware lexer.
