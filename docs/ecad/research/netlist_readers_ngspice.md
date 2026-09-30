# How ngspice reads a netlist: claims checked

> 2026-09-30 · Checks 19 claims about ngspice's netlist reader that the netlist writer (`docs/ecad/netlist_writer.md`) and our reader (`crates/spicy_netlist/src/reader/`) rely on. Each claim was checked in the source and, where possible, by running a small deck through the installed `ngspice` (ngspice-42) in batch mode (`ngspice -b deck.cir`). **Status: research only. Nothing here changes code.**
>
> **Sources.** The tree at `/tmp/refs/ngspice` is *not* ngspice-42: it is the `pre-master-48` branch (after ngspice-47, Sept 2026). So I also fetched the `ngspice-42` tag (and 43–47 for C12) from the ngspice git repository. Citations are `file:line` under `src/` **in ngspice-42**, the version we run. Where the claim quoted a line number from the newer tree, it is given as "cur". The line numbers in `netlist_writer.md` (`subckt.c:1107-1143`, `inpgmod.c:159-170`, `inp.c:1242-1269`, `vsrcload.c:190`) are from the newer tree; the code is the same in 42, a few lines off. The manual quoted is the ngspice-42 manual (PDF, §3.3.6, §3.3.10).
>
> **Test decks** were run from a scratch directory outside the repo. Each row says what the deck contained and what ngspice printed. The AC tests print `vm(b)`, the size of the voltage at node `b` for a 1 V AC source (AC analysis: a small sine wave at one frequency).

## Summary

