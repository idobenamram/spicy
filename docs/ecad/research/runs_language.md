# When and How Simulations Run: Language, Project Config, CLI, Editor, CI

> 2026-09-30 · Research note. Question from the project lead: *specs are like tests. Some are fast and can run continuously while editing; others take minutes and belong in CI or manual runs. The language doesn't reflect this at all, and it must.*
> Reads with: `contract_syntax_v5.md` (the language and its example), `engine_plan.md` §1.1 (when the engine runs) and §3 (verdicts and statuses), `engine_types.md` §9 (the report and its store), `engine_flows.md` §3–§5 (triggers, the run cache, scheduling tiers), `contract_hierarchy.md` §4 (block-scoped result keys, `rests_on`), `agent_flows.md` §4 and §6 (the agent's tools and guardrails), `contract_tool_gaps.md` G17 and P11 (CI and result diffs), `language_specs.md` §11.3 (the results store and `signoff.lock`).
> `runs_references.md` appeared as this note was finished. Its three-way split (what a check costs, when it runs, whether the stored answer is still true) matches §2 and §7 here, and it recommends the same shape: a derived cost class, a project-level trigger policy, override attributes, and content-keyed results. Two of its references are added to §10. `runs_cost_tiers.md` did not exist yet and is not used. Where this note gives a cost it is marked **[E]**: a rough estimate made for this note from the plan's measured figures (0.4 s for 579 DC/AC runs of the CE amp on libngspice, `engine_plan.md` §2.8) and stated assumptions about per-run cost. They are for illustration only; nothing was simulated.

---

## Summary

**The problem.** Today one command, `spicy check`, runs every spec of a file, cold. That is fine for the CE amp (0.4 s). It is not fine for the v5 example as a whole. There the gain stage's settling spec, the sensor board's wake transient and its unplug fault could each take minutes **[E]**, while the CE amp, the LDO's DC specs and the reversed-polarity check take under a second. Nothing in the language, the config or the report says which is which. Nothing says what runs while you type, what runs in CI, what gates a merge, or what the editor should show for a spec that hasn't been run for this revision.

**One foundation, whatever option is chosen.** Every option below needs the same three pieces. Without them, tiers are just a way to skip work and forget about it:
1. **A per-spec content key.** It is a hash of exactly what the spec's verdict depends on: the block's elaborated definition, the setups it uses, the `env` and `const` values it reads, the spec text, the confidence, the engine settings, and the engine and backend versions. This refines today's whole-design `rev` (`engine_types.md` §9.3) and matches the "(spec path, design hash, engine version)" key that `language_specs.md` §11.3 already promises.
2. **A result store keyed by it,** shared between the editor, the CLI, CI and the agent. A verdict computed by CI last night is the verdict for your editor today if the key matches, and it is shown as fresh, with who produced it.
3. **Display states beyond PASS/FAIL:** fresh, running, queued, pending (waits for CI), stale (from which revision, with its old verdict), never run, skipped, and conditional (rests on a child result that isn't fresh).

**The three options differ only in who decides when each spec runs:**

| | Who decides | What the author writes | Strong at | Weak at |
|---|---|---|---|---|
| **(a) Per-spec annotations** | The author, in the `.spl` file | `#[run(ci)]`, `#[run(manual)]`, `#[budget(30s)]` | Visible in the text and in code review; simple rules | Guesses go stale as circuits grow; clutter; machine speed isn't in the text |
| **(b) Project profiles** | The team, in `spicy.toml` | `[profile.ci] select = "not attr(outside)"` | Different policies for laptop, CI and nightly without touching designs | You can't see from a spec when it runs; selectors by kind (`tran`, `fault`) are poor proxies for cost |
| **(c) Automatic tiering** | The engine, from its cost estimate | Nothing (overrides allowed) | Zero effort; adapts as the circuit grows; the agent can reason from the numbers | Estimates are rough before the first run; placement can move between machines |

**Recommendation: a hybrid, "gate by meaning, schedule by cost".**
- **What gates a merge is decided by meaning, and it is deterministic.** Every spec gates CI, faults included, except `#[outside]` characterization and specs the author marks `#[run(manual)]`. Machine speed and cost estimates never change the gating set.
- **When a spec runs locally is decided by cost, automatically.** The engine estimates each spec's cost and places it in `live` (while you type), `save`, or `ci` (not run locally, pending until CI runs it) against budgets in a small `spicy.toml`. The editor shows every spec's tier and estimate.
- **The author can override** with one attribute, `#[run(live | save | ci | nightly | manual)]`, on a spec, a setup or a contract. It is also where the author states intent ("nightly: this Monte Carlo is a release gate, not a merge gate").
- **The content-keyed store makes tiers compose.** A parent's live check may rest on a child's `pub spec` that only CI can afford. It is trusted when the child's key matches, the record came from CI on a protected branch, and CI re-runs a sample of cache hits.

The case that decides it is in the v5 example itself. The reversed-polarity fault is a DC check, it takes well under a second **[E]**, and it **fails** as drawn (−1.53 V on the LDO's input against a −0.3 V rating). A kind-based rule such as "faults run in CI" (option b's natural selector) hides that failure from the person drawing the board until CI. Cost-based placement puts it in `live`, where it belongs.

**What changes.**
- `contract_syntax_v5.md`: one attribute (`#[run(…)]`), an optional `#[tag(…)]` and `#[budget(…)]`, and a new rule section, "1.6 When specs run", which says that a tier changes when a spec runs, never what it means.
- `engine_plan.md`: §1.1 gains the tiers and the store; §3.6 gains the display states; §7.5 gains the rule for claims with pending specs.
- `engine_types.md` §9: the store is keyed per spec, and records carry who produced them and what they rest on.

**What the MVP needs.**
- `spicy check` runs everything, as planned, and exits non-zero on any FAIL or UNDECIDED, so it can gate CI with a five-line workflow step.
- The parser accepts `#[run(…)]`; the MVP honours only `manual` (skipped unless `--include-manual`, and reported as "not run", never as a pass).
- `--only <spec>` and `--skip <spec>` on the CLI.
- Two seams in the report: a per-spec `key` field and each spec's measured wall time. They are needed later for the store and the estimates.

---

## 1. The problem, on the v5 example

Every check in the v5 example (`contract_syntax_v5.md` §2), with its kind of simulation and a rough cost. The costs assume libngspice, one worker, and these per-run costs **[E]**:
- the CE amp's DC and AC runs: 0.7 ms each (measured: 0.4 s / 579, `engine_plan.md` §2.8);
- a vendor macro-model (LDO, op-amp) in DC or at a few AC points: about 2–5 ms; over a dense AC sweep: about 10 ms;
- transient runs: 20 ms for a 200–300 µs window with 1 µs edges, 0.5 s for the 5 ms board wake, several seconds for the 1 s unplug window with its fast edges.

Knob counts are the cone sizes the engine would see. Enumeration is capped at 2^12 corners per side (`engine_plan.md` §1.3).

| Block | Check | Setup | Kind | Knobs **[E]** | Cost **[E]** | Natural home |
|---|---|---|---|---|---|---|
| CeAmp | `bias`, `gain`, `bass`, `base_bias` | Operating | DC, AC | 8 | **0.4 s** for all four (measured) | live |
| Ldo3v3 | `output`, `quiescent` | Operating, both modes | DC | 7 | ≈ 1 s (they share runs) | live |
| Ldo3v3 | `psrr` | Operating, `vin.v` pinned | AC at 100 kHz | 6 | ≈ 0.5 s | live |
| Ldo3v3 | `output_z` (**pub**) | Operating, Run | AC sweep 10 Hz–1 MHz | 7 | ≈ 3 s; **minutes** on a transistor-level vendor model | save |
| Ldo3v3 | `dip` | LoadStep, Run | tran, 300 µs | 6 | ≈ 3–10 s | save |
| Ldo3v3 | `dropout` | DropoutRow (`#[outside]`) | DC sweep | 6 | ≈ 1 s | manual: characterization, never gates |
| Ldo3v3 | `rated vin.v` | every setup | reads existing runs | — | ≈ 0 | with whatever runs |
| GainStage ×2 (`#[check(A = [Mcp6001, Tlv9001])]`) | `gain`, `bandwidth`, `loop_ok`, `input_z`, `output_z`, `supply` | Operating | DC, AC | ≈ 12 | ≈ 10–20 s for both op-amps | save |
| GainStage ×2 | `settling` | InputStep | tran, 200 µs | ≈ 12 | 2 × 4,096 corners × 20 ms ≈ **3 min** | ci |
| SensorBoard | `sensitivity`, `rail_ok` | AtZero, AtFull, Operating | DC | ≈ 18 | over 2^12: **UNDECIDED (budget)** in the MVP; seconds with the loop (after M3) | live or save |
| SensorBoard | `wake_noise` | McuWakes | tran, 5 ms | ≈ 18 | **minutes** | ci |
| SensorBoard | `holdup`, `recovers` (share runs) | Unplug (`#[fault]`) | tran, up to 1.01 s, `lasts` swept | ≈ 19 | **10–30 min** | ci or nightly |
| SensorBoard | `rated` on Reversed | Reversed (`#[fault]`) | DC | ≈ 18 | **under 1 s** (one DC run per point) | **live: and it FAILS as drawn** |

Four things this table shows, which every option must handle:

1. **Cost spans five orders of magnitude inside one small project.** The fast end runs in under a second; the slow end takes half an hour **[E]**.
2. **The kind of setup is a poor proxy for cost.** Both fault setups are `#[fault]`. One is a DC check under a second that catches a real bug; the other is a long transient. The `#[outside]` row is cheap but must never gate.
3. **Cost belongs to groups of specs that share runs, not to single specs.** `holdup` and `recovers` read the same Unplug runs; the LDO's `output` and `quiescent` share DC runs (`engine_flows.md` §4.1: "one run serves many measures"). Putting `holdup` in `live` and `recovers` in `ci` would buy nothing: the runs are the cost. **Placement must be per run group** (specs with the same block, setup and analysis), even when it is shown per spec.
4. **Cost changes with the design, not the spec text.** GainStage's specs cost twice as much because `#[check]` lists two op-amps. Adding a third doubles nothing in any spec's text. A board spec's cost jumps when a child gains a knob.

---

## 2. The foundation every option needs

### 2.1 The per-spec key

```
spec key  =  hash(  the block's definition after elaboration              (its circuit, its parts' models and D-A defaults)
                  + for a flat check: its children's definitions           (for a composed check: the children's result keys, see §2.5)
                  + the setups the spec uses, after derivation and modes   (Operating, LoadStep in Run, …)
                  + the env and const values it reads                      (ambient = -10°C..=60°C)
                  + the spec's normalized text                             (measure, limit, pins, sweep axis, confidence)
                  + the rated limits checked on those setups
                  + the engine settings and policy                         (reltol, budgets, distribution defaults)
                  + seeds, for anything sampled                            (engine_flows.md §5.5)
                  + engine version + backend identity                      (ngspice-42 build id, options)
                  + the content hashes of model files and part records    (contract_tool_gaps.md P9)                )
NOT in the key: spans and comments; other specs; the tier; who ran it; the git commit
```

Why per spec and not per design:
- **An edit to the LDO's `c_out` changes the keys of `output_z`, `dip`, `psrr` and the board specs.** It leaves the CE amp's and GainStage's keys alone, so their results stay fresh. With one `rev` per design, every result would turn stale on every edit, and "stale" would mean nothing.
- **It makes `carried` (`engine_plan.md` §3.6) exact and cheap.** A verdict whose key didn't change is simply fresh. No cone tracking is needed at the verdict level.
- **It is the key `contract_hierarchy.md` §4.3 already proposed for block tables** ("NOT in the key: the placement path, the parent, the neighbours"), carried up to verdicts.

Precedent: `go test` caches a test's result and replays it when the test binary and the files and environment variables it read are unchanged. Nx and Turborepo key task results by a hash of their inputs and replay them from a local or remote cache. Bazel does the same for test actions. Each of them makes "only run what changed" a property of the key, not of a git diff.

### 2.2 The result store

```
.spicy/results/                                  local; never committed
  7c/2e41…json        one record per spec key: verdict records (engine_types.md §9.1) + provenance + cost
  index/<spec path>   → the last few keys seen for that spec, with their commits (to show "stale since …")
remote (optional)                                shared; written only by CI
  GitHub Actions cache, an object store, or a results branch (§6.5)
signoff.lock                                     committed: every gating spec's key and verdict for a released commit
```

Each record gains three fields beyond today's `Record`:

```json
{ "key": "7c2e41a9…",
  "produced_by": { "who": "ci", "commit": "a1b2c3d", "run": "github:812", "trusted": true },
  "cost": { "runs": 8193, "wall_s": 182.4, "backend": "ngspice-42" },
  "rests_on": [ { "spec": "ldo.output_z", "key": "91fa…", "verdict": "PASS (all corners)", "via": "containment ok" } ] }
```

### 2.3 The display states

One set of words, used by the terminal, the editor, the JSON and the agent:

| State | Meaning | Shown as (editor gutter · terminal) |
|---|---|---|
| **fresh** | A record exists for the spec's current key | `✓ PASS` · `✗ FAIL` · `? UNDECIDED`, with `· ci a1b2c3` when someone else produced it |
| **running** | In the current batch | `⟳ 3/7 rounds` |
| **queued** | Will run locally when its tier fires (on save, on idle) | `◷ queued · save · ~8 s` |
| **pending** | Won't run locally; waits for CI (or nightly) | `⧗ pending · ci · ~3 min`, plus its last verdict if any |
| **stale** | No record for the current key, but one for an earlier key of the same spec | `PASS (stale since a1b2c3 · changed: ldo.c_out)`, dimmed |
| **never run** | No record for any key of this spec | `○ never run · ci · ~3 min` |
| **skipped** | `#[run(manual)]`, or excluded by `--skip`; not gating | `– manual · never run` or `– manual · PASS at e4f5a6` |
| **reported** | `#[outside]` characterization: a number, not a gate | `dropout = 212 mV (reported)` |
| **conditional** | A composed PASS whose child records aren't all fresh | `✓? PASS if ldo.output_z still passes (pending · ci)` |

**Stale and pending combine.** A spec can be pending *and* have a stale verdict: `⧗ pending · ci · last: PASS at a1b2c3`. The editor must never render a stale verdict in the same colour as a fresh one (`engine_plan.md` §3.6: stale "proves nothing about this design").

**"Stale since" needs the index.** When the key changes, the index still holds the old key and the commit that produced it. The editor diffs the two elaborated inputs to name what changed ("changed: `ldo.c_out` 1 µF → 2.2 µF"). That diff is cheap because elaboration is.

### 2.4 The tier ladder

Every option uses the same ordered ladder. Only the placement rule differs.

| Tier | Fires | Typical budget | Gates |
|---|---|---|---|
| `live` | after each edit, debounced 50–100 ms (`engine_flows.md` §5.2) | ≈ 1–2 s for the batch | nothing: it is feedback |
| `save` | on save or idle, and before the agent reports | ≈ 30 s | nothing locally; a pre-commit hook may run it |
| `ci` | every push and pull request | ≈ 20 min for the PR's changed keys | **merges** |
| `nightly` | on a schedule | hours | **releases** (and opens an issue on a FAIL) |
| `manual` | only when named (`--only`, `--include-manual`) | none | nothing |

Sign-off is not a tier. It is a command (`spicy signoff`, §7.3) that requires every gating result to be fresh and trusted, runs the nightly extras, and writes `signoff.lock`.

### 2.5 Composition: a parent resting on a child's slow `pub spec`

The question: can the sensor board's fast live check rely on `Ldo3v3`'s `pub spec output_z`, which only CI can afford (on a transistor-level model, minutes)?

**Yes, through the store, exactly as `contract_hierarchy.md` §4.2 describes the evidence ladder's rung 1:**
1. The editor elaborates `SensorBoard`. For its child `ldo`, it computes the key of `Ldo3v3.output_z` (µs: no simulation).
2. It looks the key up: the local store, then the remote one.
3. It finds `PASS (all corners)` from CI run 812 on commit a1b2c3.
4. It runs the containment checks in context (fast): the board drives `ldo.vin` with 4.40–5.25 V through 0.1–0.5 Ω, inside `Ldo3v3`'s Operating setup (4.3–5.5 V, `z: ..=0.5Ω`). They hold.
5. The board's record says `PASS (conditional)` with `rests_on: [ldo.output_z @ 91fa…, ci:812, containment vin.v ok, vin.z ok]`. It is fresh because every key it rests on is fresh.

**What happens after an edit to the child:**

```
edit: Ldo3v3's c_out 1uF → 2.2uF
  Ldo3v3.output_z   key 91fa… → 3d07…   no record          ⧗ pending · ci · ~3 min · last: PASS at a1b2c3
  SensorBoard specs that rest on it      ✓? PASS if ldo.output_z still passes (pending · ci)
  CeAmp, GainStage  keys unchanged        ✓ fresh
```

**What makes it trustworthy:**

| Threat | Rule |
|---|---|
| The key misses an input (a model file, a backend update) | Everything the simulator reads is in the key (§2.1). The backend's build identity is part of it, so an ngspice upgrade invalidates every record at once |
| Someone uploads a doctored or local result | **Only CI on protected branches writes to the shared store.** Records from a laptop stay local and carry `trusted: false`. They never gate a merge. Bazel's remote-cache guidance is the same: CI writes, developers only read |
| The cache is wrong in a way the key doesn't see | CI re-runs a random sample of cache hits (say 2%) and compares them bit for bit. `engine_flows.md` §3.7 already plans this audit for sign-off; §5.4 makes it possible, because every run is a pure function of its key |
| The child's record is weaker than the parent needs | The parent reads the confidence and method from the record. A parent at `sigma(3)` cannot rest on a child's `nominal`, and a parent at `worst_case` cannot rest on a child's `PASS (estimated)` without inheriting the word "estimated" |
| The child passes, but not in this placement | Containment is checked in context on every port quantity, both ways (`contract_hierarchy.md` §4.1). If it fails, the child's verdict doesn't apply, and the board's spec needs an in-context check, in whatever tier its cost puts it |
| A FAIL | Composition proves PASSes only. A composed worst corner becomes a FAIL only after one flat run at that point (`contract_hierarchy.md` §4.4). That run is a single simulation, cheap enough for `live` |

So **a tier is never a hole in the evidence.** A slow child spec in `ci` makes the parent's live verdict conditional until CI has run it, and says so by name.

---

## 3. Option (a): per-spec annotations

The author says in the design file when each spec runs, like Rust's `#[ignore]`, pytest's markers or Bazel's `size =` and `tags = ["manual"]`.

### 3.1 Syntax and file layout

Three attributes, allowed on a `spec`, a `setup` (inherited by every spec that uses it) and a `contract` (inherited by all its specs). The innermost wins:

```rust
#[run(live | save | ci | nightly | manual)]   // the earliest tier this spec may run in
#[budget(30s)]                                  // a ceiling: warn (or fail in CI) when a run takes longer
#[tag(power, thermal)]                          // free labels for selecting
```

The v5 example, annotated:

```rust
// ───────────────────────────── ldo.spl ─────────────────────────────────
#[outside(reason = "datasheet dropout row: defined at the rated 500 mA")]
setup DropoutRow for Ldo3v3 { vout.i: 500mA, vin.v: Sweep(4.5V -> 3.0V), ..Operating }

contract Ldo3v3 {
    setup = Operating;
    rated vin.v within -0.3V..=6.5V;

    spec output:    dc(vout.v)                            within 3.3V ± 2%;
    spec psrr:      ac(vout.v / vin.v).at(100kHz).db()    <= -36dB   with vin.v = 4.3V;
    #[run(save)] #[budget(20s)]
    spec dip:       tran(vout.v).min()                    >= 3.25V   on LoadStep in Run;
    spec quiescent: dc(vin.i + vout.i)                    <= 50uA    in Sleep;
    #[run(manual)]
    spec dropout:   dc(vin.v - vout.v).first(vout.v < 3.267V)   <= 250mV   on DropoutRow;

    #[run(ci)] #[tag(interface)]
    pub spec output_z: vout.z(f)                          <= 2Ω      for f in 10Hz..=1MHz  in Run;
}

// ───────────────────────────── gain_stage.spl ──────────────────────────
#[check(A = [Mcp6001, Tlv9001])]
#[run(save)]                                   // the whole contract: ~10–20 s [E]
contract GainStage {
    setup = Operating;
    …
    #[run(ci)] #[budget(5min)]
    spec settling:  tran(vout.v).settle(1%)   <= 50us     on InputStep;
    …
}

// ───────────────────────────── board.spl ───────────────────────────────
#[run(ci)]
setup McuWakes for SensorBoard { event wake: Trigger { of: mcu.wake, at: 1ms }, window: ..=5ms, ..Operating }

#[fault] #[run(nightly)] #[budget(45min)]
setup Unplug for SensorBoard { … }

#[fault]                                        // no #[run]: a DC check, runs live, and catches the -1.53 V bug
setup Reversed for SensorBoard { usb.polarity: reversed, ..Operating }
```

**The default with no attribute is `live`.** That is the only default that doesn't hide anything: an unannotated slow spec makes live checks slow, and the editor says so ("`settling` took 3 min in live; add `#[run(ci)]`?"). The other choice, "unannotated means CI", would hide new specs from their authors.

**Multi-setup specs** take the latest tier of the spec and its setups: `sensitivity(zero: AtZero, full: AtFull)` runs in `live` unless one of its setups says otherwise.

**No project file is needed**, but the tier names are fixed by the language, and so are their budgets' meaning.

### 3.2 CLI

```
spicy check                        # every tier except manual (like `cargo test` skipping #[ignore])
spicy check --tier save            # live + save only: the pre-commit run
spicy check --tier ci              # live + save + ci: what CI runs on a PR
spicy check --include-manual       # everything
spicy check --only ldo.dropout     # one spec, whatever its tier (like `cargo test -- --ignored name`)
spicy check --tag interface        # every spec tagged `interface`
spicy check --changed              # only specs with no fresh record for their key (§2.1)
```

`--changed` compares keys against the store, not files against git. It is exact: a comment edit changes nothing, and a model-file update changes every spec that uses that model.

### 3.3 Editor

The attribute sits above the spec, so the tier is visible where the spec is written. The gutter adds the state:

```
   spec output:    dc(vout.v)  within 3.3V ± 2%;                       ✓ PASS · 3.271–3.329 V
   #[run(save)] #[budget(20s)]
   spec dip:       tran(vout.v).min() >= 3.25V on LoadStep in Run;       ◷ queued · save
   #[run(ci)]
   pub spec output_z: vout.z(f) <= 2Ω for f in 10Hz..=1MHz in Run;      ⧗ pending · ci · last PASS at a1b2c3 (stale: c_out changed)
```

When a measured run blows its budget or its tier's budget, the editor offers a fix-it: "`dip` took 38 s (budget 20 s). Move to `#[run(ci)]`, or raise the budget?"

### 3.4 CI

```yaml
# .github/workflows/specs.yml
on: [pull_request, push]
jobs:
  specs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/cache@v4
        with:
          path: .spicy/results
          key: spicy-${{ hashFiles('**/*.spl', 'models/**') }}
          restore-keys: spicy-                  # a partial restore is safe: records are keyed by content
      - run: spicy check --tier ci --changed --format json,junit --base origin/${{ github.base_ref }}
      - uses: actions/upload-artifact@v4
        with: { name: spec-report, path: .spicy/report/ }
```

- **The gate:** the job fails if any spec at `ci` or below is FAIL or UNDECIDED on the head, unless it carries `#[expect(fail, reason = …)]` (`language.md` §8.3).
- **The report:** JUnit XML (GitHub and most CI UIs render it), the JSON for machines, and a verdict-delta table against the base in the job summary (§7.1).
- **Nightly:** a scheduled workflow runs `spicy check --tier nightly` and writes the shared store.

### 3.5 The agent

The agent reads the attributes: it knows `settling` is a CI spec without asking. Before claiming a fix, it runs every affected spec at `save` or below, and names the affected `ci` and `nightly` specs as pending (the full rule is in §7.2).

### 3.6 Faults, Monte Carlo, sign-off

- **Faults** are annotated like anything else: `#[fault] #[run(nightly)]` on Unplug, nothing on Reversed.
- **Monte Carlo:** a `#[confidence(yield(99.9%))]` spec gets `#[run(nightly)]` by hand. The attribute can't know that the same yield spec costs 1 s on the CE amp (`engine_flows.md` §3.7) and an hour on the board.
- **Sign-off** runs every tier except `manual`.

### 3.7 Composition

It works through the store (§2.5). The annotation adds one useful fact: a parent can see statically that it rests on a `#[run(ci)]` child spec, so the editor knows it will be conditional in live before any lookup.

### 3.8 Pros and cons

| Pros | Cons |
|---|---|
| **Visible in the text,** where the spec is read and reviewed. A PR that moves `settling` from `ci` to `manual` shows up in the diff, which is exactly the change a reviewer should question | **Authors guess, and the guesses rot.** `GainStage` goes from `save` to `ci` when someone adds a third op-amp to `#[check]`; nobody edits the attribute |
| Simple rules, no config file, familiar to anyone who has used `#[ignore]` or pytest markers | **Machine speed isn't in the text.** "Save" on a laptop is not "save" on a 64-core CI runner |
| Deterministic: the same file gives the same tiers everywhere | **Clutter:** the v5 example needs about 7 attributes, and a real board would need dozens |
| The agent can read the intent directly | **It mixes "what must hold" with "when to check it"** in the design file. A reused block (`Ldo3v3` in another project) brings its author's scheduling guesses with it |
| | An unannotated slow spec either makes `live` slow (default live) or hides from its author (default ci). There is no good default without a cost estimate |

**Strong** for small teams with a few slow specs that everyone knows about. **Weak** as circuits grow and as blocks are reused across projects.

---

## 4. Option (b): project-level profiles

The design files say nothing about scheduling, apart from optional tags. A project file defines named profiles, each a selector plus budgets, like Cargo profiles, nextest profiles and filtersets, or pytest's `-m` expressions in `pytest.ini`.

### 4.1 Syntax and file layout

**Where: `spicy.toml`, not `project.spl`.** `project.spl` holds design meaning (`env`, `const`) that changes verdicts and is in every spec's key. Run policy changes no verdict, differs between a laptop and CI, and may be overridden per user (`~/.config/spicy/config.toml`), so it belongs in tool config. Cargo draws the same line between `Cargo.toml`'s profiles and the code, and nextest keeps its profiles in `.config/nextest.toml`.

```toml
# spicy.toml
[check]
default-profile = "save"

[profile.live]
select       = "not setup(fault) and not attr(outside) and not kind(tran) and not tag(slow)"
batch-budget = "2s"          # stop and report "queued" for whatever doesn't fit
on-exceed    = "queue"       # the rest waits for the next profile

[profile.save]
inherits     = "live"
select       = "not attr(outside) and not setup(fault) and not tag(slow)"
batch-budget = "30s"

[profile.ci]
select       = "not attr(outside) and not tag(nightly)"
gate         = "all-pass"    # or "no-regress": a PASS on the base may not become FAIL/UNDECIDED
spec-timeout = "20min"       # like nextest's slow-timeout / terminate-after
report       = ["junit", "json", "summary"]

[profile.nightly]
select       = "not attr(outside)"
extras       = ["yield", "uniform-spread", "audit(2%)"]

[profile.signoff]
inherits     = "nightly"
extras       = ["yield", "second-backend", "audit(5%)"]
require      = "fresh-and-trusted"
```

**The selector language** (modelled on nextest's filtersets: `test()`, `package()`, `kind()`, combined with `and`, `or`, `not`):

| Selector | Matches | Example on v5 |
|---|---|---|
| `spec(glob)` | spec path | `spec(ldo.*)` |
| `block(Name)` | every spec of a block | `block(GainStage)` |
| `setup(Name)`, `setup(fault)`, `setup(outside)` | by setup, or by the setup's attribute | `setup(fault)` → `holdup`, `recovers`, and the `rated` checks on Reversed |
| `attr(outside)` | characterization specs | `ldo.dropout` |
| `kind(dc \| ac \| tran)` | by the kind of simulation | `kind(tran)` → `dip`, `settling`, `wake_noise`, `holdup`, `recovers` |
| `pub` | published specs | `ldo.output_z`, GainStage's three |
| `confidence(yield)` | Monte Carlo specs | — |
| `tag(x)` | from an optional `#[tag(x)]` in the file | `tag(slow)` |
| `cost(< 5s)` | by estimated cost (needs option c's estimator) | — |
| `changed()` | no fresh record for the current key | — |
| `rests-on(spec)` | parents whose composed verdict uses that spec | `rests-on(ldo.output_z)` → the board's specs |

The design files stay as v5 wrote them, plus optional tags where a selector needs a hint:

```rust
#[tag(slow)]
spec settling:  tran(vout.v).settle(1%)   <= 50us     on InputStep;
```

### 4.2 CLI

```
spicy check                              # default-profile (save)
spicy check --profile ci                 # what CI runs
spicy check --profile ci --changed       # only specs whose key has no record
spicy check --only 'block(Ldo3v3) and kind(tran)'
spicy check --list --profile live        # what would run, and why (like `cargo nextest list`)
```

`--list` matters more here than in (a): it is the only way to see a spec's tier, since the file doesn't say.

```
$ spicy check --list --profile live
 ceamp.bias ceamp.gain ceamp.bass ceamp.base_bias   selected
 ldo.output ldo.quiescent ldo.psrr                  selected
 ldo.output_z                                        selected    (pub, ac: no rule excludes it)
 ldo.dip                                             excluded    kind(tran)
 ldo.dropout                                         excluded    attr(outside)
 board.rated@Reversed                                excluded    setup(fault)          ← the failing check
 board.holdup board.recovers                         excluded    setup(fault), kind(tran)
```

### 4.3 Editor

The editor shows the profile the spec falls into, computed from the selectors, with a hover that explains the rule:

```
   #[fault]
   setup Reversed for SensorBoard { usb.polarity: reversed, ..Operating }     ⧗ pending · ci
        └ hover: excluded from live and save by `not setup(fault)` (spicy.toml:4)
```

The problem is visible in that line. A check that costs under a second and fails as drawn is pending until CI, because the selector used setup kind as a stand-in for cost.

### 4.4 CI

The same workflow as §3.4, with `spicy check --profile ci --changed`. Profiles make one thing easy that (a) can't do: **CI can use different budgets and extras from the laptop without touching any design file** (a 64-core runner can afford `extras = ["uniform-spread"]` on every PR).

### 4.5 The agent

The agent calls `spicy check --list --profile …` (or `engine.plan`) to learn tiers. It follows the same claim rule as every option (§7.2).

### 4.6 Faults, Monte Carlo, sign-off

Profiles are at their best here:
- **Monte Carlo and the other nightly extras** are policy, not design: `extras = ["yield"]` in `nightly`, no attribute per spec.
- **Sign-off** is a profile with `require = "fresh-and-trusted"` and a higher audit rate.
- **Faults** are at their worst: `setup(fault)` lumps the one-second DC check with the half-hour transient.

### 4.7 Composition

Through the store (§2.5). `rests-on()` also lets CI put the children a parent relies on first in its queue.

### 4.8 Pros and cons

| Pros | Cons |
|---|---|
| **Policy lives apart from design.** A reused `Ldo3v3` brings its specs, not its author's scheduling | **You can't see from a spec when it runs.** You need `--list` or a hover, and a selector edit in `spicy.toml` silently moves specs between tiers |
| Different machines and teams, different budgets, one set of design files | **Selectors by kind are poor proxies for cost** (the Reversed row). `cost()` fixes that, but then the placement comes from option (c)'s estimator anyway |
| Monte Carlo, the uniform spread, the second backend and audits are naturally profile extras, not spec attributes | A second file format to learn, with its own mini-language |
| Familiar from Cargo, nextest and pytest | New specs land wherever the selectors happen to put them, and nobody notices until it's slow or skipped |

**Strong** for CI policy, nightly extras and sign-off. **Weak** at placing individual specs.

---

## 5. Option (c): automatic tiering from the engine's cost estimate

The author writes nothing. The engine estimates each run group's cost and places it in the earliest tier whose budget fits. The editor shows the placement and the estimate next to every spec. An attribute overrides it.

### 5.1 How the estimate is made

```
cost(group) ≈ runs(group) × seconds_per_run(analysis, circuit size, window) × (1 + retry share)

runs(group), before any simulation, from the plan (engine_plan.md §2):
  1 nominal + 2^|cone| corners (if within budget) + interior/audit + sigma search + band
  e.g. CE amp: 1 + 256 + 46 + 265 + 11 = 579 (exactly what §2.8 reports)
  × setups × modes × #[check] instantiations × fault-axis points

seconds_per_run, three sources in order of trust:
  1. measured: the spec's own last run, from the store (by spec path, any key)   → "~3 min (last run 2m 51s)"
  2. calibrated: a 1-second micro-benchmark on this machine per analysis kind, scaled by node count,
     AC points, and for tran by window / smallest edge                           → "~3 min (estimated)"
  3. unknown: a transient with a new vendor model never run here                 → "cost unknown: runs in save once to measure"
```

- **Placement is per run group** (§1, point 3): `holdup` and `recovers` move together.
- **Hysteresis,** so specs don't flap between tiers: a group moves to a cheaper tier only when its measured cost is below half that tier's budget. It moves up at once when a run exceeds its tier's budget.
- **Unknown costs run once in `save`,** in the background with a time limit. Measuring beats guessing, and the first measurement makes every later estimate good.

On the v5 example, with budgets live 2 s, save 30 s and ci 20 min, placement comes out as §1's "natural home" column, with no annotations. Reversed lands in `live` and fails there.

### 5.2 Syntax and file layout

Nothing in the design files. A minimal `spicy.toml` holds the budgets (the same keys as option b, but no selectors):

```toml
[tiers]
live = "2s"      # per batch
save = "30s"
ci   = "20min"   # a group estimated above this goes to nightly, and the check says so
```

Overrides use option (a)'s attribute, with one meaning: **pin this placement.**

```rust
#[run(manual)]
spec dropout: … on DropoutRow;       // (implied anyway by #[outside]; shown for clarity)
```

### 5.3 CLI

```
spicy check                  # live + save placements, then lists what's pending for CI with estimates
spicy check --tier ci        # everything placed at ci or below
spicy check --all            # every tier except manual, regardless of placement
spicy plan                   # the placement table: spec, group, tier, estimate, source of the estimate
```

```
$ spicy plan
 group                               specs                          tier     estimate        source
 CeAmp · Operating · dc+ac           bias gain bass base_bias      live     0.4 s           measured
 Ldo3v3 · Operating · dc             output quiescent               live     1.1 s           calibrated
 Ldo3v3 · Operating · ac@100kHz      psrr                           live     0.5 s           calibrated
 Ldo3v3 · Operating/Run · ac sweep   output_z (pub)                 save     3 s             calibrated
 Ldo3v3 · LoadStep/Run · tran        dip                            save     ?               unknown → measure once
 GainStage×2 · Operating · dc+ac     gain bandwidth loop_ok …       save     14 s            measured
 GainStage×2 · InputStep · tran      settling                       ci       3 min           measured
 SensorBoard · Reversed · dc         rated                          live     0.6 s           calibrated
 SensorBoard · McuWakes · tran       wake_noise                     ci       6 min           calibrated
 SensorBoard · Unplug · tran         holdup recovers                ci       22 min > 20 min  calibrated  ⚠ over the ci budget
 Ldo3v3 · DropoutRow · dc            dropout                        manual   1 s             #[outside]
```

### 5.4 Editor

Every spec gets a badge with its tier and estimate. Hovering shows where the estimate came from:

```
   spec settling:  tran(vout.v).settle(1%) <= 50us on InputStep;       ⧗ ci · ~3 min · last PASS at a1b2c3
        └ hover: 2 op-amps × 4,097 runs × 21 ms (measured 2026-09-29) · [Run now] [Pin to save]
```

**The editor also announces moves:** "Adding `Tlv9002` to `#[check]` moved `GainStage` specs from save to ci (14 s → 21 s × 1.5)." Otherwise a spec leaves `live` silently as the design grows, which is this option's main risk.

### 5.5 CI

Here the automatic option has a real problem: **if the gating set depends on estimates, the same PR can gate differently on two machines.** The fix is to keep estimates out of gating: CI runs every tier except `manual`, and estimates only order the work (cheap and likely-to-fail first) and decide what goes to nightly when over the CI budget. Once that fix is applied, option (c) has become the recommended hybrid (§6).

### 5.6 The agent

This is where (c) shines. The agent gets a number for every choice: "Verifying this fix needs `holdup` and `recovers` (≈ 22 min) and `settling` (≈ 3 min). I'll run `settling` now and leave the fault pair to CI." `spicy plan --changed --format json` is the agent's call.

### 5.7 Faults, Monte Carlo, sign-off

- **Faults** are placed correctly without any rule about faults: Reversed is live, Unplug is ci (or nightly when over budget).
- **Monte Carlo** is placed by its run count: 15,526 runs for a 99.865% yield proof at 95% confidence (`engine_flows.md` §3.7) is about 1 s on the CE amp and live-able. On the board it is hours, so nightly.
- **Sign-off** is not placement; it needs a command (§7.3).

### 5.8 Composition

Through the store (§2.5). The estimator adds one thing: when a parent rests on a pending child, the editor can say how long until it can be decided ("resting on `ldo.output_z`: ~3 min in CI").

### 5.9 Pros and cons

| Pros | Cons |
|---|---|
| **Zero author effort,** and correct on cases rules get wrong (Reversed vs Unplug) | **Estimates are rough before the first run,** especially transients with new vendor models |
| **Adapts** as circuits grow, as `#[check]` lists lengthen, as machines get faster | **Placement differs between machines,** so it must never decide gating |
| The agent and the user both get costs to reason with | **A spec can drop out of `live` silently** unless moves are announced |
| New specs are placed at once, with a visible reason | The placement isn't in the text: a reviewer can't see "this is a CI spec" in a diff |
| | It needs the estimator and the measured-cost memory: more engine work than (a) or (b) |

**Strong** for local scheduling and for the agent. **Weak** as the sole authority for CI gating and for stating intent ("this Monte Carlo gates releases, not merges").

---

## 6. Comparison

| Criterion | (a) annotations | (b) profiles | (c) automatic | Hybrid (recommended) |
|---|---|---|---|---|
| Author effort for v5 | ≈ 7 attributes | a `spicy.toml` with selectors, some tags | none | none by default; one attribute to state intent |
| Tier visible where the spec is read | yes | no (`--list`, hover) | as an editor badge, not in the text | badge always; attribute when pinned |
| Correct on Reversed (cheap fault, fails as drawn) | only if the author doesn't reflexively write `#[run(ci)]` on faults | no, with the natural `setup(fault)` rule | yes | yes |
| Stays right as the design grows | no | partly | yes | yes |
| Deterministic CI gate | yes | yes | no | **yes** (gating by meaning) |
| Per-machine and per-team budgets | no | yes | yes | yes |
| Nightly extras and sign-off policy | awkward | natural | not covered | natural (in `spicy.toml`) |
| Agent can reason about cost | no numbers | no numbers | numbers | numbers |
| Reused block carries scheduling guesses | yes (bad) | no | no | only explicit pins |
| Engine work beyond the store | none | selector parser | estimator | estimator (after M3) |
| MVP cost | tiny | small | large | tiny (only `manual`) |

---

## 7. Recommendation: gate by meaning, schedule by cost

### 7.1 The design

**1. What gates, decided by meaning, deterministic, the same on every machine:**

| Spec | Gates merges (ci) | Gates releases (nightly, sign-off) | Runs locally |
|---|---|---|---|
| any spec, `rated`, fault setups included | **yes** | yes | by cost |
| `#[run(nightly)]` | no | **yes** | only on request |
| `#[run(manual)]` | no | no | only on request |
| `#[outside]` setups | no, never (reported numbers) | no | only on request |
| `#[expect(fail, reason = …)]` | tracked: fails the gate if it starts passing *or* the reason is gone, as Rust's `#[expect]` lint does | tracked | by cost |

**2. When a spec runs locally, decided by cost, automatically:** option (c)'s estimator places each run group in `live`, `save` or `ci` against the budgets in `spicy.toml`. Groups placed at `ci` are pending locally: they run in CI, and their CI results come back through the shared store. The editor shows every spec's tier and estimate and announces moves.

**3. One attribute states intent or pins a placement.** `#[run(live | save | ci | nightly | manual)]` on a spec, a setup or a contract:
- `live` / `save`: always run this locally, even if slow ("I'm tuning this; I want it on every edit"). The editor shows its cost.
- `ci`: never run locally by default (a licensed backend, a huge model). It still gates merges.
- `nightly`: a release gate, not a merge gate (a Monte Carlo yield proof on the board).
- `manual`: exploration; never gates.

`#[budget(t)]` is an optional ceiling: CI fails the spec as `UNDECIDED (budget)` if it runs longer, which catches performance regressions like nextest's `terminate-after`. `#[tag(x)]` is an optional label for `--only 'tag(x)'`.

**4. `spicy.toml` holds policy only:** the budgets, the gate rule (`all-pass` or `no-regress`), the nightly and sign-off extras, the audit rate, and the shared store's location. No selectors are needed for placement; `--only` accepts option (b)'s selector syntax for ad-hoc runs.

```toml
# spicy.toml
[tiers]
live = "2s"
save = "30s"
ci   = "20min"                     # a group over this is reported, and runs anyway: gating is by meaning

[ci]
gate  = "all-pass"                 # or "no-regress" for a project with known failures
audit = "2%"                       # re-run a sample of cache hits, bit-compared
store = "github-cache"             # or "s3://…", or "none"

[nightly]
extras = ["yield", "uniform-spread", "audit(5%)"]

[signoff]
extras  = ["yield", "second-backend"]
require = "fresh-and-trusted"
```

**5. The store (§2) makes it compose**, and the display states (§2.3) make it honest.

### 7.2 The whole flow on the v5 example

**Editing: the LDO's `c_out` 1 µF → 2.2 µF.**

```
T+0 ms     elaborate · recompute keys (µs)
           changed keys: ldo.{output, quiescent, psrr, output_z, dip}, board.* (it contains the ldo)
           unchanged:    ceamp.*, gainstage.*                            ✓ fresh, 0 runs
T+50 ms    live batch: ldo.output, ldo.quiescent, ldo.psrr, board.rated@Reversed      ⟳
T+1.9 s    ✓ ldo.output PASS · ✓ ldo.psrr PASS · ✓ quiescent PASS · ✗ board.rated@Reversed FAIL (-1.53 V < -0.3 V)
           board.sensitivity, board.rail_ok: UNDECIDED (budget) in the MVP; live with the loop, after M3
on save    ldo.output_z, ldo.dip                                                        ◷ → ⟳ → ✓
pending    board.wake_noise (~6 min), board.holdup + recovers (~22 min)                 ⧗ ci · last PASS at a1b2c3
```

**The agent is asked to fix the Reversed failure.** It proposes a series Schottky diode between `usb` and the LDO's `vin`.

1. **Affected set** (`spicy plan --proposal p1 --changed`, keys only, no runs): every SensorBoard spec and check. The children's own keys are unchanged.
2. **Live and save, run synchronously:** `rated@Reversed` now passes. But the **containment check fails:** the LDO's input is now about 3.9–4.1 V **[E]** (4.40 V minus the diode drop and the source's 0.5 Ω). That is outside `Ldo3v3`'s Operating range of 4.3–5.5 V. So the LDO's published verdicts no longer apply in this placement, and every board spec that rested on them needs an in-context check.
3. **CI-tier affected specs:** `wake_noise`, `holdup`, `recovers`, about 28 min in all **[E]**. That is over the agent's per-turn budget (say 60 s, from `spicy.toml` or the user), so the agent does not run them.
4. **What the agent says,** with one verdict per sentence and nothing stronger than the records (`engine_plan.md` §7.5):
   > Reversed polarity now passes: the LDO input stays above −0.3 V (fresh, this proposal).
   > The diode drops the LDO's input to about 3.9–4.1 V, below the 4.3 V its contract assumes, so the LDO's published specs no longer cover this board.
   > Not yet checked for this change: `wake_noise`, `holdup`, `recovers` (CI tier, about 28 min). I can't call this fix verified until they run.
   > Options: a lower-drop ideal-diode controller, or widen `Ldo3v3`'s Operating range to 3.9 V if its datasheet supports it (a requirement change, only on your say-so).

**The agent's rule, before claiming "fixed" or "verified":**
1. Compute the affected set: every gating spec whose key the proposal changes. Unchanged keys are fresh and may be cited with their provenance.
2. Run every affected spec placed at `live` or `save`. That is the floor, always.
3. For affected `ci` specs: run them if their total estimate fits the agent's budget. Otherwise leave them to CI (or ask), and **name each one as pending, with its estimate**.
4. "Fixed" or "verified" only when every affected gating spec is fresh and passing. Otherwise say "fixed for X; pending: Y". The claim lint (`engine_position_agent.md` §3.4) gains this rule: a PASS word for the design needs zero affected gating specs in `pending`, `stale`, `queued` or `never run`.
5. Never quote a stale verdict in the present tense (G9 of `agent_flows.md`).

This is `agent_flows.md` G2 ("no fix is called a fix until verify ran") made precise for tiers: *verify* means "every affected gating spec is fresh", not "the fast ones ran".

**The pull request.**

```yaml
# .github/workflows/specs.yml
name: specs
on:
  pull_request:
  push: { branches: [master] }
  schedule: [{ cron: "0 2 * * *" }]           # nightly
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }                # the base, for the verdict delta
      - uses: actions/cache/restore@v4          # PRs read master's results; they never write them
        with:
          path: .spicy/results
          key: spicy-${{ github.sha }}
          restore-keys: spicy-
      - name: specs
        run: |
          spicy check --tier ${{ github.event_name == 'schedule' && 'nightly' || 'ci' }} \
                      --changed --format junit,json,summary \
                      --base origin/${{ github.base_ref || 'master' }} \
                      >> "$GITHUB_STEP_SUMMARY"
      - if: github.ref == 'refs/heads/master'  # only protected-branch runs write the shared store
        uses: actions/cache/save@v4
        with: { path: .spicy/results, key: "spicy-${{ github.sha }}" }
      - uses: actions/upload-artifact@v4
        if: always()
        with: { name: spec-report, path: .spicy/report/ }
```

- **Caching keyed by content.** The results directory is content-addressed per spec, so restoring *any* earlier snapshot is safe: records whose keys no longer occur are simply unused. `restore-keys: spicy-` restores the newest master snapshot, and `--changed` runs only specs with no record for their key. GitHub scopes caches so that a PR can restore caches from its own branch and the default branch, but a PR's caches aren't visible to master or to other PRs. That gives the "only master writes shared results" rule for free. (GitHub caches are evicted after 7 days unused and capped per repository, so the store must tolerate misses. It does: a miss is a re-run.)
- **The gate** is a required status check on the `specs` job. It fails when any gating spec on the head is FAIL or UNDECIDED (or, with `gate = "no-regress"`, when a spec that passed on the base doesn't pass on the head). It also fails when a `#[budget]` is exceeded.
- **The report** is the verdict delta against the base, written to the job summary (and optionally a PR comment). This closes `contract_tool_gaps.md` P11. An illustrative report (the numbers after the base column are invented for the example, not simulated):

```
 spec                      base a1b2c3              head 9e8f7d              change
 board.rated@Reversed      FAIL -1.53 V < -0.3 V    PASS -0.21 V             fixed
 board.holdup              PASS (cached, ci:812)    PASS 3 codes             re-run, 21m 40s
 board.recovers            PASS                     PASS 0.41 s              re-run (same runs as holdup)
 board.wake_noise          PASS                     FAIL 3 > 2 codes         NEW FAILURE
   at usb.v 4.40 V, usb.z 0.5 Ω, temp 60 °C, … (rests on in-context runs: ldo containment failed on vin.v)
 ldo.*, ceamp.*, gainstage.*   unchanged keys: 14 specs, 0 runs (cached from master)
 cost: 3 groups run · 31m 12s · 2% audit: 1 cached record re-run, bit-identical
```

Most PRs touch one block. With content keys, CI re-runs only that block's specs and the parents above it, so a spec that "takes 20 minutes" costs 20 minutes only on the PRs that change what it depends on.

### 7.3 Faults, Monte Carlo, sign-off in the hybrid

- **Faults** gate merges like any spec, because a fault spec is a requirement (`contract_tool_gaps.md` G19: most board field failures are connectors, cables and supplies). Their cost places them. Fault campaigns (faults × corners) are one run group per fault, so a campaign with a cheap fault and an expensive one splits naturally.
- **Monte Carlo** (`#[confidence(yield(…))]`) gates merges by default, like any spec, and its run count places it. On the board, the author writes `#[run(nightly)]` to say "release gate". The seeds are in the key (`engine_flows.md` §5.5), so a yield result is cacheable like any other. The rule that sampling never supports a `worst_case` or `sigma(3)` PASS (`engine_plan.md` §3.2) is unchanged.
- **`#[outside]`** characterization: never gates, runs on request, and its result is stored with the state `reported`.
- **Sign-off** is the command `spicy signoff <commit>`:
  1. every gating spec (ci and nightly) must have a **fresh, trusted** record for that commit's keys. Missing ones are run;
  2. the sign-off extras run (yield, the second backend when it exists);
  3. a larger audit sample of cache hits is re-run and bit-compared;
  4. it writes `signoff.lock` (`language_specs.md` §11.3): every key, verdict, method, seed, waiver, engine and backend version.
  Opening that commit later shows the signed-off verdicts without the store.

### 7.4 Why not the others alone

- **(a) alone** puts guesses in design files that go stale, and it has no good default for an unannotated slow spec.
- **(b) alone** hides the tier from the reader and places specs by kind, which gets the Reversed fault wrong. Its good part (policy, budgets, extras, the gate rule, selectors for `--only`) is kept in `spicy.toml`.
- **(c) alone** can't gate deterministically and can't state intent. Its good part (placement and numbers) is kept for local scheduling.

The hybrid gives the lead what was asked, "the language reflects that some specs are slow", in the form that stays true: the language says what each spec *means* for gating (`#[run(nightly)]`, `#[run(manual)]`, `#[outside]`), and the tool shows what each spec *costs*, next to the spec, always.

---

## 8. What changes in the docs

### 8.1 `contract_syntax_v5.md`

- **§0 Decisions,** new rows:

  | Decision | Choice |
  |---|---|
  | When specs run | Automatic placement by estimated cost (live / save / ci); `#[run(…)]` pins it or states intent |
  | What gates a merge | Every spec except `#[run(nightly)]`, `#[run(manual)]` and `#[outside]` setups; faults included |

- **§1.1 Items:** attributes on `spec`, `setup` and `contract`: `#[run(live | save | ci | nightly | manual)]`, `#[budget(<duration>)]`, `#[tag(<name>, …)]`.
- **New §1.6 "When specs run":**
  1. A spec's tier decides when it runs, never what it means. A verdict is the same whichever tier produced it.
  2. Every spec gates merges except `#[run(nightly)]` (gates releases), `#[run(manual)]` (never gates) and specs on `#[outside]` setups (reported numbers).
  3. `#[run]` on a setup applies to every spec that uses it, and on a contract to all its specs. The innermost wins. A multi-setup spec takes the latest tier of the spec and its setups.
  4. `rated` limits are checked in every run of every setup, so they are checked in the tier of whatever runs that setup. A setup used by no spec (like `Reversed`) gets its own run group for its `rated` checks, placed by cost like any other.
  5. `#[budget(t)]` is a ceiling: exceeding it makes the spec UNDECIDED (budget) in CI.
- **§2 The example:** unchanged apart from one line to show intent, e.g. `#[fault] #[run(nightly)] setup Unplug …` if the team wants the half-hour transient as a release gate. A comment on `setup Reversed` notes that it runs live.
- **§3 MVP subset:** add "`#[run(manual)]` on specs; other `#[run]` values, `#[budget]` and `#[tag]` are parsed and ignored."

### 8.2 `engine_plan.md`

- **§1.1 When it runs:** replace the "Editor" and "Later" rows with the tier ladder (§2.4 here) and a note that local placement is by estimated cost, while gating is by meaning. Add rows for `spicy check --tier`, `--changed`, `--only`, `spicy plan` and `spicy signoff`.
- **§3.6 Statuses:** add `pending`, `queued`, `skipped`, `reported` and `conditional` (§2.3). `stale` gains "since <commit>, changed: <inputs>". `carried` is redefined as "a record whose per-spec key is unchanged", which makes it exact and shows it as plain fresh with its provenance.
- **§6.2 Core types:** the per-spec key, and `Record.produced_by` and `Record.rests_on`.
- **§7.5 What the agent may claim:** add the rule of §7.2 step 4.
- **§9.3 After M3:** add "the shared result store and CI gating (P11)" and "the cost estimator and automatic placement" as steps. The estimator needs measured costs, so the MVP should start storing wall time per spec now.

### 8.3 `engine_types.md` §9 (not asked, but it's where the store lives)

- §9.2: `.spicy/results/<spec key>.json` per spec, plus the index by spec path, instead of (or beside) `.spicy/checks/<rev>.json`.
- §9.3: `rev` stays as the design-wide identity. A per-spec key is added, built the same way (FNV-1a over a canonical encoding) from the spec's inputs only. For the shared store, where records cross machines and trust boundaries, a longer hash (e.g. BLAKE3 or SHA-256) should replace FNV-1a's 64 bits, since §9.3 itself says FNV-1a "guards against stale files, not attackers".

---

## 9. What the MVP needs

Very little. The MVP has one file per check, DC and AC specs, and a 0.4 s check: tiers don't matter yet. But three cheap things now avoid churn later:

| MVP item | Why now |
|---|---|
| `spicy check` runs every spec and exits non-zero on any FAIL or UNDECIDED (0 = all pass, 1 = a FAIL or UNDECIDED, 2 = an error) | CI can gate on it from day one with a five-line workflow step |
| The parser accepts `#[run(live \| save \| ci \| nightly \| manual)]` on specs. Only `manual` is honoured: skipped unless `--include-manual`, reported as `not run (manual)`, never as a pass | Files written now won't need editing when tiers arrive; slow experiments can be kept out of the default run |
| `--only <spec>` and `--skip <spec>` | The minimum way to run one slow spec or leave one out |
| Each record carries `key` (the design `rev` for now) and `cost.wall_s` per spec | The store and the estimator need both, and adding fields to a JSON schema later is a break |

Not in the MVP: profiles, `spicy.toml`, the estimator, `--changed`, the shared store, `spicy plan`, sign-off. Faults, transients, modes and composition aren't in the MVP subset anyway (`contract_syntax_v5.md` §3).

---

## 10. Precedents, and where we follow or depart

| Precedent | What it does | We |
|---|---|---|
| Rust `#[ignore = "reason"]`, `cargo test -- --ignored` / `--include-ignored` | Skip marked tests by default; run them by name or all at once | Follow for `#[run(manual)]` and `--include-manual` |
| pytest markers (`@pytest.mark.slow`, `-m "not slow"`, registered in `pytest.ini` with `--strict-markers`) | Free labels plus a selection expression | Follow for `#[tag]` and `--only` expressions; tags are declared, so a typo is an error |
| Go `testing.Short()` / `go test -short`, and the test result cache | A binary fast/slow switch; results replayed when inputs are unchanged | Depart from the binary switch; follow the input-keyed cache |
| cargo-nextest profiles (`.config/nextest.toml`, `[profile.ci]`), filtersets, `slow-timeout` with `terminate-after` | Per-environment policy; a selection language; time limits per test | Follow for `spicy.toml`, the `--only` syntax and `#[budget]`; depart by not using selectors for placement |
| Cargo profiles (`dev`, `release`, custom with `inherits`) | Named configurations with inheritance | Follow for `inherits` in profile sections |
| Bazel `size = small \| medium \| large \| enormous` (default timeouts 60 / 300 / 900 / 3600 s), `tags = ["manual"]`, `--test_size_filters`, and its warnings when a test's size doesn't match its measured time | Declared cost classes, checked against measured time | Follow the "measure and warn" idea; depart by making the measurement the default and the declaration the override |
| Bazel remote cache, with CI as the only writer | Shared results keyed by content; developers read | Follow for the shared store's trust rule |
| Nx `affected`, Turborepo remote cache | Run only tasks whose input hash changed; replay the rest | Follow, with per-spec keys instead of per-package |
| rust-analyzer's flycheck (debounced `cargo check` after edits) | Background checks after edits, cancelled on the next edit | Follow for `live` (as `engine_flows.md` §5.2 already does) |
| Cadence ADE Assembler run plans, ADE Verifier's requirement status | Named run plans; per-requirement pass/fail across runs | Follow the per-spec status; depart by making run plans automatic and text-based |
| OpenTitan dvsim: `smoke`, `nightly` and sign-off regressions (via `runs_references.md`) | Three named regressions per change, per night, per release | Follow: `ci`, `nightly`, `spicy signoff` |
| ClusterFuzzLite: a short fuzzing budget per PR, long runs in batch (via `runs_references.md`) | Budgeted statistical work, short on changes and long on a schedule | Follow for Monte Carlo: gate by meaning, budget by tier, `#[run(nightly)]` for the long form |
| cicsim (ngspice corners in GitHub Actions) | The only analog CI example `contract_tool_gaps.md` found | Go further: gating, verdict deltas, content-keyed caching |

---

## 11. Open questions

1. **The agent's per-turn budget.** 60 s is a guess. It should probably be the user's setting, shown in the agent's answer ("running these would take ~3 min; go ahead?").
2. **Should `live` stop at the first FAIL?** `engine_flows.md` §4.3 argues against stopping loops early. But for display, showing a FAIL from round 2 before the rest converges is valuable. This is a display question, not a verdict question.
3. **Where the shared store lives beyond GitHub's cache:** an object store, or a `refs/spicy/results` branch. The 7-day eviction and the size cap are fine for a cache, but not for "what did CI verify on the release commit". `signoff.lock` covers releases; the rest can be re-run.
4. **`no-regress` vs `all-pass` as the default gate.** New projects want `all-pass`. Imported designs with known failures need `no-regress` until they are cleaned up, or `#[expect(fail)]` on each.
5. **Placement of a group whose specs have different `#[run]` pins.** The proposed rule is that the earliest pin wins for the whole group, since the runs are shared. The editor should say so.
6. **Cross-machine determinism of cached results.** ngspice results can differ in the last bits between builds and CPUs. The backend's build identity is in the key, so records only match on the same build. Whether CI and laptops should share records at all when their builds differ, or share only "PASS by a margin far above the band", is to be decided when the store is built.
