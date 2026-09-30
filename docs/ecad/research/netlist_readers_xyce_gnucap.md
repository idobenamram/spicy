# SPICE Readers: How Xyce and Gnucap Differ from ngspice

> 2026-09-30 · Research for the SPICE reader in `crates/spicy_netlist/src/reader/` (design notes: `netlist_writer.md`). **Status: research only. Nothing is decided or implemented.**
>
> **Versions read.** Xyce 7.11.0, git `6243c62` (2026-09-21), at `/tmp/refs/xyce`. Gnucap release 2024.02.20, git `874f2d5`, at `/tmp/refs/gnucap`.
>
> **How it was checked.**
> - Xyce was only read, never run.
> - Gnucap was read, and also built by hand (g++, no readline). Its own `tests/bmm_res.1.ckt` reproduces the expected output. Small test decks were then run on it; those results are marked **run**.
> - Items marked *(inferred)* come from reading the code, not from a run.
>
> **The ngspice column** is what we believe today, taken from `netlist_writer.md` and the brief. It was not re-checked here; another report does that. A `?` means we don't know yet.
>
> **Citation paths.**
> - Xyce: `IO/` = `src/IOInterfacePKG/`, `UTL/` = `src/UtilityPKG/`, `DEV/` = `src/DeviceModelPKG/Core/`, `OM/` = `src/DeviceModelPKG/OpenModels/`, `RG/` = `doc/Reference_Guide/`, `UG/` = `doc/Users_Guide/`.
> - Gnucap: paths are from its repo root (`apps/`, `lib/`, `include/`).

## Words used here

- **Instance line:** one line that places one part, e.g. `R1 a b 1k`. The first letter gives the kind: R resistor, C capacitor, L inductor, D diode, Q bipolar transistor (BJT), V voltage source, X subcircuit.
- **Model card:** a `.model NAME TYPE p=v ...` line. It holds parameters shared by every part that names it.
- **Subcircuit:** a named block of parts (`.subckt ... .ends`), placed with an `X` line. **Flattening** copies its parts into the top level under longer names.
- **Ground:** the reference node, where the voltage is 0 by definition.
- **TNOM:** the temperature at which a model's parameters were measured. The model corrects them for the actual circuit temperature.
- **IS, N, BF, XTB, XTI, EG:** diode and BJT model parameters.
  - IS: saturation current, the tiny reverse current that sets the forward voltage.
  - N: emission coefficient, which sets how steep the diode curve is.
  - BF: forward current gain, collector current ÷ base current.
  - XTB: the temperature exponent of BF.
  - XTI: the temperature exponent of IS.
  - EG: the band-gap energy in eV, used in IS's temperature correction.
- **AC phase:** the angle of a sine source in small-signal (AC) analysis.
- **SIN phase:** the starting angle of a time-domain sine wave.
- **Correctly rounded:** the stored `f64` is the one nearest the decimal text, as Rust's `str::parse` gives.

## Summary

On most of the basics the three simulators agree. They ignore case. `M` means milli and `MEG` means mega. Phases are in degrees. TNOM is 27 °C, and the diode and BJT defaults are the same. A model name may start with a digit. The model name before the value (`C1 a 0 CM 1n`) is the only order that all three accept.

The differences that matter most:

1. **The `R`, `C` and `L` parameters on a model card.** In Xyce, as in PSpice, the card's `R` is a *multiplier* with default 1. In ngspice it is the *default value*. Gnucap has no such parameter at all.
   - So `.model RM R r=2k` then `R1 a b RM` gives three different circuits:
     - 2 kΩ in ngspice;
     - 2 MΩ in Xyce, because the missing value defaults to 1000 and 1000 × 2000 = 2 MΩ, with only a misleading warning;
     - a 10 µΩ short in Gnucap, because the card's `r` is ignored with a warning and the value defaults to 0.
   - Xyce also errors when a capacitor or inductor has no value on its line.
