# SPICE Readers: LTspice, PSpice and HSPICE, and What KiCad Writes

> 2026-09-30 · Research for the SPICE reader in `crates/spicy_netlist/src/reader/` (design notes: `netlist_writer.md`). Companion to `netlist_readers_ngspice.md` and `netlist_readers_xyce_gnucap.md`. **Status: research only. Nothing is decided or implemented.**
>
> **How it was checked.** Only manuals, help files and source code were read. None of the three simulators was run. They are commercial, and most of their manuals are old copies hosted by universities. A few facts come only from user forums; those are marked *(forum)*. "Not found" means the sources below don't say.

## Words used here

- **Instance line:** one line that places one part, e.g. `R1 a b 1k`. The first letter gives the kind: R resistor, C capacitor, L inductor, D diode, Q bipolar transistor (BJT), V/I voltage/current source, X subcircuit.
- **Model card:** a `.model NAME TYPE p=v ...` line holding parameters shared by every part that names it.
- **Flattening:** replacing each `X` placement of a subcircuit (a named block of parts) by copies of its parts, under longer names.
- **Ground:** the reference node, 0 V by definition.
- **AC analysis:** a small-signal sweep over frequency. Each source has an AC size and a **phase** (its angular offset).
- **TNOM:** the temperature at which a model's parameters were measured. The simulator corrects them for the actual circuit temperature.
- **IS, N, BF, NF, XTB, XTI, EG:** diode and BJT model parameters. IS is the saturation current, the tiny current that sets the forward voltage. N and NF are emission coefficients (how steep the diode curve is). BF is the forward current gain (collector current ÷ base current). XTB and XTI are the temperature exponents of BF and IS. EG is the band-gap energy, which sets how fast IS grows with temperature.

## Sources and citation keys

Every claim below carries a key. The key gives the source, then a page (p.), line (L) or section (§).

| Key | Source |
|---|---|
| `LTR-syn`, `LTR-el`, `LTR-cmd`, `LTR-wave` | Analog Devices' official LTspice reference for version 24 and later, 2026. The files are `SPICE-SYNTAX-REFERENCE.md`, `CIRCUIT-ELEMENTS-REFERENCE.md`, `SIMULATION-COMMANDS-REFERENCE.md` and `WAVEFORM-VIEWER-GUIDE.md` under `https://github.com/analogdevicesinc/ltspice-reference/blob/b4358f83e9d09a719ee5c08f25b7e236a896c41d/ai_ref/`. Cited by line (`LTR-syn:143` = line 143). |
| `LTH-<page>` | LTspice XVII help, mirrored at `https://ltwiki.org/LTspiceHelpXVII/LTspiceHelp/html/<page>.htm`. The pages used are `GeneralConventions`, `R-device`, `C-device`, `L-device`, `D-device`, `Q-device`, `V-device`, `DotTemp`, `DotModel` and `DotSubckt`. |
| `LTW` | "Undocumented LTspice", LTwiki: `https://ltwiki.org/index.php?title=Undocumented_LTspice`. Cited by section. |
| `PS` | Cadence *PSpice A/D Reference Guide*, product version 16.6, October 2012: `https://i-t.com/wp-content/uploads/2019/05/pspcref.pdf`. Cited by printed page. |
| `PS9` | OrCAD *PSpice Reference Guide*, 2000 (version 9.2): `https://www.engineering.upenn.edu/~jan/spice/PSpice_ReferenceguideOrCAD.pdf`. It says the same as `PS` on every point checked. |
| `PSUG` | OrCAD *PSpice User's Guide*, 2000: `https://www.engineering.upenn.edu/~jan/spice/PSpice_UserguideOrCAD.pdf`. Cited by printed page. |
| `HS` | Synopsys *HSPICE User Guide: Simulation and Analysis*, B-2008.09: `https://cseweb.ucsd.edu/classes/wi10/cse241a/assign/hspice_sa.pdf`. Cited by printed page. |
| `HSC` | Synopsys *HSPICE Reference Manual: Commands and Control Options*, B-2008.09: `https://cseweb.ucsd.edu/classes/wi10/cse241a/assign/hspice_cmdref.pdf`. Cited by printed page. |
| `HS98-12`, `-13`, `-14` | *Star-Hspice Manual*, release 1998.2, chapters 12 (passive devices), 13 (diodes) and 14 (BJTs): `https://web.engr.oregonstate.edu/~moon/ece323/hspice98/files/chapter_12.pdf` (and `chapter_13.pdf`, `chapter_14.pdf`). The 2008 guide points to a later edition of these chapters for the model parameters, which I couldn't download. |
| `KC` | KiCad source at commit `c2bf0552a9f07943542f1713910f8d8790132500` (master, 2026-09-30): `https://gitlab.com/kicad/code/kicad/-/blob/c2bf0552a9f07943542f1713910f8d8790132500/<path>`. Cited as `file:line`. |
| `AAC-PS` *(forum)* | `https://forum.allaboutcircuits.com/threads/what-am-i-doing-wrong.75967/`: a PSpice 9.1 output file. |
| `AAC-LT` *(forum)* | `https://forum.allaboutcircuits.com/threads/ltspice-error-log-unknown-parameter-using-vishay-fet-model.179218/`: an LTspice error log, 2021. |
| `GIO-LT` *(forum)* | `https://groups.io/g/LTspice/topic/netlist_error_duplicate/50206587`: the thread title only; the page is behind a login. |