Most claims hold. Five are only partly right. **C3:** `gnd` becomes ground only through a text substitution that the variable `no_auto_gnd` turns off. **C7:** `1d-3` is an error, although `1d3` works; also, the three example numbers in the claim happen to round correctly (0.7, 0.47 and 100e-9 don't). **C11:** `l=` works on an inductor line only when there is no model name. **C13:** a second bare number is silently taken as the value (`R1 a b 2k 1` is 1 Ω), and a diode's bare area number after the model fails. **C16:** a resistor with no value and no model is dropped, not set to 1 mΩ. **C12's root cause is confirmed:** the pass that deletes unused `.model` cards (`inp_rem_unused_models`, `frontend/inpcom.c`) guesses where the model name sits on each line. For R it skips a leading value; for C and L it doesn't. So in `C1 a 0 1n CM` it takes `1n` as the model name, decides `CM` is unused and comments the card out. The line parser then can't find `CM` and reports "unknown parameter (cm)". The code is unchanged in ngspice-43 through 47 and in the current development tree. The same guessing breaks `D1 a 0 DM 2` and `Q1 c b 0 QM area 2` (a parameter written without `=`). The writer's plan (model name first, every value written as `name=value`) avoids all of these. Every named spelling it may use is accepted and the card is applied, except `L1 a b LM l=1u` (C18).

## Verdicts

| # | Claim (short) | Verdict | Evidence | Meaning for our reader |
|---|---|---|---|---|
| C1 | `_` and `.` in names; flattened `r.x1.r1`, `x1.mid` | TRUE | `subckt.c:587` (the prefix is the whole X name, `x1`), `1109-1127` (node: `x1.` + name), `1133-1145` (device: first letter + `.` + `x1.` + name; X lines get no letter). Deck with `R_2.a`, `R_load.x`, nested `Xin` printed `r.x1.r1`, `r.x1.r_2.a`, `r.x1.xin.r1`, node `x1.mid`. | Accept `_` and `.` after the first letter. Nested names stack: `r.x1.xin.r1`. |
| C2 | Names compared ignoring case | TRUE | `inpcom.c:1566-1583`: every line is lowercased on reading, except `.lib`, `.inc` and a few command lines (`echo`, `write`, `source`, …). `R6` + `r6` → "device already exists"; model `2N3904` used as `2n3904`; `print v(A)` works. | Compare device, node, model and section names ignoring case. ngspice reports everything in lowercase. |
| C3 | `gnd` (any case) is ground | PARTLY | Only `inp_fix_gnd_name` (`inpcom.c:2002-2023`, called at `1104-1105`) rewrites it; the parser's only ground is `"0"` (`inppas2.c:31`). It's skipped when `no_auto_gnd` is set (`.spiceinit`: `set no_auto_gnd`), and so is the `.global gnd` line inserted at `1669-1670`. See note. | Map `gnd`/`GND` to ground as ngspice does by default. Reject or warn on any other use of the word `gnd` (a model or subcircuit called `gnd`). |
| C4 | Duplicate device names are an error | TRUE | `devices/cktcrte.c:30-35` returns `E_EXISTS_BAD`, message at `parser/sperror.c:33`. Deck `R6 a 0 1k` + `r6 a 0 2k` → "device already exists, bail out". | Reject duplicates, ignoring case. See the extras for duplicate `.model` names, which ngspice does *not* reject. |
| C5 | Model names may start with a digit | TRUE | Deck `.model 2N3904 NPN is=1e-14 bf=200` + `Q1 c b 0 2N3904`: Ic = 5.67 mA, Ib = Ic/200 as expected; `D1 b 0 1N4148` also works. | Accept them. But on R, C and L lines a digit-leading model name written where the value goes is read as the value: `R1 a b 2RM` is 2 Ω and the card `2RM` is silently unused (checked). The reader should reject a digit-leading model name on R/C/L lines. |
| C6 | `.dc` finds its source ignoring case | TRUE | Deck `V1 …` + `.dc v1 0 2 1` swept correctly (follows from C2). | Look the source up ignoring case. |
| C7 | Number reading: not correctly rounded; suffixes; trailing letters; `d` exponent | PARTLY | `inpeval.c:67-116` adds up all digits into one `double`, `201-202` multiplies by `pow(10, e)`: two roundings. `4.7e-9`, `1.23456789e-11`, `0.1` come out correctly rounded, but `0.7` → `0.7000000000000001` and `0.47` → `0.47000000000000003` (exact bits read from a binary raw file). Also `0.3`, `100e-9`, `300.15`, `1.380649e-23` (17-digit print). Suffixes `T G MEG K M MIL U N P F A` checked (`inpeval.c:143-193`); `5V` = 5, `10uF` = 1e-5, `1d3` = 1000. **But `1d-3` and `1d+3` are errors** ("unknown parameter (-3)"). | Keep our correctly rounded reader; compare with ngspice within 1 ulp. Add `mil` (25.4e-6; ours reads `1mil` as milli) and `d` as an exponent without a sign. `1d-3` should be an error, as in ngspice. |
| C8 | `1.5rad` reads 1.5 (so 1.5°); `45deg` reads 45 | TRUE | `r` is not a suffix, so it's ignored. In `45deg` the `d` is read as an exponent marker with no digits, so the value is 45 × 10⁰. AC deck: phase `90` → `vp` 1.5708 rad; `1.5rad` → 0.02618 rad (= 1.5°); `45deg` → 0.7854 rad. | Keep rejecting `rad` (already decided). `deg` is harmless. |
| C9 | Resistor spellings; a card takes instance names as defaults except `m`, `level`; `res=` on the line is an error | TRUE | `res.c:14-15` (line: `resistance`, `r`), `62-63` (card: `r`, `res`); `inpgmod.c:140-156` (card fallback, `m`/`level` swallowed). Decks: line `resistance=1k`, `r=1k` = 1 kΩ; line `res=1k` → "unknown parameter (res)"; cards `r=2k`, `res=2k`, `resistance=2k` = 2 kΩ; card `r=2k m=2` = 2 kΩ (m ignored). | As designed. Also mind the precedence rule in the notes: with the value first, a card's `resistance=` beats the line's value. |
| C10 | Capacitor spellings; card `c=` is the type flag and gives 0 F | TRUE | `cap.c:13-15` (line: `capacitance`, `cap`, `c`), `40` (card: `cap`), `58` (`IP("c", CAP_MOD_C, IF_FLAG…)`). Deck `.model CM C c=3n` + `C1 b 0 CM`: `vm(b)` = 1.000, i.e. 0 F, **no warning**; `cap=3n` and `capacitance=3n` on the card give 3 nF. | Keep rejecting card `c=`. |
| C11 | Inductor: line `inductance=` only (`l=`?); card `ind=`, `inductance=` | PARTLY | `ind.c:13` has only `inductance`; `ind.c:51` makes `l` the card's type flag. `inp2l.c:81` special-cases a first token `l`. Decks: `L1 a b l=1u` works; **`L1 a b LM l=1u` → "unknown parameter (l)"**; `ind=1u` on the line → error; card `ind=3u`, `inductance=3u` work; **card `l=3u` → 0 H, no warning**. | Accept line `l=` only with no model name, or reject it everywhere (simpler). Reject card `l=`, as for `c=`. |
| C12 | `C1 a 0 1n CM` and `L1 a 0 1m LM` rejected; model-first accepted; R both; manual shows value-first | TRUE | Root cause confirmed; see the C12 section. Manual (ngspice-42, §3.3.6, §3.3.10): `CXXXXXXX n+ n- <value> <mname> …`, `LYYYYYYY n+ n- <value> <mname> …`. The manual's own example `CEB 1 2 1u cap1 dtemp=5` fails the same way (checked). | Our reader should accept both orders, as it does. The writer keeps model-first. |
| C13 | `off` anywhere after the model; extra bare numbers rejected | PARTLY | `off` after the model: `D1 a 0 DM off`, `… off area=2`, `… area=2 off`, `Q1 … QM off` all work. `R2 n1 n0 2k 1 5 280` → "unknown parameter (5)". **But `R1 a b 2k 1` is accepted as 1 Ω** (the second bare number becomes the value again, `inp2r.c:225-229`). **And `D1 a 0 DM 2` → "could not find a valid modelname"** (the C12 pass again). See note. | Reject every bare number after the value/model, except the one right after the model on D and Q lines (area). Reject a flag written before the model (see the extras: `D1 a 0 off DM`). |
| C14 | `.temp 25 60` warns, runs at 27 °C; the last `.temp` wins; `.temp=125` works | TRUE | `inp.c:1120-1131` (the last one replaces earlier ones, `=` skipped), `1135-1147` (`strtod`, warning, 27). Decks: "Warning: Could not set temperature to 25 60", TEMP = 27; `.temp 100` then `.temp 60` → 60; `.temp=125` → 125. | As is. Also: `.temp` uses C's `strtod`, so `.temp 1k` and `.temp 100C` warn and run at **27 °C**. `.temp` beats `.options temp=` in either order (checked). |
| C15 | `.options` apply wherever they are, the last one wins; an unknown model parameter only warns | TRUE | `options.c:218-238` pulls every `.opt…` line out of the deck before anything else; `inp.c:1324-1365` applies them. Decks: `temp=50` then `temp=70` → 70; `.options` between device lines works. Card `foo=3` → "Warning: Model issue … unrecognized parameter (foo) - ignored", run continues (`inpgmod.c:170-173`, cur `189`). | As is. Also, an unknown `.options` name is accepted without any message. |
| C16 | Defaults | PARTLY | 27 °C and TNOM 27 °C: `cktntask.c:126-127` (300.15 K). BJT (`bjtsetup.c`): IS 1e-16 (`48`), BF 100 (`57`), NF 1 (`60`), BR 1 (`80`), NR 1 (`83`), XTB 0 (`143`), EG 1.11 (`146`), XTI 3 (`149`). Diode (`diosetup.c`): N 1 (`40`), IS 1e-14 (`43`), EG 1.11 (`115`), XTI 3 (`118`), RS 0 (`198-205`). All confirmed by `showmod` on empty cards. Resistor card: default width **and default length 10 µm** (`ressetup.c:31-32`). **1 mΩ claim: wrong.** See note. | Use these defaults. A resistor line with no value and no model should be an error. |
| C17 | Phases stored in degrees, turned into radians where used | TRUE | `vsrc/vsrctemp.c:68` (AC), `vsrc/vsrcload.c:177` (SIN; cur `190`), same in `isrc/isrctemp.c:64`, `isrc/isrcload.c:154`. Deck `SIN(0 1 1k 0 0 30)` gives 0.5 at t = 0. | As designed: degrees on the line, radians inside. |
| C18 | Which named value spellings ngspice-42 accepts, and whether the card is applied | 8 of 9 | Table in the C18 section. Only `L1 a b LM l=1u` fails. With a model, the card's `tc1` changed the response exactly as an inline `tc1` did. | The writer can write every value as `name=value` after the model name. |
| C19 | BJT and diode named instance parameters | TRUE | `bjt.c:17-25, 73-74`; `dio.c:12-28`. Every listed name was accepted and had its effect: `area=2` doubles the current, `m=3` triples it, `temp=127` and `dtemp=100` give the same current. `ic=0.7,5` and `ic=0.7, 5` both parse. `pj`, `lm`, `wm`, `lp`, `wp` are accepted (they only matter at diode level 3). | As designed. |

## Notes on the PARTLY verdicts

**C3, `gnd`.** By default ngspice rewrites the word `gnd` as `0` in the text of every line that isn't a `*` comment. It does this before it reads any device (`inpcom.c:2002-2023`), matching `gnd` between spaces, commas and brackets. Because lines are already lowercased, `GND` and `Gnd` count too. Deck: `V1 a GND 1`, `R2 b Gnd 1k`, and `R3 p gnd 1k` inside a subcircuit all went to ground. It is not unconditional:
- `set no_auto_gnd` in `spinit` or `.spiceinit` turns it off. Checked: `gnd` then became an ordinary node, and inside the subcircuit it became `xs.gnd`, because the `.global gnd` line is also left out.
- In later versions, ngspice-46 leaves `gnd` alone in PSpice compatibility mode inside a subcircuit that lists `gnd` as a port. ngspice-47 also maps KiCad's `/gnd` (NEWS; `inpcom.c` cur `2382-2445`).
- Because it rewrites text, it hits every token spelled `gnd`. Deck `D1 a 0 gnd` + `.model gnd D` → "could not find a valid modelname".

**C7, numbers.** Two things. (1) The claim's examples happen to round correctly: 47 × 10⁻¹⁰ lands on the right double. Everyday values like 0.7 and 0.47 don't. I copied the arithmetic of `INPevaluate` into Python and checked it against ngspice on seven values. On 200 000 random literals of up to 15 digits it differs from correct rounding in 21 % of cases, always by at most 1 ulp (one step between neighbouring doubles). So a tolerance of 1 ulp is enough for values that go straight into a device. (2) `d` works as an exponent only without a sign. The number tokenizer allows a sign inside a token only after `e` (`inpgtok.c:293-311`). So `1d-3` is split into `1d` and `-3`, and ngspice reports "unknown parameter (-3)". The same holds for `1d+3`.

**C11, `l=`.** `inp2l.c:81` treats a first token `l` like the value (the same trick as `r=` and `c=` in `inp2r.c:162` and `inp2c.c:83`). This happens only when that token stands where the model name would be. `r` and `c` also exist as instance-parameter names, so `R1 a b RM r=1k` and `C1 a b CM c=1n` work. There is no instance parameter called `l`, so `L1 a b LM l=1u` fails.

**C13, positional numbers.** `INPdevParse` (`inpdpar.c:56-61`) accepts one bare number at the start of what follows the model. For R, C and L that number becomes the value again, after any earlier value. So `R1 a b 2k 1` gives 1 Ω without a message (checked: `vm(b)` = 0.999). A third number is rejected. For BJTs the bare number is the area and must come right after the model: `Q1 c b 0 QM 2` works, `Q1 c b 0 QM off 2` → "unknown parameter (2)". For diodes, `D1 a 0 DM 2` fails, and so does `D1 a 0 DM area 2`. The reason is the C12 pass: `get_number_terminals` for `d` (`inpcom.c:4855-4867`) counts tokens up to the first `off`, `thermal` or `=`. So it takes `DM` for a third node and `2` for the model name, and deletes the `DM` card. With a second diode `D2 a 0 DM off` keeping the card alive, `D1 a 0 DM 2` works and doubles the current.

**C16, the 1 mΩ resistor.** `R1 a b` (no value, no model) is dropped with "Warning: 'r1 a b' is not a valid resistor instance line, ignored!" (`inp2r.c:63-73`). The circuit then runs without that resistor (checked). The 1 mΩ fallback in `restemp.c:61-74` is used in two cases. (1) A line names a model whose card gives neither `r` nor a sheet resistance: `.model RM R` + `R1 a b RM` → "resistance too low or not given, set to 1 mOhm". A sheet resistance (`rsh`) is the resistance of one square of a resistive film; ngspice multiplies it by length/width. (2) The value is 0: `resparam.c:40` turns 0 into 1 mΩ. Two more details: with `rsh` on the card, the geometry beats the card's `r` (card `rsh=100 r=2k` gave 100 Ω), and a card `r` ≤ 0 is ignored (`resmpar.c:82`). Diode RS is 0 except in PSpice or LTspice compatibility mode, where it becomes 100 µΩ (`diosetup.c:198-205`).

**C9, precedence between line and card.** `INPdevParse` applies a card's instance-name defaults *before* the line's own parameters (`inpdpar.c:63-100`). But `inp2r.c:218-221` has already stored a value written *before* the model. So with `.model RM R resistance=2k`:
- `R1 a b 1k RM` → **2 kΩ**: the card overwrites the line's value.
- `R1 a b RM 1k` → 1 kΩ.
- `R1 a b RM r=1k` → 1 kΩ.

All three were checked. With the card's own name `r=2k`, the line's value wins in both orders. The ideal reader: a value on the line always beats a card default.

## C12: why `C1 a 0 1n CM` fails

Before any device is built, `inp_readall` runs `comment_out_unused_subckt_models` and `inp_rem_unused_models` (`inpcom.c:1068-1072`; skipped when the deck has an `.if`). The second one:
1. lists every `.model` card;
2. for each device line, guesses the model name with `get_model_name(line, num_terminals)` (`inpcom.c:2584-2605`). The guess skips the name and the nodes, and **only for lines starting with `r`** also skips a leading value (`2597-2601`: "special dealing for r models");
3. passes that token to `is_a_modelname` (`2642-…`). `1n` is a number with a scale factor, so the answer is "not a model name", and the card is never marked as used;
4. comments out every unmarked card by overwriting its first character with `*` (`rem_unused_xxx`, `8826-8832`).

Then `INP2C` reads the line (`inp2c.c:77-83`). It takes `1n` as the value, looks up `cm` with `INPlookMod`, and finds nothing, because the card is now a comment. So it hands `cm` to `INPdevParse`, which reports "unknown parameter (cm)". `INP2L` is identical.

Two decks confirm this is the cause and not the line parser:
- `C1 b 0 1n CM` plus a second line `C2 b 0 CM 1n` keeps the card marked; then `C1` is accepted with model `cm`.
- `C1 b 0 1n CM` inside `.if (p == 1) … .endif` skips the pass; then `C1` is accepted too.

**Later versions.** `get_model_name` still special-cases only `r` in the tags `ngspice-43`, `44`, `45`, `46` and `47`, and in the current `pre-master-48` tree (`inpcom.c` cur `3001-3017`). `inp2c.c` and `inp2l.c` are unchanged, and NEWS mentions no fix. So value-first C and L lines with a model fail in every release up to 47.

The same guessing is behind two other failures. `D1 a 0 DM 2` fails because the diode's terminal count is miscounted (C13). `Q1 c b 0 QM area 2` fails because the terminal count relies on `=`: without it, `area` is taken for the model name. Both work once another line keeps the card alive.

## C18: named values, checked at 127 °C

Each deck ran at `.temp 127`, with an inline `tc1=0.003` as the reference and a card `.model XM R|C|L tc1=0.003` where a model is named. `tc1` is the first-order temperature coefficient: the value is multiplied by 1 + tc1·(T − TNOM), here 1 + 0.003·100 = 1.3. Circuits:
- R: `R1 a b …` + `R2 b 0 1k`. `vm(b)` = 0.500000 for 1 kΩ, 0.434783 for 1.3 kΩ.
- C: `R0 a b 1k` + `C1 b 0 …` at 159.155 kHz. 0.707107 for 1 nF, 0.609711 for 1.3 nF.
- L: `L1 a b …` + `R2 b 0 1k` at 159.155 MHz. 0.707107 for 1 µH, 0.609711 for 1.3 µH.

| Line | Accepted | `vm(b)` | Card applied |
|---|---|---|---|
| `R1 a b r=1k` | yes | 0.500000 | no card |
| `R1 a b RM r=1k` | yes | 0.434783 | yes, same as inline `tc1` |
| `R1 a b resistance=1k` | yes | 0.500000 | no card |
| `R1 a b RM resistance=1k` (extra) | yes | 0.434783 | yes |
| `C1 a b c=1n` | yes | 0.707107 | no card |
| `C1 a b CM c=1n` | yes | 0.609711 | yes |
| `C1 a b capacitance=1n` | yes | 0.707107 | no card |
| `C1 a b CM capacitance=1n` (extra) | yes | 0.609711 | yes |
| `C1 a b CM cap=1n` (extra) | yes | 0.609711 | yes |
| `L1 a b inductance=1u` | yes | 0.707107 | no card |
| `L1 a b LM inductance=1u` | yes | 0.609711 | yes |
| `L1 a b l=1u` | yes | 0.707107 | no card |
| `L1 a b LM l=1u` | **no**: "unknown parameter (l)" | — | — |

A named value after the model name also avoids the C12 bug: the model name then sits where the pruning pass looks for it. The writer should always write the `=`: ngspice's line parser treats `=` as a separator, but the pruning pass counts on it (C12).

## Other things our reader should know

- **Title line.** The first line is always the title, even if it looks like a device. Deck starting with `R9 b 0 1k`: that resistor was not in the circuit.
- **Comments** (`inpcom.c:3156-3285`). `*` in column 1 comments out the whole line. End-of-line comments start with `;`, with `//`, or with `$` preceded by a space, tab or comma. The `$` form doesn't apply inside `.control` (there `$` needs a space after it) or in PSpice mode. A line starting with `$` works but warns. `R3 b 0 1k$c4` is 1 kΩ: the number reader just stops at `$`.
- **Continuation.** A line starting with `+` continues the previous line. So does `\\` (two backslashes) at the end of a line (`inpcom.c:2448-2462`). Both checked.
- **`.end` doesn't stop reading.** `.end` is marked (`inpcom.c:1627-1632`), but reading goes on, so `.control` blocks can follow it. **Devices after `.end` are still added**: deck with `R3` after `.end` → R3 was in the circuit. SPICE3-style readers stop at `.end` (recalled, not checked), so the reader should reject anything after `.end` except a `.control` block.
- **`.include` path.** Resolution order (`inpcom.c`, `inp_pathresolve_at`, `inp_pathresolve`): an absolute path as is; `~/` expanded; then **the current working directory first**, then the directory of the including file, then each entry of the `sourcepath` variable. Checked: with `part.inc` both in the working directory and next to the main file, ngspice took the working-directory one. The file name keeps its case (these lines aren't lowercased). Our reader has to pick one order. With ngspice's order, a stray file of the same name in the working directory wins.
- **`.lib file section`** includes the lines between `.lib section` and `.endl` in that file. The section name ignores case (checked: `.lib models.lib ff` found `.lib FF`).
- **Duplicate `.model` names are not an error.** The first card wins, silently (`inpmkmod.c:35-36`; checked, including `DM` vs `dm`). The ideal reader rejects them.
- **Unit-looking suffixes that bite.** `2A` is 2 × 10⁻¹⁸ (atto), `1MHz` is 1 × 10⁻³ (milli), `1F` is 1 fF. `4k7` is 4 kΩ, not 4.7 kΩ: the "4k7" style (RKM code) is read only in LTspice compatibility mode (`inpeval.c:208-448`). All checked. The reader could warn on `A`, `F` and `MHz`-style suffixes and on RKM codes.
- **A flag before the model name becomes a node.** `D1 a 0 off DM` is accepted: `inp2d.c:47-61` takes `off` as the diode's optional third node (the thermal node, used for self-heating), and a node called `off` appears. The ideal reader rejects it.
- **Resistor `tc=a b`** is rewritten to `tc1=a tc2=b` (`inp2r.c:95-150`; checked).
- **`@c1[capacitance]`** reports the value after the temperature correction (1.005 µF for `dtemp=5`, `tc1=0.001`). This is useful for checks against ngspice.
- **SFFM parameter order changed** in ngspice-46 ("FM and FC exchange place", NEWS). This matters only if `Waveform` ever gets SFFM.
- **Default geometry.** Resistor cards default to width = length = 10 µm (`ressetup.c:31-32`), so `rsh` alone gives 1 × `rsh`. Capacitor cards default to width 10 µm and length 0 (`capsetup.c:38,41`). On R and C cards, `w` and `l` are the card's own default-geometry parameters (`res.c:57-58`, `cap.c:46-48`), not instance defaults. On an L card, `l` is the type flag (C11).
