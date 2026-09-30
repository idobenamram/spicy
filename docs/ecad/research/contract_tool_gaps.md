# What Today's Tools Can't Do: A Gap Map for Contracts, Benches and Hierarchy

> Research report · 2026-09-29
> Question: for every tool a circuit designer might use to **specify, set up, verify and reuse** circuit behavior, what does it concretely fail to do? Which of those failures does our design (engine.md v4, engine_plan.md, language.md v0.1, and the round-4 research) already fix, partly fix, or not fix? Which ones must we keep a path to?
> Builds on, and does not repeat: `contract_references.md` (how tools *express* setups, specs and hierarchy), `contract_hierarchy.md` (when reuse is sound, measured on ngspice), `next_synthesis.md` (what post-MVP circuits need), `engine_method_redteam.md` §3 and §6 (what industry's worst-case and statistics methods do), `language_editor_mapping.md` §4.6 and §6.3 (drawings vs text, version control), `synthesis.md` (round 1).
> Scratch notes, with every quote and URL: `/root/.claude/jobs/443154a8/tmp/engine4/gaps/` (`ic_tools.md`, `board_tools.md`, `code_ai_tools.md`, `formal_practice.md`).

**How sure is each piece of evidence?** Every item carries two tags.
- **Source tag.** **[F]** means the page or PDF was fetched and read. Some web quotes went through a summarizing fetch, so the wording may be lightly paraphrased. PDF quotes are exact. **[S]** means search-result text only; the page was blocked (Cadence community, TI E2E, ADI EngineerZone and EEVblog all returned 403). **[R]** means it comes from an earlier report in this repo, which carries its own tags. **[M]** means from memory, not rechecked.
- **Kind tag.** **doc** is a vendor manual, a standard, a paper or a tracker description. **vendor** is a vendor's own claim about a problem, usually while selling the fix. Read it as proof that the problem exists, not as a neutral measurement. **user** is a practitioner's opinion.

---

## Summary

Every tool we looked at can *run* a circuit. Almost none can say, in one place and in a form a machine can check, **what the circuit must do, under which conditions, and whether that was shown for every condition**. The gaps fall into eight groups:

1. **Specs have no home.** Board simulators have no spec field at all: `.meas` output is read by eye. Cadence keeps the limit in two places (an ADE output and a Verifier requirement). The datasheet and the worst-case analysis (WCCA) live in documents and spreadsheets that the Aerospace Corporation says often "do not reflect as-built hardware".
2. **Benches don't travel.** Copying a Cadence maestro view to a new design loses the setup. KiCad can only simulate a whole schematic. Every tool except SIMPLIS DVM makes test plans a per-project chore.
3. **"Worst case" usually isn't.** PSpice, SIMetrix, Multisim and Micro-Cap all make one jump along the nominal sensitivity signs, and their own manuals say it's "not guaranteed". LTspice's `wc()` idiom enumerates the vertices but can't see inside the box. IC tools sample, fit a surrogate, and say how confident they are, not what they proved.
4. **Statistics rest on defaults nobody checks.** Altium runs 5 uniform runs at 10%. PSpice can't correlate DEV variations. Sigma conventions differ between tools.
5. **Hierarchy is trust, not checking.** "Black boxes that are presumed to be pre-verified" (Cadence's own words). Loading is lost in real-number models. The behavioral model and the schematic drift apart, with "no equivalence checks" between them.
6. **Models don't say what they leave out.** Encrypted models are locked to one simulator, ngspice can't read them, and output impedance "is often not modeled correctly" (TI).
7. **Nothing fits git, and nothing runs in CI.** Maestro views and OrCAD designs are binary, KiCad has no simulation command in its CLI, and Keysight says outright that IC data needs locking, not merging.
8. **AI tools check single points.** Flux's simulation launch mentions no tolerance, corners or temperature. Its design review applies nominal rules. The vendor warns about "confident-sounding nonsense".
9. **Faults are hand-drawn switches.** A dropped cable, a reversed connector, a short, hot-plug or brown-out is a timed switch wired in by hand, one fault per edit, at nominal. IC fault simulators (Legato, Tessent DefectSim, CustomFault) automate *manufacturing-defect* campaigns, not operating events. Simscape has triggered faults but no worst-case verdicts. Board FMEA is a spreadsheet of judged effects, not linked to the design.

For board-level designers, our target, the gaps that hurt most often are 1, 2, 3, 4, 6 and 9. Groups 5 and 7 hurt IC teams more today, but board teams meet them as soon as blocks are reused. Our design already answers the core of 1, 3 and 4: specs live in the contract, every corner is simulated, and verdicts say what they rest on. It partly answers 2, 5 and 6: benches, interface knobs and the coverage map are designed but not built. It says almost nothing yet about **faults and operating events, requirement traceability, analog coverage, datasheet revision tracking, behavioral-model validation, or CI result diffs**. Those are the paths we must keep open (§5).

---

## 1. Tool by tool

Each table lists limitations only. What each tool *does* is in `contract_references.md` §3–§14 and `engine_method_redteam.md` §3.1.

### 1.1 Cadence Virtuoso ADE Explorer / Assembler / Verifier, Spectre

| Limitation | Evidence | Tags |
|---|---|---|
| Before Verifier, specs were tracked by hand in documents | "Manual attempts to do so often lead to mistakes since they rely on constantly updated documents." [semiengineering.com/plan-based-analog-verification-methodology-2](https://semiengineering.com/plan-based-analog-verification-methodology-2/) | [F] vendor |
| With Verifier, each spec exists twice: a requirement "mapped" to an output in a maestro test | [Virtuoso ADE Verifier page](https://www.cadence.com/en_US/home/tools/custom-ic-analog-rf-design/circuit-design/virtuoso-ade-verifier.html). How drift between the two is caught was **not verified** | [S] doc |
| Analog verification is ad hoc, and traceability is forced from outside (ISO 26262) | "much less structured, with various ad hoc methods"; traceability must keep "the link between tests and design specifications". [semiwiki 5658](https://semiwiki.com/eda/cadence/5658-analog-design-verification-traceability-is-required/) | [F] vendor |
| No verification planning or coverage for analog | "directed tests run over sweeps, corners, and Monte Carlo … little or no support for verification planning or coverage metrics". Cadence mixed-signal white paper, 2012 ([mirror](https://www.kluniversity.in/alumni/pdfs/70.pdf)) | [F] vendor |
| Benches don't move to another design | "copy of a maestro view and changing design cause all the setup to be lost" (IC23.1, 2025), [forum 65435](https://community.cadence.com/cadence_technology_forums/f/custom-ic-design/65435/copy-of-a-maestro-view-and-chnaging-design-cause-all-the-setup-to-be-lost). A user asks how to copy a maestro view "without the copy retaining the links to the first schematic", [forum 52660](https://community.cadence.com/cadence_technology_forums/f/custom-ic-design/52660/copying-maestro-view-simulation-setup-to-another-testbench) | [S] user |
| Version control doesn't handle maestro views | DesignSync "works fine for schematics and symbols, but not for maestro", [forum 63122](https://community.cadence.com/cadence_technology_forums/f/custom-ic-design/63122/synchronicity-vs-maestro-view). "all the circuit files are binary files, so git features like merging don't work at all", [blog, 2011](http://randomengineeringnotes.blogspot.com/2011/04/revision-control-for-cadence-virtuoso.html) | [S]/[F] user |
| Saved worst-case corners go stale | "The worst sample from each output is saved as a corner to replace Monte Carlo samples in future design iterations." [Cadence blog](https://community.cadence.com/cadence_blogs_8/b/cic/posts/what-s-the-worst-that-could-happen). A snapshot per output, out of date once the design changes (our reading) | [S] vendor |
| Result data is huge | 600 corners × 500 MB of psfxl from one "save all" corner sweep, [forum 42934](https://community.cadence.com/cadence_technology_forums/f/custom-ic-design/42934/is-there-a-way-to-eliminate-undesired-data-from-a-psfxl-file-to-save-area-in-a-maestro-output) | [S] user |
| Hierarchy by presumption | "no longer adequate to bolt together … 'black boxes' that are presumed to be pre-verified"; pin, polarity and power-domain errors are "found only in lengthy analog simulation runs, if they are found at all". Cadence 2012 white paper | [F] vendor |
| WCD and K-Sigma aren't proofs | Covered in `engine_method_redteam.md` §3.2: the vendor recommends brute Monte Carlo (MC) for nonlinear behavior | [R] |
| AI features: trust is the open issue | "Sign-off is all about trust … trust built up over time" (Rob Knoth, Cadence), [semiengineering.com/the-limits-of-ais-role-in-eda-tools](https://semiengineering.com/the-limits-of-ais-role-in-eda-tools/). No documented limits were found | [F] vendor |

### 1.2 Siemens Solido

| Limitation | Evidence | Tags |
|---|---|---|
| Corner explosion and guessing | "1215 corners takes too long to run, guessing which are worst-case is error prone, no standardized methodology"; foundry corners "are either overly conservative … or overly optimistic". [semiwiki 2780](https://semiwiki.com/eda/solido/2780-process-variation-is-a-yield-killer/) | [F] vendor |
| PVT and statistics checked separately miss interactions | "simulating them independently can lead to missed critical interaction effects and failures". [design-reuse 42615](https://www.design-reuse.com/news/42615/solido-pvtmc-verifier.html) | [F] vendor |
| Reduced MC still falls short on tails | A bandgap needed "108,000 simulations for all 36 PVT corners" and still had "limited accuracy" from "long tail or non-gaussian characteristics". [semiwiki 328065](https://semiwiki.com/eda/siemens-eda/328065-using-ml-for-statistical-circuit-verification/) | [F] vendor |
| Fast PVT's worst case is a surrogate model's confidence, not a proof | Full factorial of 3375 corners takes "13.5 days even when on 10 parallel cores"; the model reports "its confidence in that prediction". Solido book chapter | [S] doc |
| Importance sampling fails with many variables or several failure regions | [DATE 2019 paper 0044](https://past.date-conference.com/proceedings-archive/2019/pdf/0044.pdf) | [S] doc |
| Trust | "transparency and visibility into how it came up with the solution, and therefore make it verifiable" (Amit Gupta, Siemens) | [F] vendor |

### 1.3 Synopsys HSPICE / PrimeSim / PrimeWave / Custom Compiler / ASO.ai

| Limitation | Evidence | Tags |
|---|---|---|
| Corners don't catch variation, and per-block sigma is hard | "Running multiple corner (PVT) simulations is never going to catch variation problems"; "Tools have a difficult time applying different sigma values to different parts of the circuit." [semiengineering.com/increase-in-analog-problems](https://semiengineering.com/increase-in-analog-problems/) | [F] user (tool vendor) |
| Sweep composition limits, so decks get split | HSPICE "does not allow multiple sweeps in the same transient analysis", so users fall back on `.ALTER`. US patent 8204730 describes HSPICE use | [S] doc |
| Regressions are user Tcl scripts, not declared specs | PrimeWave: "Tcl-based scripting … regressions across thousands of corners" ([datasheet](https://www.synopsys.com/content/dam/synopsys/implementation&signoff/datasheets/primewave-ds.pdf)) | [S] doc |
| All-at-3σ is "overly pessimistic" (Synopsys's own guide) | `engine_method_redteam.md` §3.2 | [R] |
| ASO.ai has no public limits, only benefit claims | [synopsys.com/ai/ai-powered-eda/aso-ai.html](https://www.synopsys.com/ai/ai-powered-eda/aso-ai.html) | [S] vendor |
| Custom Compiler: no user complaints found | — | not verified |

### 1.4 Keysight ADS

| Limitation | Evidence | Tags |
|---|---|---|
| Yield needs many trials, and the optimizer's yield differs from a separate yield run | "N = 1600 trials" for ±2% at 80% yield and 95.4% confidence; the yield-optimization estimate "often differs from that for a single yield analysis". [ADS 2011 optstat.pdf](https://edadownload.software.keysight.com/eedl/ads/2011_01/pdf/optstat.pdf) | [F] doc |
| The user picks a distribution per variable, and nothing checks it | Same manual: Gaussian, Uniform, Discrete, Lognormal | [F] doc |
| Electrothermal excludes the statistics | "Electrothermal does not support batch simulation, Monte Carlo, tuning, Optimization." [ADS 2023 release notes](https://docs.keysight.com/display/engdocads/ADS+2023+Release+Notes) | [S] doc |
| Design data needs locking, not merging | Schematics and layouts can't be auto-merged; files run "from hundreds of megabytes to gigabytes"; flows use "check-out/check-in with locking, rather than merge-based workflows". [Keysight blog](https://www.keysight.com/blogs/en/tech/sim-des/optimizing-ic-design-data-management) | [F] vendor |

### 1.5 PSpice / OrCAD

| Limitation | Evidence | Tags |
|---|---|---|
| One analysis, one output and one function per `.MC`; runs capped at 2,000 (400 in Probe); tolerances only from model DEV/LOT | [PSpice Reference Guide 16.5](https://home.agh.edu.pl/~godek/pspcref.pdf) | [F] doc |
| `.WCASE` varies one parameter per run, then does one combined run; you can run `.MC` or `.WCASE`, "but not both" | Same | [F] doc |
| "Worst case" is guaranteed only if the response is monotonic | "true worst-case results when the collating function is monotonic … otherwise, there is no guarantee". [User Guide ch. 13](https://resources.pcb.cadence.com/pspiceuserguide/13-monte-carlo-and-sensitivity-worst-case-analyses) | [S] doc |
| No correlated variation | "PSpice does not support correlated DEV variations in Monte Carlo analysis." Reference Guide | [F] doc |
| Stress (Smoke) runs at nominal only; op-amp tolerances silently ignored without the Advanced Analysis licence | `engine_method_redteam.md` §3.3 items 5–6 | [R] |
| PSpice for TI caps third-party models | "maximum of 3 signals at a time" with an imported third-party model. [Hackaday, 2020](https://hackaday.com/2020/09/20/ti-and-cadence-make-pspice-free/) | [F] doc (reported) |
| Binary design files | OpenOrCadParser parses "OrCAD Capture binary file formats" by reverse engineering. [github.com/Werni2A/OpenOrCadParser](https://github.com/Werni2A/OpenOrCadParser) | [F] doc |

### 1.6 LTspice

| Limitation | Evidence | Tags |
|---|---|---|
| Worst case is a hand-written idiom, 2^N + 1 runs, read by eye from the error log | `.func wc(nom,tol,index) …`; results through "SPICE Error Log (Ctrl-L) … Plot .step'ed .meas data". [ADI article](https://www.analog.com/en/resources/technical-articles/ltspice-worst-case-circuit-analysis-with-minimal-simulations-runs.html) ([mirror](https://embeddedcomputing.com/technology/software-and-os/simulation-modeling-tools/getting-the-worst-case-circuit-analysis-with-a-minimal-number-of-ltspice-simulation-runs)). No monotonicity caveat, and nothing inside the box | [F] doc |
| `.step` nests at most three levels | "Step sweeps may be nested up to three levels deep." [ltwiki](https://ltwiki.org/LTspiceHelp/LTspiceHelp/_STEP_Parameter_sweeps.htm) | [F] doc |
| Automation lives outside the tool | PyLTSpice exists to run "batch mode without having to open the LTSpice GUI" and to "overcome the limitation of only stepping 3 parameters". It parses `.MEAS` out of log files. [github.com/nunobrum/PyLTSpice](https://github.com/nunobrum/PyLTSpice) | [F] doc |
| Vendor models: TI won't support LTspice | "LTSpice EULA does not allow TI … to assist"; models are "only … tested in PSpice or TINA-TI". TI E2E thread 546520 | [S] vendor |
| Encrypted models can't be inspected when they fail | An onsemi SiC MOSFET gives "Time step too small", and the encrypted model can't be opened. [AllAboutCircuits](https://forum.allaboutcircuits.com/threads/ltspice-sic-mosfet-simulation-convergence-problem.180228/) | [F] user |
| UI | "arcane text commands on the diagram". [HN 22011994](https://news.ycombinator.com/item?id=22011994) | [F] user |

### 1.7 ngspice, Xyce, Qucs-S

| Limitation | Evidence | Tags |
|---|---|---|
| ngspice can't use encrypted vendor models | "Encrypted models cannot be used in ngspice." (Holger Vogt, developer). [sourceforge](https://sourceforge.net/p/ngspice/discussion/127605/thread/d8423f0629/) | [F] doc |
| ngspice has no MC statement; MC is a script | "The ngspice scripting language may be used to run Monte-Carlo simulations". [manual](https://nmg.gitlab.io/ngspice-manual/statisticalcircuitanalysis/monte-carlosimulation.html) | [F] doc |
| Hand-written MC scripts go wrong | A user saw spreads "very much greater than those from the book"; the cause was a missing `reset` and a misread sigma. [sourceforge](https://sourceforge.net/p/ngspice/discussion/133842/thread/381bf1f6/) | [F] user |
| Xyce is not netlist-compatible with PSpice or LTspice | "primarily compatible with basic SPICE3F5 syntax … some PSpice compatibility"; translating needs XDM. [Xyce FAQ](https://xyce.sandia.gov/documentation-tutorials/frequently-asked-questions/). Xyce *does* have sampling UQ (`.SAMPLING`) | [F] doc; [M] |
| Qucs-S had no MC until 2025 | "Monte Carlo simulation is only possible using a Nutmeg script." [qucs_s #1476](https://github.com/ra3xdh/qucs_s/issues/1476); closed after PRs #1721 and #1727 | [F] doc |

### 1.8 KiCad's simulator

| Limitation | Evidence | Tags |
|---|---|---|
| No MC, tolerance or worst-case GUI; measurements have no limit field | None of "Monte", "tolerance" or "worst" appears in the 10.0 schematic manual; the built-in measures are Min/Max/RMS/P-P/Integral/Fourier. [docs.kicad.org 10.0](https://docs.kicad.org/10.0/en/eeschema/eeschema.html) | [F] doc |
| MC is an open request; libraries with MC data are refused | [#17641](https://gitlab.com/kicad/code/kicad/-/issues/17641); "if the library containing the model has any Monte Carlo information, KiCad will refuse to use it", [#17325](https://gitlab.com/kicad/code/kicad/-/issues/17325) | [F] doc |
| No block-level simulation | [#6329](https://gitlab.com/kicad/code/kicad/-/issues/6329) "Simulate subsheets only"; "Simulation can be run only on the whole schematic"; the workaround is "one project dedicated for simulation". [forum](https://forum.kicad.info/t/spice-simulation-for-individual-hierarchical-sheets/43068) | [F] doc, user |
| No simulation from the CLI, so no CI | kicad-cli has ERC, DRC and exports, but no simulate. [CLI docs](https://docs.kicad.org/10.0/en/cli/cli.html); [#4561](https://gitlab.com/kicad/code/kicad/-/work_items/4561) still has "Run the simulation" unchecked | [F] doc |
| Fragile results | Shipped demos broke in 8.0.0 ([#17389](https://gitlab.com/kicad/code/kicad/-/issues/17389)); "measurements just disappear" on re-run ([#18326](https://gitlab.com/kicad/code/kicad/-/issues/18326)); `.meas` results only in the console ([forum](https://forum.kicad.info/t/simulation-measuring-with-meas/49905)) | [F] doc, user |

### 1.9 SIMetrix/SIMPLIS, Altium

| Limitation | Evidence | Tags |
|---|---|---|
| SIMetrix worst case not guaranteed; MC misses extremes | "worst-case analysis is not guaranteed to locate the worst possible result. The algorithm assumes that the relationship … is linear. This is almost never the case in practice." MC "rarely locates the extremes". [SIMetrix 8.5 help](https://help.simetrix.co.uk/8.5/simetrix/simulator_reference/topics/montecarloanalysis_overview.htm) | [F] doc |
| SIMPLIS DVM is the closest spec-driven tool, but it's a paid add-on with no tolerance-aware verdicts | It "compare[s] the measured results with design performance specifications, and generate[s] a … test report"; "Optional feature for all products". [simetrix.co.uk/products/dvm.html](https://www.simetrix.co.uk/products/dvm.html); see also `engine_method_redteam.md` §3.3 item 7 | [F] doc; [R] |
| Altium: subcircuits are not varied; Sensitivity disables the sweeps; MC defaults are 5 runs, uniform, 10% | "subcircuit data is not varied during the analysis". [Altium docs](https://www.altium.com/documentation/altium-designer/circuit-simulation/configuring-running) | [F] doc |
| Altium: no user complaints found | — | not verified |

### 1.10 Code-first and AI tools: Flux, atopile, PolymorphicBlocks, tscircuit, SKiDL, JITX

| Tool | Limitation | Evidence | Tags |
|---|---|---|---|
| **Flux** | The AI SPICE launch (March 2026) shows simple AC and transient runs and never mentions MC, worst case, tolerance or temperature. It warns users to ask "whether it's using a full manufacturer model, a behavioral approximation, or an ideal stand-in" | [flux.ai blog: simulate circuits with a prompt](https://www.flux.ai/p/blog/simulate-circuits-with-a-prompt) | [F] doc |
| Flux | Vendor admits hallucination and partial actions | "confident-sounding nonsense — what we call hallucinations"; "some actions may succeed, others may only partially complete." [Copilot under the hood](https://www.flux.ai/p/blog/flux-copilot-under-the-hood) | [F] vendor |
| Flux | Design review is nominal, single-point rules (resistor power, capacitor voltage margin, pull-ups) | [AI design review tab](https://www.flux.ai/p/blog/introducing-the-ai-design-review-tab) | [F] doc |
| Flux | Users: wrong formulas, high token cost | "hallucinate me a formula, which gave me half the frequency I asked for on a 555" ([HN 36497019](https://news.ycombinator.com/item?id=36497019), 2023); "After about 50-100$ in tokens … I couldn't get more than a couple of simple components on the schematic" ([HN 48368721](https://news.ycombinator.com/item?id=48368721), 2026). CEO: an "intern … oftentimes requires some supervision" ([engineering.com](https://www.engineering.com/fluxs-electrical-engineering-ai-when-it-works-its-magical/)) | [F] user, vendor |
| Flux | No independent technical review exists; Adafruit's review was paused after a demand letter | [Slashdot, June 2026](https://yro.slashdot.org/story/26/06/02/1647209/adafruit-pauses-blog-after-demand-letter-from-fluxais-lawyers) | [F] news |
| **atopile** | Intervals are uncorrelated even with themselves | "all other sets are treated as uncorrelated, even with themselves. This is why `X - X` is not necessarily `{0}`". [solver SKILL.md](https://github.com/atopile/atopile/blob/main/.claude/skills/solver/SKILL.md) | [F] doc |
| atopile | Asserts are static range checks, one comparison each; no SPICE in the README | "Only one comparison per assert". [ato SKILL.md](https://github.com/atopile/atopile/blob/main/.claude/skills/ato/SKILL.md); [README](https://github.com/atopile/atopile) | [F] doc |
| atopile | Solver robustness | "The solver enters an infinite loop when an assert contains arithmetic … involving a variable computed from a product of two parameters" ([PR #1800](https://github.com/atopile/atopile/pull/1800), March 2026) | [F] doc |
| **PolymorphicBlocks** | Checks are "not a design assurance tool"; performance outside max ratings is "out of scope"; no simulation | [README](https://github.com/BerkeleyHCI/PolymorphicBlocks) | [F] doc |
| PolymorphicBlocks | Users loosen tolerances by hand and distrust the context | "All participants encountered failed checks, often due to tolerances set too strict for parts like resistive dividers"; checks seen as "sanity checks as the modeled values were based on datasheets which might assume certain conditions, context that is lost in our model". [UIST'20 paper](https://people.eecs.berkeley.edu/~bjoern/papers/lin-pblocks-uist2020.pdf) | [F] doc (user study) |
| **tscircuit** | Transient only in the SPICE guide; its own JavaScript engine lists no supported devices or limits | [docs](https://docs.tscircuit.com/guides/spice-simulation/introduction); [spicey](https://github.com/tscircuit/spicey) | [F] doc |
| **SKiDL** | ERC checks connections, not behavior; simulating needs the netlist twice with different libraries | [skidl docs](https://devbisme.github.io/skidl/); [discussion 134](https://github.com/devbisme/skidl/discussions/134) | [F] doc |
| **JITX** | Hand-written checks can cover tolerance and temperature for one circuit type (the crystal load), but as static formulas; its divider interval example is inflated | [jitx.com detailed checks](https://www.jitx.com/product/detailed-design-checks); `engine_method_redteam.md` §3.5 | [F] doc; [R] |
| **AI-EDA overall** | "most of them involve GenAI creating SPICE simulations which require a human to interactively validate them … no autonomous functional … validation frameworks are presented" | [arXiv 2606.17074](https://arxiv.org/pdf/2606.17074) | [F] doc |
| Diode Computers | "Models can sound confident about circuits that would never work"; "Engineers still sign off every design." | [blog](https://blog.diode.computer/anthropic-partnership) | [F] vendor |

### 1.11 Modelica requirements, SysML v2, SystemVerilog/UVM-MS

| Area | Limitation | Evidence | Tags |
|---|---|---|---|
| **Modelica_Requirements** | Time window and condition fused into one block cause a combinatorial explosion of library blocks, and requirements can't be combined | "impossible to have a stable library because of the combinatorial explosion"; "It was not possible to combine requirements together". CRML paper, [Modelica 2023](https://ecp.ep.liu.se/index.php/modelica/article/download/960/868/981) | [F] doc |
| Modelica | Checked per simulation run; worst case over parameters left to "additional software on top" | "check in every simulation run whether the defined requirements are satisfied or violated (or are not tested)"; "not intended to perform formal model verification". [Otter et al. 2015](https://elib.dlr.de/99941/1/ecp15118625_OtterThuyBouskelaBuffoniElmqvistFritzsonGarroJardinOlssonPayellevilleSchamaiThomasTundis.pdf) | [F] doc |
| Modelica | Two-valued pass is falsely optimistic | "With two-valued logic it cannot be stated that a simulation did not test all required properties … this might be too optimistic or simply wrong." Otter 2015 | [F] doc |
| Modelica | Test scenarios are written by hand | "Currently, test scenarios are generated by hand" (CRML 2023) | [F] doc |
| **SysML v2** | No execution standard; demos are tool-specific and mostly parametric | SysML v2 "does not include a standard for model execution". [arXiv 2606.29006](https://arxiv.org/html/2606.29006) | [F] doc (summarized) |
| SysML v2 | Tools are immature | "no SysML v2 tool matches the maturity of leading SysML v1 tools". [sysml.org/sysml-v2/tools](https://sysml.org/sysml-v2/tools/) | [F] user |
| SysML v2 | The model is dropped once hardware testing starts | "Once verification moves to hardware, the system model is routinely left behind." arXiv 2605.11248 | [F] doc (abstract only) |
| **UVM-MS** (formerly UVM-AMS) | Approved Feb 2025; multi-language is out of scope; no verification components shipped | "The multi-language aspects are out-of-scope." [Accellera FAQ](https://www.accellera.org/activities/working-groups/uvm-ms/uvm-ms-faq) | [F] doc |
| **Real-number models** | Loading is lost | "Scalar representation is enough to represent … either voltage or current, but not both. This is a big limitation when modeling loading effects." [DVCon UDN/EEnet paper](https://dvcon-proceedings.org/wp-content/uploads/enabling-digital-mixed-signal-verification-of-loading-effects-in-power-regulation-using-systemveriloguser-defined-nettyp.pdf) | [F] doc |
| RNM | Model and schematic aren't checked against each other | "Model validation remains an area for improvement" (ADI, [DVCon](https://dvcon-proceedings.org/wp-content/uploads/real-number-modeling-of-rf-circuits.pdf)); "No logic equivalence checks between Analog schematics and behavioral model" ([design-reuse 28333](https://www.design-reuse.com/articles/28333/analog-mixed-signal-verification-methodology.html)); models need updating "since lower-level analog design tends to keep changing" ([ADI DVCon](https://dvcon-proceedings.org/wp-content/uploads/Mixed-Signal-Design-Verification-Leveraging-the-Best-of-AMS-and-DMS-2.pdf)) | [F] doc |
| Analog assertions | No coverage; isolated from the flow | "currently no way to set up and verify more complex circuit conditions"; no "Measurement of coverage". [DVCon mixed-signal ABV](https://dvcon-proceedings.org/wp-content/uploads/mixed-signal-assertion-based-verification.pdf) | [F] doc |
| IP integration | Documentation inadequate; model ≠ schematic | "mismatch between behavioral model (HDL) and schematic (spice)"; IP views "not adequate for integration". [Infineon, design-reuse 59338](https://www.design-reuse.com/article/59338-analog-ip-integration-in-soc-challenges-and-solutions/) | [F] doc |
| Industry numbers | Analog as a respin cause | "tuning analog circuitry was cited in 41 per cent of cases … almost twice as high as in the 2018 study" (Wilson 2020, [techdesignforums](https://www.techdesignforums.com/blog/2020/12/04/analog-surges-as-cause-of-ic-respins-wilson-functional-verification-2020-part-three/)); "47% of ASICs with fewer than 1 million gates had respins due to analog issues" (Wilson 2022, [semiengineering](https://semiengineering.com/analog-creates-ripples-in-digital-verification/)). The often-quoted "70%" is about *spec changes*, not analog ([semiengineering](https://semiengineering.com/first-time-silicon-success-plummets/)) | [F] doc |

### 1.12 Datasheets as specs, and hand-written WCCA

| Area | Limitation | Evidence | Tags |
|---|---|---|---|
| **Datasheets** | Typical-only values guarantee nothing; conditions go missing | "Parameters with typical values only. This is the least useful type of data"; makers "sometimes forget to mention the specific temperature for a given current rating". [EE Times](https://www.eetimes.com/unraveling-component-truth-from-datasheets/) | [F] user |
| Datasheets | Plots mislead; definitions are ambiguous (propagation delay 50–90% vs 50–50%) | [Electronic Design](https://www.electronicdesign.com/guest-blogger/article/21802782/) | [F] user |
| Datasheets | Parts change silently; PCN subscriptions are opt-in per part | "Same label, different plating, resin, tooling, or process" | [S] user |
| Datasheets → LLM | Extraction is unreliable without structure; it mixes operating conditions | 14.9% accuracy for plain upload-and-query, 62.7% with criteria, 97.5% with a structured pipeline; the LLM "occasionally combines specifications from incompatible operating conditions or device variants". [arXiv 2608.25217](https://arxiv.org/html/2608.25217) | [F] doc (summarized) |
| **WCCA** | The analysis drifts from the hardware and the requirements | "WCCA does not reflect as-built hardware"; "Incomplete flow of applicable requirements"; analyses skipped when no explicit requirement names them, "such as phase and gain margin"; "Unknown or ill-defined tolerances (especially aging)"; "No uniform standards". [Aerospace Corp TOR-2012(8960)-4 Rev A](https://aerospace.org/sites/default/files/maiw/TOR-2012(8960)-4_RevA.pdf) | [F] doc |
| WCCA | Redone on every change; EVA hard for nonlinear circuits; end-of-life data late; third-party IP blocks a full WCCA | Same report: redo when a circuit is "redesigned or modified"; "determining the worst set of parameters may be extremely difficult"; "RLAT testing … will not be completed in time"; it proposes "encrypted, validated models" | [F] doc |
| WCCA | The part database is the critical, manual step; MC at 1,000–50,000 runs | The "part characteristic database" is "one of the most critical steps". [NASA/SRC WCCA sheet](https://s3vi.ndc.nasa.gov/ssri-kb/static/resources/wcca.pdf) | [F] doc |
| WCCA | Spread across a word processor, a spreadsheet and a math tool | "error prone, difficult to create and keep updated". Maplesoft | [S] vendor |
| WCCA | Rigor | "The number one problem is lack of rigor." [EDN/AEi](https://www.edn.com/wcca-lack-of-rigor-will-cost-you/) | [F] user (consultancy) |
| ECSS practice | Methods and aging rules | `engine_method_redteam.md` §3.4 | [R] |

### 1.13 Vendor models, across tools

| Limitation | Evidence | Tags |
|---|---|---|
| Output impedance often wrong in op-amp macromodels | ZO "is often not modeled correctly". [TI Precision Hub, "Trust but verify"](https://e2e.ti.com/blogs_/archives/b/precisionhub/posts/spice-op-amp-macromodels-trust-but-verify) | [F] vendor (summarized) |
| Macromodels omit effects, and the only statement of coverage is netlist comments | They "may omit second- or third-order effects … or use a piecewise-linear lookup table". [Electronic Design](https://www.electronicdesign.com/technologies/analog/article/21806271/spice-it-up-understanding-and-using-op-amp-macromodels) | [F] doc |
| Models unvetted against data | Vendor and EDA models are "of poor quality and fidelity. They aren't always vetted against test data or even their own data sheet." (AEi, Hymowitz) | [S] user (consultancy) |
| Encrypted models are locked to one simulator; plain text needs an NDA | TI NexFET models "can only run in PSpice version 15.7 or higher … TI requires an NDA" (TI E2E 329627); ngspice can't read them (§1.7) | [S] vendor |
| IBIS files fail parsers, get temperature wrong and have bad tables | "The worst issue to encounter is a file that does not pass the parser"; "I-V tables not passing through 0A at 0V". [iConnect007](https://iconnect007.com/index.php/article/62271/top-10-issues-in-ibis-models/62274) | [F] user (expert) |
| Temperature ignored | ADI switch macromodel headers state that no supply or temperature dependence is modeled | [S] vendor |

### 1.14 Faults and events during operation

This covers a cable dropping mid-run, miswired or reversed connectors, swapped pins, shorts, opens, hot-plug, brown-out and ESD-like transients. Tools split into three camps. **Board simulators** have only primitives: a switch driven by a timed source. **IC fault simulators** automate *defect* campaigns for manufacturing-test coverage and ISO 26262 metrics. **System tools** (Simscape) treat faults as first-class, triggered objects. Board FMEA itself is a spreadsheet.

| Tool / practice | What it supports | Limitation | Evidence | Tags |
|---|---|---|---|---|
| **SPICE time-controlled switches** (LTspice, ngspice, PSpice, KiCad) | A voltage-controlled switch (`S`, `.model SW(Ron Roff Vt Vh)`) driven by a PULSE source opens or shorts a branch at a set time. ADI: a way "to add open-/short-circuit behavior in a circuit that can be changed in the middle of a simulation" | Every fault is hand-wired into the schematic as an extra part, one fault per edit. There is no fault list, no campaign, no "spec during fault", and hard switching causes convergence trouble ("Using switches during a transient simulation run may create convergence issues") | [ADI Analog Dialogue](https://www.analog.com/en/resources/analog-dialogue/articles/how-to-add-a-voltage-controlled-switch.html); [ltwiki S-switch](https://ltwiki.org/LTspiceHelp/LTspiceHelp/S_Voltage_Controlled_Switch.htm); [ngspice tips: switch resistance during transient](https://sourceforge.net/p/ngspice/discussion/ngspice-tips/thread/496dec49c2/) | [S] doc |
| LTspice workarounds | Switches with PULSE control; `.step` over a fault-index parameter to run one fault per step; XSPICE digital sources bridged to analog for contact bounce (ngspice) | The `.step` fault index is a manual idiom like `wc()` (§1.6). Contact bounce and hot-plug "on-off-on-off-on" insertion are modeled by hand if at all | [EDAboard: time-controlled switch in LTspice](https://www.edaboard.com/threads/time-controlled-switch-in-ltspice.411613/); [ngspice-users: modeling mechanical switches](https://sourceforge.net/p/ngspice/discussion/133842/thread/63832e1333/) | [S] user |
| **Cadence Legato Reliability / Spectre AMS fault simulation** | Fault identification (sites where defects can occur), then fault simulation in the *manufacturing* testbench, then comparing measured values with test limits; automated fault campaigns "for different failure modes" and "functional-safety diagnostic coverage reports" | Aimed at IC manufacturing defects and ISO 26262 diagnostic coverage, not board operating events. Inside Virtuoso, at IC prices | [Legato Reliability page](https://www.cadence.com/en_US/home/tools/custom-ic-analog-rf-design/custom-ic-analog-rf-flows/legato-reliability-solution.html); [Cadence Safety Solution press release, 2021](https://www.businesswire.com/news/home/20211019005190/en/Cadence-Introduces-Comprehensive-Safety-Solution-for-Faster-Certification-of-Automotive-and-Industrial-Designs) | [S] vendor |
| **Siemens Tessent DefectSim** | Automatic defect lists and statistical sampling, stop-on-detection, AC/DC mode, parallel runs; claims up to 10^6× less simulation time than a flat campaign | The defect universe explodes: "for 100 nodes there are 100×99/2=4950 possible two-node connections, many of which might be impossible"; transistor-level runs take "minutes to days" | [Tessent DefectSim](https://eda.sw.siemens.com/en-US/ic/tessent/test/defectsim/); [TechDesignForums](https://www.techdesignforums.com/blog/2017/02/22/tessent-defectsim-analog-fault-simulation/); [Mentor paper](https://semiwiki.com/wp-content/uploads/2017/08/mentorpaper_98144.pdf) | [S] vendor |
| **Synopsys TestMAX CustomFault** (now PrimeSim Custom Fault) | "traditional short and open models for MOS, R, L, C, and BJT, as well as transient and parametric faults"; fault reduction "based on electrical and logical equivalence or weighted random sampling"; functional safety and test coverage | Same IC defect framing. "The two biggest problems in analog fault simulation are the lack of an industry-accepted fault model and impractically long simulation time" | [Synopsys blog](https://www.synopsys.com/blogs/chip-design/introduction-to-analog-fault-simulation-using-synopsys-custom-design-platform.html) [F]; [white paper](https://www.synopsys.com/content/dam/synopsys/verification/testmax-customfault-wp.pdf) [S] | [F]/[S] vendor |
| **IEEE 2427-2025** (analog defect modeling and coverage) | Standardizes defect coverage accounting for analog and mixed-signal *ICs* | IC manufacturing defects only; no board or operating-event scope | [IEEE Xplore](https://ieeexplore.ieee.org/iel8/11343927/11343928/11343929.pdf) | [S] doc |
| **Saber** (automotive, aerospace) | Research and practice use Saber for "circuit FMEA by fault simulation": build a fault model per component failure mode, simulate, compare with the fault-free run | The fault models are built per study. The paper's motivation is that experience-based FMEA has "information gaps or wrong filling" | [IEEE 9021116](https://ieeexplore.ieee.org/document/9021116/) | [S] doc |
| **Simscape** (MATLAB) | The closest to first-class: a fault is attached to a block, with a **temporal** trigger (at a time, for a duration), a **behavioral** trigger (voltage or current out of range for longer than a time to fail) or an external one; faults can be enabled per run | A system-level tool, not SPICE-grade for board electronics; no tolerance or worst-case verdicts on specs during a fault | [Fault block](https://au.mathworks.com/help/sps/ref/fault.html); [Introduction to Simscape Faults](https://www.mathworks.com/help/simscape/ug/about-simscape-faults.html) | [S] doc |
| **Board FMEA / FMEDA practice** | Spreadsheets list each component's failure modes (open, short, drift), local and system effects, and failure rates. Pin FMEA (ADI, Nexperia HEF4000) tabulates each pin open, shorted to ground, to supply and to its neighbour. Bent-pin analysis does the same for connectors | Effects are *judged*, not simulated: bent-pin analysis "relies on human judgment there can be errors in the conclusions", examines "one failure mode at a time", and uses connector drawings that "often omit critical dimensions". FMEDA spreadsheets can't keep "a test-evidence record" per diagnostic-coverage value. Nothing links the FMEA row back to the schematic, so it drifts like the WCCA (§1.12) | [HandWiki: bent pin analysis](https://handwiki.org/wiki/Bent_pin_analysis) [F]; [ADI Know Your Safety part 3](https://www.analog.com/en/resources/technical-articles/know-your-safety-part-3.html) [S]; [EDN HEF4000 pin FMEA](https://www.edn.com/pin-failure-modes-and-effects-analysis-fmea-for-hef4000/) [S]; [LHP FMEDA](https://www.lhpes.com/fmeda-analysis-tool) [S] | doc, user |
| **ESD and surge** | TVS and ESD models exist, some now built from TLP measurements | "Many models only account for the steady-state IV curve of the device, and not its transient characteristics"; small errors in turn-on threshold "substantially change the simulation result"; automated model creation excludes snapback devices | [IEEE 10274181](https://ieeexplore.ieee.org/document/10274181/); [NSF PAR 10392666](https://par.nsf.gov/servlets/purl/10392666) | [S] doc |

**What's missing everywhere below IC tools.**
1. **A fault is not an object.** It's a hand-drawn switch.
2. **No fault list is derived from the design:** every connector pin open, adjacent-pin short, reversed polarity, a supply ramp and drop.
3. **No spec can be scoped to "during" or "after" a fault** ("output stays below 5.5 V while the sense cable is open", "no part exceeds abs-max when the connector is reversed", "recovers within 10 ms after brown-out").
4. **Nothing checks the fault case over tolerances and temperature.** The fault case is run once, at nominal.
5. **The FMEA spreadsheet isn't linked to the design,** so it drifts.

IC tools solve (2) and (5) for manufacturing defects, but not (3) or (4) for operating events.

---

## 2. The gaps, dimension by dimension

Short notes on the patterns across §1. The gap IDs (G1–G19) are used in §3–§5.

**Where specs live.**
- **G1 No machine-checkable home for specs.** Board simulators have `.meas` and eyes (§1.6–1.9). KiCad's measurements have no limit field. Only SIMPLIS DVM and Cadence ADE have spec columns, and both are paid tiers.
- **G2 The limit is stored twice or more.** It sits in the requirements document, the ADE output, the Verifier requirement, the WCCA spreadsheet and the datasheet. Each copy drifts (Aerospace: "Incomplete flow of applicable requirements").

**Test setups.**
- **G3 Benches can't be reused across designs, and aren't maintained when the design changes.** Maestro copies lose the setup. KiCad has no sub-sheet simulation, so users build "one project dedicated for simulation". Modelica test scenarios are "generated by hand".
- **G4 Bench drift from the block's assumptions goes unnoticed.** No tool checks that a bench stays inside the block's operating range (`contract_references.md` §18.2). The Aerospace list adds analyses that are skipped because no requirement names them.

**Conditions and corners.**
- **G5 Corners are picked by hand and explode.** 1215 corners in Solido's example; about 1,200 on EDAboard; 3375 corners take 13.5 days in Fast PVT's example. LTspice nests at most three `.step` levels.
- **G6 The "worst case" is a guess.** One sensitivity jump (PSpice, SIMetrix, Multisim, Micro-Cap), vertex-only enumeration (LTspice `wc()`), or surrogate confidence (Fast PVT). Worst cases inside the box are never looked for. PVT and statistics are checked separately, missing their interaction (Solido's own pitch).

**Tolerance and statistics.**
- **G7 Unchecked distribution and sigma defaults.** Altium: 5 runs, uniform, 10%. LTspice's `mc()` is uniform. ADS leaves the distribution to the user. Sigma conventions differ between tools (`engine_method_redteam.md` A6).
- **G8 No correlation and no lots.** PSpice doesn't support correlated DEV. atopile's `X − X ≠ 0`. edg's intervals, which users then loosen by hand.
- **G9 MC cost and no proof.** 2,000 samples for brute 3σ; 1,600 trials for ±2% yield; about 10^7 for 5σ. MC never proves a worst case (SIMetrix: it "rarely locates the extremes").

**Hierarchy.**
- **G10 Sub-blocks are trusted, not checked in context.** "Black boxes … presumed to be pre-verified". Real-number models lose loading. Datasheet numbers lose their fixture (edg users: "context that is lost").
- **G11 Behavioral models drift from the circuit.** "No logic equivalence checks between Analog schematics and behavioral model". Models need updating on every netlist release.
- **G12 IP handoff is paper.** IP documentation is "not adequate for integration". A full WCCA of third-party IP is hard to obtain.

**Traceability.**
- **G13 No requirement → spec → result chain, and no "what changed".** Traceability is imposed from outside (ISO 26262). Saved corners and WCCA documents go stale silently. SysML v2 is "left behind" once testing starts.
- **G14 Pass/fail with no "not tested" state, and no coverage.** Modelica's two-valued logic critique. The DVCon analog-assertion paper: no "Measurement of coverage". No tool reports which sub-block specs or conditions a system run never exercised.

**Models.**
- **G15 Vendor models: locked, unvetted, and silent about what they leave out.** Encrypted and simulator-locked; ZO wrong; temperature missing; coverage stated only in netlist comments.
- **G16 Datasheet data is unreliable as a spec source.** Typical-only values, conditions lost, silent revisions, LLM extraction errors.

**Collaboration, CI, AI, speed.**
- **G17 No text diffs, no merges, no CI for analog.** Binary maestro and OrCAD files; locking instead of merging; no KiCad simulation in the CLI; results in the gigabytes. The only CI example found is a personal project (cicsim).
- **G18 AI checks single points and can't show what it verified.** Flux: nominal only, with hallucination admitted. The survey: humans validate the simulations. Trust is the stated issue even for Cadence and Siemens.

**Faults and events during operation.**
- **G19 Faults are hand-wired, one at a time, at nominal, with no spec that holds during or after them.** SPICE offers timed switches. IC tools automate defect campaigns for test coverage. Simscape has triggered faults but no worst-case verdicts. Board FMEA is a spreadsheet of judged effects (§1.14).

Also from `engine_method_redteam.md` §3.3, not repeated here: **stress and derating are checked at nominal only** in every board tool (PSpice Smoke, Micro-Cap, TINA, SIMetrix SOA). We fold this into G6.

---

## 3. Gap table

**How often it hurts** is our judgement from the evidence, not a measurement. **High** means most designs, most weeks. **Med** means on reuse, on sign-off, or on some circuit classes. **Low** means rare or niche.

| # | Gap | Tools that have it | Key evidence | Board designers | IC designers |
|---|---|---|---|---|---|
| G1 | No machine-checkable spec home | LTspice, ngspice, KiCad, Qucs-S, Altium, PSpice (outside AA), Xyce; Flux; SKiDL | KiCad measures have no limit field; `.meas` read from logs | **High**: specs live in heads and PDFs | Low (ADE has spec columns) |
| G2 | Limit duplicated across requirements doc / tool / WCCA / datasheet | ADE + Verifier, every WCCA flow | Aerospace TOR: "Incomplete flow of applicable requirements" | Med | **High** |
| G3 | Benches not reusable, not maintained with the design | All; worst in KiCad and ADE copies | Maestro copy loses setup; KiCad "one project dedicated for simulation" | **High** | **High** |
| G4 | Bench outside assumptions, unnoticed; analyses skipped | All | Aerospace: phase and gain margin skipped | Med | Med |
| G5 | Manual corner picking, explosion | All IC tools; LTspice `.step` ≤ 3 | Solido 1215 corners; 13.5 days for 3375 | Med (few range knobs) | **High** |
| G6 | "Worst case" = one sensitivity jump, vertices only, or a surrogate; interior and PVT × stats missed; stress at nominal | PSpice, SIMetrix, Multisim, Micro-Cap, LTspice, Solido Fast PVT | "not guaranteed … almost never the case in practice" (SIMetrix) | **High**: the board WCA method *is* the sensitivity jump | **High** |
| G7 | Unchecked distribution and sigma defaults | Altium, LTspice, ADS, PSpice | Altium 5 runs uniform 10% | **High** | Med (PDK supplies statistics) |
| G8 | No correlation, no lots | PSpice, atopile, edg, JITX | "does not support correlated DEV variations" | Med (matched pairs, networks, dividers) | Med (mismatch models exist) |
| G9 | MC cost, and MC never proves a worst case | All MC users | 2,000 samples at 3σ; 10^7 at 5σ | Med | **High** |
| G10 | Sub-blocks trusted, not checked in context; loading lost | ADE, RNM flows, atopile, edg, datasheet use | "presumed to be pre-verified"; RNM "not both" V and I | Med, rising with block reuse | **High** |
| G11 | Behavioral model drifts from the circuit | Every AMS flow; board: macromodels vs silicon | "No logic equivalence checks" | Med | **High** |
| G12 | IP handoff is paper | IC IP, board modules, vendor reference designs | Infineon: views "not adequate for integration" | Med | **High** |
| G13 | No requirement → spec → result trace; no change impact | Everything except ADE Verifier (partial) | ISO 26262 forcing; WCCA "does not reflect as-built" | Med (High in aerospace, auto, medical) | **High** |
| G14 | No "not tested" state; no analog coverage | All | Modelica two-valued critique; DVCon ABV | Med | **High** |
| G15 | Vendor models: locked, unvetted, silent about gaps | All | ngspice can't read encrypted models; ZO wrong; no temperature | **High** | Med (PDKs are better qualified) |
| G16 | Datasheet data unreliable as a spec source | All board flows; LLM extraction | Typical-only values; LLM 14.9% plain | **High** | Low |
| G17 | No diffs, merges or CI for analog | ADE, OrCAD, ADS; KiCad has no simulation in its CLI | "binary … merging don't work at all" | Med | **High** |
| G18 | AI verifies single points and can't show its basis | Flux, Diode, Circuit Mind, Cadence and Synopsys AI | "confident-sounding nonsense"; humans validate | Med, rising | Med |
| G19 | Faults and operating events (cable drop, reversed or miswired connector, short, open, hot-plug, brown-out, ESD) hand-wired one at a time, at nominal; no "spec during/after fault"; FMEA not linked to the design | All board simulators; Saber (per study); Simscape (no verdicts); spreadsheets. IC fault tools cover manufacturing defects, not operating events | Timed-switch idiom; bent-pin analysis "relies on human judgment"; "lack of an industry-accepted fault model" | **High** for anything with connectors, cables or external supplies (most boards); it's where field returns come from. Required evidence in automotive, industrial and medical | Med (IC: defect coverage for ISO 26262; operating faults are the system team's job) |

---

## 4. Opportunities: what a language + engine + agent can do, and where our design stands

Status words: **Addressed** means designed in the accepted docs and in the M3 scope, or already built. **Designed** means designed but after M3. **Partly** means some of it is covered. **Not** means the design says nothing yet.

| # | What we could do better | Our design today | Status |
|---|---|---|---|
| G1 | Specs are declarations in the block's own file, with a spec table as a view; every spec gets a verdict | language §8.1 and §8.3 (contract in the same file, one text home, spec table = view); engine §3.3 verdicts | **Addressed** |
| G2 | One home for the limit; external documents *reference* it by a stable ID instead of copying it | One text home (language §8.1; `contract_references.md` §18.2). There's no stable external requirement ID, and no way to import a requirement and check that a spec satisfies it | **Partly** |
| G3 | Benches are items that place the block by its ports, so they survive edits inside the block and can be reused by any block with the same interface | Default, form and sheet benches (language §8.5); benches in the language are L1 in `next_synthesis.md`, not in M3. Bench reuse *across blocks with the same interface* isn't designed. The default bench, derived from assumptions, updates with the design by construction | **Designed** (reuse across blocks: **Not**) |
| G4 | Every bench is checked to lie inside the block's assumptions; the report lists what wasn't checked | language §8.5 ("Every bench is checked to stay inside the block's assumptions"); R3 silent omissions (`next_synthesis.md` §7) | **Designed** |
| G5 | Corners are never picked by hand: the engine enumerates the whole box per spec side, cones keep it small, and the loop takes over beyond | engine §5.3 (≤ 2^12 corners per side), cones, §5.8 loop; E4 budget | **Addressed** (to 12 knobs per side); past that: **Designed** |
| G6 | Real worst case: every corner, guards inside the box, a σ search, counterexamples that reproduce, PVT and statistics searched together | engine §5.3–5.5, §3.5; P2–P3. Stress at worst case: language §8.7 derating checks at every corner | **Addressed** for simulated specs; stress: **Designed** |
| G7 | Distribution assumptions are visible, and the verdict turns UNDECIDED (distribution) when they matter | engine §2.2 provenance, §2.4 (the default checked under uniform), tags `relies-on-typical`, `distribution-sensitive` | **Addressed** |
| G8 | Shared knobs are counted once; lots tie matched parts | Per-run measures (D-F) make `X − X = 0` exact; lots are M6 (engine §2.5) | **Addressed** (per run); lots: **Designed** |
| G9 | The worst case is proved by enumeration, not sampled; statistics answer "how often", never "how bad" | P5; M7 yield with confidence bounds | **Addressed**; yield: **Designed** |
| G10 | Child contracts travel with placement paths; interface knobs (`z_src`, `z_load`, current) are checked both ways; characterized tables are reused only inside their assumptions; in context otherwise | `contract_hierarchy.md` §4–§5; engine §7; L4 and M1 in `next_synthesis.md`. v0.1 **drops** sub-contracts silently today (L4) | **Designed**; today: **Not** (warn first) |
| G11 | A behavioral model is a claim that the engine checks against the circuit over the child's assumption box: same specs, both backends, within a stated band | Strategy 4 in `contract_references.md` §16 and the evidence ladder mention it. There's no mechanism for "this model refines that block", and no check | **Not** |
| G12 | A block ships with its contract, its verdicts, its rests-on list and its characterization table: a checkable handoff, not a PDF | Contracts plus `Record.rests_on` (`next_synthesis.md` R1), the characterization cache (`contract_hierarchy.md` §4.3). A package or handoff format isn't designed; encrypted or IP-protected blocks aren't considered | **Partly** |
| G13 | Every result is keyed to a design revision; an edit shows which verdicts changed and why; requirement changes are shown apart from design changes | `rev` and `stale` (engine §8); agent_flows G6 (requirement edits apart from design hunks); `--base` diff (engine §10). No link to external requirements; no explanation of *why* a verdict moved (which knob or model changed) | **Partly** |
| G14 | Verdicts are more than two-valued (UNDECIDED with a reason, UNSPECIFIED); the report lists what wasn't checked; the parent says which of a child's knobs its benches span | engine §3.3; R3; `contract_references.md` §16 caution (coverage of child knobs). The coverage report isn't specified | **Partly** |
| G15 | Each model has a coverage map (which knobs it responds to); gaps tag the verdict model-conditional; standard wrappers put missing effects back; defaults are written out | engine §6.6, §3.4 `not_modeled`, D-A; E2 model level. Encrypted models: the backend trait asks only for values (P1), so a PSpice or Spectre backend could run them, but only ngspice is planned | **Designed** (encrypted: **Partly**) |
| G16 | Part records keep provenance per number (tested / by design / typical / missing), the condition columns, the source page, and review state; typical never becomes a limit | engine §2.2–2.3; language §6.5; M5; UNSPECIFIED. Not covered: datasheet **revisions** and PCNs, and a check of extracted records against the datasheet | **Designed** (revisions: **Not**) |
| G17 | Text is the format: diffs and merges are line-based; `spicy check` runs headless; JSON results for CI | language_editor_mapping §6.3; engine §8 CLI; D-C says JSON is **unstable** in M3. No CI gating rule (e.g. "a PR may not turn a PASS into FAIL or UNDECIDED") and no result diff | **Partly** |
| G18 | The agent calls the engine, and may paraphrase but never strengthen a verdict; every AI edit is shown with re-checked verdicts | engine §3.3 `claim`; agent R1; language §10 (AI edits as diffs with re-checked verdicts) | **Addressed** in design; built after M3 |
| G19 | Faults are declared, not drawn. The engine derives a fault list from port types and connectors (each pin open, adjacent pins shorted, polarity reversed, supply ramp and drop). Each fault is a **mode** that switches on at a time or on a condition. Specs are scoped `during` or `after` a fault and checked over every corner like any other spec. The abs-max and derating checks run inside every fault case ("no damage if reversed"). The report is a generated FMEA table linked to the design. The agent proposes the fault list and explains the counterexample | Nothing on faults yet. Nearby pieces: `KnobKind::Mode` and scoped specs (engine §2.1, E5; `next_synthesis.md` L3); the transient tier (T1); abs-max during start-up as an automatic check (language §8.7); start-up interactions such as the eFuse pushed into current limit mid-ramp (engine §7); the power-on-reset supply ramp (MS). None of these is a fault: they are normal operation | **Not** |

**The biggest openings.** No tool combines them today.
1. **Proven worst case at board scale, with its basis printed** (G6, G7, G9). Board tools make one sensitivity jump and admit "no guarantee". IC tools sample and fit surrogates. Enumeration plus guards, with PASS words that say what they rest on, is new at board level, and the M3 scope already delivers it.
2. **Specs, benches and verdicts in the design's own text** (G1, G2, G3, G17). This makes one home for the limit, diffs and CI possible, and gives the AI something it can actually check. SIMPLIS DVM and ADE have pieces, but in paid, binary, GUI-bound form.
3. **Checked reuse** (G10, G12, G14). Child contracts become monitors in context, interface assumptions are checked both ways, and characterized results are reused only inside their box. This is the IC world's biggest pain, and no board tool attempts it. It's designed, but not built.
4. **Faults as declared events with worst-case verdicts** (G19). Nobody below IC tools derives a fault list from the design or checks a spec "during" or "after" a fault over every corner. Our mode knobs, transient tier and automatic abs-max checks are the pieces; the fault object is not designed yet (P15).

---

## 5. Must-have paths: gaps we must not design ourselves out of

Each row names the decision to protect now, even where the feature is far off. "Seam" means a cheap change today.

| # | Path to keep open | Why (gaps) | What to protect now |
|---|---|---|---|
| P1 | **Stable spec identity and external requirement links** | G2, G13: ISO 26262, DO-254 and ECSS reviews demand traceability; ADE Verifier exists for this | `SpecPath` (placement path + name, `next_synthesis.md` M1) as the stable key. Reserve an attribute such as `#[req("SYS-123")]`, and don't let renames lose identity (the formatter already keys by name). A requirement import can then map onto specs |
| P2 | **Several benches per spec, and measures over several runs** | G3, G4: real verification needs many benches (PSRR, load step, CMRR, calibration) | `BenchId` in requests and the run-table key (seam, `next_synthesis.md` T2); `spec … on <bench>`; multi-run measures (E7). Keep benches *interface-typed*, so a bench written for `Analog<In> → Analog<Out>` can be reused by other blocks (G3 reuse) |
| P3 | **Two-way port quantities and a block-scoped result cache** | G10, G12 | `KnobSpec.origin = Interface(…)`, `Record.rests_on`, run tables keyed by block definition, not placement (`contract_hierarchy.md` §4.3, §4.7) |
| P4 | **Behavioral models that are checked against their circuit** | G11: the top AMS pain | Let a block have more than one *implementation* (circuit, behavioral, table) behind one contract, with the contract as the equivalence criterion. Don't hard-wire one netlist per block in `spicy_model` |
| P5 | **Backends beyond ngspice, including ones that run encrypted models** | G15: TI models are PSpice-only; ngspice refuses encrypted models | Keep the backend trait value-only (P1): no assumption that we can read the model text. Model coverage comes from probing (sensitivity to each knob), not from parsing the card |
| P6 | **Three-valued results and coverage** | G14 | Keep UNDECIDED and UNSPECIFIED; record per side which knobs were spanned and which child specs were evaluated in context; R3's "not checked" list is part of the report schema, not a log line |
| P7 | **Statistics with real distributions, lots and correlation** | G7, G8, G9 | Provenance and distribution fields on every knob now (seam M2); lot groups (M6); the run table never assumes independence |
| P8 | **Transient, modes, mixed-signal, and digital at the boundary** | Code tools can't express AC or transient specs; RNM loses loading | `Needs.analyses` as a list (T1), `KnobKind::Mode` (E5), `Measured::Beyond` (E6). Port types that carry both V and I, so we never have RNM's "not both" limit |
| P9 | **Part data with revisions** | G16: PCNs, silent changes, LLM extraction errors | Part records carry the source document, its revision or hash, and review state. Include the part-data hash in `rev`, so a datasheet revision makes verdicts `stale` |
| P10 | **Aging, end-of-life, radiation and derating at worst case** | WCCA practice (Aerospace, ECSS, NASA) | `life` and aging-law knobs (engine §2.3, M6); derating as automatic checks at every corner (language §8.7); drift transforms that aren't ±% (NASA hFE example, `engine_method_redteam.md` §3.4) |
| P11 | **A stable, diffable result format and CI gating** | G17 | Stabilize the JSON schema after M3. Records keyed by `SpecPath` + `rev`, so two runs diff. Plan a `spicy check --base <rev>` gate that fails on PASS → FAIL / UNDECIDED |
| P12 | **Reviewer-ready reports** (a generated WCCA document) | G2, G13: WCCA's audience is reviewers ("show everything") | Every verdict already carries claim, basis, contributors and counterexample (engine §3). Keep them structured, so a report generator can emit a WCCA chapter per spec with its trace |
| P13 | **IC-scale knob counts and remote or parallel runs** | G5, G9 | The loop (§5.8); run requests that are independent and serializable, so they can go to a pool |
| P14 | **Import of existing benches** (LTspice `.asc` with `.meas`, KiCad workbooks) | G3: users' benches exist today | Keep `.meas`-like measures expressible in our measure language, so an importer can map them |

| P15 | **Faults and operating events as declared, first-class objects** | G19: most board field failures come from connectors, cables and supplies | See the list below |

**P15 in detail: what to keep possible now, so faults can come later.**
1. **A fault is a mode with a trigger.** Extend `KnobKind::Mode` (E5) so a mode can switch *during* a run, at a time or on a condition (Simscape's temporal and behavioral triggers). Keep the transient analysis list (T1) able to carry piecewise topology changes. Don't assume one fixed netlist per run.
2. **Faults attach to structure, so they can be generated.** Open, short-to-neighbour, reversed and hot-plug faults need to know which pins are adjacent in a connector and which ports are external. Keep pin order and connector identity in part records and port types. Don't erase them in flattening.
3. **Spec scoping by phase.** Reserve `where`-like scoping for `during <fault>` and `after <fault> + t`, next to `.window()`. Keep "no damage" expressible as the automatic abs-max and derating checks evaluated inside a fault's runs (language §8.7), so "no damage if reversed" needs no new spec kind.
4. **Faults multiply with corners, so budget them as a separate axis.** A fault campaign is (faults × corners). Keep `Scenario` (E5) and the run table keyed by fault id as well as bench id, so a fault's corners are enumerated per fault. Keep fault reduction (equivalent faults, stop at the first FAIL) possible, as DefectSim and CustomFault do.
5. **Switching models that converge.** Smooth switch models (ngspice's `PS vswitch`) and a per-run timeout (B1) are needed before faults are practical on ngspice.
6. **The output is an FMEA row.** Record per fault: its effect on every spec (PASS / FAIL / UNDECIDED at worst case) and on abs-max. Keep `Record` able to hold a fault id, so a report generator can emit a linked FMEA or FMEDA table instead of a spreadsheet.
7. **ESD and surge stay out of scope for now,** because the models are weak (§1.14). But a fault trigger that injects a source (an IEC 61000-4-2 pulse) instead of a switch should fit the same object.

**Not a must-have, but worth noting.** Solido's "additive learning" (reuse results across iterations, `engine_method_redteam.md` §3.2 item 7) matches our cached run tables. Carrying verdicts whose cone an edit didn't touch (engine §8) is the board-scale version. It already has a path.

---

## 6. What I could not verify

- **Cadence:** the community forums and most of cadence.com block fetches (403). Every forum quote is a search snippet. I couldn't confirm how ADE Verifier detects drift between a requirement and its maestro output.
- **TI E2E, ADI EngineerZone, EEVblog, groups.io:** blocked. Their quotes (TI's LTspice policy, the NexFET NDA, the macOS batch-mode bug) are snippets.
- **Synopsys Custom Compiler and Altium:** no user complaints found. The limits listed come from documentation only.
- **Flux:** the simulation engine isn't named. Whether it has MC, sweeps or temperature isn't documented. I found no independent technical review (see the Adafruit note).
- **atopile:** the enterprise page's "validate with simulation" claim is a snippet only.
- **Solido Fast PVT failure modes** (discontinuous outputs, many variables): only the general importance-sampling limit (DATE 2019) was found. The Solido chapter returned 403.
- **The "70% of respins are analog" figure** couldn't be traced. The figures I could trace are 41% (Wilson 2020), about 47% of designs under 1M gates (Wilson 2022), and ">60% at ≤ 45 nm" (Cadence 2012, uncited industry estimate).
- **ECSS-Q-HB-30-01A** text was not read here; see `engine_method_redteam.md` §3.4.
- **Analog CI adoption:** I found no study or talk that measures it. The only concrete example is cicsim (GitHub Actions publishing ngspice corners).
- **Datasheet error rates:** I found no study that counts datasheet inconsistencies or errata.
- **The frequency column in §3** is our judgement from the evidence, not survey data.
- **Faults (§1.14):** most items are search snippets. The pages I tried to fetch in full either timed out (ADI pin FMEA) or held only an intro (the Semiengineering fault-simulation page). Only the Synopsys blog and the bent-pin article were read. I found no study of how often board field failures come from connector or cable faults; G19's "High" is judgement. Whether Saber (Synopsys) ships an automated fault campaign product, rather than per-study models, was not confirmed.