## Summary

The biggest trap is the value on a resistor, capacitor or inductor model card. In PSpice, `R`, `C` and `L` on `RES`, `CAP` and `IND` cards multiply the line's value (default 1). So `R1 a b RMOD 1k` with `.model RMOD RES (R=2)` is 2 kΩ (`PS`:305–306). In HSPICE, the card's `RES`/`CAP` is only a default, used when the line gives no value, so the same circuit written the HSPICE way is 1 kΩ (`HS98-12`:12-12, 12-13). The ngspice report finds that ngspice follows HSPICE here. Next come the names of ground and of flattened parts. PSpice knows only `0` as ground (`PS`:106). LTspice adds `GND` (`LTH-GeneralConventions`). HSPICE adds `GND`, `GND!` and `GROUND` (`HS`:60), and KiCad writes `GND` (`KC` `netlist_exporter_spice.cpp:294-301`). Flattened names use `.` in PSpice and HSPICE (`X3.Q13`) but `:` in LTspice (`x1:Q1`). Third, a few number spellings mean different things in different dialects. `X` means 10⁶ only in HSPICE, and `A` means 10⁻¹⁸ only in HSPICE (and ngspice). `6K34` means 6.34k only in LTspice, and `1D-3` is an exponent only in HSPICE. In LTspice and PSpice, `X` and `A` are unit letters that are ignored. Everywhere, `M` is milli and `MEG` is mega. Fourth, HSPICE's default TNOM is 25 °C, against 27 °C in LTspice and PSpice (`HSC`:598, `PS`:76, `LTR-cmd`:579). Fifth, the three rank a parameter written on both the line and the card differently: in HSPICE the line wins (`HS98-14`:14-2), but a PSpice resistor takes its temperature coefficients from the card (`PS`:302). The three agree on much more: names ignore case; phases are in degrees; `*`, `+` and a title first line work the same; they read the same default diode and BJT parameters; and a `.temp` list means one run per temperature. PSpice and HSPICE both reject duplicate device names. An ideal reader accepts everything the dialects agree on and gives an error wherever the same text means different circuits.

## Comparison

