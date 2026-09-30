# The netlist reader

> **Status:** parked (2026-09-30, the user). Netlists are a side path: real runs use ngspice as a library, so the reader only needs to read circuits mostly right. The items below are fixed only when one gets in the way of real work. This note comes from three research reports: `research/netlist_readers_ngspice.md` (every claim checked by reading ngspice-42 and running it), `research/netlist_readers_xyce_gnucap.md` (Xyce and Gnucap source; Gnucap also built and run) and `research/netlist_readers_commercial.md` (LTspice, PSpice and HSPICE manuals, and KiCad's netlist exporter). "Ours today" was observed by feeding each case to the reader.

SPICE readers disagree more than their manuals admit, sometimes silently. The same line can be a different circuit in two simulators. The reader follows four rules:

1. **Read ngspice's dialect the way ngspice does.** It's our backend and the writer's target.
2. **Where readers give the same text different meanings, refuse it**, with an error that says how to write it unambiguously. Never guess.
3. **Never drop or change a value silently**, even where ngspice does.
4. **Accept harmless spellings** from other dialects when every reader means the same thing by them.

## 1. Same text, different value

Here our reader silently disagrees with ngspice today, or the readers disagree with each other.

| # | Text | ngspice | Others | Ours today | Proposed |
|---|---|---|---|---|---|
| A1 | `1d3` | 1000 (`d` is an exponent); `1d-3` is an error | Xyce: error; HSPICE: exponent | **1** | Error: write `1e3` |
| A2 | `10mil` | 2.54e-4 (mil = 25.4 µ) | same in all | **0.01** (milli) | Accept as 25.4e-6 |
| A3 | `1µ` | **1** (µ ignored) | Gnucap: 1; Xyce: error; LTspice: micro | **1** | Error: write `1u` |
| A4 | `1A` | 1e-18 (atto) | atto in HSPICE and Gnucap; a unit (×1) in Xyce, LTspice, PSpice | 1e-18 | Error (`10mA` stays fine) |
| A5 | `1x` | 1 (ignored) | mega in HSPICE and Xyce | 1 | Error: write `1meg` |
| A6 | `4k7` | 4000 | 4700 in LTspice | 4000 | Error: write `4.7k` |
| A7 | node `gnd` | ground, by default (`inpcom.c:2002-2023`) | ground in LTspice and HSPICE, and KiCad writes `GND`; ordinary in Xyce, Gnucap, PSpice | ordinary node | Ground, in any case |
| A8 | `R1 a 0 2k 1` | **1 Ω**: a second bare number replaces the value | rejected elsewhere | 2 kΩ, with `ac=1` | Error (see C2) |
| A9 | `R1 a 0` | dropped, with a warning | Xyce: 1 kΩ | 1 mΩ | Error: a resistor needs a value |
| A10 | `R1 a 0 0` | 1 mΩ | | 0 Ω (a singular matrix later) | Error |
| A11 | card `r=2k` and line `1k` | line wins, except that a card `resistance=` beats a value written before the model name | PSpice and Xyce multiply: 2 MΩ | line wins | Error, unless the card's value is 1 |

## 2. ngspice accepts, we reject

| # | Text | Ours today | Proposed | Cost |
|---|---|---|---|---|
| B1 | `.dc v1 …` for a source named `V1` | error | Find the source ignoring case | small |
| B2 | `L1 a 0 l=1u` | error | Accept `l=` on an inductor line (Xyce and HSPICE use it) | small |
| B3 | `.model 2N3904 NPN` (digit first), common in vendor libraries | error | Accept: read model names as the raw text of their field | small |
| B4 | inline comments `R1 a 0 1k ; note`, `$ note`, `// note` | error | Accept `;` anywhere, and `$` and `//` after whitespace | small |
| B5 | names like `/VCC`, `+5V`, `Net-_R1-Pad2_` (KiCad), `X§U1` (LTspice) | error | Accept any character but whitespace and `= ( ) , ; { } ' "` in node and device names | moderate; only for KiCad and LTspice netlists |
| B6 | other instance parameters on a card (`.model DX D area=2`), a default for the card's devices | error | Accept as defaults, the line winning (ngspice and HSPICE do this; Xyce and Gnucap drop them) | moderate; nothing needs it yet |

## 3. We accept, ngspice rejects

| # | Text | ngspice and others | Ours today | Proposed |
|---|---|---|---|---|
| C1 | two devices named `R6` | error in ngspice, Xyce, PSpice and HSPICE; Gnucap keeps both | keeps both | Error, pointing at both lines |
| C2 | numbers by position after the value and model (`R2 n1 n0 2k 1 5 280`) | error (or A8) | read as `ac`, `m`, `scale`, … | Error. Only the value and a diode's or BJT's area may be positional |
| C3 | `.options reltol=1e-3` then `.options reltol=1e-4` | last wins; Xyce keeps the first | last wins | Error when an option gets two different values |

## 4. Already right; keep

- **Numbers are correctly rounded.** ngspice isn't: one literal in five lands one step off in the last bit, never more. Xyce and Gnucap are off too (`4.7n`, `0.3`).
- **`rad` is an error.** ngspice reads `1.5rad` as 1.5 degrees; Xyce errors; Gnucap reads it as a DC value.
- **A suffix after an exponent (`1e3k`) is an error.** ngspice and Xyce read 1e6, Gnucap 1e3, and HSPICE forbids it.
- **`c=` on a capacitor card and `l=` on an inductor card are errors.** ngspice takes both as the card's type flag and leaves 0 F or 0 H, with no warning.
- **Two `.model` cards with one name are an error.** ngspice silently keeps the first.
- **A flag before the model name (`D1 a 0 off DM`) is an error.** ngspice silently makes `off` a third node.
- **A `.temp` list is an error.** The readers disagree: one run per value (LTspice, PSpice, HSPICE), 27 °C (ngspice), the first value (Gnucap), or nothing (Xyce ignores `.temp`).
- **Unknown parameters are errors.** ngspice only warns on an unknown *card* parameter; there is no warning channel yet.
- **Reading stops at `.end`.** ngspice keeps reading, and adds devices written after `.end`.
- **Settled everywhere:**
  - names ignore case;
  - AC and SIN phases are in degrees;
  - the first line is the title;
  - TNOM is 27 °C (HSPICE alone uses 25 °C);
  - the diode and BJT defaults are the same in all readers.

## 5. Later

- **A warning channel.** For the traps the dialects agree on but people get wrong: `1M` on a resistor is 1 mΩ, `4F` on a capacitor is 4 fF, and a card-only value means different things in PSpice and Xyce.
- **`.include` path order.** ngspice looks in the working directory first; Xyce and Gnucap look next to the including file first. Ours looks next to the main file first. The including file's directory is the most predictable.
- **`.temp` lists as several runs**, once the engine runs sweeps.
- **`tc=a,b`** for `tc1`/`tc2` (ngspice and LTspice).

## 6. Test inputs to update

Three of the reader's own test netlists use what §3 would reject:
- `basic_resistor.spicy` repeats `R6`;
- `waveforms.spicy` repeats `V1`;
- `params_mixed.spicy` repeats `R3` and gives parameters by position.

They would get distinct names and named parameters, with the same values.
