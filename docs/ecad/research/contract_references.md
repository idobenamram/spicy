# Contract References: Setups, Specs, Conditions and Hierarchy in Mature Tools

> Research report · 2026-09-29
> Question: how do mature tools and formalisms write **test setups** ("benches"), **specs**, **conditions**, and how do these relate to **design hierarchy**? What should our contract syntax (language.md §8) learn from them?
> Reads with: `language.md` §3 and §8, `specs.md`, `engine.md` §2 and §7, `research/next_synthesis.md` (L1, L4), `research/language_specs.md` §7 and §10.
> Scratch and downloaded sources: `/root/.claude/jobs/443154a8/tmp/engine4/refs/`.

**How sure is each claim?** Each section ends with a *Verified* line.
- **[verified]**: I read the primary source (spec, manual, source code or datasheet) during this study.
- **[secondary]**: from a vendor page, a training blurb or a third-party article.
- **[from memory]**: from general knowledge of the tool. The primary manual is behind a login or a bot wall. Treat the exact syntax as approximate.

---

## Summary

Every mature tool splits a check into the same parts. There is the **thing under test** (the DUT, the "subject"). There is the **setup** that surrounds it: sources, loads and an analysis. There are the **conditions**: values held fixed or swept. And there is the **check**: a measured number and its limit. Cadence ADE calls these a test, its variables and corners, and outputs with specs. A datasheet puts them in columns: parameter, test conditions, min/typ/max, and "see Figure 6-1". UVM calls them the DUT, the test/env, and the scoreboard. SysML v2 has the `subject`, the `verification def`, and the `require constraint`. In almost every tool the setup is a **named object that sits outside the design** and places the design inside it. Specs point at a setup by name. Conditions are written once as defaults and overridden per row (the datasheet's "unless otherwise noted"). For hierarchy, the tools that work well agree on one rule: **checkers travel up the hierarchy, stimulus does not.** A sub-block's checks are re-evaluated on the real signals of the parent. Its assumptions become obligations on its neighbours. Its own test benches stay behind as unit tests. Reusing a sub-block's verified behavior *without* simulating the whole design is sound only when the context stays inside the conditions under which that behavior was established. That includes the **load and source impedance**. IBIS and datasheets make this concrete: every published number carries its test fixture (`R_fixture = 50 Ω`, `CL = 100 pF, RL = 2 kΩ`). Assume-guarantee theory gives the exact rule: a context may use a block's guarantees only if the context lies inside the block's assumptions. When it doesn't, you fall back to simulating in context.

---

## 1. The shared vocabulary

The same five ideas appear everywhere under different names.

| Idea | Our word | ADE (Cadence) | Datasheet | UVM / SV | Modelica | SysML v2 | A/G theory |
|---|---|---|---|---|---|---|---|
| What is checked | block | DUT instance in the testbench cell | the device | DUT | the component model | `subject` | component M |
| How it is driven and loaded | bench | **test** (testbench cell + analyses) | test circuit ("Parameter Measurement Information") | test + env (drivers) | test model (`extends … Example`) | `verification def` | environment E |
| Values held or swept | assumption, knob, condition | design variables, global variables, **corners**, sweeps | "Test conditions", header defaults, "full range" | config, constrained-random | parameters, `experiment` | `assume constraint`, attributes | assumptions A |
| The number measured | measure | **output** (expression) | parameter (row) | monitor / scoreboard | observer variable | attribute / calc | — |
| The limit | spec | **spec** column on an output | min / typ / max | `assert`, scoreboard compare | `Requirement` block | `require constraint` | guarantees G |

---

## 2. The baseline: a plain SPICE deck

Before the tools, it helps to see what they improve on. A SPICE deck is **one setup** in one file. It holds the DUT (usually a subcircuit), the sources, the load, the analyses, and the measurements, all mixed together.

```spice
* ce_amp_tb.cir  (LTspice dialect; ngspice has no .step and loops in .control instead)
.include ce_amp.sub
.param vcc=12
Vcc vcc 0 {vcc}
Vin in  0 dc 0 ac 1
X1  vcc 0 in out ce_amp          ; the DUT
RL  out 0 10k                    ; the load
.ac dec 20 1 1meg
.step param vcc list 11.4 12 12.6
.temp -10 25 60
.meas ac gain find mag(v(out)) at=1k
.end
```

| Question | SPICE deck |
|---|---|
| Setup written how? | the whole deck *is* the setup |
| First-class, named? | one per file; the file name is the only name. HSPICE `.alter` adds variants inside one deck |
| Spec → setup link | none. `.meas` measures but has no limit. Pass/fail is done by an outside script |
| Stimulus and load | ordinary elements (`Vin`, `RL`) mixed into the netlist |
| Corners | `.step`, `.temp`, `.lib file section` (process corners). Their product is the full grid |
| Hierarchy | `.subckt`. There is no notion of a sub-block's specs |

**Verified:** [from memory] standard SPICE/LTspice syntax.

---

## 3. Cadence Virtuoso ADE Explorer / Assembler ("Maestro") and ADE Verifier

This is the industry reference for analog IC verification.

### 3.1 How it is organized

- **The design** is a schematic cell, say `ce_amp/schematic`. It holds no sources.
- **The testbench** is a *separate* schematic cell, say `ce_amp_tb/schematic`. It places the DUT symbol and wires sources, loads and probes around it. Engineers often keep several: `ce_amp_tb_ac`, `ce_amp_tb_tran`, `ce_amp_tb_psrr`.
- **A test** is the unit of simulation. It names one testbench cell, a simulator, the analyses (DC, AC, tran, noise, PSS…), the model files and process section, **design variables**, and its **outputs**.
- **The maestro cellview** (ADE Assembler) holds *many tests* for one design, all run together.
- **Outputs** are expressions on the results, such as `value(dB20(VF("/out")) 1k)` or `bandwidth(VF("/out") 3 "low")`. Each output belongs to one test.
- **Specs** are a column on each output: `<`, `>`, `range`, `tol`, or `info` (report only). A spec can be overridden per corner or per sweep point.
- **Global variables** are set once in the maestro view. They override the design variables of the same name in every test. That is how one `vdd` drives all tests. A local test variable can override a global one again.
- **Corners** are named sets: a process section (`tt`, `ss`, `ff`), a temperature, and variable values. The corner table is a matrix of corners × tests, and **each corner is enabled or disabled per test**. The "Nominal" corner is always there.
- **Sweeps vs corners.** A variable given several values is *swept*: every value is combined with everything else. A corner is a *hand-picked point* in that space. Sweeps and corners run in one job, and their product is the run count.
- **Run modes** reuse the same setup: single run (sweeps and corners), Monte Carlo sampling, worst-case corners, high-yield estimation, sensitivity, optimization.
- **Hierarchy of model levels** is chosen by a **config view** (the Hierarchy Editor). It picks per instance whether to use the `schematic`, a `veriloga` behavioral view, or an extracted view. So a top-level test can run a sub-block as its verified behavioral model.
- **ADE Verifier** sits above many maestro views. It holds a tree of **requirements**. Each requirement is mapped to one or more (test, output) pairs in the maestro views of possibly different blocks and owners. It reports coverage ("requirements with no measurement") and a **spec check**. The spec check flags a requirement whose limit differs from the spec in the mapped test, because the limit is stored in two places.

### 3.2 What the text form looks like

Maestro is a GUI database. Its scripted form is OCEAN (SKILL). A single-test OCEAN script:

```lisp
simulator('spectre)
design("myLib" "ce_amp_tb" "schematic")        ; the testbench cell, not the DUT
modelFile('("models.scs" "tt"))
analysis('ac ?start 1 ?stop 1e6 ?dec 20)
desVar("vcc" 12)
temp(27)
run()
gain = value(mag(VF("/out")) 1000)
```

The maestro SKILL API has calls like `maeSetVar`, `maeAddOutput`, `maeSetSpec` and `maeRunSimulation`, operating on a named test. I could not check their exact signatures.

### 3.3 The six questions

| Question | ADE |
|---|---|
| Setup written how? | a testbench **cell** (schematic) + a **test** (analyses, variables, outputs) |
| First-class, named? | **yes**, both. One design has many named tests |
| Spec → setup link | structural: a spec lives *on an output*, and an output lives *in a test* |
| Stimulus and load | real instances in the testbench schematic (`vsource`, `res`, `cap`, `analogLib` parts), set by design variables |
| Corners and conditions | global variables → test variables → corner overrides → sweeps. Corners enabled per test. The run set is the product |
| Hierarchy | each block has its own maestro view. Sub-block specs are **not** re-checked in the parent's tests automatically. Reuse happens through (a) a config view that swaps a sub-block for its behavioral model, and (b) ADE Verifier, which gathers requirements across blocks |

**Lessons from ADE.**
- Tests as named, separate objects scale well: PSRR, load step and loop gain each have their own fixture.
- Global variables are the "default conditions" idea: written once, overridden per test.
- The weak spot is duplication. The limit lives in the maestro output *and* in the Verifier requirement. Cadence had to add a "spec check" to catch drift.
- Another weak spot: the testbench is written by hand and nothing checks that it stays inside the block's operating range.

**Verified:** [secondary] Cadence product pages and datasheets ([ADE Assembler](https://www.cadence.com/en_US/home/tools/custom-ic-analog-rf-design/circuit-design/virtuoso-ade-assembler.html), [ADE Verifier datasheet](https://www.cadence.com/en_US/home/resources/datasheets/virtuoso-ade-verifier-ds.html), [training S2 "multi test corner analysis"](https://www.cadence.com/en_US/home/training/all-courses/86254.html), [support note: "override global variables with local design variable values"](https://support1.cadence.com/public/docs/content/20453803.html)). [from memory] the spec-column kinds, per-test corner enable, config view, OCEAN syntax, `mae*` function names. The Cadence user guides and community forum were behind a login or a bot wall.

---

## 4. Keysight ADS

ADS puts everything on one schematic, but the pieces are separate components.

- **Simulation controllers** are schematic components, one per analysis: `SP1` (S-parameters), `AC1`, `Tran1`, `HB1`. Each is a named setup.
- **Variables** (`VAR` blocks) hold design variables. Sweeps are `ParamSweep` controllers.
- **Specs** are `Goal` components (for optimization) and `YieldSpec` components (for yield). Each has:
  - `Expr`: the measured expression, such as `dB(S(2,1))`;
  - **`SimInstanceName`**: *the name of the simulation controller that produces the data*. This is how a spec names its setup;
  - `LimitMin`, `LimitMax`, and a `LimitType`;
  - up to six **independent variables** with ranges (`IndepVar`, `Indep1Min`, `Indep1Max`), for example "only for freq in 1–2 GHz".

Netlist form (approximate):

```
Goal:OptimGoal1  Expr="dB(S(2,1))"  SimInstanceName="SP1"  Weight=1 \
    LimitMin[1]=10  IndepVar[1]="freq"  Indep1Min[1]=1 GHz  Indep1Max[1]=2 GHz
```

| Question | ADS |
|---|---|
| Setup | a simulation controller (named) plus the sources placed on the schematic |
| Spec → setup | **explicit by name** (`SimInstanceName`) |
| Measure axis | explicit: the goal holds only over an independent-variable range (a *band*) |
| Corners | statistical variables and `YieldSpec` for Monte Carlo; sweeps for corners |
| Hierarchy | subnetworks. Specs are flat at the top schematic |

**Lesson:** the spec names its setup, and it names its **measure axis range** ("for freq in 1–2 GHz") separately from the conditions. That is our `.band(…)` idea.

**Verified:** [verified] *ADS 2011.01 Tuning, Optimization, and Statistical Design*, Goal parameter table: `Expr`, `SimInstanceName` ("the instance name for the simulation control component … which will generate the data used by the Expr field"), `LimitMin`/`LimitMax`, `IndepVar`, `Indep{i}Min/Max` ([PDF](https://edadownload.software.keysight.com/eedl/ads/2011_01/pdf/optstat.pdf), p. 26). [from memory] the netlist line.

---

## 5. Synopsys PrimeWave Design Environment (and Custom Compiler SAE)

Public information is thin. The datasheet says:
- it has multi-testbench mode, corners, Monte Carlo, and sweeps over "thousands of corners";
- it supports HSPICE `.measure` statements for post-processing;
- **dependencies between testbenches**: "measurements from one testbench to be used in another for a specification-driven flow".

The last point matters for us. One setup's result feeds another setup (e.g. a DC bias found in one test becomes a condition of the next). Our "measure over several runs" (E7, calibration) is the same need.

**Verified:** [secondary] [PrimeWave datasheet](https://www.synopsys.com/content/dam/synopsys/implementation&signoff/datasheets/primewave-ds.pdf). No public syntax.

---

## 6. Datasheet electrical-characteristics tables

A datasheet is the most widely read spec format in electronics. It is worth reading as a data model.

### 6.1 A real table: TI TLV755P (a 500 mA LDO)

The table header sets **default test conditions** for every row:

> *"at operating temperature range (TJ = –40°C to 125°C), VIN = VOUT(NOM) + 0.5V or 2.0V (whichever is greater), IOUT = 1mA, VEN = VIN, and CIN = COUT = 1μF (unless otherwise noted); all typical values at TJ = 25°C"*

Rows then override some of those conditions:

| Parameter | Test conditions (row overrides) | Min | Typ | Max | Unit |
|---|---|---|---|---|---|
| Output accuracy | −40°C ≤ TJ ≤ +85°C, DBV and DRV package | −1 | | 1 | % |
| Output accuracy | VOUT ≥ 1 V (full TJ range from header) | −1.5 | | 1.5 | % |
| Line regulation | VOUT + 0.5V ≤ VIN ≤ 5.5V, VOUT > 1.5V | | 2 | | mV |
| Ground current | TJ = 25°C, IOUT = 0mA | 14 | 25 | 31 | µA |
| Ground current | −40°C ≤ TJ ≤ +125°C, IOUT = 0mA | | | 40 | µA |
| PSRR | f = 1 kHz, VIN = VOUT + 1V, IOUT = 50mA | | 52 | | dB |
| PSRR | f = 100 kHz, VIN = VOUT + 1V, IOUT = 50mA | | 46 | | dB |

### 6.2 A real table with test circuits: TI TL072 (op amp)

Header: *"at VS = ±15 V and TA = 25°C (unless otherwise noted)"*. Some rows:

| Parameter | Test conditions | Min | Typ | Max |
|---|---|---|---|---|
| VOS, TL07xC | VO = 0 V, RS = 50 Ω | | 3 | 10 mV |
| VOS, TL07xC | same, **TA = Full range** | | | 13 mV |
| Slew rate | VI = 10 V, **CL = 100 pF, RL = 2 kΩ** | 8 | 20 | V/µs |
| Phase margin | G = +1, RL = 10 kΩ, CL = 20 pF | | 56° | |

Footnotes finish the conditions:
- (1) *"All characteristics are measured under open-loop conditions with zero common-mode voltage, unless otherwise specified."*
- (2) *"Full range is TA = 0°C to 70°C for the TL07xC …; TA = –40°C to +85°C for the TL07xI; and TA = –55°C to +125°C for the TL07xM."*

Section 6, **"Parameter Measurement Information"**, draws the test circuits: *Figure 6-1, Unity-Gain Amplifier* (with CL = 100 pF, RL = 2 kΩ) and *Figure 6-2, Gain-of-10 Inverting Amplifier*. Rows elsewhere say "See Figure 2".

### 6.3 The datasheet as a spec model

| Datasheet section | What it is | Our concept |
|---|---|---|
| Absolute Maximum Ratings | limits that must never be exceeded | automatic checks (§8.7) |
| **Recommended Operating Conditions** | the range the part is meant to work in (VIN 1.45–5.5 V, TJ −40…125 °C, COUT 1–200 µF) | the block's **assumptions** |
| **Electrical Characteristics** | one row per guaranteed number | **specs** |
| Table header | default test conditions | the **default bench** |
| Row "Test conditions" | per-row overrides | a **form bench** (`..default`) or `where` |
| "see Figure 6-1" | a drawn test circuit | a **sheet bench** |
| Footnote "Full range is …" | a named condition, differing by grade (C/I/M) | a named range, depends on a variant |
| Typical Characteristics (graphs) | measured curves, not guaranteed | characterized behavior |
| "Guaranteed by design / by characterization" | provenance of a limit | knob provenance (engine.md §2.2) |

**What the table teaches.**

1. **Conditions have three different roles.** They look alike in the column, but they are not the same thing:
   - **fixed point:** `IOUT = 1mA` holds the value;
   - **quantified range:** `TJ = −40…125 °C` means *the limit holds at every temperature in the range* (our range knob);
   - **measure axis:** `VOUT + 0.5V ≤ VIN ≤ 5.5V` for line regulation is *part of the definition of the measure*: ΔVOUT between the two VIN end points. It is not "for all VIN". The same goes for "0.1 mA ≤ IOUT ≤ 500 mA" in load regulation, and "f = 10 Hz to 10 kHz" for integrated noise.
2. **Min/max and typ use different conditions in the same row.** Min and max hold over the stated range. Typ is one value at TJ = 25 °C. Typ is not a statistical claim and not a limit. (TL072's typ offset is 3 mV; its max is 10 mV at 25 °C and 13 mV over the full range.)
3. **One parameter often has several rows**, one per condition set (25 °C vs full range; package; VOUT band). A spec is really the pair (condition set, limit).
4. **The load is always part of the conditions** (CL, RL, COUT, IOUT). Without it the number means nothing. A slew rate at RL = 2 kΩ, CL = 100 pF is a different claim from one at 600 Ω.
5. **Defaults plus overrides** keep 60 rows readable. The header states the common case once.
6. **Test circuits are named and referenced**, not repeated in every row.

**Verified:** [verified] TLV755P datasheet SBVS320D, §5.3 and §5.5 ([PDF](https://www.ti.com/lit/ds/symlink/tlv755p.pdf)); TL072 datasheet SLOS080W, §5.8, §5.9, §6 ([PDF](https://www.ti.com/lit/ds/symlink/tl072.pdf)). Text extracted with `pdftotext` and quoted.

---

## 7. IBIS: publishing an interface's behavior

IBIS (I/O Buffer Information Specification) describes a digital pin's driver and receiver **as tables**, with no transistor netlist. It is the industry's way to publish "how this pin behaves" so a board designer can simulate without the vendor's internals. That is exactly "reuse a verified block without simulating its insides".

```
[Model]        DQ_out
Model_type     I/O
C_comp         1.2pF   1.0pF   1.4pF
[Voltage Range]   3.3V   3.0V   3.6V          | typ  min  max
[Temperature Range] 50  100    0
[Pulldown]
| voltage   I(typ)    I(min)    I(max)
  -3.3V    -40mA     -32mA     -48mA
   …
[Ramp]
| variable   typ          min          max
dV/dt_r      2.20/1.06n   1.92/1.28n   2.49/650p
dV/dt_f      2.46/1.21n   2.21/1.54n   2.70/770p
R_load = 300ohms
[Rising Waveform]
R_fixture = 50
V_fixture = 0.0
| time    V(typ)   V(min)   V(max)
  0.0ns   0.0V     0.0V     0.0V
  …
```

(The `[Ramp]` block is verbatim from the spec; the rest is a shortened sketch.)

| Question | IBIS |
|---|---|
| Setup | **every table carries its own fixture**: `R_fixture`, `V_fixture`, `C_fixture`, `L_fixture`; `[Ramp]` has `R_load` (default 50 Ω). The spec says: *"The 'fixture' subparameters specify the loading conditions under which the waveform is taken."* |
| Conditions | `[Voltage Range]`, `[Temperature Range]`, process. Always three columns: typ, min, max |
| Corners | **coherent**, not independent: *"the 'min' column describes slow, weak performance, and the 'max' column describes the fast, strong performance."* Low voltage, the slow temperature and the slow process go together in one column. For CMOS, "min" temperature is the *highest* one |
| Hierarchy | a component has pins; each pin names a model. Board tools compose IBIS models with transmission lines and receivers |

**Lessons.**
- Published behavior is **data plus the conditions it was taken under**. The fixture is part of the record, never implied.
- A reused block's behavior is only as good as the match between its fixture and the real context. That is why IBIS allows several waveform tables, each with a different fixture.
- Corners can be **correlated**. Three coherent columns are not the same as 3ⁿ independent combinations. Our engine treats knobs as independent unless told otherwise (engine.md §2.5 lots); a published interface may need to say "these move together".

**Verified:** [verified] IBIS 8.0 spec: `[Ramp]` §6.1 p. 106 (`R_load`, 50 Ω default); `[Rising Waveform]` sub-params and fixture text p. 107; §9 "Notes on data derivation method" (min = slow/weak, max = fast/strong) ([PDF](https://ibis.org/ver8.0/ver8_0.pdf)).

---

## 8. SystemVerilog, UVM and SVA

Digital verification is the most mature field for *test infrastructure*. Its central idea is strict separation of the DUT from its environment.

### 8.1 Structure

- **Top module** instantiates the DUT and an *interface* (a bundle of signals). The DUT never knows it is being tested.
- **`uvm_env`** holds reusable pieces: **agents** (a *driver* that applies stimulus, a *monitor* that watches pins, a *sequencer*), and **scoreboards** (checkers that compare observed behavior with a reference).
- **`uvm_test`** picks an environment configuration and a stimulus **sequence**. The test is chosen at run time: `+UVM_TESTNAME=loud_test`.
- **Constrained-random stimulus**: sequences draw inputs from declared constraints. That is "for all inputs in the assumed set", sampled.
- **Functional coverage** (`covergroup`, `coverpoint`, `bins`) records *which conditions were actually exercised*. A pass with 0 % coverage of a corner is not a pass there.

```systemverilog
module tb_top;
  dut_if vif(clk);
  amp_dut dut(.clk(clk), .din(vif.din), .dout(vif.dout));   // the DUT
  initial begin
    uvm_config_db#(virtual dut_if)::set(null, "*", "vif", vif);
    run_test();                                              // test chosen by +UVM_TESTNAME
  end
endmodule

class loud_test extends base_test;                           // a named setup
  `uvm_component_utils(loud_test)
  task run_phase(uvm_phase phase);
    loud_seq seq = loud_seq::type_id::create("seq");
    phase.raise_objection(this);
    seq.start(env.agent.sequencer);                          // the stimulus
    phase.drop_objection(this);
  endtask
endclass
```

### 8.2 Assertions (SVA) and the `assume` keyword

SVA has three verbs, and they map onto our contract directly:

```systemverilog
assume property (@(posedge clk) req |-> !$isunknown(addr));   // what the block may rely on
assert property (@(posedge clk) req |-> ##[1:3] ack);         // what the block promises
cover  property (@(posedge clk) req && ack);                  // a condition that must be reached

bind fifo fifo_checker chk (.*);   // attach a checker to every fifo instance, without editing fifo
```

- In **formal** tools, `assume` constrains the inputs, and `assert` is proven under them.
- When the block is placed inside a bigger design, the same `assume` is typically **turned into an `assert`** and checked against the real neighbours. That is the assume-guarantee step (§13) done by tools.
- **`bind`** attaches a checker module to a design module from outside. Every instance of that module, anywhere in the hierarchy, gets the checker.

### 8.3 Hierarchy: "vertical reuse"

The standard UVM practice for block → system reuse:
- the block's **environment is nested** inside the system's environment;
- the block's **agents switch from active to passive**: the driver is removed, because the real neighbouring block now drives those pins; the **monitor stays**;
- the block's **scoreboards and assertions stay**, checking the block on the real in-context traffic.

So in context, the block's *checks* are kept and its *stimulus* is dropped.

| Question | UVM / SV |
|---|---|
| Setup | `uvm_test` + `uvm_env` configuration. Named, first-class, selected at run time |
| Spec → setup | scoreboards and assertions live with the env or the block, not the test. They run in every test that includes them |
| Stimulus and load | drivers and sequences. Digital has no "load" in the analog sense |
| Conditions | constraints on randomization. Coverage shows which were reached |
| Hierarchy | env nesting; active → passive agents; `bind`; `assume` becomes `assert` at integration |

**Verified:** [from memory] UVM 1.2 / IEEE 1800.2 class names and the vertical-reuse practice; SVA syntax per IEEE 1800. Standard, widely documented; I did not re-read the standard for this report.

---

## 9. cocotb

cocotb writes testbenches in Python against an HDL design. A **test** is a decorated async function. It gets a handle to the top-level DUT.

```python
import cocotb
from cocotb.triggers import Timer

@cocotb.test()
async def adds(dut):
    dut.a.value = 2
    dut.b.value = 3
    await Timer(1, "ns")
    assert dut.sum.value == 5

@cocotb.parametrize(width=[8, 16])     # cocotb 2.0: one test, many conditions
@cocotb.test(expect_fail=False)
async def wide(dut, width): ...
```

| Question | cocotb |
|---|---|
| Setup | a Python function. Named (the function name), first-class, discoverable |
| Spec → setup | the spec is a Python `assert` *inside* the test. Spec and setup are fused |
| Conditions | `@cocotb.parametrize` (2.0, replaced `TestFactory`) generates one test per combination |
| Known failures | `expect_fail`, `@cocotb.xfail` (2.1): like our `#[expect(fail)]` |
| Hierarchy | the runner picks one `hdl_toplevel`. A test can target any module. There is no reuse of sub-block tests in context |

**Lesson:** easy to write, but because the check is fused into the stimulus code, you cannot re-run the check under another setup, and you cannot list "the specs of this block" without reading code. The Spade HDL uses cocotb this way (language_specs.md §7.1).

**Verified:** [secondary] cocotb docs and release notes: `cocotb.parametrize` replaced `TestFactory` in 2.0; `xfail`, `skipif` in 2.1 ([release notes](https://docs.cocotb.org/en/v2.1.0/release_notes.html), [upgrade guide](https://docs.cocotb.org/en/stable/upgrade-2.0.html)).

---

## 10. VHDL-AMS and Verilog-A / Verilog-AMS

### 10.1 VHDL-AMS

A testbench is an **entity with no ports**. Its architecture places the DUT and the sources and loads, like an ADE testbench cell but in text. **Configurations** choose which architecture each instance uses. That is how a behavioral model replaces a structural one.

```vhdl
entity tb_ce_amp is end entity;                       -- no ports: a bench

architecture bench of tb_ce_amp is
  terminal vin, vout, vdd : electrical;
  quantity v_out across vout to electrical_ref;
begin
  src : entity work.v_sine(ideal)  generic map (ampl => 0.1, freq => 1.0e3)
                                   port map (vin, electrical_ref);
  sup : entity work.v_dc(ideal)    generic map (level => 12.0)
                                   port map (vdd, electrical_ref);
  dut : entity work.ce_amp(struct) port map (vcc => vdd, input => vin, output => vout,
                                             gnd => electrical_ref);
  rl  : entity work.resistor(ideal) generic map (r => 10.0e3) port map (vout, electrical_ref);

  assert not v_out'above(6.5) report "output above 6.5 V" severity error;   -- a check
end architecture;

configuration fast of tb_ce_amp is                    -- swap the DUT's model level
  for bench
    for dut : ce_amp use entity work.ce_amp(behavioral); end for;
  end for;
end configuration;
```

(A sketch in IEEE 1076.1 style; the source entities are user-defined, not a standard library.)

### 10.2 Verilog-A / Verilog-AMS

Verilog-A describes **models**, not benches. There is no testbench construct in Verilog-A. The bench is a SPICE or Spectre netlist that includes the `.va` file:

```
// amp_tb.scs (Spectre)
simulator lang=spectre
ahdl_include "opamp.va"
parameters vcc=12
V1  (vdd 0)  vsource dc=vcc
Vin (in 0)   vsource type=sine ampl=0.1 freq=1k mag=1
X1  (in out vdd 0) opamp
RL  (out 0)  resistor r=10k
swp sweep param=temp values=[-10 25 60] {
    ac1 ac start=1 stop=1M dec=20
}
```

Verilog-AMS adds `@(cross(...))` events and `$bound_step`, which let a module watch its own signals, but a limit check is still a hand-written `$strobe` or `$error`.

| Question | VHDL-AMS | Verilog-A/AMS |
|---|---|---|
| Setup | portless entity. Named, first-class | outside the language: a netlist |
| Spec | `assert … report … severity` inside the bench | none built in (Spectre has netlist-level `assert` device checks) |
| Stimulus and load | instances in the bench | netlist elements |
| Corners | generics; outer scripts | Spectre `sweep`, `alter`, `altergroup` |
| Hierarchy | **configurations** select a model level per instance | the netlist chooses which `.va` or subcircuit to include |

**Verified:** [from memory] IEEE 1076.1 and Spectre syntax. Not re-read for this report.

---

## 11. Modelica

### 11.1 Test models and the `experiment` annotation

In Modelica, a **test is just another model**. It either *instantiates* the component and adds sources and loads, or *extends* a system model and adds observers. Simulation settings live in the test model as an annotation.

```modelica
model CeAmpTest "Bench for CeAmp"
  extends Modelica.Icons.Example;                 // marks it as a runnable example
  CeAmp amp;
  Modelica.Electrical.Analog.Sources.ConstantVoltage vcc(V = 12);
  Modelica.Electrical.Analog.Sources.SineVoltage     vin(V = 0.1, f = 1000);
  Modelica.Electrical.Analog.Basic.Resistor          rl(R = 10e3);
  Modelica.Electrical.Analog.Basic.Ground            gnd;
equation
  connect(vcc.p, amp.vcc);  connect(vin.p, amp.input);  connect(amp.output, rl.p);
  // … grounds
  annotation(experiment(StartTime = 0, StopTime = 0.02, Interval = 1e-6, Tolerance = 1e-6));
end CeAmpTest;
```

- `experiment(StartTime, StopTime, Interval, Tolerance)` is standard: MLS 3.6 §18.4.
- `TestCase(shouldPass = false)`, also §18.4, marks a model that is *expected* to fail to translate or simulate.
- `replaceable` / `redeclare` let one test model swap the DUT variant or a sub-model.

### 11.2 Modelica_Requirements (Otter et al., 2015)

The library adds **requirement blocks** that watch a simulation and report a three-valued result (*violated*, *untested*, *satisfied*). Its example `CheckPumpingSystem` shows the pattern: **extend the system**, add one requirements block per component, and **bind** each to its component through an observer function.

```modelica
model CheckPumpingSystem "Check model PumpingSystem"
  extends Modelica_Requirements.Examples.SimplePumpingSystem.Components.PumpingSystem;
  inner Verify.PrintViolations printViolations(printSatisfied = true);

  SimplePumpingSystem.Components.Requirements.TankRequirements tankRequirements(
      observationName = "reservoir",
      observation = Bindings.TankObservation_from_OpenTank(reservoir));
  SimplePumpingSystem.Components.Requirements.PumpRequirements pumpRequirements(
      observationName = "pumps",
      observation = Bindings.PumpObservation_from_PrescribedPump(pumps));
  annotation(experiment(StopTime = 2000, Tolerance = 1e-006));
end CheckPumpingSystem;
```

Inside `TankRequirements`: `LogicalBlocks.WithinBand band1(u_max = levelMax, u_min = levelMin)` feeding `Verify.BooleanRequirement R_InBand(text = "Tank level must be within given bounds")`.

- **Requirements are per component type** (a `PumpRequirements` block) and **re-instantiated per component instance in context**. That is sub-block checks travelling into the system model.
- The check library has blocks such as `During`, `MinDuration`, `MaxRising`, `WhenRising` that combine *when* to check with *what* to check.

### 11.3 The newer work: CRML (Common Requirement Modeling Language)

EDF, Linköping and partners found a flaw in Modelica_Requirements and designed CRML to fix it (ITEA EMBRACE, spec 2021; paper at Modelica 2023). Their diagnosis:

> *"This library proved to be unsatisfactory because the time period and the condition to be verified were mixed within the same block. Therefore, it was impossible to have a stable library because of the combinatorial explosion of possibilities of associating conditions with time periods."*

CRML splits every requirement into **four parts** (from FORM-L):
- **WHERE**, the spatial locator: which objects the requirement is about;
- **WHEN**, the time locator: the periods during which it must hold;
- **WHAT**: the condition to be fulfilled;
- **HOW WELL**: a probabilistic constraint.

```
Boolean coldW_ics_1 is
    during not (state_stopped or state_stopping)
    ensure sen.tCW <= sen.tCWMax;

Requirement R5 is 'while' P1 'check at end'
    (('probability' (R4) 'at' b1 'becomes false') > 99.99%);
```

Requirements are coupled to the behavior models by separate **bindings** (FMI/SSP), so the requirement text never names a model's internals. Values are four-valued: undefined, undecided, false, true.

| Question | Modelica | Modelica_Requirements / CRML |
|---|---|---|
| Setup | a test model (instantiate or extend). Named, first-class | the system model plus bindings |
| Spec | none native; asserts or plots | requirement blocks / `during … ensure …` |
| Conditions | parameters; `experiment`; `redeclare` | WHEN (time periods) kept separate from WHAT |
| Hierarchy | a test extends a system; `redeclare` swaps sub-models | per-component requirement blocks re-bound in every system that uses the component; CRML class extension adds requirements to refined components |

**Lessons.**
- A test is a model that places the design. It needs no new concept.
- Keep *when/under what conditions* apart from *what must hold*. Fusing them multiplies the library.
- CRML writes "during X ensure Y", not "assume X guarantee Y". That is a readable alternative to `assume` for condition-scoped checks.
- A three- or four-valued verdict (untested ≠ satisfied) is standard in this field. Our UNDECIDED and UNSPECIFIED (engine.md §3.3) fit it.

**Verified:** [verified] MLS 3.6 §18.4 `experiment` and `TestCase` ([spec](https://specification.modelica.org/maint/3.6/annotations.html)); Modelica_Requirements source, `Examples/SimplePumpingSystem.mo`, `Verify.mo`, `ChecksInFixedWindow.mo` ([repo](https://github.com/modelica-3rdparty/Modelica_Requirements)); CRML paper, Bouskela, Buffoni et al., *Proc. Modelica Conference 2023*, pp. 500ff ([PDF](https://ecp.ep.liu.se/index.php/modelica/article/download/960/868/981)), quotes and code from pp. 500–502. [secondary] vVDR / ModelicaML "mediators" for bindings (Schamai, Fritzson et al.). I could not confirm whether the Modelica Association has a standard requirements feature in progress beyond CRML's stated plan to become an open standard.

---

## 12. SysML v2

SysML v2 is the new OMG systems-modeling standard. It has explicit, separate concepts for requirements and verification.

### 12.1 Requirements have a subject, assumptions and requirements

```sysml
requirement def <'1'> VehicleMassLimitationRequirement :> MassLimitationRequirement {
    doc /* The total mass of a vehicle shall be less than or equal to the required mass. */
    subject vehicle : Vehicle;                                        // what it is about
    attribute redefines massActual = vehicle.dryMass + vehicle.fuelMass;
    assume constraint { vehicle.fuelMass > 0[kg] }                    // when it applies
}
// inherited from MassLimitationRequirement:
//     require constraint { massActual <= massReqd }                  // what must hold
```

- **`subject`** is the thing the requirement is about. It is a *parameter*: the requirement is written once, then bound to a part.
- **`assume constraint`**: if it is false, the requirement doesn't apply (it is vacuously met).
- **`require constraint`**: what must hold.
- **`satisfy R by part`** claims that a design element meets a requirement.

### 12.2 Verification cases name their subject and objective

```sysml
verification def VehicleMassTest {
    subject testVehicle : Vehicle;
    objective vehicleMassVerificationObjective {
        verify vehicleMassRequirement;        // the subject is bound to testVehicle automatically
    }
    action collectData { in part testVehicle : Vehicle = VehicleMassTest::testVehicle;
                         out massMeasured :> ISQ::mass; }
    action evaluateData { in massProcessed :> ISQ::mass = processData.massProcessed;
        out verdict : VerdictKind =
            PassIf(vehicleMassRequirement(vehicle = testVehicle, massActual = massProcessed)); }
    return verdict : VerdictKind = evaluateData.verdict;
}

verification vehicleMassTest : VehicleMassTest {
    subject testVehicle :> vehicleTestConfig;   // a usage: bind the test to a concrete configuration
}
```

- **The verification case is first-class and named.** It says *which requirement* it verifies (`verify`) and *what it is applied to* (`subject`).
- **The procedure** (collect, process, evaluate) is separate from the requirement.
- **Definition vs usage.** `verification def` is reusable. `verification x : Def { subject … }` applies it to one configuration.

### 12.3 Hierarchy: sub-requirements bind their subject to a part of the parent's subject

```sysml
requirement def DeliverGifts {
    subject redefines sleigh : SantaSleigh;
    requirement : CargoCapacity {
        subject :>> cargoBay = sleigh.cargoBay;    // the child requirement is about a sub-part
    }
}
satisfy vehicleSpecification by vehicle_design;
satisfy engineSpecification  by vehicle_design.engine_v1;   // a sub-block's spec, satisfied in context
```

The parent requirement contains the child's requirement, with the child's subject bound to a part of the parent. That is exactly a sub-block's specs being carried into the parent with a placement path.

| Question | SysML v2 |
|---|---|
| Setup | `verification def` + usage. Named, first-class, separate from the requirement |
| Spec → setup | reversed: the **setup names the spec** (`verify R`). A requirement can be verified by several cases |
| Conditions | `assume constraint` (applicability), attributes |
| Stimulus and load | not modeled electrically; actions describe the procedure |
| Hierarchy | nested requirement usages with subject binding; `satisfy … by a.b.c` |

**Lessons.**
- The "subject" idea is clean: a spec is written against a type, then bound to an instance path.
- SysML v2 uses the word `assume`, but for **applicability** ("the requirement only applies when fuel > 0"), which is our `where`. It is not an obligation placed on the neighbours. Our `assume` means both "a range knob for my specs" *and* "a check on my neighbours". SysML keeps those apart: the obligation on neighbours would be a separate requirement on the environment.

**Verified:** [verified] SysML v2 release repo, training files `32. Requirements/Requirement Definitions.sysml`, `Requirement Satisfaction.sysml`, `34. Verification/Verification Case Definition Example.sysml` and `…Usage Example.sysml` ([repo](https://github.com/Systems-Modeling/SysML-v2-Release)); [Sensmetry Advent lesson 24](https://sensmetry.com/advent-of-sysml-v2-lesson-24-requirement-satisfaction-and-verification/) for the nested-subject example.

---

## 13. Assume-guarantee contract theory

### 13.1 The rules (Benveniste et al., *Contracts for System Design*, 2018, §5.1)

A **contract** is a pair C = (A, G):
- **A**, the assumptions: the set of environments the component is designed for.
- **G**, the guarantees: the set of behaviors it promises, *when the environment is in A*.

Three rules follow:

| Rule | Definition (saturated form) | Plain meaning |
|---|---|---|
| **Implementation** | M implements C if A × M ⊆ G | inside the assumed environments, the component only does guaranteed things |
| **Refinement** | C₂ ≼ C₁ iff **A₂ ⊇ A₁** and **G₂ ⊆ G₁** | a replacement may accept *more* environments and promise *tighter* behavior. Then it can be swapped in with no re-check of the parent |
| **Composition** | G = G₁ ∩ G₂; A = (A₁ ∩ A₂) ∪ ¬(G₁ ∩ G₂) | two blocks together promise both guarantees. What's left to assume is only what neither block's guarantees already cover |

"Saturated" means G already includes "anything goes outside A": G := G ∪ ¬A. The monograph also warns about **circular reasoning**: block 1 assumes what block 2 guarantees *and vice versa*. It is sound only under extra conditions. In circuits, every feedback loop is such a circle (a regulator and its load; a bias loop).

Nuzzo, Sangiovanni-Vincentelli et al. applied A/G contracts to analog interface design ("Methodology for the design of analog integrated interfaces using contracts", *IEEE Sensors J.* 12(12), 2012). They used *horizontal* contracts (between neighbours) and *vertical* ones (between abstraction levels).

### 13.2 What it means for a circuit, with the CE amp

Take `CeAmp` from roadmap §4.2, with a loading assumption added:

- **A (assumptions):** `vcc.v in 12V ± 5%`, `temp in −10…60 °C`, `input.z_src ≤ 1 kΩ`, `output.z_load ≥ 100 kΩ` (it was checked nearly unloaded).
- **G (guarantees):** `bias: dc(output.v) in 4.5…6.5 V`, `gain: 4.6 ± 5%`, `bass: f_low ≤ 30 Hz`.

**Composition, done right.** Place two `CeAmp`s in series. Stage 2's input looks like R1 ∥ R2 ∥ (β·RE) ≈ 47k ∥ 10k ∥ (100k…300k) ≈ 7.6–8 kΩ. That load is far below the assumed 100 kΩ. So stage 1's gain guarantee **does not transfer**: the context is outside A.

A rough hand check shows why this matters. Stage 1's collector sees RC ∥ 7.6k ≈ 4.7k ∥ 7.6k ≈ 2.9 kΩ, so its gain drops from ≈ 4.6 to ≈ 2.9. The product becomes ≈ 2.9 × 4.6 ≈ 13, not 4.6² ≈ 21. The post-MVP study (next_synthesis.md L1, "BD") measured exactly this: the product of stage guarantees is 19.1–23.3, while the real two-stage gain is 12.27–13.93.

**What a reusable contract needs.** To reuse stage 1's result without simulating the pair, its contract must:
1. **assume a load range** that covers the real context (e.g. `output.z_load ≥ 5 kΩ`), and be checked under it;
2. **guarantee its own port impedances** (`output.z_out ≤ 4.8 kΩ`, `input.z_in ≥ 7 kΩ`), so that the *neighbour's* assumptions can be discharged at the connection.

Then the connection check is plain set inclusion: stage 2 guarantees `input.z_in ≥ 7 kΩ` ⊆ stage 1 assumes `output.z_load ≥ 5 kΩ` ✓. Now both guarantees hold in context, with no system simulation. But the gains still multiply only if each gain was measured *at the load it really sees*. The A/G rule is sound; the art is choosing A so the real context fits inside it.

**Refinement, in circuit terms.** Suppose a new revision `CeAmp2` accepts `z_load ≥ 5 kΩ` (A₂ ⊇ A₁) and guarantees `gain 4.6 ± 3%` (G₂ ⊆ G₁). It can replace `CeAmp` anywhere, and no parent needs re-checking. If it narrows the supply range to `12 V ± 3%`, it is *not* a refinement. Every parent must re-check its supply connection.

**Verified:** [verified] monograph §5.1, Definitions 5.3–5.4 and eq. (5.7), and the related-work paragraph citing Nuzzo et al. [207] ([PDF](https://people.rennes.inria.fr/Albert.Benveniste/pub/ContractsMonograph2018.pdf), pp. 184–186, 3450ff in the extracted text). The loading numbers are my hand estimate; the 12.27–13.93 and 19.1–23.3 figures are from `next_synthesis.md` L1. Note: Shali et al., "Series composition of simulation-based assume-guarantee contracts for linear dynamical systems" ([arXiv 2209.01844](https://arxiv.org/abs/2209.01844)) covers series composition for linear systems. "Simulation" there means a *simulation relation* between behaviors, not SPICE.

---

## 14. Open ECAD languages

### 14.1 atopile

atopile uses one keyword, `assert`, for two different things: **equations that set values** (`is`) and **checks** (`within`, `<`, `>`). A solver propagates ranges and picks parts that satisfy them. There is no simulation and no setup.

```
module VoltageDivider:
    power  = new ElectricPower
    output = new ElectricSignal
    r_top = new Resistor
    r_bottom = new Resistor
    power.hv ~> r_top ~> output.line ~> r_bottom ~> power.lv
    assert r_bottom.resistance is v_out / max_current      # an equation
    assert v_out is v_in * ratio

module App:
    my_vdiv = new VoltageDivider
    assert my_vdiv.power.voltage is 10V +/- 1%              # the PARENT sets the child's condition
    assert my_vdiv.output.reference.voltage within 3.3V +/- 10%   # a spec
    assert my_vdiv.max_current within 10uA to 100uA
```

- **Conditions come from the parent.** The child is checked *in context*, with the value the parent gives.
- **Mistake to avoid:** `is` (a binding) and `within` (a check) share one keyword. A reader can't tell at a glance which lines are inputs and which are requirements.

**Verified:** [verified] atopile repo, `examples/equations/equations.ato`, `examples/auto-picking/auto-picking.ato`, `src/vscode-atopile/syntax_examples.ato` ([repo](https://github.com/atopile/atopile)).

### 14.2 PolymorphicBlocks (edg)

edg (Berkeley, UIST 2020) is a Python board HDL. Every port carries **interval parameters**. A connection creates a **link** that aggregates its ports and **requires** consistency.

```python
class VoltageLink(Link):
    def contents(self) -> None:
        self.assign(self.voltage, self.source.voltage)
        self.assign(self.voltage_limits, self.sinks.intersection(lambda x: x.voltage_limits))
        self.require(self.voltage_limits.contains(self.voltage), "voltage out of limits")
        self.assign(self.current_limits, self.source.current_limits)
        self.assign(self.current_draw, self.sinks.sum(lambda x: x.current_draw))
        self.require(self.current_limits.contains(self.current_draw), "current draw out of limits")
```

A regulator block reads its *context* through the link:

```python
self.pwr_in = self.Port(VoltageSink(voltage_limits=RangeExpr(), current_draw=RangeExpr()), [Power, Input])
self.assign(self.pwr_in.current_draw, self.pwr_out.link().current_draw + self.actual_quiescent_current)
self.require(self.pwr_out.voltage.lower() + self.actual_dropout.upper()
             <= self.pwr_in.link().voltage.lower(), "excessive dropout")
```

- **Assumptions are port properties**, not statements: a sink has `voltage_limits` (what it accepts), a source has `current_limits` (what it can give). No `assume` keyword. The *role* (sink or source) says which side owns which number. That is our §3.2 role rule.
- **Every connection is checked** by the link type: source voltage ⊆ intersection of sink limits; sum of draws ⊆ source limit. Our "upstream guarantee ⊆ downstream assumption".
- **Hierarchy:** values propagate through `PortBridge`s at block boundaries, so a deep sub-block's requirement is checked against the real top-level values. It is checked in context, by interval arithmetic, with no simulation.
- **No setups:** edg has no test benches. It checks static ranges only.

**Verified:** [verified] PolymorphicBlocks source, `edg/electronics_interfaces/VoltagePorts.py`, `edg/abstract_parts/LinearRegulator.py` ([repo](https://github.com/BerkeleyHCI/PolymorphicBlocks)); UIST 2020 paper ([PDF](https://people.eecs.berkeley.edu/~bjoern/papers/lin-pblocks-uist2020.pdf)).

### 14.3 tscircuit

tscircuit (React/TSX) puts the simulation **inside the board**: the source, the probes and the analysis are elements of the design itself.

```tsx
<board width={16} height={16}>
  <voltagesource name="V1" voltage="5V" />
  <resistor name="R1" resistance="187ohm" footprint="0402" />
  <capacitor name="C1" capacitance="10uF" footprint="0402" />
  <trace from={".V1 > .pin1"} to={".R1 > .pin1"} />
  …
  <voltageprobe connectsTo={".C1 > .pin1"} />
  <analogsimulation duration="15ms" timePerStep="0.1ms" spiceEngine="ngspice" />
</board>
```

**Mistake to avoid:** one setup, drawn into the design, with no specs. A second setup means copying the board. The fixture `V1` is indistinguishable from a real part.

**Verified:** [verified] tscircuit docs, [low-pass filter example](https://docs.tscircuit.com/guides/spice-simulation/Passive%20Filters/low-pass-filter), [`<analogsimulation>`](https://docs.tscircuit.com/elements/analogsimulation).

### 14.4 SKiDL, PySpice, spicelib

These are Python libraries. The **testbench is a Python script**: it builds a circuit, adds sources and loads, and calls an analysis. Conditions are function arguments.

```python
# PySpice
circuit = Circuit('CE amp bench')
circuit.include('ce_amp.lib')
circuit.V('cc', 'vcc', circuit.gnd, 12@u_V)
circuit.SinusoidalVoltageSource('in', 'in', circuit.gnd, amplitude=100@u_mV, frequency=1@u_kHz)
circuit.X('amp', 'ce_amp', 'vcc', circuit.gnd, 'in', 'out')
circuit.R('load', 'out', circuit.gnd, 10@u_kΩ)
simulator = circuit.simulator(temperature=25, nominal_temperature=25)
analysis = simulator.ac(start_frequency=1@u_Hz, stop_frequency=1@u_MHz,
                        number_of_points=20, variation='dec')
```

```python
# spicelib: tolerances and worst-case, applied to an existing netlist
wca = WorstCaseAnalysis(AscEditor("sallenkey.asc"), runner)
wca.set_tolerance('R', 0.01)                      # all resistors ±1 %
wca.set_tolerance('R1', 0.05)                     # override one
wca.set_parameter_deviation('Vos', 3e-4, 5e-3)
```

SKiDL builds netlists in Python and can hand them to PySpice for simulation in the same way.

- The setup is code: flexible, but invisible to tools. You can't list a block's setups or specs without running the script.
- Pass/fail is ad hoc (`assert` in pytest).
- spicelib separates **tolerances** (per component class, overridable per instance) from the netlist. That is our knob model's "defaults by kind, override by instance".

**Verified:** [secondary] [spicelib sim-analysis docs](https://spicelib.readthedocs.io/en/latest/modules/sim_analysis.html). [from memory] PySpice and SKiDL API.

---

## 15. Side by side

| Tool | Setup first-class & named? | Setup outside the design? | Spec → setup link | Default + override | Load explicit? | Corners | Sub-block checks in context? | Reuse without re-simulating |
|---|---|---|---|---|---|---|---|---|
| SPICE deck | one per file | mixed in | none | `.param` | yes, as elements | `.step`/`.temp`/`.lib` grid | no | subckt only |
| **Cadence ADE** | **yes** (test + testbench cell) | **yes** | spec on output in test | **global vars → test vars → corner** | yes (in testbench) | named, enabled **per test** | no (manual, via Verifier) | config view → Verilog-A model |
| Keysight ADS | yes (sim controller) | same schematic | **by name** (`SimInstanceName`) | VAR blocks | yes | sweeps, yield | no | subnetworks |
| **Datasheet** | yes ("Figure 6-1") | yes | "see Figure", per row | **header "unless otherwise noted"** | **always in conditions** | 25 °C vs full range; by grade | n/a | the datasheet *is* the reuse |
| **IBIS** | per table | yes | fixture *inside* each table | defaults (50 Ω) | **always** (`R_fixture`) | **coherent** typ/min/max | n/a | yes, by design |
| **UVM / SVA** | **yes** (test, env) | **yes** | checks live with the block/env, run in every test | config db | n/a | constrained random + coverage | **yes**: passive agents, `bind`, assume→assert | no (digital re-sims) |
| cocotb | yes (function) | yes | fused into the test | parametrize | n/a | parametrize | no | no |
| VHDL-AMS | yes (portless entity) | yes | `assert` in bench | generics | yes | scripts | no | **configurations** swap models |
| Modelica | yes (test model) | yes (extends/instantiates) | requirement blocks bound to parts | modifiers | yes | parameters | **yes**: per-component requirement blocks | `redeclare` |
| CRML | bindings | yes | `during … ensure` | — | — | HOW WELL (probability) | yes (class extension) | — |
| SysML v2 | **yes** (`verification def`) | yes | **setup names the spec** (`verify`) | attributes | n/a | — | **yes**: nested `subject` binding, `satisfy … by a.b` | — |
| A/G theory | — | — | — | — | assumptions include env | — | **composition rule** | **refinement rule** |
| atopile | no | — | — | parent sets child values | no | ranges | yes (solver) | yes (ranges only) |
| edg | no | — | — | port params | no | intervals | **yes**: link `require`s | yes (intervals only) |
| tscircuit | no | **no** (in the board) | none | — | as parts | — | no | no |
| PySpice / spicelib | script | yes | ad hoc | function args / per-class tolerance | yes | WCA, MC | no | no |

---

## 16. Hierarchy: what the references do

The lead's question: *how do setups interact with hierarchy, and can a sub-block's verified behavior be reused without simulating the whole design?* The references show five strategies. Mature flows combine them.

| # | Strategy | Who | What happens to the child's **specs** | … its **benches** | … its **assumptions** | Needs a system simulation? |
|---|---|---|---|---|---|---|
| 1 | **Checks travel, stimulus stays home** | UVM vertical reuse; SVA `bind`; Modelica_Requirements | re-evaluated as monitors on the parent's runs | dropped in context (drivers go passive) | become asserts on the neighbours | yes, but only the parent's own benches |
| 2 | **Connection checks by ranges** | edg, atopile, our §8.8 | not re-simulated | not used | checked against the neighbour's guarantees by interval inclusion | no |
| 3 | **A/G composition and refinement** | Benveniste et al.; Nuzzo (analog) | reused as-is *if* the context ⊆ A | not used | must be discharged by the context | no, if every A is discharged |
| 4 | **Swap in a verified abstraction** | ADE config view (Verilog-A); VHDL-AMS configurations; IBIS | the abstraction is validated against the child's specs once | used to validate the abstraction | are the abstraction's validity range | yes, but cheap (behavioral model) |
| 5 | **Requirement decomposition with subject binding** | SysML v2 nested `subject`; ADE Verifier tree | a sub-requirement is bound to a sub-part path and verified by the sub-part's own cases | stay with the sub-part | traced, not auto-checked | no (bookkeeping) |

**What this means in practice.** For the two-stage CE amp:
- **Strategy 2 or 3** is the cheap path. Stage 1's `gain` guarantee is reused *only if* stage 2's input impedance (a stage-2 guarantee) satisfies stage 1's load assumption. If the contract has no load assumption, this path is **unsound**, and that is the BD failure.
- **Strategy 1** is the safe path. Run the parent's bench and evaluate stage 1's `bias` and `gain` specs as monitors on it. The child's `gain` definition (`ac(output.v / input.v)`) is still meaningful in context, because probes are relative to the child's ports. Its own default bench (a 1 V AC source on `input`) is *not* used: in context, stage 1's input is driven by the real upstream circuit.
- **Strategy 4** matters later: a child checked once in detail publishes a behavioral model plus its validity range (our "characterized results", the evidence ladder's middle rung).

One caution for strategy 1. A child spec measured on the parent's bench is measured under the *parent's* conditions, not the child's assumptions. If the parent's bench never exercises the child's worst case (e.g. the parent's supply range is narrower), the in-context pass is weaker than the child's own. UVM handles this with coverage. Our engine can report which of the child's knobs the parent's bench actually spans.

---

## 17. What I could not verify

- **Cadence ADE:** user guides, the `mae*` SKILL API signatures, and community forum threads are behind a login or a bot wall. The spec-column kinds, per-test corner enable, precedence of global over test variables (only the support note's title was visible), config-view behavior and Verifier "spec check" come from general knowledge and vendor marketing pages.
- **Synopsys PrimeWave / Custom Compiler:** only the datasheet is public. No syntax.
- **Keysight ADS:** Goal parameters are verified from the 2011 manual. The exact netlist line is from memory.
- **UVM/SVA, VHDL-AMS, Spectre, PySpice:** standard syntax from memory. The VHDL-AMS bench uses user-defined source entities.
- **Modelica Association:** I found no public MA standard for requirements beyond CRML's stated aim. The "newer Modelica Requirements work" here means CRML (2021 spec, 2023 paper) and the vVDR/ModelicaML binding work.
- **SysML v2:** verified from the official training models, not from the full specification text.
- **Nuzzo et al. 2012:** known only through the monograph's summary. Not read.

---

## 18. Lessons for our contract syntax

### 18.1 Patterns that recur

1. **Four parts, kept apart: subject, setup, conditions, check** (plus confidence). ADE (test / variables and corners / output and spec), datasheets (device / test circuit / test conditions / min-typ-max), SysML v2 (`subject` / `verification def` / `assume` / `require`), CRML (WHERE / WHEN / WHAT / HOW WELL). CRML's diagnosis of Modelica_Requirements is the warning: fuse two of the parts and the combinations explode. Our spec grammar `spec name: measure relation bound [on bench] [where cond]` already has the slots. Keep them separate in the syntax too.

2. **The setup is a named object that places the design from outside.** ADE testbench cells, VHDL-AMS portless entities, Modelica test models, UVM tests, SysML verification cases: all first-class and named. None of the mature tools draws fixtures into the design; tscircuit, which does, is stuck at one setup. This supports making benches first-class (L1), with a name every spec can point at.

3. **Defaults once, overrides per row.** The datasheet header "(unless otherwise noted)", ADE global variables, and our `..default` are one idea. The default setup should be *visible* (printed on the I/O page, like the header), even if never written.

4. **A spec points at its setup by name.** ADS `SimInstanceName`, ADE's output-in-test, datasheet "see Figure 6-1". SysML v2 reverses it (`verify R` in the case) so one requirement can have several verification cases. For us, `on bench` on the spec is the readable direction. A spec that needs two setups (calibration, E7) is the exception that needs design.

5. **Conditions come in three kinds, and they need different syntax:**
   - a **fixed point** (`IOUT = 1 mA`): belongs in the bench;
   - a **quantified range** (`TJ −40…125 °C`, "for all"): a range knob, from assumptions;
   - a **measure axis** (`VIN from VOUT+0.5 V to 5.5 V` for line regulation; ADS `IndepVar` ranges): part of the measure, like our `.band()` and `.window()`.

   The datasheet column hides the difference, and readers pay for it. Our language can make it explicit.

6. **"Typical" is a point, not a limit.** Datasheets give typ at 25 °C and min/max over the range, in the same row. If we ever print or import datasheet-style specs, typ belongs to the nominal run (`#[confidence(nominal)]`), never to a pass/fail limit.

7. **The load is part of every guarantee.** IBIS stores `R_fixture` with each table. Datasheets list `CL`, `RL`, `COUT` in the conditions. A guarantee published without its load can't be reused. So assumptions on `z_load` / `z_src` (and guarantees on `z_out` / `z_in`) are not optional extras. They are what makes composition sound (§13.2).

8. **Corners are named partial assignments, enabled per setup,** and sometimes **correlated** (IBIS min = slow and weak *together*). Our engine searches the whole box, so named corners are for documentation and regression ("pin as corner"), not for coverage. But a published interface may need to say "these knobs move together".

9. **In hierarchy: checks travel, stimulus stays home.** UVM passive agents, SVA `bind`, Modelica requirement blocks re-bound per instance, SysML nested `subject`. Concretely for us:
   - a child's **specs** become monitors evaluated on the parent's benches, with a placement path (L4, M1);
   - a child's **benches** stay local, as its unit tests;
   - a child's **assumptions** become checks at its connections (§8.8), which is the SVA assume → assert step.

10. **Reuse without re-simulating is the A/G refinement and composition rule, nothing more.** A child's guarantee can be used by the parent *only when* the parent's context lies inside the child's assumptions, including loading. When it doesn't, fall back to in-context simulation (the evidence ladder). Every reference that reuses published behavior (IBIS, datasheets, edg) states the conditions next to the numbers.

### 18.2 Mistakes to avoid

| Mistake | Seen in | Consequence | What we do instead |
|---|---|---|---|
| One keyword for "set this value" and "check this value" | atopile (`assert … is` vs `assert … within`) | readers can't tell inputs from requirements | separate words for conditions and checks (bench fields / assumptions vs `spec`) |
| Fixtures drawn into the design | tscircuit | one setup only; fixture parts look like real parts | benches are separate items that place the block |
| The limit stored twice | ADE (output spec and Verifier requirement) | drift; needs a "spec check" tool | one text home per spec; the table is a view of it |
| Check fused into the stimulus code | cocotb, PySpice scripts | can't re-run the check under another setup; can't list a block's specs | specs are declarations; benches are data |
| Time window mixed with the condition | Modelica_Requirements (CRML's critique) | combinatorial library growth | `where`, `.window()`, `.band()` stay orthogonal to the measure |
| A bench outside the operating range, unnoticed | hand-made ADE testbenches | a spec "passes" under conditions the block never promised | "every bench is checked to stay inside the assumptions" (§8.5); keep it |
| Guarantees without load conditions | unloaded stage checks (BD in next_synthesis) | gain product 19.1–23.3 vs real 12.27–13.93 | `z_load` / `z_src` assumptions and `z_out` / `z_in` guarantees at every analog port |
| Treating correlated corners as independent, or the reverse | IBIS (explicitly coherent) | pessimistic or optimistic results | lot knobs and declared correlations (engine.md §2.5) |
| Circular assume-guarantee through a feedback loop | A/G monograph warning | an unsound "both pass" | detect loops at connections; fall back to in-context simulation for them |
| Silent omission of sub-block checks | our v0.1 (L4) | wrong verdict | carry every child contract with its path, or warn "not checked" |

### 18.3 On the word `assume`

The lead dislikes `assume`. The references use several words for the same slot. They also show that "assume" covers two different jobs:

| Word | Where | Meaning there |
|---|---|---|
| `assume` | SVA `assume property`, SysML v2 `assume constraint`, A/G theory | SVA: an input constraint that becomes an obligation on neighbours at integration. SysML: *applicability* (the requirement doesn't apply otherwise) |
| "Recommended operating conditions" | every datasheet | the range the block is designed for |
| "Test conditions" | every datasheet | the setup of one row |
| `during … ensure …` | CRML | a condition that scopes a check |
| port properties `voltage_limits`, `current_limits` | edg | what a sink accepts / a source can give, *owned by the port's role* |
| `[Voltage Range]`, `R_fixture` | IBIS | conditions stored with the data |

Our `assume` today does **two jobs**:
1. it defines a **range knob** that every spec of the block is checked over (a datasheet's "operating conditions");
2. it creates an **obligation on the neighbours**, checked at every connection (SVA's assume → assert).

The references keep these apart more often than not. Datasheets separate "Recommended operating conditions" (job 1 and 2 together, as a table) from "Test conditions" (per-row setup). edg attaches the range to the **port**, so the role (sink or source) says who must satisfy it. SysML uses `assume` only for applicability, which is our `where`.

This suggests two directions for the redesign. Both have precedents; choosing is the lead's call:
- **Port-attached ranges, datasheet style.** Ranges live on the port or on an `operating` / `conditions` section. Job 2 follows from the port's role, as in edg. There is no statement keyword at all.
- **A condition word plus a spec word, CRML style.** For example `when` / `given` for the ranges a spec is checked over, and `spec` for the guarantee. Connection checks are generated from roles.

Either way, the lessons above say: keep **fixed test conditions** (bench fields), **ranges over which specs must hold** (knobs), and **measure axes** (`.band`, `.window`) as three visibly different things.