2. **Ground.** Only ngspice makes `gnd` ground. In Xyce it takes `.PREPROCESS REPLACEGROUND TRUE`, which also covers `GND!` and `GROUND`. Gnucap knows only `0`.
3. **`.temp`.** Xyce ignores `.TEMP` with a warning; the temperature goes in `.OPTIONS DEVICE TEMP=`, and several temperatures go in `.STEP TEMP LIST`. Gnucap reads the first value of `.temp` and silently drops the rest.
4. **Numbers.** None of the three converts numbers correctly rounded. The suffix letters also differ:
   - `X` is mega in Xyce but ignored in Gnucap.
   - `A` is atto in Gnucap, but in Xyce it is read as the unit "amperes" and means ×1.
   - `1e3k` is 1e6 in Xyce but 1e3 in Gnucap.
   - Xyce rejects trailing text that isn't on its unit list (`10Ohms`, `1.5rad`, `1d3`). Gnucap skips any letters, so `1µ` quietly becomes 1.
5. **Instance-line spellings and extras.**
   - Xyce accepts both value orders, and also `R=`, `C=` and `L=`. It does not accept ngspice's long names (`resistance=`, `capacitance=`, `inductance=`).
   - Gnucap accepts only the model name before the value, and no `R=`.
   - Only ngspice takes an instance parameter written on a model card (`.model DX D area=2`); the other two warn and drop it.
   - Duplicate device names are an error in Xyce, as in ngspice. Gnucap keeps both, silently.

## Comparison table

