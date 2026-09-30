# Circuit Conditions, Options and Names: How Simulators Do It

> 2026-09-30 · Research for `spicy_circuit` (`circuit.md`, `pipeline.md` §2–§3, `engine_plan.md` §5.2–§5.3). It checks five proposed additions to `Lowered` against ngspice (including XSPICE and its OSDI glue), Xyce, Gnucap, VACASK and OpenVAF, all read at the source level. **Status: research only. Nothing is decided or implemented.**
>
> Citations are `file:line`, using these base directories: ngspice `src/`, Xyce `src/`, Gnucap repo root, VACASK repo root, OpenVAF `openvaf/`. I read every line cited. A few items are marked *(inferred)*: they come from reading the code, not from running it.

---

## Summary

**The proposal is mostly right.** Here is what the four simulators agree on:
- They all have exactly the two circuit-wide temperatures it proposes: the circuit temperature, and the default temperature at which model parameters were measured.
- They all keep a device's `temp`/`dtemp` apart from the circuit temperature.
- All but Gnucap store model parameters raw and compute temperature-adjusted copies once per temperature, never in every Newton iteration.
- They all keep model names apart from instance names.

The proposal still needs five corrections.

| # | Proposed | Verdict | What to change |
|---|---|---|---|
| 1 | `Conditions { temp, tnom }` in `Lowered`, in kelvin | **Right** | Both default to 300.15 K (27 °C), as in ngspice (`cktntask.c:128-129`). ngspice and Xyce store kelvin internally too, so convert °C only at the edges. **When `gmin` arrives it belongs here**, beside temperature, not with the tolerances (Xyce's `DEVICE` group) |
| 2 | `BjtModel` gains `xtb`, `xti`, `eg`, `tnom: Option<f64>` | **Right, but too narrow** | The defaults check out: 0, 3, 1.11, and "the circuit's" (`bjtsetup.c:156-164`, `bjttemp.c:42`). But: **every** model kind with temperature behavior needs `tnom` (R, C, L, D), the diode needs `eg` and `xti`, and `BjtParams` needs `temperature: DeviceTemperature` (it's the only temperature-dependent kind without one) |
| 3 | `SolverOptions { reltol, vntol, abstol }` next to the analyses | **Right** | The defaults check out: 1e-3, 1e-6 V, 1e-12 A (`cktntask.c:99-102`). Keep **one set per job**, not per analysis (ngspice and Gnucap). When our Newton loop starts reading this struct, it must test voltages against `vntol` and currents against `abstol` (today one 1e-6 covers both), and the iteration limits (`itl1`, `itl2`, `itl4`) join the struct |
| 4 | Model names in `CircuitNames` | **Right, but lowering must change first** | Today lowering merges identical cards even when their names differ (`spicy_netlist/src/lower.rs:174-197`), and the Deck drops the model name (`BjtSpec.model` is a copy, `spicy_netlist/src/devices/bjt.rs:11`). A merged model then has no single name. **Key the model tables by card name**, as every simulator does |
| 5 | Later: origins, opaque vendor subcircuits, new kinds | **Right direction** | Give **models** origins too, not only devices. An opaque vendor subcircuit is a kind only the exporter understands, and our simulator rejects it with an error. New kinds follow the same template: `tnom` on the model, `DeviceTemperature` on the instance |

**The specific questions:**

| Question | Answer | Main evidence |
|---|---|---|
| Should `gmin` go with the options? | **No, it goes with `Conditions`.** gmin is a real conductance (1e-12 S) added across every junction in every load, so it changes the circuit being solved. The engine's "tight tolerance" re-check must not change it. Add it only when our simulator applies it | `bjtload.c:460-461, 490-491`; `dioload.c:552-559`; Xyce keeps GMIN in `DEVICE`, beside TEMP and TNOM (`N_DEV_DeviceOptions.C:90, 99-100`) |
| Should per-analysis options exist? | **Not now.** ngspice and Gnucap keep one global set. Analysis-specific needs become separate fields of that one set, such as ngspice's `itl1` (DC op), `itl2` (DC sweep) and `itl4` (transient) | `tskdefs.h:37-39`; Gnucap `u_opt1.cc:110-120`; Xyce `NONLIN`/`NONLIN-TRAN` (`N_NLS_Manager.C:1336-1339`) and VACASK's per-analysis snapshot (`an.cpp:133-145`) show where to go later |
| Should `tnom` live per model or in `Conditions`? | **Both, as in all four simulators.** Each model has `tnom: Option<f64>`, and `None` means `Conditions.tnom`. It's resolved in the derive step, like ngspice's `if(!model->BJTtnomGiven) model->BJTtnom = ckt->CKTnomTemp` | `bjttemp.c:42`; Xyce `N_DEV_BJT.C:4008-4009`; Gnucap `e_model.cc:101-105`; OSDI models call `$simparam("tnom")` (`bsim4v8.va:3377-3379`) |
| Store temperature parameters raw and compute a `Derived` layer? | **Yes.** ngspice, Xyce and OSDI keep raw model values and compute adjusted copies once per temperature. Gnucap recomputes on every evaluation, and it's the slow outlier. The raw values are also what the ngspice export must write | ngspice `bjttemp.c:163-246` → `bjtload.c:623, 632`; Xyce `N_DEV_BJT.C:1941-1943` → `:3009`; OpenVAF `init.rs:28-36`; Gnucap `mg_out_model.cc:717-719` |
| Should the diode get the same fields? | **Yes: `eg`, `xti` and `tnom`.** Its saturation current follows the same formula as the BJT's | ngspice `diotemp.c:116-119` vs `bjttemp.c:163-166`; defaults `diosetup.c:140-155, 240-242` |

§11 gives the recommended shape, and §12 walks one engine run through it with real numbers.

---

## 1. What's being checked

`spicy_circuit` today is `Lowered { circuit, params, names, analyses }` (`crates/spicy_circuit/src/lib.rs:74-80`). Three things about the current code matter here:
- R, C, L and diode instances carry `temperature: DeviceTemperature { Offset(dtemp), Fixed(temp) }` (`params.rs:29-45`).
- R, C and L models carry `tc1`/`tc2` but no `tnom`.
- The simulator uses a fixed thermal voltage of 25.85 mV (`spicy_simulate/src/devices/bjt.rs:11`, `devices/diode.rs:7`). Its one `NewtonConfig { abs_tol: 1e-6, rel_tol: 1e-3, max_iters: 50 }` applies to every unknown (`spicy_simulate/src/lib.rs:33-46`, `trans.rs:46-59`).

The proposal adds `Conditions`, new `BjtModel` fields, `SolverOptions`, model names, and later origins and new kinds. It has three consumers:
- our simulator;
- the ngspice exporter (the engine's backend, `engine_plan.md` §5);
- the engine, which changes values between thousands of runs.

**Terms used below:**
- **TNOM** is the temperature at which a model's parameters were measured. A device's IS at 60 °C is computed from the IS at TNOM.
- **XTI** and **EG** set how fast IS grows with temperature.
- **XTB** sets how β (BF, BR) grows with temperature.
- **gmin** is a tiny conductance the simulator puts across every junction so the matrix never becomes singular.
- **reltol/vntol/abstol** say when Newton's method has converged. A voltage has converged when it changes by less than `reltol·|V| + vntol`, and a current when it changes by less than `reltol·|I| + abstol`.

---

## 2. Circuit temperature

| | Where it's stored | Default | Set by | How it reaches devices | When it changes between runs |
|---|---|---|---|---|---|
| **ngspice** | `CKTtemp`, `CKTnomTemp` in kelvin (`cktdefs.h:98-99`), copied from the task's `TSKtemp`/`TSKnomTemp` at every job (`cktdojob.c:52-53`) | 300.15 K for both (`cktntask.c:128-129`, `cktinit.c:69-70`) | `.options temp= tnom=` in °C, converted to K (`cktsopt.c:71-76`); `.temp` (below) | `CKTtemp()` calls each device type's temperature routine (`ckttemp.c:26-33`), for example `BJTtemp` | Every `run` passes `reset=1` (`spiceif.c:390`), so every run redoes `CKTunsetup`, `CKTsetup` and `CKTtemp` (`cktdojob.c:163-171`). Only `.dc temp` changes it in place, rerunning just `CKTtemp` (`dctrcurv.c:212-218`) |
| **Xyce** | `DeviceOptions::temp`, `tnom` in kelvin (`N_DEV_DeviceOptions.h:104-105`) | 300.15 K (`N_DEV_DeviceOptions.C:99-100`) | `.OPTIONS DEVICE TEMP= TNOM=` in °C (`N_DEV_DeviceOptions.C:209-220`). There is **no `.TEMP`**; the docs say to use `.STEP TEMP LIST …` (`Xyce_RG_app2.tex:575-576`) | Each instance's `processParams` calls `updateTemperature(TEMP)` (`N_DEV_BJT.C:802-822`) | `.STEP TEMP` calls `DeviceMgr::updateTemperature`: set the global, then `setParam("TEMP")` + `processParams()` on every model and instance (`N_DEV_DeviceMgr.C:5512-5591`) |
| **Gnucap** | Global `OPT::temp_c`, `tnom_c` in °C (`u_opt.h:101, 122`), plus a per-run `_sim->_temp_c` (`u_sim_data.h:43`) | 27 °C (`u_opt1.cc:45, 66`) | `.options`, `.temp` (`c_comand.cc:90-103`), **and each analysis command**: `.tran … temperature=` or `dtemp=` (`s_tr_set.cc:180-197`), `.dc` (`s_dc.cc:279`). A bare `.op 27 50` is a temperature sweep (`s_dc.cc:248-257`) | Read live inside every device evaluation (`d_diode.model:196-197`) | Nothing to invalidate: every analysis resets `_sim->_temp_c = OPT::temp_c` (`s_tr_set.cc:180`) and devices read it live |
| **VACASK** | `SimulatorOptions::temp`, `tnom` in °C (`options.h:12-13`) | 27 °C (`options.cpp:48-49`) | The `temp` option, which can also be swept | Passed as `opt.temp + 273.15` to every OSDI `setup_instance` (`osdiinstance.cpp:294-295`) | Changing `temp` or `tnom` forces a full setup of every model and instance (`cirparams.cpp:417-428`, `options.cpp:356-392`) |

**The ngspice path in full**, because the exporter depends on it:

```
.temp 60             inp.c:1242-1252      the front-end pulls the card out of the deck ("*" comments it)
                     inp.c:1259-1270      strtod → cp_vset("temp", 60); a non-number warns and uses 27 °C
cp_vset("temp")      variable.c:194-227   an option variable: stored with this circuit (ci_vars)
if_option            spiceif.c:549        setAnalysisParm on the circuit's default options job
CKTsetOpt            cktsopt.c:74-76      task->TSKtemp = 60 + 273.15
run                  spiceif.c:390        doAnalyses(ckt, reset = 1, task)
CKTdoJob             cktdojob.c:52-53     ckt->CKTtemp = task->TSKtemp; CKTnomTemp likewise
                     cktdojob.c:163-171   CKTunsetup → CKTsetup → CKTtemp
CKTtemp              ckttemp.c:26-33      CKTvt = k·T/q; each device type's DEVtemperature
BJTtemp              bjttemp.c:42, 79-80  model TNOM default; instance T = CKTtemp + dtemp
```

Two consequences for our exporter:
- `reset` throws away the circuit and re-sources the deck file (`runcoms2.c:175-186`). An interactive `option temp=` lives in the old circuit's task and is lost. The deck's `.temp` is read again. `engine_types.md` §11.3 already pins this with a test.
- `.temp` must be a plain number by the time ngspice reads it (`inp.c:1262-1266`). The `.temp {k0}` form works only because numparam evaluates the braces first. A list of temperatures isn't supported: only the first number is parsed (`strtod`, `inp.c:1262`).

**What they agree on:** temperature is an input to the **device setup step**, not to the Newton loop (Gnucap is the exception). Changing it reruns the temperature step and never the circuit structure: ngspice's `.dc temp`, Xyce's `.STEP TEMP`, VACASK's setup, which remaps unknowns only if node collapsing changed (`osdiinstance.cpp:318-378`). That's exactly `pipeline.md`'s `Derived = f(Params, Conditions)`.

---

## 3. Device temperature: `temp` and `dtemp`

| | Parameters | Precedence | Code |
|---|---|---|---|
| **ngspice**, built-in devices | `temp` and `dtemp` on R, C, L, D and BJT (`res.c:17-18`, `cap.c:17-18`, `ind.c:17-18`, `dio.c:14-15`, `bjt.c:76-77`) | **`temp` wins.** `dtemp` is ignored, and R, C and L print "dtemp ignored" | `restemp.c:35-49`, `bjttemp.c:76-80`, `diotemp.c:308-311` |
| **ngspice**, OSDI glue | the same names | **Both apply**: `temp` if given, then `+ dtemp` | `osdisetup.c:250-259`. That's inconsistent with its own built-in devices |
| **Xyce** | `TEMP`, `DTEMP` on BJT, diode and resistor (`N_DEV_BJT.C:89-101`, `N_DEV_Diode.C:83-95`) | **`TEMP` wins**, with the warning "Instance temperature specified, dtemp ignored" | `N_DEV_BJT.C:805-819`; the offset is added in `updateTemperature` (`:1864-1868`) |
| **Gnucap** | `temp`, `dtemp` on the common (`e_compon.h:60-62`) | `temp` if given, else the analysis temperature + `dtemp` (`e_compon.cc:277-279`). But the diode ignores both and uses the analysis temperature (`d_diode.model:196-197`). The BJT honors only `TEMP` (`d_bjt.model:378-379`) | |
| **VACASK** | none in the simulator: one global temperature for every instance (`osdiinstance.cpp:294-295`) | Models that want `dtemp` or `trise` declare it themselves in Verilog-A | |

**For us:** `DeviceTemperature { Offset, Fixed }` matches ngspice's and Xyce's built-in devices, and `Fixed` wins, as `lower.rs:233-239` already does. **`BjtParams` is the only kind missing it**, although ngspice has `temp` and `dtemp` on the BJT (`bjt.c:76-77`).

**Pitfall to avoid (Xyce, inferred):** `.STEP TEMP` writes `TEMP` into every instance through `setParam`, and `setParam` marks the value "given" (`N_DEV_DeviceEntity.C:627-631`). A netlist instance's own `TEMP` is overwritten, and its `DTEMP` is zeroed. The general rule: never write a resolved temperature back into an input field. Our enum stays as the netlist said it, and `Derived` holds the device's actual temperature.

---

## 4. Model TNOM and temperature parameters

### 4.1 Which exist, and their defaults

| Parameter | ngspice BJT | Xyce BJT (level 1) | Gnucap BJT | ngspice diode | Xyce diode | Gnucap diode |
|---|---|---|---|---|---|---|
| `TNOM` | the circuit's `CKTnomTemp`, applied in the temperature routine (`bjttemp.c:42`); °C → K at parse (`bjtmpar.c:42`) | the options TNOM, applied at construction (`N_DEV_BJT.C:4008-4009`) | `OPT::tnom_c` (`d_bjt.model:208` inherits `d_diode.model:375`) | the circuit's, applied at setup (`diosetup.c:240-242`) | the options TNOM (`N_DEV_Diode.C:1727-1728`) | `OPT::tnom_c` (`d_diode.model:375`) |
| `XTB` | 0 (`bjtsetup.c:156-158`) | 0 (`N_DEV_BJT.C:661`) | 0 (`d_bjt.model:326-327`) | n/a | n/a | n/a |
| `XTI` | 3 (`bjtsetup.c:162-164`) | 3 (`N_DEV_BJT.C:696`) | 3 (`d_bjt.model:328`) | 3 (`diosetup.c:153-155`) | 3 (`N_DEV_Diode.C:255`) | 3 (`d_diode.model:394-395`) |
| `EG` (eV) | 1.11 (`bjtsetup.c:159-161`) | 1.11 (`N_DEV_BJT.C:687`) | 1.11 (`d_bjt.model:329`) | 1.11; 1.16 when `tlev=2` (`diosetup.c:140-146`) | 1.11 (`N_DEV_Diode.C:248`) | 1.11 (`d_diode.model:392-393`) |
| Resistance tempcos | `TRB1/2`, `TRC1/2`, `TRE1/2` (`bjt.c:177-183`), applied to RB, RC, RE (`bjttemp.c:107-119`) | none: RB is copied unscaled (`N_DEV_BJT.C:1948-1950`) | none | `TRS1/2` (`dio.c:60-62`) | `TRS1/2` (`N_DEV_Diode.C:278-292`) | none |
| Other | `TLEV`/`TLEVC` formula selectors and per-parameter `T*1`/`T*2` coefficients (`bjt.c:157-214`) | none | none | `TLEV`, `TCV`, `TTT1`, `TM1`, … (`dio.c:66-122`) | `TIKF`, `TBV1/2` | none |

R, C and L have a model `tnom` in ngspice too, with the circuit's as the default:
- resistor: `res.c:61`, `ressetup.c:29`
- capacitor: `cap.c:54`, `capsetup.c:58`
- inductor: `ind.c:45`, `indsetup.c:35`

The resistor's scaling uses `T − TNOM` (`restemp.c:83-106`), so **our R, C and L models' `tc1`/`tc2` mean nothing without a `tnom`**.

The core set is XTB, XTI, EG and TNOM, the same in all three with the same defaults. The `T*1`/`T*2`/`TLEV` family is ngspice-only. The resistance tempcos come with RB/RC/RE and RS, which our devices don't have yet. None of those belong in this step.

### 4.2 When they're applied

| | When temperature values are computed | What the Newton loop reads |
|---|---|---|
| **ngspice** | Once per temperature, in `BJTtemp`: `BJTtSatCur = area·IS·exp(factlog)` and `BJTtBetaF = BF·(T/TNOM)^XTB` (`bjttemp.c:163-172, 233-246`) | `bjtload.c:623, 632` divides by `BJTtBetaF`; it recomputes only `vt = T·k/q` (`:140`) |
| **Xyce** | Once per temperature, in `updateTemperature`: `tSatCur`, `tBetaF` (`N_DEV_BJT.C:1941-1943`) | `csat = tSatCur * AREA` (`N_DEV_BJT.C:3009`) |
| **OSDI / OpenVAF** | Temperature is passed only to `setup_instance` (`osdi_0_3.h:178-180`). OpenVAF moves code that depends only on temperature and parameters into setup and caches it (`init.rs:28-36`) | Eval reads the stored temperature and the cache slots, never a per-eval value (`setup.rs:291-293, 312`; `eval.rs:143-145`) |
| **Gnucap** | **On every evaluation.** The generated `tr_eval` rebuilds the temperature block on the stack (`mg_out_model.cc:717-719`), and the diode recomputes `_isat` inside `eval` (`d_diode.model:196-200`) | everything |

The one per-iteration exception in ngspice is **self-heating**: a diode with a thermal node updates its temperature values inside the load (`dioload.c:330-332`). That's a feature for later, with its own thermal unknown.

**Raw values vs adjusted values.** Each simulator keeps the value as written on the model and the adjusted value on the instance:
- ngspice: `BJTsatCur` on the model, `BJTtSatCur` on the instance.
- Xyce: `satCur`/`betaF` on the model (`N_DEV_BJT.h:953-956`), `tSatCur`/`tBetaF` on the instance (`:555-556`).

That's `Params` (raw) and `Derived` (adjusted), split by storage instead of by field name. Keeping the raw values is also required for export, because ngspice applies its own equations to what we write (`pipeline.md` §5).

---

## 5. Solver options

### 5.1 Where they're kept

- **ngspice** has two layers:
  - The **task** holds what the user asked for, `TSKreltol` and the rest (`tskdefs.h:15-78`). `.options` cards write it through `INPdoOpts` → `CKTsetOpt` (`inpdoopt.c:61`, `cktsopt.c:44-76`). The defaults are set in `cktntask.c:95-129`.
  - The **circuit** holds what this job uses, copied field by field at the start of every job (`cktdojob.c:52-110`).
  - `temp` and `tnom` sit in the same options table as `reltol` (`cktsopt.c:281-282`), so ngspice doesn't separate physics from numerics.
- **Xyce** has one options package per subsystem:
  - `DEVICE` (`DeviceOptions`): gmin, TEMP, TNOM, and device-level abstol/reltol for its own current checks (`N_DEV_DeviceOptions.C:87-100`).
  - `NONLIN`, `NONLIN-TRAN` and `NONLIN-HB`: separate Newton settings for DC, transient and harmonic balance (`N_NLS_Manager.C:1336-1339`).
  - `TIMEINT`: time-step error control (`N_TIA_TIAParams.C:102-104`).
  - `LINSOL`: the linear solver.
- **Gnucap** keeps static members of `OPT` (`u_opt.h:84-173`), read directly by the solvers (`e_compon.h:42-46`, `u_sim_data.h:158`).
- **VACASK** keeps one `SimulatorOptions` struct with a defaulted `operator==` (`options.h:12-113`). It groups options by **what a change invalidates** (`options.cpp:356-406`):
  - exposed to OSDI models, so a change forces a full setup: tnom, temp, gmin, gdev, minr, scale, reltol, vntol, abstol, chgtol, fluxtol;
  - used in parameter expressions (`$temp`, `$tnom`, `$scale`): temp, tnom, scale;
  - changes the circuit topology: none;
  - changes the tolerance tables: tolmode, abstol, chgtol, vntol, fluxtol.

### 5.2 Defaults

| Option | Meaning | ngspice | Xyce | Gnucap | VACASK |
|---|---|---|---|---|---|
| `reltol` | relative convergence tolerance | 1e-3 (`cktntask.c:100`) | DC 1e-3, TRAN 1e-2 (`N_NLS_NOX_ParameterSet.C:163, 144`) | 1e-3 (`u_opt1.cc:37`) | 1e-3 (`options.cpp:56`) |
| `vntol` | absolute voltage tolerance | 1e-6 V (`cktntask.c:102`) | none; one weighted norm for all unknowns (`N_NLS_NOX_XyceTests.C:388-428`) | 1e-6 (`u_opt1.cc:39`) | 1e-6 (`options.cpp:58`) |
| `abstol` | absolute current tolerance | 1e-12 A (`cktntask.c:99`) | DC 1e-12, TRAN 1e-6 (`N_NLS_NOX_ParameterSet.C:162, 143`) | 1e-12 (`u_opt1.cc:38`) | 1e-12 (`options.cpp:57`) |
| `gmin` | junction conductance | 1e-12 S (`cktntask.c:95`) | 1e-12, in `DEVICE` (`N_DEV_DeviceOptions.C:90`) | 1e-12 (`u_opt1.cc:34`) | 1e-12 (`options.cpp:50`) |
| `chgtol` | charge tolerance (time step) | 1e-14 (`cktntask.c:101`) | 1e-12, in `DEVICE` (`N_DEV_DeviceOptions.C:89`) | 1e-14 (`u_opt1.cc:41`) | 1e-15 (`options.cpp:59`) |
| `trtol` | truncation-error factor | 7 (`cktntask.c:107`) | n/a | 7 (`u_opt1.cc:40`) | n/a |
| `itl1` | DC op iteration limit | 100 (`cktntask.c:110`) | MAXSTEP 200 (`N_NLS_NOX_ParameterSet.C:166`) | 100 (`u_opt1.cc:112`) | `op_itl` 100 (`options.cpp:132`) |
| `itl2` | DC sweep iteration limit | 50 (`cktntask.c:111`) | n/a | 50 (`u_opt1.cc:113`) | `op_itlcont` 50 (`options.cpp:133`) |
| `itl4` | transient iteration limit per step | 10 (`cktntask.c:109`) | MAXSTEP 20 (`N_NLS_NOX_ParameterSet.C:147`) | 20 (`u_opt1.cc:115`) | `tran_itl` 10 (`options.cpp:180`) |
| `temp`, `tnom` | temperatures | 300.15 K | 300.15 K | 27 °C | 27 °C |

### 5.3 Physics or numerics?

What each one does in ngspice:
- **`reltol` + `vntol`** judge voltage unknowns (`niconv.c:55-57`). **`reltol` + `abstol`** judge current unknowns (`niconv.c:68-69`) and each device's current check (`bjtconv.c:57`). **`chgtol` and `trtol`** control the time step (`cktterr.c:37-41, 69`). A tighter value gives the same circuit, solved more accurately.
- **`gmin`** is added to every junction's conductance and current in every load: `gben += CKTgmin; cben += CKTgmin*vbe` (`bjtload.c:460-461, 490-491`; `dioload.c:552-559`). It's the target of gmin stepping (`cktop.c:190-191`) and a `$simparam` for OSDI models (`osdiload.c:263-271`). A different gmin is a **different circuit**: a 1 TΩ resistor across every junction.
- **`temp` and `tnom`** are physics.

Two simulators draw this line and two don't:

| | Where gmin lives | Where temp and tnom live | Where the tolerances live |
|---|---|---|---|
| ngspice | one options table | the same table | the same table |
| Gnucap | `OPT`, mutated during gmin stepping (`s__solve.cc:103-118`) | `OPT` | `OPT` |
| **Xyce** | **`DEVICE`** | **`DEVICE`** | `NONLIN`, `TIMEINT` |
| **VACASK** | "exposed to OSDI" | "exposed to OSDI" and "parametrization" | "tolerances" (plus "exposed to OSDI") |

For us the line matters because of the engine. It re-checks a verdict with a tight-tolerance deck ("Reproduces at reltol 1e-9 within 7e-9 V", `engine_types.md` §9). That re-check means *the same circuit, solved more accurately*. If gmin sat in the struct the engine swaps, a tight run could silently compare a different circuit. So `SolverOptions` holds only numerics, and gmin goes with `Conditions`.

### 5.4 Per-analysis options

| | Per-analysis options? |
|---|---|
| ngspice | **No.** One set per task (`cktntask.c`), copied at every job. Analysis-specific needs are separate fields: `itl1` for DC op, `itl2` for DC sweep, `itl4` for transient (`tskdefs.h:37-39`) |
| Gnucap | **No.** Tolerances are global. Only the temperature and the time-step bounds are per analysis (`s_tr_set.cc:180-197`) |
| Xyce | **Partly.** Separate DC and transient Newton sets (`NONLIN` vs `NONLIN-TRAN`), and each analysis keeps its own copy of `TIMEINT` (`N_ANP_Transient.C:4318-4321`) |
| VACASK | **A snapshot.** Each analysis records the options in force when it was declared (`cmd.cpp:141-142`) and applies them to a copy while it runs (`an.cpp:133-145`), then restores the circuit's (`an.cpp:618-629`). There's no options syntax on the analysis line (`docs/cmd-analysis.md:7-13`) |

**For us:** one `SolverOptions` per job. ngspice, our export target, can't express anything finer anyway. If a need appears, follow ngspice's rule of separate fields per analysis (`itl1`/`itl2`/`itl4`) inside the one struct. If that's not enough, follow VACASK and give an `Analysis` an optional override.

### 5.5 Requested options vs effective options

The circuit can change ngspice's options behind the user's back. If the circuit has any XSPICE `A` device, ngspice lowers `trtol` to 1 (`cktdojob.c:77-91`). The same goes for diode IS, clamped to `epsmin` at setup (`diosetup.c:236-237`). So `Lowered.options` holds what was **requested**, and the simulator decides what it actually uses. An exporter writes the requested values.

---

## 6. Model storage and naming

| | How a model card is stored | Namespace | Model without a card | Models inside subcircuits |
|---|---|---|---|---|
| **ngspice** | One `GENmodel` per card (`gendefs.h:40-48`), created lazily when the first instance uses it (`inpgmod.c:379-380`). Instance parameters written on a model card become instance defaults (`inpgmod.c:158-168`, `gendefs.h:47`) | **Separate** tables: `MODnameHash` and `DEVnameHash` (`cktdefs.h:330-331`, `cktmcrt.c:41`, `cktcrte.c:66`) | A default model named after the device letter, `"R"` or `"D"` (`inp2r.c:186`, `inp2d.c:88`) | Renamed `x1:qn` (`subckt.c:1761`). Instances become `q.x1.q1` (`subckt.c:1137-1142`) |
| **Xyce** | A `ModelBlock` with name, type, level and location (`N_DEV_DeviceBlock.h:63-136`). One `Model` object shared by all its instances (`N_DEV_DeviceMaster.h:510-520`) | **Separate**, case-insensitive `modelMap_` and `instanceMap_` (`N_DEV_DeviceMaster.h:107-108`). A clash is only a warning (`:524-525`) | A default model per device | Looked up outward through parent contexts (`N_IO_CircuitContext.C:2804-2838`). A model with expression-valued parameters is copied per subcircuit instance under a prefixed name (`:2815-2826`) |
| **Gnucap** | A `MODEL_CARD` in the same `CARD_LIST` as the instances, with no duplicate check (`lang_spice.cc:830-838`). Identical commons are deduplicated (`e_compon.cc:296-305`) | **Shared** with instances | n/a | Looked up outward (`e_compon.cc:747-807`), and cloned per subcircuit instance (`e_cardlist.cc:459-461`) |
| **VACASK** | Device → models → instances (`devbase.h:87, 219`) | **Separate** `instanceMap` and `modelMap` (`circuit.h:551-558`) | n/a | The prefix `name:` for both instances and models (`devbase.cpp:112-119`, `osdidevice.cpp:45-47`). Definitions use `::` so they can't clash (`circuit.cpp:713-725`) |
| **XSPICE** | Code-model parameters live **only** on the model card. Instances share the model's parameter array by pointer (`mif_inp2.c:677-680`) | ngspice's | n/a | ngspice's |

**All four key models by card name.** A card is one model however many instances use it, and a second card with the same values is a second model. Three of the four keep models and instances in separate namespaces.

**Our current lowering departs from this in two places:**
- `ModelTable` merges cards **by value**. Its key is the numbers alone (`lower.rs:174-197, 199-221`), so `.model QA NPN bf=200` and `.model QB NPN bf=200` become one entry.
- The Deck already lost the names: `BjtSpec.model` is a copied `BjtModel` with no name (`spicy_netlist/src/devices/bjt.rs:11`, `netlist_models.rs:289-296`).

A merged entry has two names, so `CircuitNames.bjt_models[i]: String` can't be filled. The engine deck needs a name per model it writes (`engine_plan.md` §5.1).

**Recommendation:**
- Keep the card name on each Deck instance, and key the model tables by name.
- An instance that overrides a model parameter gets its own entry, named after the instance: ngspice's subcircuit rule (`x1:qn`) applied to overrides, for example `q1:QN`.
- The default model of a kind is named after the letter, as in ngspice (`"R"`, `"D"`, `"Q"`).
- Merging identical *override* models by value can stay: they have no card name to lose.

---

## 7. Names and origins for diagnostics

| | Where source locations live | Used in runtime diagnostics? |
|---|---|---|
| **ngspice** | Only on the input cards: `linenum`, `linenum_orig`, `linesource` (`inpdefs.h:77-80`). While parsing, a global current line (`inpdefs.h:101-103`, `inppas2.c:89`) | No. Instances and models keep only their name (`gendefs.h:22, 46`), so a temperature-step warning names the model only: "BJT model %s, parameter fc limited to 0.9999" (`bjttemp.c:60-66`) |
| **Xyce** | `NetlistLocation` (file number, line; `N_UTL_NetlistLocation.C:61-84`) on every model and instance (`N_DEV_DeviceEntity.h:294-321`) | Yes. `ParamWarning` and `ParamError` print it (`N_DEV_Message.C:120-131`) |
| **Gnucap** | None. Only a label (`e_base.h:40`); syntax errors echo the line text without file or line (`ap_error.cc:53-66`) | No |
| **VACASK** | A 16-byte `Loc` (file stack, file, line, column; `sourceloc.h:13-33`) on parse nodes. Runtime models and instances expose `location()` (`devbase.h:203-204, 345-346`) | Yes, mostly. But errors raised inside OSDI setup name the instance only (`osdidevice.cpp:583-592`) |

**For us:** the side table of `circuit.md` §4.9 is the right place. It's rust-analyzer's source map, and it holds what Xyce and VACASK attach to objects. Two additions:
- **Models get origins too** (the `.model` card, or the language part that produced them). The first runtime messages our derive step will print are model messages ("fc limited", "IS below epsmin"), and neither ngspice nor VACASK's OSDI path can point at a line for those today.
- An origin is a small handle: a `Span` into a source, or for the language path a `FlatDesign` device id that leads to the span.

---

## 8. Repeated runs: what reruns when a value changes

| Change | ngspice | Xyce | Gnucap | VACASK | Proposed for us |
|---|---|---|---|---|---|
| Circuit temperature | Everything, on the next `run` (`cktdojob.c:163-171`). In `.dc temp`, only `CKTtemp` (`dctrcurv.c:212-218`) | `processParams` → `updateTemperature` on every device (`N_DEV_DeviceMgr.C:5512-5591`) | Nothing; devices read it live | Full setup (`cirparams.cpp:417-428`) | `Derived` only |
| `tnom` | Everything, on the next run | Fixed when models are built (`N_DEV_BJT.C:4008-4009`) | `precalc_first` (`e_model.cc:101-105`) | Full setup | `Derived` only |
| Model parameter | `altermod` reruns `CKTtemp` if a run already happened (`spiceif.c:976-990`); the next run redoes everything | That model's `processParams`, plus every instance's (`N_DEV_DeviceMgr.C:6490-6640`) | Re-entering a `.model` card rebuilds the whole circuit (`e_model.cc:33-53`, `u_sim_data.cc:336-356`) | Setup of that model and its instances (`osdimodel.cpp:152-158, 198-208`) | `Derived` only; a new plan only if a structural parameter changes the structure key (`circuit.md` §4.3) |
| Instance parameter | `alter`, then everything on the next run; a `.dc` of a resistor reruns `CKTtemp` (`dctrcurv.c:172`) | That instance's `processParams` | `precalc_last` | Setup of that instance (`osdiinstance.cpp:435-441`) | `Derived` only |
| `.param` | `alterparam` edits the deck text; `mc_source` re-parses it (`inp.c:1817-1823`) | `processParams` of every dependent entity (`N_DEV_DeviceMgr.C:6490-6640`) | Re-evaluated next run (`c_param.cc:32-39`) | Re-elaboration of the changed subtrees (`cirparams.cpp:366-380`) | Above `spicy_circuit`: the binding writes `Params` |
| Tolerance option | Copied at the next job (`cktdojob.c:66-72`) | Not steppable (`STEP_Command.tex:76-94`) | Read live | Tolerance tables rebuilt (`cirparams.cpp:482-493`), plus a full setup, because OSDI models can read tolerances | Nothing; the Newton loop reads it |
| gmin | Copied at the next job; read in every load | not steppable as an option | read live | Full setup | Nothing; the load reads it |

Two cautionary examples, for the speed goal:
- **Xyce's embedded sampling** reruns `processParams` and `updateTemperature` for every sample on every Newton load (`N_LOA_ESLoader.C:211-224`). That's the cost of one mutable parameter set, already cited in `circuit.md` §4.1.
- **Gnucap decides topology only at the first expansion** (`mg_out_dev.cc:316-344`), so a parameter change that should drop RS = 0 is never re-decided. `circuit.md` §4.3's structure key avoids this.

**For the future OSDI kind:** VACASK and ngspice pass `tnom`, `gmin`, `reltol`, `vntol` and `abstol` to compiled models as `$simparam` (`osdidevice.cpp:489-512, 538-548`; `osdiload.c:250-279`). A compiled model's derive step therefore depends on `SolverOptions` and `Conditions.gmin` as well as temperature. That's a note for the OSDI kind; it doesn't change built-in devices.

---

## 9. XSPICE

- **Code models** keep their parameters on the model card only, and instances share them by pointer (`mif_inp2.c:677-680`; `mifdefs.h:67-68, 113-114`). It's the model-table pattern taken to its limit: an instance has connections and nothing else.
- **Temperature** is not precomputed. The code-model generator writes `DEVtemperature = NULL` (`writ_ifs.c:1089`), and every load passes the circuit temperature in °C (`mifload.c:221`, `mifcmdat.h:334`). A code model that depends on temperature pays for it on every call, as Gnucap does.
- **Digital (event-driven) nodes** are a separate node table with a type index per node (`evt.h:89-100, 109-114`). If digital nodes ever reach `spicy_circuit` (`research/next_mixed_signal.md`), `Circuit.node_count` alone won't describe them: nodes will need a kind, analog or event.
- **XSPICE adds its own options** to the same table: `maxopalter`, `maxevtiter`, `ramptime`, `convlimit` (`cktsopt.c:259-266`). It also changes `trtol` when `A` devices are present (§5.5).

---

## 10. OSDI and VACASK: how a modern design splits it

| Concern | Where it lives | Citation |
|---|---|---|
| Circuit temperature | One `double` argument of `setup_instance`, in kelvin. Never passed per evaluation | `osdi_0_3.h:178-180`; VACASK `osdiinstance.cpp:294-295` |
| Simulator globals | A named table read by `$simparam`, given to setup and to every eval. ngspice fills `iniLim, gmin, gdev, tnom (°C), simulatorVersion, sourceScaleFactor, epsmin, reltol, vntol, abstol`; VACASK a superset | `osdi_0_3.h:74-88`; `osdiload.c:250-279`; `osdidevice.cpp:489-512` |
| TNOM | Usually a model parameter with its own default (`parameter real Tnom = 27`). Models ported from SPICE fall back to `$simparam("tnom")` | `devices/diode.va:33`; `devices/spice/bsim4v8.va:3377-3379` |
| Parameter precedence | Instance given, else model given, else default. The model struct holds instance-parameter defaults too | `setup.rs:242-259`; `model_data.rs:26-44` |
| What's computed once vs per eval | Code that doesn't depend on the operating point runs in `setup_instance` and is cached. `$simparam` counts as operating-point dependent, so it's read on every eval | `init.rs:28-36`; `callbacks.rs:153-163` |
| Options vs everything else | One options struct, grouped by what a change invalidates. Circuit flags record which group changed | `options.cpp:356-406`; `circuit.h:179-194`; `cirparams.cpp:255-510` |

VACASK's grouping is the clearest statement of the split we want:
- temperature changes the device setup (`Derived`);
- tolerances change only the solver;
- nothing changes the topology except structural parameters.

---

## 11. Recommended shape

```rust
pub struct Lowered {
    pub circuit: Circuit,
    pub params: Params,
    pub conditions: Conditions,      // new
    pub names: CircuitNames,
    pub analyses: Vec<Analysis>,
    pub options: SolverOptions,      // new
}

/// Circuit-wide inputs to the device equations. One per run: the engine's
/// `temp` knob writes `temp`. The derive step reads it; the Newton loop doesn't.
pub struct Conditions {
    /// Circuit temperature (K). Default 300.15 (27 °C), ngspice `cktntask.c:128`.
    /// Not simulated yet.
    pub temp: f64,
    /// Parameter-measurement temperature (K) for models whose `tnom` is `None`.
    /// Default 300.15, ngspice `cktntask.c:129`. Not simulated yet.
    pub tnom: f64,
    // Later, when the simulator adds junction gmin (not before; nothing reads it):
    // pub gmin: f64,   // S. Default 1e-12, ngspice `cktntask.c:95`.
}

/// How accurately to solve. One per job (the engine's normal vs tight decks).
/// Read only by the Newton loop, and later by time-step control.
pub struct SolverOptions {
    /// Default 1e-3 (`cktntask.c:100`).
    pub reltol: f64,
    /// Absolute tolerance for voltage unknowns (V). Default 1e-6 (`cktntask.c:102`).
    pub vntol: f64,
    /// Absolute tolerance for current unknowns (A). Default 1e-12 (`cktntask.c:99`).
    pub abstol: f64,
    // With the simulator wiring (it reads a hard-coded 50 today):
    //   dc_iterations: u32 = 100 (itl1), sweep_iterations: u32 = 50 (itl2),
    //   tran_iterations: u32 = 10 (itl4)          (`cktntask.c:109-111`)
    // With adaptive time steps: chgtol = 1e-14, trtol = 7 (`cktntask.c:101, 107`)
    // With gmin/source stepping: gminsteps, srcsteps, gshunt
}

pub struct BjtModel {
    pub polarity: Polarity, pub is: f64, pub bf: f64, pub br: f64, pub nf: f64, pub nr: f64,
    /// β temperature exponent. Default 0 (`bjtsetup.c:156-158`). Not simulated yet.
    pub xtb: f64,
    /// IS temperature exponent. Default 3 (`bjtsetup.c:162-164`). Not simulated yet.
    pub xti: f64,
    /// Energy gap for IS(T) (eV). Default 1.11 (`bjtsetup.c:159-161`). Not simulated yet.
    pub eg: f64,
    /// Parameter-measurement temperature (K); None = `Conditions.tnom`
    /// (`bjttemp.c:42`). Not simulated yet.
    pub tnom: Option<f64>,
}
pub struct BjtParams { pub area: f64, pub m: f64, pub off: bool, pub ic_vbe: f64, pub ic_vce: f64,
    pub temperature: DeviceTemperature,          // new: ngspice `bjt.c:76-77`
}
pub struct DiodeModel { pub is: f64, pub n: f64, pub rs: f64,
    pub xti: f64,          // 3    (`diosetup.c:153-155`)
    pub eg: f64,           // 1.11 (`diosetup.c:140-146`)
    pub tnom: Option<f64>, // None (`diosetup.c:240-242`)
}
pub struct ResistorModel  { /* tc1, tc2, default_width, default_length, */ pub tnom: Option<f64> } // `ressetup.c:29`
pub struct CapacitorModel { /* tc1, tc2, */ pub tnom: Option<f64> }                               // `capsetup.c:58`
pub struct InductorModel  { /* tc1, tc2, */ pub tnom: Option<f64> }                               // `indsetup.c:35`

pub struct CircuitNames {
    pub title: String,
    pub nodes: Vec<String>,
    pub resistors: Vec<String>, /* … one per device kind, as today … */
    // new: one per model table, indexed by model id. The card's name; `q1:QN`
    // for an instance's own model; the kind's letter ("Q") for the default model.
    pub resistor_models: Vec<String>, pub capacitor_models: Vec<String>,
    pub inductor_models: Vec<String>, pub diode_models: Vec<String>, pub bjt_models: Vec<String>,
    // later: origins for every device *and* model (a Span, or a FlatDesign id)
}
```

**Why `tnom` stays `Option`, and why it isn't resolved at lowering.** Resolving it at lowering would give one fewer moving part: `tnom: f64` on every model, and no `Conditions.tnom`. But:
- ngspice, Gnucap and VACASK all resolve the default at run time from the circuit's value (`bjttemp.c:42`, `e_model.cc:101-105`, VACASK's forced setup on a `tnom` change).
- A future OSDI model needs a circuit-level `tnom` anyway, for `$simparam` (`osdiload.c:271`).
- Keeping `None` makes the export faithful. A card without `TNOM` plus `.options tnom=` round-trips to the same `Params`, and the "same `Circuit + Params + Conditions`" check (`engine_plan.md` §5.4) doesn't have to reason about where each number came from.

The cost is one `unwrap_or` per model per run, in the derive step. It still fits `params.rs`'s rule: `Option` only where the default depends on another value that can change per run.

**What each consumer reads:**

| | Conditions | SolverOptions | New model fields | Model names |
|---|---|---|---|---|
| Our simulator | the derive step (with temperature support; `Vt` is fixed at 25.85 mV until then) | the Newton loop | the derive step | messages |
| ngspice exporter | `.temp <°C>` (not `option temp`, §2), `.options tnom=` | `.options reltol= vntol= abstol=` | `XTB= XTI= EG= TNOM=` on every card (TNOM only when `Some`) | `.model` names, the `QM_<path>` rule |
| Engine | the binding writes `temp` per run | picks normal or tight per job | knobs write `bf`, etc. | reports, deck names |

**Deliberately left out now:**
- `gmin`: nothing reads it until our simulator adds junction gmin; ngspice's default is used meanwhile.
- The `T*1`/`T*2`/`TLEV` family and the resistance tempcos: they come with the resistances.
- Per-analysis options.
- A `.dc temp` sweep (ngspice `dctrcurv.c:212-218`, Gnucap's `.op` temperature sweep). It would be a new `DcSweep` target, not a change to `Conditions`.

---

## 12. Worked example: one engine run at −10 °C

The CE amplifier's transistor uses the D-A default model (`engine_plan.md` §5.4): IS 1e-14 A, BF 200 (the β knob at nominal), XTB 1.5, XTI 3, EG 1.11, TNOM 25 °C. Engine run 17 sets `temp = −10 °C`.

```
Lowered (nominal)   conditions { temp: 300.15, tnom: 300.15 }         options { reltol: 1e-3, vntol: 1e-6, abstol: 1e-12 }
                    bjt_models[0] { is: 1e-14, bf: 200, xtb: 1.5, xti: 3, eg: 1.11, tnom: Some(298.15) }   name "q1:Npn"
                    bjts[0] { area: 1, m: 1, temperature: Offset(0) }
run 17              binding: conditions.temp = 263.15                 (nothing else changes; the plan is reused)
Derived             device T  = 263.15 + 0 = 263.15 K                 (Offset(0): follows the run)
                    model TNOM = Some(298.15) → 298.15 K              (None would give conditions.tnom)
                    Vt   = k·T/q = 22.68 mV
                    IS(T) = IS · exp((T/TNOM − 1)·EG/Vt + XTI·ln(T/TNOM)) = 2.197e-17 A       (bjttemp.c:163-172)
                    BF(T) = BF · (T/TNOM)^XTB = 200 · 0.8292 = 165.8                         (bjttemp.c:233-241)
Newton              reads Derived + options; VBE at 1.2 mA = Vt·ln(Ic/IS(T)) = 0.717 V (0.655 V at 25 °C)
```

Numbers computed with ngspice's constants (`const.h:32, 37`).

**Why TNOM has to travel with the model:** with `tnom: None`, the same run would use `Conditions.tnom` = 27 °C, and IS(−10 °C) would be 1.615e-17 A instead of 2.197e-17 A. That's 26% less current at the same VBE, or 7 mV more VBE at the same current. The D-A decision exists because ngspice's default (27 °C) isn't the datasheet temperature (25 °C).

**What the ngspice export writes for this run:**

```
.temp -10
.options tnom=27 reltol=1e-3 vntol=1e-6 abstol=1e-12
.model QM_q1 NPN IS=1e-14 BF=200 XTB=1.5 XTI=3 EG=1.11 TNOM=25
```

ngspice then runs the same `BJTtemp` arithmetic. That's why `Params` must keep the raw values: the export can't write `Derived`.

---

## 13. Questions for the user

1. **Model tables keyed by card name** (§6): this stops merging differently named cards that have identical values. Every simulator works this way, and it's needed for model names. *Recommended: yes.*
2. **`gmin`** (§5.3): record its place (`Conditions`) now, and add the field when our simulator applies junction gmin. *Recommended: later.*
3. **Iteration limits in `SolverOptions`** (§11): add them in the same change that makes the Newton loop read `SolverOptions`. That change also splits today's single `abs_tol = 1e-6` into `vntol` for voltages and `abstol` for currents (`niconv.c:55-69`), which tightens current convergence from 1e-6 A to 1e-12 A. *Recommended: together, as one reviewed change, with the simulator's temperature/tolerance milestone.*
