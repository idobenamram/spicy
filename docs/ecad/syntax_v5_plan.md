# Plan: the language moves to the v5 syntax

> 2026-09-30 · The language front end (lexer, parser, resolve) moves to the syntax of `research/contract_syntax_v5.md`. The old syntax is dropped, not supported alongside. Every test, case file, circuit and doc example is rewritten in v5.
>
> **Scope:** syntax only. The exporter comes next, then the engine; both are out of this plan. Grammar details follow `research/contract_v4_review_implementation.md` ("[IR]"), §2.
>
> **Status:** steps 1, 2 and 3 done. Steps 3 and 4 cover only the MVP subset (decided 2026-09-30): what steps 1 and 2 already parse stays, and nothing past the MVP is added.

---

## 0. What changes, on one file

Today (v0.1):

```rust
block CeAmp {
    port vcc: Power<In>;
    port gnd: Ground;
    …
    net base;
    let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };
}

contract CeAmp {
    assume temp in -10°C..=60°C;
    assume vcc.v in 12V ± 5%;
    spec bias: dc(output.v) in 4.5V..=6.5V;
}
```

In v5:

```rust
env ambient: Temperature in -10°C..=60°C;

pub block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }

circuit CeAmp {
    net base;
    let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };
}

setup Operating for CeAmp {
    vcc: Supply { v: 12V ± 5% },
    input: Signal { v: 0V },
    output: Load { },
    temp: ambient,
}

contract CeAmp {
    setup = Operating;
    spec bias: dc(output.v) within 4.5V..=6.5V;
}
```

- **Ports** move into the block's header.
- **Parts and nets** move into a `circuit` of the same name.
- **`assume`** is gone: the environment is a `setup`, and the contract names its default one.
- **`in`** as a limit becomes `within`.

---

## 1. The steps

Each step ends green:
- tests pass, with every snapshot change reviewed;
- clippy and fmt are clean;
- a fuzz run is clean;
- the parser is benchmarked;
- `/stage-review` has run, and you've reviewed the result.

We do them together, one at a time.

### Step 1: tokens

- **New tokens:** strings `"…"` on one line, with only `\"` and `\\` as escapes. (v5's `->` was added, then dropped: see step 2.)
- **Keywords:**
  - new: `circuit`, `setup`, `within`, `rated`, `ensure`;
  - promoted from reserved: `env`, `const`, `pub`, `for`;
  - no longer keywords: `port`, `assume`;
  - reserved: `trait`, `impl`;
  - no longer reserved: `bench` (v5 writes a test bench as a `setup`) and `param` (a parameter is a generic);
  - contextual, recognized only in their position: `mode`, `event`, `observe`, `emits`, `with`, `on`.
- **Units:** `y` (year) and `h` (hour).
- **Tests:** the lexer's case files and snapshots, `keywords.spl`, plus a case for the new tokens. `lexer.md` is updated.

### Step 2: expressions

- **`within`** is the limit operator, at level 1. `in` is no longer an operator, so it ends an expression.
- **Open ranges:** `..=b` (no lower end) and `a..` (no upper end, legal only before `,` `}` `)` `;`). `a..b` stays an error.
- **Index:** `m[s]`, a postfix, in the same loop as `.` and `(`.
- **Call arguments** can be named: `f_high(-3dB, ref: dc)`.
- **No transitions** (decided 2026-09-30, replacing v5's `5mA -> 30mA`): a step's or a sweep's levels are named fields, like every other field, so a struct literal has no positional parts and no `->` token is needed. `Step { from: 5mA, to: 30mA, edge: 1us, at: 50us }`, `Sweep { from: 4.5V, to: 3.0V, step: 10mV }`.
- **One range node** (decided 2026-09-30): `Range { lo, hi }` for `a..=b`, `..=b` and `a..`, as rustc's `ExprKind::Range`.
- **Named generic arguments at a placement:** `GainStage<A = Mcp6001> { … }`, recognized by `< IDENT =` ([IR] G4).
- **Strings** as values.

### Step 3: `block` headers and `circuit`, with every file converted

- **The block header:** `[pub] block X { port: Type, … }`. Generics, bounds, `observe` and `emits` aren't in the MVP, so they aren't parsed.
- **`circuit X { net …; let …; }`:** today's block body, without ports.
- **Resolve** reads the ports from the header and the body from the `circuit`. It builds the same `Design` as today, so flatten doesn't change. The errors: a circuit without a block; a second circuit for a block; a placed block with no circuit.
- **The `port` statement is gone.**
- **Every file is converted to v5 in this step:**
  - the 103 case files;
  - the ~93 inline test sources in the Rust tests;
  - `circuits/ce_amp.spl`;
  - the fuzz seed corpus.

  A throwaway converter does it (not committed), and I review its diff.
- **The proof nothing changed:** every resolve and flatten dump comes out byte-identical. Those dumps print no spans (E22), so any difference is a conversion bug. Only the parser's tree dumps and the rendered diagnostics (line numbers) change.

### Step 4: `setup`, `env`, `const` and the contract statements, parsed (the MVP subset)

- **`setup S for X { … }`:**
  - entries `key: value`, where a key is a port, a field path (`vin.v`) or `temp`;
  - its own recovery level, so a broken entry keeps the others.
  - Not in the MVP, so not parsed: `mode`, `event`, `..Base`, pairs.
- **`env name: Type in range;`** and **`const NAME: Type = value;`**.
- **The contract statements:**
  - `setup = S;`
  - `let …;`
  - `[pub] spec name: … within|<=|>= …;`
  - Not in the MVP, so not parsed: `rated`, the clauses (`with`, `for x in`, `on`, `in M`), the function form `spec name(a: A) { … }` and `ensure`. `rated`, `ensure` and `for` become reserved words, with no token kinds of their own.
- **`assume` is gone.**
- **The parser rule this needs** ([IR] §2.3 G1): `setup` and `pub` start both items and statements, so a two-token rule decides.
- **Resolve** matches each `setup` and `contract` to its block by name, and reports one without a block. What they contain isn't elaborated yet: setups and contracts are resolved in the next phase.
- **Tests:** v5's MVP subset parses whole, as a parser `ok` case. There's an `err` case for each rule, and every fix is applied by a test.

### Step 5: the design notes

- **`grammar.md` goes to v0.2:** keywords, tokens, the EBNF from [IR] §2.5, precedence, recovery, the error list.
- **`lexer.md`:** the token list.
- **`ast.md`:** the new item and statement kinds.
- **`language.md`:** rewritten from v5.
- **`model.md`:** its examples in v5.

---

## 2. Decisions to confirm

| # | Question | Recommendation |
|---|---|---|
| **1** | The old syntax gets no migration messages: `port …;` or a limit written with `in` is an ordinary syntax error. Nothing outside this repo uses v0.1 | Yes, no migration messages |
| **2** | Convert the files with a throwaway script, then review its diff | Yes |
| **3** | `circuits/ce_amp.spl` in v5, with `env ambient` in the file itself (there are no multi-file projects yet), and its three specs as today (v5's internal `base_bias` goes in its own case file) | Yes |
| **4** | Resolve in this plan covers only `block` + `circuit` (the same `Design` as today). Setups, `env`, `const` and contracts are parsed and matched to their blocks, but not elaborated until the next phase | Yes |

---

## 3. After this plan

1. Setups, `env`, `const` and contracts elaborated (the MVP subset of [IR] §4.4).
2. The exporter.
3. The engine.