| Point | ngspice (as we understand it) | Xyce | Gnucap |
|---|---|---|---|
| **P1** Name characters; flattened names | `_` and `.` allowed. A flattened device is `r.x1.r1` and a node is `x1.mid` (`subckt.c:1107-1143`). | Any printable ASCII except whitespace and `( ) { } , : ; " '`. `*` and `?` are allowed but act as print wildcards (RG/Xyce_RG_app1.tex:708-731). Token breaks: `IO/N_IO_SpiceSeparatedFieldTool.C:127`. Flattened names put the path in front with `:`: node `X1:mid`, device `X1:R1`. `0`, `$G*` and `.GLOBAL` nodes keep their names (IO/N_IO_DistToolBase.C:797-803, 824-825). `-hspice-ext separator` switches the separator to `.` (IO/N_IO_ParsingMgr.h:85-88; RG/Xyce_RG_app2.tex:57). | Any run of characters up to whitespace or `, = ( ) { } ;` (include/ap.h:120-128; apps/lang_spice.cc:463-472). **Run:** `R_a.b:c` and node `mid/x#1` are accepted. There is no flattening: each placement keeps its own card list. The printed name is `X1.R1`, outer part first (lib/e_card.cc:65-72). Probes also accept `R1.X1` (lib/u_prblst.cc:235-249, lib/findbr.cc:53-65). A `.` inside a device name breaks probing it (**run**). |
| **P2** Case-insensitive? | Yes. | Yes, except file names in `.include` (UG/Xyce_UG_ch04.tex:124-127). Device and node names are upper-cased (IO/N_IO_DeviceBlock.C:263, 420). | Yes in `spice` and `acs` modes (apps/lang_spice.cc:84, 96). Nodes are lower-cased (lib/u_nodemap.cc:86-92), and labels compare with `strcasecmp` (lib/e_base.cc:172-179). Listings keep the original spelling (**run**). |
| **P3** Is `gnd` ground? | Yes (checked on ngspice-42, `netlist_writer.md` §5). | Only `0` by default (UG/Xyce_UG_ch04.tex:156). `.PREPROCESS REPLACEGROUND TRUE` maps `GND`, `GND!` and `GROUND`, in any case, to `0` (RG/PREPROCESS_Command.tex:11-33; IO/N_IO_SpiceSeparatedFieldTool.C:391-402). | Only `0` (lib/u_nodemap.cc:28, 33). **Run:** `gnd` is an ordinary node. |
| **P4** Same device name twice | Error, "device already exists". | Error, "Duplicate device" (IO/N_IO_CircuitBlock.C:878-883). A duplicate `.model` is an error too (IO/N_IO_CircuitBlock.C:1466-1468; DEV/N_DEV_DeviceModel.C:150-153). | Kept, with no message (include/e_cardlist.h:97). **Run:** `R1`, `R1` and `r1` became three resistors. With a duplicate `.model`, the first wins silently. |
| **P5** Model name starting with a digit | Accepted. | Accepted. The model is found by looking up each field's text among the defined models, not by its spelling (IO/N_IO_DeviceBlock.C:1767-1795, 1698-1731). | Accepted: there is no first-character rule (**run**: `.model 1N4148 D`). |
| **P6** Numbers | Suffixes T G MEG K M MIL U N P F. Trailing letters are ignored (`10uF`). Not correctly rounded: digits are summed, then multiplied by a power of ten (`inpeval.c:202`). `1d3`, `µ` and `a`: `?`. | T G K M MEG MIL U N P F, and **X = mega**. `A` means atto only under `-hspice-ext units`; otherwise ×1 (UTL/N_UTL_ExtendedString.C:58-100). Text after the suffix must be one of C V VOLT VDC A AMP AMPERE F FARAD HENRY HY IL EG H HZ HERTZ OHM SECOND S METER M MEG MIL (:149-280). So `10uF` works; `10Ohms`, `1d3`, `1.5rad`, `10Farad` and `µ` are not numbers, and on an R line that is an "Illegal value" error (IO/N_IO_DeviceBlock.C:633-638). Conversion is `atof`, then one multiply by the suffix, so a suffixed value can be 1 ulp off: `4.7n` → `4.700000000000001e-09`. In expressions the exponent has at most 3 digits (UTL/ExpressionSrc/ExpressionLexer.l:149-199). | T G K MEG M MIL U N P F **A = atto**, and `%` (lib/ap_convert.cc:303-338). No `X`, no `µ`. Any trailing letters are skipped (:339-341). A suffix after an exponent is ignored: `1e3k` = 1000 (:290-302). **Run:** `1µ` → 1 with no warning. `1d3` reads as `1`, then a stray `3` with a warning, and on an R line the last number wins. Not correctly rounded: `val*=10`, then `power*=.1` for each digit after the point (:281-289). **Run:** `0.3` → `0.30000000000000004`. |
| **P7** Phases | AC and SIN phases are in degrees. `1.5rad` is read as 1.5°. | Degrees: AC phase (RG/V_Device.tex:60; DEV/N_DEV_SourceData.C:3106-3110) and SIN phase, the 6th argument (RG/V_Device.tex:87-99; N_DEV_SourceData.C:485-490). `rad` and `deg` are rejected, because they aren't on the unit list (P6). | Degrees, but only as the keyword `phase=` (lib/bm.cc:102-103). A positional AC phase starts a new clause, and for a source that means DC. **Run:** `AC 1 90` gives DC 90, AC phase 0, with no warning (lib/bm_cond.cc:109-168). SIN has no phase argument (apps/bm_sin.cc:44-50). |
| **P8** R/C/L value spellings; card `R`/`C`/`L` | Line: `r`/`resistance`, `c`/`cap`/`capacitance`, `inductance`. Card: `r`/`res`/`resistance`, `cap`/`capacitance`, `ind`/`inductance` give the **default value**, and a value on the line overrides it. `c` on a C card is a type flag, which leaves 0 F. `.model RM R r=2k` + `R1 a b RM` → **2 kΩ**. `R1 a b RM 1k` with card `R=2` → **1 kΩ**. | Line: `R`, default 1000 (OM/N_DEV_Resistor.C:169). Card `R` is a **"Resistance Multiplier", default 1** (OM/N_DEV_Resistor.C:254-256; RG/R_1_Device_Model_Params.tex). With no value on the line, R = `RSH·L/W` if given, else 1000, with the warning "Resistance is set to 0, setting to the default, 1000 ohms" (N_DEV_Resistor.C:485-497). Then R × card `R` / M (:950-955). So `R1 a b RM` → **2 MΩ**, and `R1 a b RM 1k` with `R=2` → **2 kΩ**. C: card `C` is a multiplier, default 1 (OM/N_DEV_Capacitor.C:148-150, 324-326); no value on the line is an error (:233-236). L: card `L` is a multiplier, default 1 (OM/N_DEV_Inductor.C:114-116, 202-207); no value is an error (:288-290). The manual contradicts the code (RG/R_Device.tex:57-59). | Card types `r`/`res` and `c`/`cap` only (apps/bmm_semi.cc:167-171). Neither has an `r` or `c` parameter: the R card has rsh, narrow, defw, tc1, tc2, tnom (:548-589). The value is `value·(1+tc1·dt+tc2·dt²)`, or `rsh·L/W` when rsh is given (:329-363). **Run:** `.model RM R r=2k` warns "bad parameter r ignored". Then `R1 a b RM` has value 0, which becomes the 10 µΩ short circuit (apps/d_res.cc:86-92; lib/u_opt1.cc:67). `R1 a b RM 1k` with `R=2` → 1 kΩ. A C card ignores the line's value entirely (cj·L·W; **run**: 0 F). There is no inductor card type at all (**run**: `L … LM 1u` → 0 H). |
| **P9** Instance parameter on a card as a default | Yes, for every instance parameter except `m` and `level` (`inpgmod.c:159-170`). | No. An unknown card parameter warns "No model parameter X found … parameter ignored" (IO/N_IO_ParameterBlock.C:663-668). A few devices do fall back to the card's `TC1`/`TC2` (N_DEV_Resistor.C:476-483). | No. It warns "bad parameter area ignored" (apps/lang_spice.cc:428; **run**). |
| **P10** Value and model order on R/C/L | R: either order. C and L: model first only; ngspice-42 rejects `C1 a 0 1n CM`. | Both orders (IO/N_IO_DeviceBlock.C:584-677). The manual shows model first (RG/R_Device.tex:17, RG/C_Device.tex:18). | Model first only. `R1 a b 1k RM` warns "what's this?" and loses the value (lib/bm_cond.cc:121-123; **run**). Named parameters after a model must be in parentheses: `R2 2 0 T1 (w=2u l=1u)` (tests/bmm_res.1.ckt:4). |
| **P11** `R=`, `C=`, `L=` on the line | `r=`, `c=` and the long names. `L=`: no, only `inductance=`. | `R=`, `C=`, `L=` (and `Q=` for C) (IO/N_IO_DeviceBlock.C:604-611; RG/R_Device.tex:28, RG/L_Device.tex:28). The long names are **not** accepted: "Illegal value found" (:625-641). | None in SPICE mode. **Run:** `R1 1 0 R=1k` → "parameter R not specified, using default", which gives a 10 µΩ short. `r=` works only in Spectre mode. |
| **P12** `off` flag; positional extras | Positional extras after value and model → "unknown parameter (5)". `off` placement: `?`. | `off` is accepted anywhere among the named parameters (IO/N_IO_DeviceBlock.C:1904-1916). After those come, in fixed order, a positional `AREA`, then `ON`, `OFF`, `TEMP=` (:706-798). Anything else is "Unrecognized fields" (:801-810). `D1 a 0 off DM` silently drops the `off` *(inferred)*. The only positional extra is `AREA`, on D and Q. `R2 n1 n0 2k 1 5` is an error. | No bare flags. `off` needs a value: **run** `D2 2 0 DY off` → "off has no value?", and off is not set. Tests write `off=1`. The only positional extra is `area` (modelgen/mg_out_h.cc:232). **Run:** on a 3-terminal BJT, a trailing `off` or `2` is read as a 4th node. `R7 1 0 1k 5` → R = 5 with a warning. |
| **P13** `.temp` with several values; `.options` | `.temp`: `?`. `.options` repeats: `?`. | **No `.TEMP` at all**: "Unrecognized dot line will be ignored" (IO/N_IO_CircuitBlock.C:1559-1565; RG/pspicetbl.tex:139-142). Use `.OPTIONS DEVICE TEMP=`, or `.STEP TEMP LIST 27 85` for several temperatures (RG/Xyce_RG_app2.tex:575-576). `.OPTIONS` takes a package name. A bare `TEMP=`/`TNOM=` maps to DEVICE; any other SPICE-style option warns and is ignored (IO/N_IO_PkgOptionsMgr.C:236-290). Repeats merge, but a repeated parameter keeps the **first** value, with a warning (:159-170; RG/OPTIONS_Command.tex:68-73). Top level only; inside a subcircuit it warns and is ignored (:79-81). | `.temp` reads one value and silently ignores the rest (apps/c_comand.cc:90-103; **run** `.temp 50 100` → one run at 50 °C). Commands take effect in file order, so `.temp` and `.options` change only later analyses; the latest value before an analysis wins (**run**). |
| **P14** Unknown parameter | Instance: error. Model: warning. | Instance: error, "Unrecognized parameter X for device Y" (IO/N_IO_DeviceBlock.C:2289-2294). Model: warning, and the parameter is ignored (IO/N_IO_ParameterBlock.C:663-668; RG/Xyce_RG_app2.tex:458-460). | Both: warning, "bad parameter X ignored", and the run continues (apps/lang_spice.cc:428; **run**). An unknown card type warns too (:844). |
| **P15** Defaults | TNOM 27. D: IS 1e-14, N 1, EG 1.11, XTI 3. Q: IS 1e-16, BF 100, XTB 0, XTI 3, EG 1.11. | Same. TNOM 27 °C (DEV/N_DEV_DeviceOptions.C:99; DEV/N_DEV_Const.h:48; RG/devtbl.tex:28). D: IS 1e-14, N 1, EG 1.11, XTI 3 (OM/N_DEV_Diode.C:100, 131, 248, 255). Q: IS 1e-16, BF 100, XTB 0, EG 1.11, XTI 3 (OM/N_DEV_BJT.C:127, 135, 661, 687, 696). | Same. TNOM 27 (lib/u_opt1.cc:45). D: IS 1e-14, N 1, EG 1.11, XTI 3 (apps/d_diode.model:379-395). Q: BF 100, IS 1e-16, XTB 0, XTI 3, EG 1.11 (apps/d_bjt.model:223-329). **Run:** `.list` confirmed these. |
| **P16** Comments, `+`, `.end`, includes, `.param` | `*` starts a comment line. Inline `;`, `$` and `//`. `+` continues a line. `.end` is optional. `.include` paths are relative to the including file. `{}` expressions. | `*` starts a comment line. Inline `;` works anywhere (IO/N_IO_SpiceSeparatedFieldTool.C:679-688). **A line starting with whitespace is a comment** unless its first character is `+` (:711-747, 823-835; UG/Xyce_UG_ch04.tex:198-199). There is no `$` or `//` comment. `.END` is required by the manual (UG/Xyce_UG_ch04.tex:47), reading stops there (IO/N_IO_CircuitBlock.C:1346-1355), and no error for a missing one was found. `.INCLUDE` looks first relative to the including file, then the top netlist's folder, then the working directory (RG/INCLUDE_Command.tex:37-44). `.LIB file section` is HSPICE-style. `.PARAM` values may be `{}`, `'…'` or bare; a redefinition keeps the last one silently (RG/PARAM_Command.tex:38-57). `log()` is base 10 (RG/pspicetbl.tex:23-24). A repeated parameter on one card is an error (DEV/N_DEV_DeviceModel.C:126-129). | Comment lines start with `*`, `;`, `#`, `'` or `"` (apps/lang_spice.cc:104). `*>` marks a line only Gnucap reads. There are **no inline comments**. **Run:** an inline `$` became a parameter name, and an inline `//` stopped the run. An inline `;` on a device line hung our test build. `+` or a trailing `\` continues a line (lib/ap_construct.cc:246-263). `.end` is optional (**run**). `.include` uses a plain `fopen`, so paths are relative to the working directory (lib/ap_construct.cc:50-51; **run**: a nested relative include failed). `.param`, `{}` and `'…'` all work. |

