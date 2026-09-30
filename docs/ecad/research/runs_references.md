# When Runs Happen: References from Software and Hardware Verification

> 2026-09-30 · Research for the question "when and how do simulations run?" The language (`contract_syntax_v5.md`) says nothing about cost, triggers, CI, manual runs or staleness; `engine_plan.md` §1.1 has only an engine-side tier table. This file surveys how software testing and chip verification decide **what runs when**, and draws lessons for spicy.
>
> **Source marks.** **[V]** checked against the source during this research (web fetch or search, link given). **[K]** well-known documented behavior, stated from the tool's docs but not re-fetched here. **[U]** industry practice or lore I could not verify from a public source; treat as a claim to confirm.

---

## Summary

Every mature system separates **three questions** that our language currently merges into one ("is the spec met?"):

1. **What does a check cost?** Declared roughly by the author (Bazel `size`, nextest `slow-timeout`, pytest `@slow`), then **measured** by the tool and used for scheduling. Nobody asks the author for seconds; they give a *class* (small/medium/large) and the tool enforces a timeout per class.
2. **When does it run?** Decided by **triggers** that belong to the *project* (a config file or CI definition), not to the test: on save, pre-commit, per change, nightly, release/sign-off. The test says what it *is*; the project says when each kind runs. Chip teams do the same: a `smoke` regression per commit, a `nightly` regression with coverage and more seeds, a sign-off regression with everything (OpenTitan's dvsim writes this as three named regressions **[V]**).
3. **Is the stored answer still true?** Answered by **content-hashed caching**: a result is keyed by the hash of every input (Bazel action keys, Go test cache, ccache). A result whose key no longer matches is **stale**, and tools say so (`(cached) PASSED`, Go's `(cached)`, ADE Assembler's history and "reference history" reuse **[V]**).

Plus two cross-cutting rules: **selection** (run only what a change can affect: Bazel's dependency graph, Google TAP, Meta's predictive selection that halves cost while still catching >99.9% of bad changes **[V]**), and **honest non-determinism** (flaky tests are tracked as a status of their own, retried, quarantined; ~1.5% of Google test runs flaky **[V]**). Monte Carlo and fuzzing are the analogues of our slow statistical specs: they are **budgeted** (time or sample count), run short on changes and long on a schedule (ClusterFuzzLite: 10 minutes per PR, hours in batch **[V]**).

**Main lessons for spicy (details in §4):**

- Give every spec a **derived cost class** the tool computes (analyses needed, knob count, transient length, sample count), and let the author **override** with an attribute only when needed: `#[slow]`, `#[manual]`. Do not ask for seconds.
- Put **triggers in a project file** (`spicy.toml` profiles: `edit`, `commit`, `nightly`, `signoff`), not in the contract. The contract says *what must hold*; the profile says *which classes run when*.
- Make **results first-class state**: every spec verdict is stored with the hash of everything it depends on, and the editor shows `fresh`, `stale`, `never run`, `running`, `budget-limited` beside pass/fail. A stale PASS is never shown as a PASS.
- Run **only affected specs** on an edit, using the elaboration dependency graph (which knobs, parts and setups each spec touches). This is exact for us, unlike software's heuristics.
- Statistical and long specs get a **budget and a confidence**, never a silent timeout. A timeout is a verdict (`UNDECIDED: budget`), not a failure and not a pass.

---

## 1. Software testing

### 1.1 Rust: `#[test]`, `#[ignore]`, filters, nextest

| Mechanism | What it does | Who decides | Where written |
|---|---|---|---|
| `#[test]` | Marks a function as a test | Author | Source annotation |
| `#[ignore]` / `#[ignore = "reason"]` | Skipped by default; run with `cargo test -- --ignored` or `--include-ignored` **[K]** | Author | Source annotation |
| `cargo test <substring>` | Runs tests whose path contains the substring **[K]** | Runner (person) | Command line |
| nextest profiles | Named config sets (`[profile.ci]`), picked with `--profile ci` | Project | `.config/nextest.toml` |
| nextest overrides | Per-test settings chosen by a **filter expression** (`test(...)`, `package(...)`) | Project | Config file |
| `slow-timeout` | Marks tests "SLOW" after a period, optionally kills after N periods **[V]** | Project | Config file |
| test groups | Limit concurrency of a subset (`max-threads`) **[V]** | Project | Config file |
| retries | Re-run failures; a test that passes on retry is reported **FLAKY**, not PASS **[K]** | Project | Config file |

nextest's slow-test configuration, quoted from its docs **[V]** ([nexte.st slow tests](https://nexte.st/docs/features/slow-tests/)):

```toml
[profile.ci]
slow-timeout = { period = "60s", terminate-after = 5, grace-period = "30s" }

[[profile.default.overrides]]
filter = 'test(test_e2e)'
slow-timeout = { period = "120s", terminate-after = 5 }

[[profile.default.overrides]]
filter = 'package(fuzz-targets)'
slow-timeout = { period = "30s", terminate-after = 1, on-timeout = "pass" }
```

Test groups **[V]** ([nexte.st test groups](https://nexte.st/docs/configuration/test-groups/)):

```toml
[test-groups]
serial-integration = { max-threads = 1 }

[[profile.default.overrides]]
filter = 'package(integration-tests)'
test-group = 'serial-integration'
```

**What to notice.** The source only says "this is a test" and optionally "ignore by default". Everything about *time, concurrency and retries* lives in a project file, selected by **filters over test names**, grouped into **profiles** chosen at the command line. The `on-timeout = "pass"` for fuzz targets is the explicit form of "a budget ran out; that is expected".

### 1.2 pytest markers and `-m`

```python
@pytest.mark.slow
def test_full_sweep(): ...
```

```ini
# pytest.ini
[pytest]
markers =
    slow: long-running; deselect with -m "not slow"
addopts = --strict-markers
```

Run `pytest -m "not slow"` locally, `pytest` in CI **[K]**. Markers are free-form labels; `--strict-markers` turns an unregistered (misspelled) marker into an error. Lesson: free-form tags need a registry, or `@pytest.mark.slwo` silently runs in the fast tier.

### 1.3 Bazel: `size`, `timeout`, tags, sharding, caching

Bazel's test encyclopedia **[V]** ([bazel.build/reference/test-encyclopedia](https://bazel.build/reference/test-encyclopedia)):

| `size` | Default `timeout` | Seconds |
|---|---|---|
| small | short | 60 |
| medium | moderate | 300 |
| large | long | 900 |
| enormous | eternal | 3600 |

Size also implies an expected resource footprint (CPU/RAM) that the scheduler reserves **[K]**. Tags with meaning to Bazel **[V]**:

| Tag | Meaning |
|---|---|
| `manual` | Excluded from wildcards (`//...`); runs only when named |
| `exclusive` | No other test runs at the same time |
| `external` | Depends on the outside world; **disables result caching** |
| `flaky` | Non-deterministic (the `flaky = True` attribute retries up to 3 times **[K]**) |

```python
cc_test(
    name = "pll_lock_test",
    size = "large",          # author's cost class → 900 s budget
    shard_count = 8,         # split across 8 processes (TEST_TOTAL_SHARDS / TEST_SHARD_INDEX)
    tags = ["nightly"],      # free-form; selected with --test_tag_filters=nightly
)
```

Selection flags: `--test_size_filters=small,medium`, `--test_tag_filters=-nightly`, `--test_timeout_filters` **[K]**. Test results are cached by action key (hash of the test binary, its runfiles, env and flags); an unchanged test prints `(cached) PASSED`; `--cache_test_results=no` or `--nocache_test_results` forces a re-run; `external` tag opts out **[K]**. With a remote cache, one machine's result is reused by everyone **[K]**.

**What to notice.** The author declares a *coarse class*, the tool turns it into a hard budget and resource reservation. The class is also a **filter axis**. Sharding is declared by the author but executed by the tool via env vars: the test must cooperate (touch `TEST_SHARD_STATUS_FILE`) **[V]**.

### 1.4 Go `-short` and the Go test cache

```go
func TestMonteCarlo(t *testing.T) {
    if testing.Short() {
        t.Skip("skipping in -short mode")
    }
    ...
}
```

`go test -short` sets `testing.Short()` **[K]**. It is the opposite polarity to `#[ignore]`: long tests run by default, the fast mode is opt-in. Go caches passing results: a run matches the cache if it uses the same test binary and only "cacheable" flags (`-run`, `-short`, `-timeout`, `-v`, …); tests that read files in the module or env vars only match when those are unchanged; `-count=1` disables it **[V]** ([cmd/go docs](https://pkg.go.dev/cmd/go#hdr-Test_packages)). Output shows `ok  pkg  (cached)`.

**What to notice.** Go tracks the *dynamic* inputs a test actually read (files, env vars) as part of the cache key. That is the "record what you touched" approach to staleness.

### 1.5 JUnit 5 tags

```java
@Tag("slow") @Test void fullCharacterization() { ... }
```

Selected with tag expressions in Maven/Gradle (`includeTags "fast"`, `excludeTags "slow"`, `"fast & !flaky"`) **[K]**. Same model as pytest: labels in source, selection in build config.

### 1.6 Selection: test impact analysis and change-based testing

| System | How it picks tests for a change | Exact or statistical |
|---|---|---|
| Bazel / Buck | Reverse dependencies of changed targets (`rdeps`); unchanged tests are cache hits **[K]** | Exact on the declared build graph |
| Google TAP | Presubmit runs affected tests; postsubmit runs batches ("milestones") across the repo **[V]** ([Memon et al., ICSE-SEIP 2017](https://dl.acm.org/doi/10.1109/ICSE-SEIP.2017.16)) | Graph-based, then scheduling heuristics to cut lag |
| Meta predictive test selection | ML model over historical outcomes picks a subset; **halves infra cost while reporting >95% of individual test failures and >99.9% of faulty changes** **[V]** ([Machalica et al., ICSE-SEIP 2019](https://arxiv.org/abs/1810.05286)) | Statistical |
| Launchable (now CloudBees Smart Tests, acquired Aug 2024) | Same idea as a product: learns from past builds and changes, returns the most relevant subset **[V]** ([CloudBees docs](https://docs.cloudbees.com/docs/cloudbees-smart-tests/latest/features/predictive-test-selection)) | Statistical |
| Jest `--watch` / `-o` / `--changedSince` | Runs tests related to files changed since the last commit, via its module graph **[K]** | Exact on imports |

**What to notice.** The dependency graph is the safe basis; ML selection is layered on top only when the graph selects too much. Google still runs everything later (postsubmit), so selection never becomes the only line of defence.

### 1.7 Tiers: editor, pre-commit, CI, nightly, release

A common shape **[K/U]** (the exact split is per team):

| Tier | Trigger | Budget | Runs |
|---|---|---|---|
| Editor | Save / keystroke idle | < 1–2 s | Type check, lint, maybe the tests of the file |
| Pre-commit hook | `git commit` | seconds | Format, lint, fast unit tests |
| Presubmit / PR CI | Push, PR | minutes (often a 10–30 min target) | Affected tests; small+medium sizes |
| Postsubmit / merge | Merge to main | tens of minutes | All non-manual tests, sharded |
| Nightly | Schedule | hours | `large`/`enormous`, `#[ignore]`d, long fuzz, perf |
| Release | Tag | as long as needed | Everything, plus manual/external |

### 1.8 Flaky tests

- Google: about **1.5% of all test runs** report a flaky result; about 16% of tests show some flakiness. Mitigations: automatic re-run on failure, tools to monitor and **quarantine**, a dedicated team **[V]** ([Micco, Google Testing Blog 2016](https://testing.googleblog.com/2016/05/flaky-tests-at-google-and-how-we.html)).
- nextest and Bazel both report "passed on retry" as **FLAKY**, a separate status **[K]**.

For us, simulator "flakiness" is not randomness but **numerical fragility**: a convergence failure, a timestep-too-small, a result that moves with `reltol`. engine_plan.md already re-runs decisive points at tight tolerance (step 4). The lesson is the same: give it its **own status**, never retry until green.

### 1.9 Editor checks and watch modes

| Tool | Behavior |
|---|---|
| rust-analyzer "flycheck" | On save, runs `cargo check` (configurable: `rust-analyzer.check.command`, `checkOnSave`) in the background and maps diagnostics to the editor; a newer save cancels the running check **[K]** |
| cargo-watch | Re-run a command on file change. Now "on life support", final release; the author recommends **bacon** or **watchexec** **[V]** ([cargo-watch README](https://github.com/watchexec/cargo-watch)) |
| bacon | Background checker with named jobs (`check`, `test`, `clippy`) in `bacon.toml`, switched by key **[K]** |
| jest `--watch` | Re-runs only tests affected by changed files; interactive filter by name/pattern **[K]** |

**What to notice.** The editor tier is **cancellable and debounced**: a new edit kills the stale run. And the editor tier is a *different command* (`check`, not `test`): the cheapest thing that gives useful signal.

### 1.10 Property and fuzz tests as long runs

- proptest: the number of cases is a knob (`PROPTEST_CASES`, `ProptestConfig::with_cases`) **[K]**. Same test, different budget per tier.
- cargo-fuzz / libFuzzer: `-max_total_time=<s>`; no natural end **[K]**.
- ClusterFuzzLite: **code-change mode** fuzzes each PR for a short budget (default 10 minutes, `FUZZ_SECONDS`), **batch mode** fuzzes for hours on a schedule; both share one corpus **[V]** ([ClusterFuzzLite docs](https://google.github.io/clusterfuzzlite/running-clusterfuzzlite/)).
- OSS-Fuzz runs continuously and files bugs **[K]**.

**What to notice.** Open-ended searches are parametrized by a **budget**, not split into "fast test" and "slow test". The *same* definition runs short on a change and long on a schedule, and **state carries over** (the corpus). That is exactly our Monte Carlo and sigma search: the same spec, more samples in nightly, and samples from earlier runs kept.

---

## 2. Hardware verification

### 2.1 UVM regressions and test lists

A UVM testbench has many tests (a test = a sequence configuration) and each runs with many random **seeds**. A **regression** is a named list of (test, seed count, options) **[K]**. OpenTitan's open-source flow (dvsim) makes this concrete; its common config defines three default regressions **[V]** ([common_sim_cfg.hjson](https://github.com/lowRISC/opentitan/blob/master/hw/dv/tools/dvsim/common_sim_cfg.hjson)):

```hjson
regressions: [
  { name: smoke,   tests: [], reseed: 1, run_opts: ["+smoke_test=1"] }
  { name: all }
  { name: nightly, en_sim_modes: ["cov"] }
]
```

(`tests: []` means "run everything" in that file; per-block cfgs add their own tests with a `reseed` count.) The same tests run in each regression; the regression changes **seed count** and **mode** (coverage on in nightly).

### 2.2 Verification plans: vManager, VC Execution Manager

| Tool | What it does | Mark |
|---|---|---|
| Cadence vManager (now Verisium Manager) | **vPlan**: a hierarchy of features linked to spec documents; each feature is mapped to metrics (tests, checks, coverage items). A **VSIF** (session input file) describes a regression run; results from runs are mapped back onto the vPlan to show per-feature progress | [V] ([Cadence datasheet](https://www.cadence.com/en_US/home/resources/datasheets/vmanager-ds.html), [MDV white paper](https://semiwiki.com/wp-content/uploads/2018/03/maximizing-metric-driven-ver-wp.pdf)) |
| Synopsys VC Execution Manager / Verdi Planner | Same roles: regression execution and a hierarchical plan (`.hvp`) annotated with coverage and test results | [U]: I did not fetch Synopsys docs |
| Accellera UCIS | Standard database for coverage and test history exchange | [K] |

**What to notice.** The plan (what must be shown) is **separate** from the regression (what to run), and the tool joins them. Progress is per plan item: "feature X: 3 of 4 metrics met". Our contract is already the plan; a spec *is* a plan item. What we lack is the **session/regression layer** and the back-annotation of runs onto specs.

### 2.3 Coverage closure

Digital sign-off is not "all tests pass" but "all tests pass **and** coverage goals are met" (code, functional, assertion) **[K]**. Random seeds are added until coverage stops growing; holes are closed with directed tests or waived with a reason **[K]**. Waivers are tracked, reviewed artifacts **[U]**.

The analog equivalent is **how much of the knob space was covered** for a spec: all corners enumerated, sigma search converged, a transient only at typical. engine_plan.md's "worst_case vs sigma(3)" and UNDECIDED `next` steps already hold this information; it should be shown as coverage, not hidden in a run log.

### 2.4 Analog and mixed-signal: corners, Monte Carlo, run plans, history

| Tool / practice | Behavior | Mark |
|---|---|---|
| ADE Assembler (maestro view) | Many **tests** (testbench + analyses + outputs with spec limits) × **corners** × sweeps in one setup; each output shows pass/fail/near against its spec | [K] |
| ADE Assembler **run plans** | A sequence of runs (e.g. nominal → corners → Monte Carlo) with conditions, where a later run can use results of an earlier one as starting points | [V] ([Cadence training course S4](https://www.cadence.com/en_US/home/training/all-courses/86256.html), search summary) |
| ADE **history** | Every run is saved as a history item with its setup; you can go back to old results and old setups | [V] ([ADE Assembler datasheet](https://www.cadence.com/en_US/home/resources/datasheets/virtuoso-ade-assembler-ds.html)) |
| ADE **reference history** | After adding corners/points or changing values, simulate **only the changed points** and copy the rest from a previous history into one combined history | [V] ([Cadence support note](https://support1.cadence.com/public/docs/content/20495458.html)) |
| Plan-based analog verification | Cadence methodology linking a vPlan to ADE results so analog specs appear in the same plan as digital | [V] title only ([white paper page](https://www.cadence.com/en_US/home/resources/white-papers/plan-based-analog-verification-methodology-wp.html), 403 on fetch) |
| Siemens Solido | Variation-aware design: PVT corner verification and **high-sigma** Monte Carlo with ML, claiming orders of magnitude fewer simulations than brute force (e.g. 4,000 sims for a 6σ flip-flop check) | [V] ([Siemens SDE page](https://www.siemens.com/en-us/products/ic/solido/design-environment/), [Xcelerator blog](https://blogs.sw.siemens.com/xcelerator-academy/2024/10/21/solido-design-environment-a-solution-for-variation-aware-and-high-sigma-verification-challenges/)) |

Typical analog team rhythm **[U]** (practice, not a public source): during design, a designer runs nominal + a few typical corners interactively; before a review, the full PVT corner set; Monte Carlo (often 200–1000 samples) at milestones; high-sigma and full-chip mixed-signal regressions at tape-out. Results are signed off as a **frozen history** tied to a schematic version.

**What to notice.**
- Analog tools already treat **results as saved state tied to a setup**, and reuse unchanged points (reference history). That is our `rev`-keyed run table.
- The weak spot (well known among users **[U]**): the link between a history item and the *current* schematic is loose. A designer can look at a green history from before a schematic edit. Staleness is the user's job to notice.
- Run plans are the analog form of "cheap first, expensive only if cheap passes".

### 2.5 Digital CI with Verilator; FPGA flows

- Verilator compiles RTL to C++ and is free and fast, so open-source hardware projects run it in ordinary CI on every PR (OpenTitan, CHIPS Alliance cores, many RISC-V cores) **[K]**; commercial simulators with licences run nightly or on a farm **[U]**.
- Formal property checks (SymbiYosys, JasperGold) are often split into a quick bounded depth per PR and deep proofs nightly **[U]**.
- FPGA: synthesis + place-and-route takes minutes to hours, so it is typically nightly or on release; timing closure is a sign-off gate; a small "does it still synthesize" build may run per PR **[U]**.

### 2.6 What chip teams run when

| Tier | Digital | Analog / AMS | Mark |
|---|---|---|---|
| Every commit / check-in | Lint, compile/elaborate, smoke regression (few tests, 1 seed) | Netlist, DC op-point sanity, typical-corner quick tests | [U] with OpenTitan `smoke` as [V] example |
| Nightly | Full regression, many seeds, coverage on | Full PVT corners on key specs | [U]; OpenTitan `nightly` with `cov` [V] |
| Weekly / milestone | Gate-level sims, long tests | Monte Carlo on key specs, AMS top-level | [U] |
| Sign-off | Coverage closure, all waivers reviewed, formal | High-sigma, full PVT+MC, post-layout extracted | [U] |

---

## 3. Results as state

| System | Key of a result | How freshness is shown | History |
|---|---|---|---|
| Bazel (local and remote cache) | Hash of the action: inputs, command, env **[K]** | `(cached) PASSED` vs `PASSED`; never shows an old result for changed inputs | Build Event Protocol → dashboards (BuildBuddy etc.) **[K]** |
| Go test cache | Test binary + cacheable flags + files/env the test read **[V]** | `(cached)` | None built in |
| ccache | Hash of preprocessed source (or of source + headers in direct mode) + compiler + flags **[K]** | Transparent; stats show hit rate | None |
| nextest / JUnit XML | None (no caching) | Every run fresh | JUnit XML → CI dashboards, flaky trackers **[K]** |
| ADE Assembler | History item = setup snapshot + results **[V]** | User picks the history; reference history mixes old and new points **[V]** | Named histories, lockable **[K]** |
| vManager | Session per regression; mapped onto the vPlan **[V]** | Plan shows metric progress from the chosen sessions | Trend over sessions **[K]** |
| Spreadsheets (Excel manual calc) | None | Status bar shows "Calculate" when values are stale **[K]** | None |
| Jupyter | None | Execution counters show order; stale cells are not flagged **[K]** | None; a known source of wrong conclusions |

The lesson from the left column: **the key must include every input**, or a cache returns wrong answers (Bazel's `external` tag exists exactly for tests whose inputs it cannot see). For us the inputs are known: the elaborated design, knob table, contract, device models, engine settings, **and the engine version** (an ngspice upgrade must invalidate results).

---

## 4. The patterns

### 4.1 How runs are classified

| Axis | Values seen | Examples |
|---|---|---|
| **Cost** | small/medium/large/enormous; slow; short | Bazel `size`, nextest `slow-timeout`, pytest `slow`, Go `-short` |
| **Purpose** | unit, integration, smoke, perf, fuzz, characterization, coverage | JUnit tags, dvsim regressions, ADE tests |
| **Trigger** | save, commit, PR, merge, nightly, release, manual | CI config, Bazel `manual`, `#[ignore]` |
| **Determinism** | deterministic, flaky, external | Bazel `flaky`/`external`, nextest FLAKY |
| **Budget style** | fixed work vs open-ended with a budget | unit test vs fuzz `max_total_time`, proptest cases, UVM seeds, Monte Carlo samples |

The best systems keep these axes **separate**. Cost is a property of the test; trigger is a property of the project; purpose is a label.

### 4.2 Who decides

| Decision | Author | Tool | Project config |
|---|---|---|---|
| This is a check | ✔ | | |
| Rough cost class | ✔ (Bazel size) or measured | ✔ (nextest measures and flags SLOW) | |
| Exact timeout | | ✔ (derived from class) | ✔ (overrides) |
| When it runs | only as an exception (`manual`, `#[ignore]`) | | ✔ (profiles, CI) |
| Which tests a change affects | | ✔ (graph, ML) | |
| Is a stored result valid | | ✔ (hash) | |
| Flaky / quarantine | | ✔ (detects) | ✔ (policy) |

### 4.3 How it's written

| Form | Examples | Good for |
|---|---|---|
| Source annotation | `#[ignore]`, `@pytest.mark.slow`, `@Tag`, `testing.Short()` | Facts about *this* test that travel with it |
| Build-rule attribute | Bazel `size`, `shard_count`, `tags` | Declared cost and resources, checked by the tool |
| Project config with filters | nextest profiles + overrides, dvsim regressions, VSIF | Triggers, budgets, grouping; changes without touching tests |
| Command-line selection | `-m "not slow"`, `--profile ci`, `--test_tag_filters` | The person running decides today's subset |

---

## 5. Mistakes to avoid

| Mistake | Where it shows up | Why it hurts | For us |
|---|---|---|---|
| **Stale results shown as current** | Jupyter, ADE histories after a schematic edit **[U]** | People sign off on old answers | Every verdict carries a freshness state; stale is visually distinct |
| **Cache key misses an input** | Bazel needs `external`; Go only tracks files it sees | Wrong "cached PASS" | Key includes models, engine version, engine settings, setups |
| **Timeout counted as fail (or pass) silently** | Default test runners | Slow specs flip red for infra reasons, or hide problems | Budget exhaustion is `UNDECIDED: budget`, its own status |
| **Retry until green** | Flaky-test culture | Real bugs hidden | Numerical fragility is its own status with the evidence (reltol band) |
| **Free-form tags without a registry** | pytest without `--strict-markers` | Typo drops a test out of every tier | Fixed attribute set, unknown attribute = error |
| **Ignored tests that never run** | `#[ignore]` with no CI job running `--ignored` | Rot | Every class must be run by some profile; tool warns if a spec is in no profile |
| **Author guesses seconds** | Hand-written timeouts | Wrong and drifting | Derived cost class from the plan; measured time recorded |
| **Trigger policy in the test** | Tests that check `$CI` themselves | Hard to change policy | Triggers only in the project file |
| **Selection as the only defence** | Change-based only, no full runs | Missed interactions | Scheduled full runs even if selection is exact |
| **One huge regression** | Everything nightly | Feedback in 12 h | Cheap classes on edit, expensive on schedule |
| **Results with no history** | Plain CI logs | Can't see when a margin started to shrink | Store verdict + margin per rev; show trend |
| **Characterization mixed with verification** | ADE outputs with and without limits in one table **[U]** | Noise in pass/fail | Already addressed by `#[outside]`; keep it out of gating profiles |

---

## 6. Lessons for spicy's language and tooling

### 6.1 Cost: derived, overridable

The engine can compute a spec's cost before running it, because the plan (engine_plan.md §2) knows the analyses, the knob count, the transient length and the sample count. So, unlike Bazel, **we don't need the author to declare size**. Proposal:

| Class | Rule (sketch, to be measured) | CE amp / LDO example |
|---|---|---|
| `quick` | op / AC / DC only, corner enumeration fits a small budget | `bias`, `gain`, `psrr` |
| `slow` | transient, many knobs, or sigma search | `dip` on `LoadStep`, `settling` |
| `long` | Monte Carlo sample counts, fault sweeps, board-level searches | a `#[fault]` setup × every spec |

The tool shows the derived class and the measured time of the last run (nextest-style "SLOW"). The author overrides only when the rule is wrong:

```
#[slow]                          // force into the slow class
spec dip: tran(vout.v).min() >= 3.25V on LoadStep in Run;

#[manual(reason = "30 min board search")]   // like Bazel `manual`: only when named
spec startup_all_loads: ...;
```

`#[outside]` setups (characterization) already exist and are never gating. Keep the attribute set **closed** (unknown attribute = error, the `--strict-markers` lesson).

### 6.2 Triggers: in a project file, as profiles

The contract is the verification plan (vPlan); the profile is the regression (VSIF / dvsim regression). Keep them apart:

```toml
# spicy.toml  (sketch)
[profile.edit]          # editor, on save, debounced, cancellable
classes = ["quick"]
select  = "affected"    # only specs whose inputs changed
budget  = "2s"

[profile.commit]        # pre-commit / PR CI
classes = ["quick", "slow"]
select  = "affected"

[profile.nightly]
classes = ["quick", "slow", "long"]
statistics = { samples = 2000 }   # same spec, bigger budget (fuzz/proptest lesson)

[profile.signoff]
classes = "all"
include_manual = true
engines = ["ngspice", "xyce"]     # second backend (engine_plan §1.1 "Later")
require_fresh = true              # fail if any verdict is stale or budget-limited
```

`spicy check --profile nightly`, `spicy check dip` (by name, like `cargo test <filter>`), `spicy check --class slow`. The tool warns if a spec falls in no profile (the "ignored forever" mistake).

### 6.3 Results: state with freshness

Store per spec, per side: verdict, margin, **input hash**, engine version, class, run time, profile, budget used. engine_plan.md already stores `.spicy/checks/<rev>.json` per design revision; extend the key from "whole design rev" to **per-spec input hash**, so an edit to the LDO's load-step setup invalidates `dip` but not `output`. Show in editor and report:

| State | Meaning | Display |
|---|---|---|
| `fresh PASS / FAIL` | Hash matches | normal |
| `stale PASS / FAIL` | Inputs changed since | greyed, with "changed: r3.value" |
| `never run` | No result yet | hollow |
| `running` | In progress (cancellable) | spinner |
| `UNDECIDED: budget` | Budget ran out (sigma search, MC) | with confidence reached and `next` |
| `UNDECIDED: numeric` | Result moves with tolerance / convergence | the flaky analogue |

A stale PASS is never counted as a PASS in a `require_fresh` profile. This fixes the ADE weak spot.

### 6.4 Selection: exact, from elaboration

We have what software lacks: an exact dependency graph. After elaboration, each spec side knows which knobs, parts, setups and models it reads. On an edit, re-run only specs whose input hash changed (Bazel/jest style, but exact). Then **reuse unchanged points** inside a spec, as ADE's reference history does: a new corner added to `env temp` re-runs only the new corners. Scheduled full runs remain (TAP lesson).

### 6.5 Long and statistical specs: budgets and carry-over

Treat Monte Carlo and sigma searches like fuzzing: the **same spec** runs with a small budget on commit and a large budget nightly, and **keeps its samples** across runs when inputs are unchanged (corpus lesson). The verdict reports the confidence reached. A timeout is a budget verdict, never a silent fail (nextest's `on-timeout = "pass"` for fuzz is the explicit version).

### 6.6 Order cheap first

Run plans (ADE) and smoke regressions agree: run `quick` specs first; if a quick spec fails, the slow ones for the same block can wait (or run anyway in CI, but report the quick failure first). For agents this matters: an agent editing a circuit should get the `edit` profile answer in the time of one turn, and see `stale` badges on slow specs it hasn't re-run.

### 6.7 Open questions for the design discussion

1. Is the cost class purely derived, or should `pub spec`s declare one so parents can budget composition?
2. Per-spec hashing needs a stable "inputs of this spec" set from elaboration. Is that M1d's job or the engine's?
3. Should a profile be allowed to *narrow knob ranges* (e.g. edit profile checks corners only, nightly adds interior search)? That is Go `-short` inside a spec; powerful, but it changes what "PASS" means and must be shown.
4. Where does CI history live: `.spicy/checks/` in git, or an external store like a Bazel remote cache?

---

## Sources

- nextest: [slow tests](https://nexte.st/docs/features/slow-tests/), [test groups](https://nexte.st/docs/configuration/test-groups/)
- Bazel: [test encyclopedia](https://bazel.build/reference/test-encyclopedia)
- Go: [cmd/go test packages and caching](https://pkg.go.dev/cmd/go#hdr-Test_packages)
- Google TAP: Memon et al., "Taming Google-Scale Continuous Testing", ICSE-SEIP 2017, [ACM](https://dl.acm.org/doi/10.1109/ICSE-SEIP.2017.16)
- Meta: Machalica et al., "Predictive Test Selection", ICSE-SEIP 2019, [arXiv 1810.05286](https://arxiv.org/abs/1810.05286)
- Launchable / CloudBees: [acquisition](https://www.cloudbees.com/newsroom/cloudbees-acquires-launchable-to-boost-genai-efforts-across-devsecops), [predictive test selection docs](https://docs.cloudbees.com/docs/cloudbees-smart-tests/latest/features/predictive-test-selection)
- Flaky tests: Micco, [Google Testing Blog 2016](https://testing.googleblog.com/2016/05/flaky-tests-at-google-and-how-we.html)
- cargo-watch status: [README](https://github.com/watchexec/cargo-watch)
- ClusterFuzzLite: [running ClusterFuzzLite](https://google.github.io/clusterfuzzlite/running-clusterfuzzlite/)
- OpenTitan dvsim: [common_sim_cfg.hjson](https://github.com/lowRISC/opentitan/blob/master/hw/dv/tools/dvsim/common_sim_cfg.hjson)
- Cadence: [vManager datasheet](https://www.cadence.com/en_US/home/resources/datasheets/vmanager-ds.html), [MDV white paper](https://semiwiki.com/wp-content/uploads/2018/03/maximizing-metric-driven-ver-wp.pdf), [ADE Assembler datasheet](https://www.cadence.com/en_US/home/resources/datasheets/virtuoso-ade-assembler-ds.html), [run plans training](https://www.cadence.com/en_US/home/training/all-courses/86256.html), [reference history](https://support1.cadence.com/public/docs/content/20495458.html), [plan-based analog verification](https://www.cadence.com/en_US/home/resources/white-papers/plan-based-analog-verification-methodology-wp.html)
- Siemens Solido: [Design Environment](https://www.siemens.com/en-us/products/ic/solido/design-environment/), [Xcelerator Academy blog](https://blogs.sw.siemens.com/xcelerator-academy/2024/10/21/solido-design-environment-a-solution-for-variation-aware-and-high-sigma-verification-challenges/)

**Not verified this session:** Synopsys VC Execution Manager / Verdi Planner details; the per-tier split in chip teams (commit/nightly/sign-off) beyond OpenTitan's regressions; the ADE "stale history" weakness (user lore); Verilator-in-CI and FPGA tiering specifics; rust-analyzer, jest, pytest, JUnit, Bazel caching flags and ccache behavior are stated from their well-known docs without re-fetching.