| Point | LTspice | PSpice | HSPICE |
|---|---|---|---|
| **P1** Names; flattened names | Node names are "arbitrary character strings" (`LTH-GeneralConventions`); letters, digits, `_` (`LTR-syn`:147); Unicode (`LTR-syn`:47). Its own netlister writes `X§U1` (`LTR-el`:846). Hierarchy uses `:`: `V(x23:node1)` (`LTR-cmd`:367), `Ie(x1:Q1)` (`LTR-wave`:124); the old help's expansion shows `r:1:1` (`LTH-DotSubckt`). | Names are alphanumeric. A named substrate node must be written `[SUB]`, because a name can't be told from a model name (`PS`:272). Capture writes `R_R23`, `X_U33` (`PSUG`:184). Flattening: `Q13` in `X3` becomes `X3.Q13`, node `5` becomes `X3.5` (`PS`:107). | Up to 1024 characters. Table 3: `! @ # _ /` are allowed anywhere; `$ ~ % ^ & * - + < > ? [ ] { } \| : ;` only inside a name (rules vary); `( ) , = ' "` are illegal (`HS`:42–45). `.` is the hierarchy separator (`HS`:46, 60). Flattening: `X1.XBIAS.M5`, `X1.X4.sig25`; the listing also uses `4:INTNODE1` (`HS`:63–64). |
| **P2** Case-insensitive? | Yes, Unicode case folding (`LTR-syn`:47, 143). | Yes (`PS`:12; `PSUG`:80). | Yes, except file names (`HS`:40). |
| **P3** Ground names | `0`; `GND` is a synonym; `00` is a different node (`LTH-GeneralConventions`; `LTR-syn`:145–146). | Only `0`: "Zero is reserved for the global ground node" (`PS`:106); a ground part must be named 0 (`PSUG`:123). | `0`, `GND`, `GND!`, `GROUND` (`HS`:60). Another page says "0, GND, or !GND" (`HS`:63). |
| **P4** Duplicate device names | Not found in the manuals. A forum thread is titled "netlist error: duplicate instance name" (`GIO-LT`). | Error: "Name 'R_R1' is defined more than once" (`AAC-PS`). Across model libraries the first model found wins (`PSUG`:163). | Error and stop ("attempts to redefine c1"); with `.option NOELCK` the last one wins (`HSC`:475). Duplicate `.model`: error (`HS`:852). |
| **P5** Model names starting with a digit | Accepted; numeric-only names are documented (`.model 3904 ako:2N3904`) (`LTW` §9.3–9.4). | Not found. Cadence's libraries put a letter first (`Q2N2222`, `D1N3940`; `PSUG`:59, 155). | Accepted: the manual's own example uses `1stagepnp` (`HS`:166). |
| **P6** Numbers | `T G Meg K mil m u/µ n p f`, any case. Unknown trailing letters are ignored (`10V` = 10, `4Farads` = 4 fF). `6K34` = 6.34k (a setting). `.options reject_number_tails` makes trailing letters an error (`LTH-GeneralConventions`; `LTR-syn`:112–133). No `X`, no `a`. | `F P N U MIL M K MEG G T` (+ `C` = clock cycle) (`PS`:13); any case, so `M` = `m` = milli (`PSUG`:80). Unit letters never change the value (`2mV` = `.002`) (`PSUG`:567, for trace expressions). No `X`, `µ` or `a`. | `T G MEG`/`X` `K MIL M U N P F A` (atto) (`HS`:48). Exponent or suffix, not both (`1e-6u` is invalid). `D` or `E` marks an exponent. Trailing letters are unit comments (`HS`:49). `µ`: not found. |
| **P7** AC and SIN phase units | SIN phase in degrees (`LTH-V-device`; `LTR-el`:423). AC phase: the help documents only `AC=<amplitude>`, so the unit is not found (`LTH-V-device`). | AC phase in degrees (`PS`:180); SIN phase in degrees (`PS`:191). | AC phase in degrees (`HS`:180); SIN phase in degrees (`HS`:193). |
| **P8** R/C/L value spellings; model cards | Lines: `Rxxx n1 n2 <value>` (`LTH-R-device`); `R=<expr>` makes a behavioral resistor (`LTW` §7.6.1); `C … Q=<expr>`, `L … Flux=<expr>` (`LTH-C-device`, `LTH-L-device`). Cards: types `RES` and `CAP` accepted, `IND` not, with `Tc1`, `Tc2`, `T_measured` (`LTW` §7.6.4). A value multiplier on the card: not found. | Lines: `R<name> n+ n- [model] <value>` (`PS`:302). Card types `RES`, `CAP`, `IND` (`PS`:60); `R`/`C`/`L` as type names: not found. The card's **`R`, `C`, `L` are multipliers, default 1**: resistance = value · R · (1 + TC1·ΔT + TC2·ΔT²) (`PS`:305–306; C `PS`:158; L `PS`:221). | Lines: value positional or `R=`, `C=`, `L=` (`HS`:128, 134, 141). Card types `R`, `C`, `L` (`HSC`:189). The card's `RES` / `CAP` is the **default value, used only when the line gives none** (and, for R, no geometry). There is no multiplier; the line's `SCALE` scales (`HS98-12`:12-12, 12-13, 12-19; `HS`:129). On an **inductor, `R=` is its series resistance** (`HS`:142); on a resistor, `C=` is capacitance to bulk, and `L=`/`W=` are length and width (`HS`:129–130). |
| **P9** Instance parameter on the card as a default | Not found. | Not in general. For a resistor, TCs on the card **beat** the TCs on the line (`PS`:302). Device temperature can be set on the card (`T_ABS`, `T_REL_GLOBAL`) (`PS`:63). | Yes: "the element parameter always overrides the model parameter" (`HS98-14`:14-2). The diode's `AREA` and the resistor's `L`, `W`, `C` have card defaults (`HS98-13`:13-16; `HS`:129–130). |
| **P10** Value / model order | Not documented (no model form on the R line). | Model, then value (`PS`:156, 219, 302). | Model, then value; after the nodes and model, the rest in any order (`HS`:128–130, 142). If a model and a `.param` share a name, the model is taken (`HS98-12`:12-11). |
| **P11** `R1 a b R=1k` | `R=` exists, as a behavioral resistor (`LTW` §7.6.1). `C=`, `L=`: not found. | Not in the syntax; not found. | Yes (`HS`:128–130, 134, 141). |
| **P12** `OFF`; positional parameters | D: `<model> [area] [off] [m=] [n=] [temp=]`; Q: `<model> [area] [off] [temp=]` (`LTR-el`:163, 215). A flag can also be given a value (≥ 0.5 = on) (`LTR-el`:48). `OFF` placed anywhere: not found. HSPICE-style extra numbers after an R value are rejected *(forum, `AAC-LT`)*. | No `OFF` on D or Q; only a positional `[area value]` (`PS`:160, 272). Resistor TCs are written `TC=a,b` (`PS`:302). | `<area> <OFF> <IC=…> <M=…> <DTEMP=…>`, or `AREA=` (`HS`:162, 164). Resistor: positional `Rval [TC1 TC2 TC3]` (`HS`:128). "The nodes and model name must precede other fields" (`HS`:165). |
| **P13** `.temp` list; `.options` | `.temp T1 T2 …` = `.step temp list …`, one run each (`LTH-DotTemp`; `LTR-cmd`:503). Line order doesn't matter (`LTH-GeneralConventions`). Repeated `.options`: not found. | One run per temperature (`PS`:64, 109). `.OPTIONS` is cumulative; if an option repeats, the last value wins (`PS`:71). | One set of analyses per temperature; HSPICE RF keeps only the last `.TEMP` (`HSC`:269). Any number of `.OPTION` lines; the last definition wins (`HSC`:205). |
| **P14** Unknown parameters | Manual: not found. Unknown model parameters (`TRS`, `CAPOP`) show up in the error log *(forum, `AAC-LT`)*. Rating parameters are accepted and have no effect: BJT `Vceo`, `Icrating`, `mfg`; diode `Vpk`, `Ipk`, `Iave`, `Irms`, `diss` (`LTH-Q-device`, `LTH-D-device`). | Not found in the manuals. | Unknown Verilog-A parameters: ignored with a warning (`HS`:778). An undefined `.param` name: error (`HS`:852). Built-in devices: not found. |
| **P15** Default TNOM; diode and BJT defaults | TNOM 27 °C, temp 27 °C (`LTR-cmd`:579). Diode IS 1e-14, N 1, EG 1.11, XTI 3 (`LTH-D-device`). BJT IS 1e-16, BF 100, NF 1, XTB 0, XTI 3, EG 1.11 (`LTH-Q-device`). | TNOM 27 °C, also the default temperature (`PS`:76). Diode IS 1e-14, N 1, EG 1.11, XTI 3 (`PS`:161–162). BJT IS 1e-16, BF 100, NF 1, XTB 0, XTI 3, EG 1.11 (`PS`:280–282). | **TNOM 25 °C** (27 with `.OPTION SPICE`); the circuit temperature defaults to TNOM (`HS`:867; `HSC`:598). Diode IS 1e-14, N 1, XTI 3, EG 1.11 (1.16 at TLEV=2) (`HS98-13`:13-17, 13-23, 13-24). BJT IS 1e-16, BF 100, NF 1, XTB 0, XTI 3, EG 1.11 (`HS98-14`:14-19, 14-20, 14-28, 14-32). |
| **P16** Comments, `+`, `.end`, include, expressions | Title first line; `*` for a line; `;` inline; `+` continues (`LTR-syn`:19–57). `.end` is optional and later lines are ignored (`LTR-syn`:20). `.lib` drops top-level parts (`LTR-cmd`:670). Expressions in `{}` (`LTH-X-device`). | `*`; `;` inline; `+` (`PS`:125–127). `.END` must be the last statement, and one file can hold several circuits (`PS`:44). `.INC` nests at most 4 deep (`PS`:50). `.LIB` holds only models, subcircuits, `.PARAM`, `.FUNC` and `.LIB` (`PS`:51). Expressions in `{}` (`PS`:80). | The first line is always the title; `*`; `$` inline after a space, comma or number; `+`; ` \` continues quoted strings (`HS`:40–41, 55–56). A missing `.END` is an error (`HS`:40). `.include 'f'`, `.lib 'f' entry` (`HS`:40–41). Expressions in `'…'` quotes; `{}` inside names becomes `[]` (`HS`:41, 44). |

Two HSPICE pages contradict each other on numeric node names. Page 60 says `1A` and `1` are different nodes, but pages 59 and 63 say letters after a number are ignored, so `1c` and `1d` are the same node (`HS`:59, 60, 63).

## What KiCad's exporter writes

KiCad (eeschema) writes an ngspice netlist from the schematic. The code is `netlist_exporter_spice.cpp` and the `sim/` model classes.

- **Frame.** The first line is `.title KiCad schematic` and the last is `.end` (`KC` `eeschema/netlist_exporters/netlist_exporter_spice.cpp:119-128`). Libraries come in as `.include "path"`, in double quotes with `/` separators (`:800-846`).
- **Device names.** A device takes its reference designator. If the reference doesn't start with the device's SPICE letter, KiCad puts the letter in front: `U1` placing a subcircuit becomes `XU1`, and a diode labelled `LED1` becomes `DLED1` (`KC` `eeschema/sim/spice_generator.cpp:138-144`). The check is case-sensitive. Duplicate references can't happen: the exporter asserts they are unique (`netlist_exporter_spice.cpp:635-642`).
- **Node names.** A node takes KiCad's net name, with `% ( ) , [ ] < > ~` and spaces replaced by `_` (`netlist_exporter_spice.cpp:282-292`). So:
  - An unnamed net `Net-(R1-Pad2)` (`KC` `eeschema/connectivity/conn_pin_name.cpp:27-66`) is written `Net-_R1-Pad2_`. With a named pin, `Net-(U1A-OUT)` becomes `Net-_U1A-OUT_`.
  - A local label carries its sheet path: `/VCC`, `/amp/in`.
  - A label on the root sheet that would start with `//` becomes `/root/…`, because `//` starts a comment in ngspice (`:303-306`).
  - Global labels and power nets have no prefix: `+5V`, `VCC`.
  - A pin with no net becomes `NC-1`, `NC-2`, … (`:938-949`). A symbol pin missing from the model becomes `NC-R1-0` (`spice_generator.cpp:159`).
  - Node names can therefore start with `/`, `+`, `-` or a digit.
