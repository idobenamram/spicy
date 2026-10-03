---
description: Run the stage cleanup review (readability first, reference research, verified changes) on one pipeline stage
argument-hint: <stage> [files] [design notes]
---

Review the pipeline stage **$ARGUMENTS** with the process below. If only a stage name is given, find its files under `crates/` and its design notes under `docs/ecad/`.

## The goal

**Readability first.** A newcomer should be able to read the stage top to bottom and see what it does and in what order. The rules for judging each change are in [`docs/code_quality.md`](../../docs/code_quality.md): tests, less code, one clear reason for each struct, speed, and code that reads from top to bottom. Read it first, and give its path to every subagent.

Clean-up means **simplifying without losing anything**: behavior, error messages and speed all stay the same unless a change is decided on purpose.

## The standard for a stage (in addition to `docs/code_quality.md`)

- **Each pass of the top function has one comment** (`// Pass 1, the file's names: …`). No index tricks, counters or truncations a reader has to decode. Typical rules that must become one helper: a duplicate-name check, "report and stop", a missing closing bracket.
- **Each part is added together with its span** (a builder with `push_*`), so the data and its source map can't drift apart.
- **One home for each concept, in the lowest crate that needs it.** One `Span`, one `Diag<K>`, one unit enum. A struct that repeats another's fields, a field stored twice, or a parallel enum is merged. A one-use wrapper is removed.
- **The file reads top-down in the order things happen,** and long files split along real concerns (e.g. `resolve/body.rs`).
- **Grounded in references:** rustc, rust-analyzer, Zig, Spade (`externals/spade`), atopile (`externals/atopile`), with citations.

## The process

1. **Map the flow yourself first.** Read the stage and its design notes. Write down the passes as they really are, and where the code hides them.