## Notes where it matters

**P1, names and flattening.**
- The three simulators give a flattened name three different shapes:
  - ngspice: `r.x1.r1`, with the kind letter in front;
  - Xyce: `X1:R1`, joined with `:`;
  - Gnucap: `X1.R1`, keeping the hierarchy and joining for display only.
- Xyce reserves `:` for this, so its manual forbids `:` in user names. It also lets a netlist reach inside a placement from outside, e.g. `R2 X1:mid 0 1` (RG/Xyce_RG_app1.tex:669-699). Neither of the others can.
- Gnucap reserves `.` the same way: a device named `R_a.b` works, but can't be probed.
- The only characters every reader accepts in a name are letters, digits and `_`, plus `$ # ! - / % & ~ ^ @ < > ? [ ] |` in Xyce and Gnucap. The `.` is safe everywhere except in a Gnucap probe.

**P6, numbers.**
- None of the three converts correctly rounded:
  - ngspice multiplies a digit sum by a power of ten.
  - Xyce does `atof` (correctly rounded in glibc) and then one more multiply for the suffix. For the one-decimal mantissas 0.1–99.9 with a suffix of m, u, n, p, k or MEG, 1334 of 5994 combinations come out 1 ulp off the correctly rounded value (checked with Python's float arithmetic, which is IEEE double like C).
  - Gnucap accumulates `power *= .1` for each digit.
- So any check against a simulator needs a tolerance of a few ulp, as `netlist_writer.md` §2.4 already says.
- The suffix letters differ in meaning:
  - `1X`: 1e6 in Xyce, 1 in Gnucap.
  - `1A`: 1e-18 in Gnucap, 1 in Xyce.
  - `1e3k`: 1e6 in Xyce, 1e3 in Gnucap.
  - `1µ`: an error in Xyce, a silent 1 in Gnucap.
- The one trap common to all three: `10F` is 10 femtofarads, not 10 farads.

**P8, the model card multiplier.** This is the biggest trap for a reader that takes vendor libraries.
- PSpice libraries write `.model RMOD RES(R=1 TC1=…)` and put the value on the line. PSpice multiplies the two. This is from the PSpice reference manual *(recall; no PSpice source here)*, and the PSpice example in Xyce's manual, `R1 1 2 RMOD 1` with `.model RMOD RES(R=30)` swept 30→50, only works that way (RG/Xyce_RG_app2.tex:343-345).
- Xyce follows PSpice. ngspice reads the same card as a default value that the line overrides.
- When the card has `R=1`, as is usual in PSpice libraries, all readers agree. They disagree when the card's `R` differs from 1 and the line also has a value (the product vs the line value), and when the line has no value (2 kΩ, 2 MΩ, or a short).

**P10 and P11, the portable way to write R, C and L.**
- Model name first, then a positional value (`C1 a 0 CM 1n`), is the only form that ngspice-42, Xyce and Gnucap all read the same. That is already what our writer does.
- Named values (`r=`) are not portable: Gnucap rejects them, and for inductors ngspice and Xyce share no spelling.

**P13, temperature.**
- A netlist with `.temp 85` runs at 27 °C in Xyce, with only a warning. The writer's `.temp` line (netlist_writer.md §2.5) is therefore ngspice-only.
- Xyce needs `.OPTIONS DEVICE TEMP=85`. It also ignores SPICE-style `.options reltol=…` with a warning.

**P16, leading whitespace.** Xyce skips any line that starts with a space or tab (unless its first character is `+`). An indented instance line would vanish without an error. Our writer must never indent a line.

## What an ideal reader does

One line each, with the reason.

- **P1:**
  - Accept any printable character in a name except whitespace and `( ) { } , = ; ' "`, and accept `:` as an ordinary character.
  - Keep `.` as our hierarchy separator.
  - Reason: this is the union of what the three accept, minus the delimiters. Xyce's `X1:node` reach-in is Xyce-only, and a plain-character reading of `:` matches the other two.
- **P2:** Match names, models, parameters and keywords ignoring case, but keep the original spelling for messages, and keep file names case-sensitive. Reason: all three do this.
- **P3:** Treat `gnd`, in any case, as ground, as ngspice does, and not `ground` or `gnd!`. No warning. Reason: ngspice is our reference, most people who write `gnd` mean ground, and the writer already renames such a node.
- **P4:** Error on a duplicate device name or a duplicate `.model` name, pointing at both lines. Reason: ngspice and Xyce both error; Gnucap's silent keep hides mistakes.
- **P5:** Accept model names starting with a digit. Reason: all three do, and vendor libraries rely on it (`2N3904`, `1N4148`).
- **P6:**
  - Keep correct rounding.
  - Accept T G MEG K M MIL U N P F.
  - After the suffix, accept only a known unit list (V, A, F, H, OHM/OHMS, HZ, S, and their long forms) and error on anything else.
  - Error on `X`, `A` (as a suffix), `µ`, a `d` exponent, and a suffix after an exponent (`1e3k`).
  - Reason: every one of those errors is a case where two readers disagree, and some of them silently (Gnucap's `1µ` = 1). The unit list stops `1.5rad` and `10Ohmz` from reading as something else.
- **P7:** Read AC and SIN phases in degrees, and keep rejecting angle suffixes (`rad`, `deg`). Reason: ngspice and Xyce agree on degrees, while `rad` means degrees in ngspice, an error in Xyce and a DC value in Gnucap.
- **P8:**
  - Keep ngspice's meaning (the card's `r`/`c`/`l` is the default value).
  - Error when a card's `r`/`c`/`l` other than 1 applies to an instance that also gives a value.
  - Warn when an instance takes its value only from the card.
  - Reason: that text means "line value" in ngspice and "product" in PSpice and Xyce. A card value of 1 (the PSpice-library norm) is safe everywhere. The value-from-card-only case gives 2 kΩ, 2 MΩ or a short depending on the reader.
- **P9:** Keep accepting an instance parameter on a card, as ngspice does, and extend it to every instance parameter except `m` and `level` (the plan in `netlist_writer.md` §5). Reason: ngspice is the reference; Xyce and Gnucap only warn and drop, so a netlist written for them never relies on it.
- **P10:** Accept both orders, value-then-model and model-then-value. The writer keeps emitting model first. Reason: Xyce accepts both and neither order is ambiguous once models are known, but only model-first loads in all three.
- **P11:** Accept every named spelling: `r`/`resistance`, `c`/`cap`/`capacitance`, `l`/`inductance`. Reason: each is unambiguous, and Xyce's `L=` and ngspice's `inductance=` both occur in real netlists.
- **P12:**
  - Accept `off` anywhere after the model name, and error on it before the model name.
  - Accept one positional `area` on D and Q, and error on any other positional extra (`R2 n1 n0 2k 1 5`).
  - Reason: Xyce silently drops a pre-model `off` on a diode and reads it as a node on a BJT. All three reject or misread extra positional values, so our current by-position reading of them should go.
- **P13:**
  - Accept one `.temp` value, and error on several until spicy has sweeps.
  - Accept repeated `.options`, but error when one option gets two different values.
  - Error on `.options` inside a subcircuit.
  - Reason: several temperatures mean one run each in PSpice, the first value in Gnucap, and nothing in Xyce. For a repeated option Xyce keeps the first value and ngspice (probably) the last.
- **P14:**
  - Error on an unknown instance parameter.
  - Warn on an unknown model parameter, naming each one.
  - Reason: ngspice and Xyce both error on instances, where a typo is likely. All three only warn on model cards, because vendor cards carry simulator-specific extras.
- **P15:** Use TNOM 27 °C and the standard defaults above. Reason: all three agree, and the writer writes every card in full anyway.
- **P16:**
  - Comment lines start with `*`. Accept inline `;`, and inline `$` and `//` when they follow whitespace. Accept `+` continuation.
  - Make `.end` optional, and ignore anything after it.
  - Resolve `.include` relative to the including file.
  - Accept `{…}` and `'…'` expressions.
  - Error on a repeated parameter on one card.
  - Read a line with leading whitespace as a normal line, and have the writer never indent.
  - Reason: this is ngspice's behaviour, and the writer's output must also survive Xyce's and Gnucap's stricter rules.
