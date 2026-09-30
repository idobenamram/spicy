# Flatten, round 2: decisions for the user

> 2026-09-29 · Questions from the second `/stage-review flatten` round (a driver reviewer, a checks reviewer, a type auditor) and from the audit of `Option` against `Reported` in the earlier stages. Each question has:
> - what the references do;
> - the options, with the code each one gives;
> - the cost;
> - a recommendation.
>
> `[V]` means verified in the source at file:line, `[R]` means recalled.
>
> **The code as it stands** (applied, not committed): everything in `flatten_decisions.md`, plus this round's merged patches.

**Decided 2026-09-29, all as recommended:**
- **P1:** yes, as its own step (applied: the parser's placeholders and failures hold the proof; a block is tainted by the body whose parse reported an error, as rustc taints the body that emits, which fixes both bugs with no snapshot change);
- **P2:** yes, after P1 (applied: value-level recovery for `let`, `net x =` and port types, and a soft error with a fix for a missing `{`; 11 parser snapshots show the statements kept, and the three cascades are gone);
- **D1:** (b), a named `LocalInstance` (applied);
- **D2:** (c), strings for now, noted as an exception under model.md E14;
- **S1:** (a), accepted, with no caching machinery;
- **W1:** yes, "has no ground net" (applied);
- **W2:** (a), one source (applied).

There are 7 questions:
- **P. The earlier stages:** P1, P2.
- **D. The data model:** D1, D2.
- **S. Speed:** S1.
- **W. Wording and semantics:** W1, W2.

---

## P. The earlier stages: should `Reported` go further?

**The audit's answer, per stage:**
- **The lexer: no.**
  - Its tokens are the lossless layer (roadmap §4.4).
  - The one lexer breakage that reaches the tree, a number that doesn't decode, gets its proof in the parser, from `Reported::among(&lex_errors)`.
  - rustc differs only for literals, because its cooking step reports an error and builds the token in one go (`LitKind::Err(guar)`, rustc_ast token.rs:201) [V].
- **The parser: yes** (P1 below).
- **Resolve:** little is left. `merge() -> Option` and a contract that fails to resolve should hold a proof when contracts land (M1d-5).

### P1. The parser's placeholders hold the proof, and a block is tainted from its tree

**Two bugs this fixes** (found by probes in a copy):
- **An unclosed statement at the end of the file.** `let r2 = Resistor { a: g` at EOF: the `Unclosed` error sits at EOF, outside the block's span. So `tainted` stays `None`, and flatten reports the damage: W-isolated on `x`.
- **A missing `}`.** `block A { …` followed by `block B {`: `Unclosed` is reported at B's `block` keyword. So **B is tainted and A isn't**, and B's own warnings are lost.

Position alone can't get these right. Widening a block's span up to the next item doesn't help either: the error sits on B's first token, which is inside B's span too.

**What rustc does:**
- `PResult<'a, T> = Result<T, Diag<'a>>`.
- Recovery builds `ExprKind::Err(ErrorGuaranteed)` (ast.rs:1927) and `TyKind::Err(err.emit_err())` (ty.rs:533).
- A statement that fails becomes `mk_stmt_err(span, guar)` (stmt.rs:1182) [V].

rust-analyzer's ERROR nodes carry no proof (`err_and_bump`, parser.rs:279-283) [V]. That fits a green tree that is reused across edits, where a node can outlive its error list. Our AST is rebuilt every time, and `Parsed` holds the tree and its errors together, so a proof in a node can't go stale.

**The change** (the audit prototyped it: 6 files, +49/−44, about 25 sites; no snapshot moved; node sizes unchanged, because `Reported` has no size):
```rust
type PResult<T> = Result<T, Reported>;        // was: Result<T, Fatal>, where any code could build a Fatal
pub enum ExprKind<'src> { …, Error(Reported) }
pub enum StmtKind<'src> { …, Error(Reported) }
pub struct Body<'src> { …, pub close: Result<Span, Reported> }   // the `}`, or the proof it's missing

// resolve pass 3: taint by position, and by the tree
block.tainted = reported.inside(item.span)
    .or(body.close.err())
    .or_else(|| body.stmts.iter().find_map(|s| match s.kind { StmtKind::Error(r) => Some(r), _ => None }));
```

**Cost:**
- about 25 parser sites, plus about 10 lines in resolve;
- the three `.expect(..)` calls in resolve that turn an `ExprKind::Error` into a proof go away;
- no speed effect.

**Recommendation: do it,** as its own step with its own `/stage-review` checks. It fixes two real bugs, and it's rustc's shape.

### P2. Should a statement keep its name when only its right-hand side breaks?

**The cascades today** (probed):
- `net base = [g g]` gives "unknown `base`" wherever `base` is used;
- `port a: ;` gives "`Child` has no pin `a`" at every placement;
- `block Child port…` (no `{`) gives "unknown `Child`".

**What rustc does:** it recovers inside the expression: "We still want a field even if its expr didn't parse" (expr.rs:3725) [V]. It keeps the name and uses `ExprKind::Err` for the value.

