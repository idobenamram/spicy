# Judging a code change

These rules tell you when a change makes the code better. Use them to compare the old version and the new version of a change, in your own work and in reviews. For the review process of a pipeline stage, and for the tools that measure speed, run [`/stage-review`](../.claude/commands/stage-review.md).

A change that is better on one rule and worse on another is a decision for the user. Show both sides with numbers.

<a name="tests"></a>
## First: the change is correct and tested

- The change has the tests that are necessary to test the feature fully. Each bug fix has a test that failed before the fix.
- Tests are code too. Two tests do not test the same thing.
- Tests live in a `#[cfg(test)] mod tests` in the source file. Shared test helpers live in the test-utilities module of the crate. Helpers that the fuzz targets also use go in a `#[cfg(any(test, fuzzing))] pub mod testing`.

<a name="less-code"></a>
## Less code

- For the same behavior and the same tests, the version with less production code is usually better.
- Count concepts, not only lines: types, states, branches and special cases. A good module has a "small interface, lots of functionality" ([Ousterhout](https://web.stanford.edu/~ouster/cgi-bin/cs190-winter18/lecture.php?topic=modularDesign)).
- When a general rule can cover a special case, remove the special case. A special case that fixes only one input is a patch.
- When the design permits it, "define errors out of existence" ([Ousterhout](https://web.stanford.edu/~ouster/cgi-bin/cs190-winter18/lecture.php?topic=exceptions)).
- Do not make code shorter by packing it. Dense one-liners and long iterator chains hide the steps. Such a version is shorter but not better.
- A data layout for speed can add lines (see [Speed](#speed)). That is acceptable when the benchmark shows the gain.

<a name="clear-structs"></a>
## One clear reason for each struct

- For each struct, you can say in one sentence what it holds, which stage makes it, and which code reads it. Write that sentence as its doc comment.
- If two structs hold the same data for the same owner, merge them. If no code reads a struct, remove it.
- Make wrong states impossible to build: "Use a data structure that makes illegal states unrepresentable" ([King](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/)). An enum is better than a set of `Option` fields or flags.
- Use a newtype index, not a bare `usize`, so that the wrong index type fails at compile time (rustc's [`IndexVec`](https://doc.rust-lang.org/nightly/nightly-rustc/rustc_index/vec/struct.IndexVec.html)).
- Arguments get their meaning from types, not from `bool` or `Option` ([C-CUSTOM-TYPE](https://rust-lang.github.io/api-guidelines/checklist.html)).
- A function returns what it builds. The caller decides where to keep it. Do not make a placeholder and fill it in later.
- Keep items `pub(crate)` unless another crate uses them.

<a name="speed"></a>
## No large loss of speed

- Measure. Do not guess. If a change makes a hot path more than about 3% slower, profile it and fix it, or give the reason to accept the loss. `/stage-review` has the tools, in its section "Measuring speed".
- Before you design, estimate the cost of memory and CPU, for bandwidth and latency ([TigerStyle](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md)).
- Know the data: the size of each item, the number of items, and the fields that each hot loop reads. "Where there is one, there are many" ([Acton](https://github.com/CppCon/CppCon2014/tree/master/Presentations/Data-Oriented%20Design%20and%20C%2B%2B)).
- Use a struct of arrays when a hot loop reads a few fields of many items. Use an array of structs when the loop reads all the fields of each item together.
- Keep hot items small, so that more of them fit in one cache line. A cache line is 64 bytes on x86-64 ([Lemire](https://lemire.me/blog/2023/12/12/measuring-the-size-of-the-cache-line-empirically/)). Use `u32` indices instead of pointers or `usize` where the count of items permits.
- Keep hot loops free of branches and allocations. Move the `if` out of the loop, and move the loop into the function that does the work. This "removes a branch from the hot loop, and potentially unlocks vectorization" ([matklad](https://matklad.github.io/2023/11/15/push-ifs-up-and-fors-down.html)).
- Give one call many items, not many calls one item each.
- On cold paths, for example error messages and argument parsing, clarity comes before speed.
- Do not add caches or lazy machinery for a cost that only input with errors pays.
- Do not change the shape of data for a reader that does not exist yet. Change it when the first real reader needs it.

<a name="linear"></a>
## Code that reads from top to bottom

- The top function of a stage shows its passes in order, with one comment for each pass.
- Write each rule once, as one helper.
- Put helpers below their caller, in the order that the caller uses them.
- Keep the control flow in the caller. "All control flow should be handled by one function, the rest shouldn't care about control flow at all" ([TigerStyle](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md)).
- "If a function is only called from a single place, consider inlining it" ([Carmack](http://number-none.com/blow/john_carmack_on_inlined_code.html)). Keep the helper only when it holds a rule or gives a step a name.
- A function longer than about 70 lines usually does more than one job. TigerStyle sets 70 lines as a hard limit.
- Use one name for each item in the code, as in the [writing rules](../AGENTS.md#writing-rules).
- For comments, see [Code comments](writing_docs.md#code-comments).

<a name="compare"></a>
## Compare the two versions

| Question | The new version is better when |
|---|---|
| Does it do the same job, with the tests that it needs? | Always yes. Otherwise, stop. |
| How many lines of production code? | Fewer |
| How many types, states, branches and special cases? | Fewer |
| Can you say the reason for each struct in one sentence? | Yes, for each struct |
| How fast is the hot path (instruction count and wall clock)? | Not more than about 3% slower |
| Can you see the passes in the top function? | Yes |