- **Ground.** A net whose last path part is `0` or `gnd` (any case) is written as that part alone: `/amp/GND` becomes `GND` (`netlist_exporter_spice.cpp:294-301`). So KiCad's ground is `0` only when the schematic uses a symbol named `0`. With the usual `GND` power symbol it is **`GND`**, and the netlist relies on the reader taking `GND` as ground.
- **Values.** Values are converted to SPICE suffixes `f p n u m k Meg G T` and printed with `{:g}`, so at most 6 significant digits (`KC` `eeschema/sim/sim_value.cpp:293-363`, `:393`). Plain numbers are passed through unchanged (`:414-434`). There is no `MIL`, no `a` and no unit letter. The schematic's SI `M` (mega) is written `Meg`, `µ`/`μ` becomes `u`, and RKM `4k7` becomes `4.7k` (`KC` `eeschema/sim/sim_model.cpp:1071-1090`, `:1223-1255`).
- **R, C, L lines.** The value goes positionally where a model name would go: `R1 /in /out 4.7k` (`KC` `eeschema/sim/sim_model_ideal.cpp:33-42`).
- **Other parameters.** Instance parameters are written `name=value`, and flags as a bare word (`spice_generator.cpp:178-205`).
- **Subcircuit calls.** Parameters are written `name=value` with no `PARAMS:` keyword (same code).
- **Sources.** `V1 n+ n- DC 5 SIN( 0 1 1k ) AC 1 0`: the waveform goes in `NAME( args )`, and the AC phase is in degrees (`KC` `eeschema/sim/sim_model_source.cpp:189-213`, `:324`, `:357`, `:1238-1243`).
- **Model names.** A library model keeps its library name, often digit-first like `2N3904`, and its file is `.include`d. If the user overrides card parameters, KiCad writes its own card named `<Ref>.<BaseModel>`, for example `.model Q1.2N3904 NPN`, with one `+name=value` continuation line per parameter. A model with no base gets the name `__<Ref>` (`spice_generator.cpp:28-110`). **Model names can therefore contain `.` and start with `_` or a digit.**

