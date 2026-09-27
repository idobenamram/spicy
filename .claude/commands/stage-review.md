---
description: Run the stage cleanup review (readability first, reference research, verified changes) on one pipeline stage
argument-hint: <stage> [files] [design notes]
---

Review the pipeline stage **$ARGUMENTS** with the process below (the same as `docs/ecad/stage_review.md`). If only a stage name is given, find its files under `crates/` and its design notes under `docs/ecad/`.

## The goal

**Readability first.** A newcomer should be able to read the stage top to bottom and see what it does and in what order. Everything else is a guard on that goal:
- no patches;
- one rule written once;
- a data model without redundant structs;
- no speed loss;
- tests for what could break.

Clean-up means **simplifying without losing anything**: behavior, error messages and speed all stay the same unless a change is decided on purpose.

## The standard (what "done" looks like)

- **The top function reads as the stage's passes or phases,** one comment per phase (`// Pass 1, the file's names: …`). No index tricks, counters or truncations a reader has to decode.
- **One helper per rule.** If the same rule is written in several places (a duplicate-name check, "report and stop", a missing closing bracket), it becomes one function, and every site calls it.
- **Functions return what they built, and the caller decides whether to store it.** No `Option` ids threaded through for "maybe store this", and no placeholders filled in later.
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
   - For a **new** stage, also run the red team first: a bug hunter, plus an optimizer that benchmarks and profiles. Every bug gets a regression test.

3. **Synthesize.** Merge the reports, and resolve any overlap between patches (a three-way merge, with the bug fix winning).
   - Apply the clear items.
   - **Bring real design choices to the user** as questions, each with the options, the trade-offs and a recommendation. Don't guess on those.
   - A rejected idea is listed, with the reason.

4. **Apply and verify, one change at a time.** After each change:
   - `cargo test -p spicy_lang -p spicy_model`, plus `--release -- --ignored`;
   - `cargo clippy --all-targets` clean, and `cargo fmt`;
   - **every existing snapshot byte-identical**. An intended change is reviewed line by line and explained;
   - a fuzz run (`cargo +nightly fuzz run fuzz_spicy_lang_parser -- -max_total_time=90`);
   - **a benchmark, before and after, interleaved,** pinned to a quiet CPU (`taskset`) and run several rounds, with medians compared. More than about 3% slower on a hot path means profiling with `perf` and fixing it (the fix is usually a clone, a rehash, or a large `Result` on the hot path). If it's accepted anyway, say so and why;
   - tests for anything the refactor could silently break that nothing pins yet. Tests go in `#[cfg(test)] mod tests` in the source file, or in case files under `test_data/<stage>/{ok,err}`.

5. **Report to the user:**
   - the flow before and after (what the top function reads like now);
   - each duplicated rule and its one replacement, with the reference it follows;
   - a speed table;
   - what wasn't done, and why.

   **Commit only when asked.** Stage only your own changes, never another session's uncommitted work.

## Rules that always apply

- Minimal dependencies: no new crates without asking.
- Errors are data (`Diag<K>`). A fix is offered only when it's the single right answer, and a test applies every fix and checks that no new error appears.
- Keep to the code's existing style: comment density, naming, idiom.
- Explain decisions with where they come from, using worked examples.