**Options:**
- **(a) Keep today's statement-level recovery.** No work, and the cascades stay.
- **(b) Recover inside the expression and the type:**
  ```rust
  fn expr_or_error(&mut self) -> Expr<'src> {
      let start = self.pos;
      self.expr().unwrap_or_else(|reported| {
          let span = self.skip_to_stmt_end(start);      // `recover` without eating the `;`
          Expr { kind: ExprKind::Error(reported), span }
      })
  }
  pub enum TypeKind { …, Error(Reported) }             // `port a: ;` keeps its name and span
  ```
- **(c) Also treat a missing `{` after `block Name` as a soft error with an insertion fix,** as the missing `;` is today.

**Cost:**
- about 60 lines, plus case files;
- the parser snapshots change;
- the new recovery has to follow the bracket rules in `recover`.

**Recommendation: (b) and (c), after P1,** as a separate step. This is where the proof pays off most: resolve reads `Error(r)` and says nothing more, as it already does for a port with a wrong type.

---

## D. The data model

### D1. Name the "(placement, `let`)" pair

**Now:** `LocalNet { at, net }` is a struct. The same shape for an instance is an anonymous tuple in two places: `FlatInstance.origin: Option<(FlatInstanceId, InstanceId)>` and `FlatDevice.origin: (FlatInstanceId, InstanceId)`. "The `let` this pair names" (`block(at).instances[i.index()]`) is written out 6 times.

**What rustc does:** `HirId { owner, local_id }` is a named struct, never a tuple (rustc_hir_id lib.rs:98) [V]. rust-analyzer has `InFile<T>` (hir-def src.rs:17) [V].

**Options:**
- **(a) Keep the tuples,** and add `Flat::instance(at, i)`. About 6 sites. One idea keeps two shapes.
- **(b) A named struct beside `LocalNet`:**
  ```rust
  pub struct LocalInstance { pub at: FlatInstanceId, pub instance: InstanceId }
  pub struct FlatInstance { pub block: BlockId, pub origin: Option<LocalInstance> }
  pub struct FlatDevice   { pub kind: PartKind, pub origin: LocalInstance, … }
  part: flat.instance(device.origin).name.clone(),        // shorted_parts
  ```
  About 17 sites. Same memory layout, so no speed or snapshot change.
- **(c) One generic `Local<T> { at, id: T }`,** with `type LocalNet = Local<NetId>`. One definition for both, but `.net` becomes `.id` at about 15 sites.

**Recommendation: (b).** It's rustc's shape, and the name says what the pair is.

### D2. Paths in error data: dotted `String`s or `HierPath`?

**Now:** six problem fields (`SeveralSources.net` and `.sources`, `NoSource.net` and `.sinks`, `IsolatedNets.nets`, `SeveralGrounds.nets`) and `InBlock.example` are dotted strings like `"left.vcc"`. E14 says a path is kept as segments, never a dotted string parsed back. An editor or the AI reading the problem's data would have to parse it.

**The prototype** (`scratchpad/simplify2/types_error_paths_prototype.patch`):
- `list` takes `Display`;
- no snapshot changes;
- but it measured flatten slower on error-heavy inputs (power_30000 +36%, iso_30000 +19%). That was against the old grouping, which cloned and hashed every problem.

The grouping has since been rebuilt (the problem is apart from where it is, compared only within one place, with no clone), so the cost has to be measured again.

**Options:**
- **(a) Keep strings (now).** Nothing reads these paths as data yet.
- **(b) Move to `HierPath` now,** and re-benchmark.
- **(c) Move to `HierPath` when the editor starts reading flatten errors.**

**Recommendation: (c).** The change is mechanical, and it should be made against its first real reader.

---

## S. Speed

### S1. Per-design tables on designs made only of cycles

**What happened:** the driver reviewer moved "each block's parts in name order" and "each block's merge targets" from every placement to one table per design. That made realistic designs faster:
- chain_10000 −10% instructions;
- fan_100000 −10%;
- place_100000 −4%.

But the tables are built for every block, including blocks no root reaches. Only a placement cycle, itself an error, leaves a block unreached. So designs made only of cycles got slower:
- pairs_30000: 53.2 → 59.3 ms, +11%, 30 000 two-block cycles and no root;
- cyc_100000: 43.9 → 47.8 ms, +9%.

**Options:**
- **(a) Accept it.** Every design without a cycle gets faster.
- **(b) Build each block's entry lazily (`OnceCell`),** as rustc caches query results per item [R]. This removes the cost but adds caching machinery.
- **(c) Drop the merge-target table.** chain gives back its −10%.

**Recommendation: (a).**

---

## W. Wording and semantics

### W1. "`X` has no ground" against "`X` has more than one ground net"

**Recommendation:** "`X` has no ground net". The two ground errors then read alike. It changes one snapshot line (`ground.spl`).

### W2. Outputs tied inside their block, with nothing behind them

**Example** (`test_data/flatten/err/power_ports.spl`, `Tied`): `port o1: Power<Out>; port o2: Power<Out>; net rail = [o1, o2];`, nothing drives `rail` inside, and a parent binds both outputs.
- **Before this round:** an error "(in `Tied`)", with its related span in the parent.
- **Now:** one source. Inside `Tied` it's one net, like KiCad's "stacked" pins (the same pin twice, `sch_pin.cpp:582-601`) [V].

**Options:**
- **(a) One source (applied).**
- **(b) One source, plus a lint:** "`o1` and `o2` are one net; one `Power<Out>` port is enough".

**Recommendation: (a).** A lint can come with the others later.