## What an ideal reader should do

**P1. Names.** **Accept** any run of characters that aren't delimiters (whitespace, `=`, `(`, `)`, `,`, quotes), including `_ . / + - ! # § [ ]` and Unicode letters. A device name must start with its kind letter; a node name may start with any of these characters. *Reason:* KiCad writes `/VCC`, `+5V` and `Net-_R1-Pad2_`, LTspice writes `X§U1`, and no dialect gives these characters another meaning where a node is expected. **For flattened names, use `.`** as the writer already does (`R.X1.R1`), not LTspice's `:`. *Reason:* PSpice, HSPICE and ngspice all use `.`, and `:` marks element attributes in HSPICE (`HS`:46).

**P2. Case.** **Accept**, and compare names ignoring case. Keep the first spelling for messages. *Reason:* all three dialects agree.

**P3. Ground.** **Accept `0` and `gnd` (any case) as ground, silently.** *Reason:* LTspice, HSPICE, ngspice and KiCad agree, and PSpice's own netlists only use `0`. **Accept `gnd!`, `ground` and `!gnd` as ground with a warning** that says only HSPICE reads them that way. *Reason:* in the other dialects they are ordinary nodes, but a node with those names that isn't ground is almost surely a mistake. `00`, `0.3` and `gnd1` stay ordinary nodes, as LTspice and HSPICE say (`LTR-syn`:146; `HS`:60).