2. **Run subagents in parallel.** Each works on its own scratch copy (`rsync` of the repo into the scratchpad, its own `CARGO_TARGET_DIR`, a baseline commit inside the copy) and **never modifies the main tree**.
   - **Reference researcher.** How rustc_parse/rustc_resolve, rust-analyzer, Zig and Spade structure the same thing:
     - the top-level driver;
     - how duplicate definitions are handled;
     - how values are built and stored;
     - how data and source maps are kept in sync;
     - per-scope state.

     It gives citations to file:line, says which it verified in the source and which are from memory, and names the patterns that confirm, refine or argue *against* the plan. It also says what not to copy.
   - **Flow reviewer, one per complicated part** (e.g. the Pratt loop, recovery, value typing). The prompt puts readability first, then:
     - patches: special cases standing in for a rule, each with the rule named;
     - functions that are too long, do several jobs, take boolean flags or duplicate another;
     - a keep / merge / remove verdict for every struct and enum, with the reason.

     It prototypes every "do" change in its copy and returns a patch.
   - **Cross-crate type auditor** (when the stage adds or touches types). It looks for concepts defined twice across stages or crates, the way `SourceSpan` duplicated `Span`. Each finding gets a verdict: merge (clear), keep (and why it's genuinely different), or ask the user.
   - For a **new** stage, also run the red team first: a bug hunter, plus an optimizer that measures with the benchmark suite (see "Measuring speed" below), adds a scaling pair for each new shape the stage reads, and profiles with the other tools there. Every bug gets a regression test.

3. **Synthesize.** Merge the reports, and resolve any overlap between patches (a three-way merge, with the bug fix winning).
   - Apply the clear items.
   - **Bring real design choices to the user** as questions, and don't guess on those. Give each one enough to decide on:
     - what the references do in the same situation, with citations;
     - the options, with the pros and cons of each;
     - how the structs and call sites look under each option, in code;
     - what each costs (sites, docs, speed, crate layering);
     - a recommendation.
   - A rejected idea is listed, with the reason.

4. **Apply and verify, one change at a time.** After each change:
   - `cargo test -p spicy_lang -p spicy_model`, plus `--release -- --ignored`;
   - `cargo clippy --all-targets` clean, and `cargo fmt`;
   - **every existing snapshot byte-identical**. An intended change is reviewed line by line and explained;
   - a fuzz run (`cargo +nightly fuzz run fuzz_spicy_lang_parser -- -max_total_time=90`);
   - **a benchmark, before and after** (see "Measuring speed"): the suite against a baseline of the base commit (instructions, estimated cycles, allocations, the scaling ratios). More than about 3% more instructions on a typical benchmark means profiling (a flame graph for time, DHAT or heaptrack for allocations) and fixing it (the fix is usually a clone, a rehash, an allocation per item, or a large `Result` on the hot path). If it's accepted anyway, say so and why;
   - tests for anything the refactor could silently break that nothing pins yet. Tests go in `#[cfg(test)] mod tests` in the source file, or in case files under `test_data/<stage>/{ok,err}`.

5. **Report to the user:**
   - the flow before and after (what the top function reads like now);
   - each duplicated rule and its one replacement, with the reference it follows;
   - a speed table;
   - what wasn't done, and why.

   **Commit only when asked.** Stage only your own changes, never another session's uncommitted work.

## Measuring speed

Count first, time second: instruction counts don't move with frequency, heat or the machine's other work, so a 1% change in them is real; timings need the precautions below.

**The benchmark suite, `crates/spicy_bench`** (gungraun), is the first tool. It runs each stage under Valgrind and reports instructions (Ir), estimated cycles and, on the typical design, allocations (DHAT); a benchmark's setup (generating, parsing, resolving what the measured stage reads) isn't counted. Typical benchmarks (`board(100)`: 100 renamed copies of `ce_amp.spl` placed by one root) give the cost per stage; scaling pairs give one shape at n and 4n (4× is linear, up to 4.5× is n log n, 8× or more is quadratic). Lexer and parser counts are exact; resolve and flatten move by up to 0.5%, because std's `HashMap` seeds its hasher randomly. It needs valgrind and `gungraun-runner` 0.20.0 (in `~/.cargo/bin`). The whole suite runs in about 20 s plus a build.

- Before/after in two trees (the old side from `git archive <commit>`): one `GUNGRAUN_HOME` for both runs, a separate one per agent running at the same time.
  ```sh
  export GUNGRAUN_HOME=<scratchpad>/gungraun_<task>
  (cd old && CARGO_TARGET_DIR=<old_target> cargo bench -p spicy_bench -- --save-baseline=old < /dev/null)
  (cd new && CARGO_TARGET_DIR=<new_target> cargo bench -p spicy_bench -- --baseline=old < /dev/null)
  ```
  In one tree: `--save-baseline=$(git rev-parse --short HEAD)` on the base commit, then `--baseline=<that>`. `--callgrind-limits='ir=3%'` exits with 3 when a count grows by more than 3%.
- For a table: `--output-format=json` prints one line per benchmark. Each benchmark's Ir and change:
  ```sh
  cargo bench -q -p spicy_bench -- --baseline=old --output-format=json < /dev/null 2>/dev/null \
    | jq -r '"\(.module_path).\(.id)  Ir \(.profiles[]|select(.tool=="Callgrind")|.data.total.metrics.Ir|"\(.values.new) (\(.change.diff_pct // "n/a")%)")"'
  ```
  The ratio of each scaling pair:
  ```sh
  cargo bench -q -p spicy_bench -- --output-format=json < /dev/null 2>/dev/null \
    | jq -rs '[.[] | select(.id|test("_[0-9]+$")) | {k: (.module_path+"."+(.id|sub("_[0-9]+$";""))), ir: (.profiles[]|select(.tool=="Callgrind")|.data.total.metrics.Ir.values.new)}] | group_by(.k)[] | select(length==2) | "\(.[0].k)  \(.[1].ir/.[0].ir*100|round/100)x"'
  ```
- A new shape the stage reads gets a scaling pair (a generator in `src/`, with a test that pins its exact problems, and two sizes in the stage's bench file). Never compare a generator change and a code change in one step: a changed generator needs a new baseline on the base commit.

**This machine:** CPUs 0–11 are performance cores, 12–19 efficiency cores: pin to a performance core (`taskset -c 2`) and name the core's counters (`cpu_core/instructions/u`), or perf prints "not counted" for the other kind. Rust 1.98 mangles symbols the v0 way, which this perf, heaptrack and inferno print raw (`_RNv…`): pipe their text through `rustfilt` (gungraun's output is already readable). `setarch -R <cmd>` turns off address randomization for steadier counts.

| Question | Tool | How |
|---|---|---|
| Did it get slower? Does it stay linear? | the suite | as above: Ir first, estimated cycles second, DHAT's total blocks for allocations; the scaling ratios |
| Which functions got slower? | the suite's flame graphs, callgrind | `target/gungraun/spicy_bench/<file>/<group>/<function>.<id>/`: a flame graph per typical benchmark and a differential one against the baseline (`*.flamegraph.diff.*.svg`); `callgrind_annotate --inclusive=yes callgrind.*.out` for a table. Outside the suite: `valgrind --tool=cachegrind --cachegrind-out-file=cg.old <old bench> …`, the same for `cg.new`, then `cg_annotate --diff cg.old cg.new` |
| Where do allocations come from? | DHAT, heaptrack | the suite's `dhat.*.out` of a typical benchmark; `valgrind --tool=dhat --num-callers=64 --dhat-out-file=dhat.json <bench> …` (the default depth is used up by inlined `raw_vec` frames), summarized by the first frame in our crates; or `heaptrack -o run <bench> …`, `heaptrack_print -f run.zst --flamegraph-cost-type allocations -F run.folded`, `rustfilt < run.folded`, grouped by the deepest frame in our crates |
| Where does the time go? (sampling) | `perf` (+ `inferno`) | `taskset -c 2 perf record -F 4999 --call-graph dwarf,65528 -o run.data <bench> …` (no frame pointers needed); `perf report -i run.data --stdio --no-children --percent-limit 1 -g folded,0.5,caller,count \| rustfilt`. For a flame graph: `perf script -i run.data \| inferno-collapse-perf \| rustfilt > run.folded`, `inferno-flamegraph < run.folded > run.svg` |
| Did it get slower? (wall clock) | gungraun's perf mode, or interleaved runs | `taskset -c 2 cargo bench -p spicy_bench -- --tools=perf --perf-events='cpu_core/instructions/u,cpu_core/cycles/u,task-clock'`; or build both sides with `RUSTFLAGS="-C llvm-args=-align-all-functions=6"` (function alignment shifts timings by several %), run with `taskset -c 2` and `GLIBC_TUNABLES=glibc.malloc.trim_threshold=1073741824:glibc.malloc.mmap_threshold=1073741824` (malloc trimming adds noise), alternate the two sides for ≥ 7 rounds, compare medians |

When a cargo command runs in the background, give it `< /dev/null`: cargo's probe reads `rustc -` from standard input, and an inherited input makes it fail or spin, and cargo caches the failure in `target/.rustc_info.json` ("failed to run `rustc` to learn about target-specific information" until the entry is removed).

## Rules that always apply

- Minimal dependencies: no new crates without asking.
- Errors are data (`Diag<K>`). A fix is offered only when it's the single right answer, and a test applies every fix and checks that no new error appears.
- Keep to the code's existing style: comment density, naming, idiom.
- Explain decisions with where they come from, using worked examples.