**P4. Duplicate device names.** **Error**, comparing names ignoring case and pointing at both lines. Do the same for duplicate `.model` names in one scope. *Reason:* PSpice and HSPICE both refuse the netlist. KiCad never produces duplicates, and "last one wins" exists only behind HSPICE's opt-in `NOELCK`.

**P5. Model names starting with a digit.** **Accept.** *Reason:* HSPICE's manual, LTspice and KiCad's libraries (`2N3904`) all use them. P10's lookup tells such a name from a value.

**P6. Numbers.**
- **Accept** `T G MEG K MIL M U N P F`, `µ` (U+00B5) and `μ` (U+03BC) as micro, in any case, with `M` = milli. **Accept and ignore** trailing unit letters (`10uF`, `5V`). *Reason:* the dialects agree, or no dialect gives the spelling another meaning.
- **Warn** on two traps: an upper-case `M` on a resistor (`1M` is 1 mΩ; LTspice calls it "a common error", `LTH-GeneralConventions`) and a capacitor value ending in a bare `F` (`4F` is 4 fF). *Reason:* these are the classic novice mistakes, and the netlist still means what it says.
- **Error** on the spellings the dialects read differently:
  - `X` right after the digits (mega in HSPICE, ignored in LTspice);
  - `A` right after the digits (atto in HSPICE and ngspice, a unit letter in LTspice and PSpice), so `1A` is an error while `10mA` is fine;
  - RKM codes like `6K34` (6.34k in LTspice, 6k in ngspice);
  - a `D` exponent (`1D-3`, HSPICE only);
  - an exponent plus a suffix (`1e-6u`, forbidden in HSPICE).

  *Reason:* each can silently change a value by up to 10¹⁸, and the fix is one edit.

**P7. Phases.** **Accept, in degrees**, for AC and SIN, and keep the reader's existing error on `rad`. *Reason:* PSpice and HSPICE say degrees for both, and so does LTspice for SIN.

**P8. R/C/L values and cards.**
- **Accept** the positional value, and `R=` / `C=` / `L=` on the device of the same letter, as the same thing. **Error** if a line gives both. *Reason:* HSPICE and LTspice use the named form, and a line with two values is ambiguous.
- **Error**, don't ignore, on HSPICE's other named parameters that we don't model: `R=` on an inductor, `C=`, `L=`, `W=` or `SCALE=` on a resistor, and so on. *Reason:* ignoring `R=` on an inductor silently drops its resistance (`HS`:142).
- **Accept** `RES`/`CAP`/`IND` and `R`/`C`/`L` as card types. *Reason:* the type names don't conflict.
- **Card values.** **Accept** a card value (`R`/`RES`, `C`/`CAP`, `L`) when the line gives no value: it is the value. **Accept** a card `R=1` (or `C=1`, `L=1`) alongside a line value, since it changes nothing either way. **Error** when both the card (≠ 1) and the line give a value. *Reason:* PSpice multiplies (2 kΩ in the example), while HSPICE and ngspice take the line's value (1 kΩ). The same text is two different circuits.

**P9. Instance parameters on the card.** **Accept** a parameter our device defines on both levels as a default from the card, with the line winning. *Reason:* this is HSPICE's rule, and the ngspice report finds ngspice uses the same fallback. **Error** when a temperature coefficient is on both the line and the card of an R, C or L. *Reason:* PSpice takes the card's (`PS`:302), HSPICE the line's.

**P10. Order of value and model name.** **Accept both orders.** Decide by lookup: a token naming a `.model` of the right kind is the model; otherwise it must read as a number. **Error** if it is neither, or both. *Reason:* PSpice and HSPICE put the model first, and HSPICE also prefers the model on a name clash (`HS98-12`:12-11). A lookup can't be fooled by digit-first model names like `2N3904`.

**P11. `R1 a b R=1k`.** **Accept** on the device's own letter, with a constant or a `{}` expression. *Reason:* HSPICE documents it, and LTspice uses `R=` too.

**P12. Flags and positional parameters.** **Accept** `off` anywhere after the model name. *Reason:* it can't be mistaken for a node, a model or a number. **Accept** the positional area on D and Q as well as `area=`. **Accept** `tc=a,b`, `tc1=` and `tc2=`. **Error** on HSPICE's positional TCs after a resistor value (`R1 a b 1k 0.001 0`), with a hint to write `tc1=`. *Reason:* LTspice rejects them *(forum)*, and the ngspice report finds ngspice silently takes a second bare number as the value.

**P13. `.temp` and `.options`.** A `.temp` list means **one run per temperature** in all three. Keep the current error until the engine can do several runs, then accept the list as a sweep, and never read it as "the last value wins". **Accept** `.options` anywhere and repeated, with the last value winning. **Warn** when the same option gets two different values. *Reason:* PSpice and HSPICE document exactly this, and a silent override is easy to miss.

**P14. Unknown parameters.** **Error**, naming the parameter and its line. **Accept and ignore** LTspice's rating annotations (`Vceo`, `Icrating`, `mfg`, `Vpk`, `Ipk`, `Iave`, `Irms`, `diss`). *Reason:* none of the manuals promises that an unknown parameter is harmless, and dropping one silently changes results. The rating annotations are defined as having no effect (`LTH-Q-device`), and vendor libraries use them.

**P15. Defaults.** **Accept**, with TNOM and the circuit temperature both 27 °C, and document that an HSPICE deck assumes 25 °C for both. *Reason:* LTspice, PSpice and ngspice use 27 °C, and HSPICE is the outlier. The writer already puts TNOM on every card, and the diode and BJT defaults (IS, N, BF, NF, XTB, XTI, EG) are the same in all three.

**P16. Syntax details.**
- The first line is the title.
- **Accept** `*` comment lines, `;` inline comments (LTspice, PSpice), and `$` inline comments after whitespace (HSPICE). The `$` rule means LTspice's automatically global `$G_…` nodes (`LTR-cmd`:696) can't be read; that is acceptable.
- **Accept** `+` continuation.
- **Accept** a missing `.end`. **Warn** if non-blank lines follow it, since PSpice would read them as a second circuit.
- **Accept** `.include` with `"…"`, `'…'` or no quotes. For `.lib`, accept both LTspice/PSpice's whole-file `.lib file` and HSPICE's `.lib file entry`.
- **Accept** expressions in `{…}` (LTspice, PSpice) and in `'…'` (HSPICE).
- Whether `//` starts a comment is not in any of the three manuals. Follow ngspice, which KiCad works around.

*Reason:* none of these conflict, except `.end` placement, which only matters to PSpice's rare multi-circuit files.
