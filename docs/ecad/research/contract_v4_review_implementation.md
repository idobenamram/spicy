# Draft 4, Seen from the Front-End That Has to Build It

> 2026-09-30 · Final-round review of `contract_syntax_v4.md` (draft 4) from one angle: **implementing it** in the lexer, parser, resolve and flatten that exist today, and what the engine then receives.
> Reads with: `grammar.md`, `lexer.md`, `ast.md`, `model.md` (E1–E26, §8–9), `engine_types.md` (T1–T11), `research/contract_v2_review_engine.md` (cited "[R2]").
> Code read (read-only): `crates/spicy_lang/src/{lexer,parser,resolve}/`, `crates/spicy_model/src/{design,flat,prelude}.rs`, `flatten/`, the 118 case files under `crates/spicy_lang/test_data/`, `circuits/ce_amp.spl`.
> Draft 4's §0 decisions are the lead's and are not reopened. Where the §2 example breaks a §0 rule, or can't be parsed as written, this review says so and gives the smallest fix.

---

## Summary

**Draft 4 can be built on what exists, and little of the existing code is thrown away.** The lexer's scanner, quantities, units and look-alikes stay. So do the Pratt loop, the `±` rule, the depth limits, recovery, `net`/`let` and the `Node`/`Body`/`Expr` AST. All of resolve's body code stays: nets, instances, fields, units and taints. Flatten stays whole. Resolve has not started on contracts (`Contract {}` is empty, `match_contracts` only pairs names), so most of what draft 4 changes is still unbuilt.

**Grammar: four places where draft 4 can't be parsed as written.** Each has a one-line fix that leaves §0 as it is:
1. **`for:` as a field name** (`usb.open { at: 1ms, for: 0us..=10ms }`). `for` is a keyword, as it is in Rust. → rename the field to `lasts:`.
2. **`usb.open { … }`, `Step on vdd.i { … }`, `mcu.wake at 1ms`, `until drop.end + 1s`**: struct literals on a field access, and infix words the expression grammar doesn't have. → ordinary constructors `Open { port: usb, at: 1ms, lasts: … }` and `Step { port: vdd.i, 5mA -> 30mA, edge: 1us }`, `event wake: mcu.wake { at: 1ms }`, and `window: ..=drop.end + 1s`, which is Rust's own `..=` prefix range.
3. **`12.0/mV`**: `mV` alone isn't a quantity (a unit must touch a number). → `12.0 / 1mV`.
4. **`GainStage<A = Mcp6001> { … }` in a `let`**: grammar.md §3 says `<` in an expression is always less-than. → generic arguments at a placement are *named only*, and `Path < IDENT =` is recognized with a three-token lookahead. `=` is never an expression operator, so this is unambiguous.

**Four implementation rules the parser needs.** They aren't draft defects:
- `setup` and `pub` start both items (`setup S for X`, `pub block`) and statements (`setup = S;`, `pub spec`). Today `ends_body` treats every item keyword as the end of an unclosed body, so `contract X { setup = Operating; … }` would close the contract at `setup`. The fix is a two-token rule.
- **`in` leaves the operator table.** `within` takes its place at level 1. Then `in` ends every expression, so `for f in 10Hz..=1MHz in Run` and `env x: T in r` parse with no lookahead.
- `spec cmrr(dm: Diff, …) {` looks exactly like the error case `spec dc(out.v) within …` that `SpecNeedsName` catches today (`nth(1) == Colon`). The parser has to look past the `)` for a `{`.
- `x..` (open upper end), `..=x` (open lower end) and `..Base` (the base of a struct update) become legal. The `HalfOpenRange` error narrows to `a..b`.

**Keywords.** New hard keywords: `circuit`, `setup`, `within`, `rated` and `ensure`, plus `env`, `const`, `pub` and `for`, promoted from reserved. `port` and `assume` become removed words with a migration message. `mode`, `event`, `observe`, `emits`, `with` and `on` stay **contextual**, recognized by position. `window` and `at` are ordinary field names. The reason for each is in §2.2.

**Semantic defects in the §2 example** (§3.10). None needs a §0 change:
- `pub spec recovers` measures `v3v3`, an internal net, and `pub spec holdup` measures `mcu.code` through a placement. Both break "internal specs are never published".
- A derived setup that writes a field its base's modes also write (`DropoutRow`'s `vout.i` against Operating's `mode Run { vout.i: … }`) has no stated precedence.
- `(inp, inn): Pair { z: 350Ω ± 0.1% }` as one knob makes the two legs always equal, which hides the imbalance CMRR depends on.
- `mcu.code[full]` names no analysis, where `ac(out.v)[dm]` does.

**Migration.** Five steps, each ending green (§4.3):
1. Tokens and keywords.
2. `within` replaces `in`.
3. The block header plus `circuit`. Resolve assembles today's `Block` from the two items, so flatten doesn't change and the span-free `Design`/`FlatDesign` dumps must come out byte-identical, a free regression check.
4. The `setup`, `env` and `const` items and the new contract statements, all parsed.
5. M1d-5: contracts resolved in the MVP subset, and setup and env knobs.

**The MVP subset** (§4.4): `block` with ports, one `circuit`, `setup` with no `..` and no modes, `env`, `const`, `contract { setup = S; let; [pub] spec name: m <limit>; }`, and internal-spec detection. Every other clause is *parsed* now and rejected in resolve as "not supported yet", so the grammar is fixed once. `ce_amp.spl` in v4 flattens to the same 8 knobs, with `temp` renamed `ambient`, and the same 3 specs, so the engine plan's 579 runs stay pinned.

**Engine** (§5). Since draft 2:
- `setup =` is `accepts` renamed. Nothing changes.
- Internal vs `pub` replaces [R2]'s port-fact guess with an explicit flag.
- **Modes** multiply setups: one `Plan.setups` entry per (setup, mode).
- **`rated`** becomes monitors on every run of every setup, and it reaches down the hierarchy: a child's ratings are monitored in the parent's runs.
- **Faults** bring transients whose stop time depends on a knob, and a topology change (`reversed`) that needs its own deck.
- **Pairs** need per-leg `z` knobs and per-setup AC excitation.
- `engine_types.md` still reads a `KnobTable` that model.md dropped on 2026-09-29. Knobs live in `FlatDesign`.

---

## 1. Draft 4 construct by construct: where each lands

| Construct | Lexer | Parser/AST | Resolve | Flatten | Engine | MVP |
|---|---|---|---|---|---|---|
| `pub block X { port: T, … }` | `pub` kw | new item form | ports from the header (same `Signature`) | none | none | ✓ |
| `circuit X { net; let; }` | `circuit` kw | today's block body | today's `BodyResolver` | none | none | ✓ |
| `setup S for X { … }` | `setup` kw, `->`, strings | new item, own entry grammar | `SetupDef`, effective setup | setup knobs | `Plan.setups` | ✓ no `..` or modes |
| `..Base`, `vin.wave: …` | `..` legal | entry form | field-by-field merge | owner of each knob | shared knobs | later |
| `const`, `env` | promoted kws | new items | `Const`, `Env` | env knobs | `origin: Global` | ✓ |
| `setup = S;` | none | contract stmt, 2-token rule | `Contract.default_setup` | none | = [R2] `accepts` | ✓ |
| `spec n: m within r;` | `within` kw | relation (op renamed) | `SpecDef`, `MExpr` | `FlatSpec` | sides | ✓ |
| `with`, `for … in`, `on`, `in M` | contextual, `in` kw | spec clauses | pins, band, setup, mode | anonymous setups | Band, Setup per mode | parse only |
| `spec n(a: A) { let; ensure; }` | `ensure` kw | fn form | slots | multi-slot | `Experiment` | parse only |
| `m[s]` | none | index postfix | slot on probes | none | `Probe { slot }` | parse only |
| `pub spec` / internal | none | flag | reach analysis | carried | `public` on sides | ✓ |
| `rated` | `rated` kw | stmt | `Rating` | monitors | monitors | parse only |
| `mode M { … }` | contextual | setup entry | `ModeDef` | per-mode setups | × modes | parse only |
| `(a, b): Pair { … }` | none | tuple key | `SetupKey::Pair` | pair knobs | 2 sources | parse only |
| `observe`, `emits` | contextual | block entries | interface | probes/PWL | quantized, PWL | parse only |
| `#[fault]`, `event`, `window` | strings for `#[outside(reason = "…")]` | attr args `k = v` | `SetupKind`, `EventDef` | event knobs | tran, decks | parse only |
| generics, `: Trait`, `trait` | `trait` reserved | header forms | monomorphize | instantiation | [R2] §6 | reserved |

---

## 2. Grammar

### 2.1 Tokens

| Change | Why | Risk to existing files |
|---|---|---|
| **`Arrow`**, `->`, when the two characters touch | `Step { 5mA -> 30mA }`, `Sweep(4.5V -> 3.0V)` | none: `a->b` was already a parse error (`-` then `>`) |
| **`Str`**, `"…"` on one line, escapes only `\"` and `\\` | `#[outside(reason = "…")]` | none: `"` was `Unknown` |
| **`..` is no longer diagnostic-only** | `10kΩ..`, `..Operating` | `HalfOpenRange` stays for `a..b` (§2.4) |
| Look-alike `→` (U+2192) → `->` | pasted from datasheets, as grammar.md §7 #23 | none |
| Unit **`y`** (year, 365.25 d), and `h` for hour | `env life: Duration in 0y..=10y` | `y` isn't in the prefix table (yocto isn't supported), so rule 1 of grammar.md §5.2 takes it |

No other token is needed. `=` stays a statement token and never an operator, and three rules below lean on that.

### 2.2 The keyword table

Hard keywords are words the parser syncs on during recovery (A8 needs statement and item starters to be real tokens). A contextual word is recognized only in one position, so it stays usable as a field or net name. That's grammar.md §2.3's rule for `part`, `from` and `on`, and Rust's for `union` and `auto`.

| Word | Today | Draft 4 | Why |
|---|---|---|---|
| `block`, `contract`, `let`, `net`, `spec` | kw | kw | unchanged |
| `circuit` | ident | **kw** `KwCircuit` | item start, a recovery sync point |
| `setup` | ident | **kw** `KwSetup` | item start *and* statement start (`setup =`), see G1 |
| `env`, `const`, `pub`, `for` | reserved | **kw** | already reserved for this; no file changes meaning |
| `within` | ident | **kw**, infix at level 1 | the limit operator. A net named `within` is implausible |
| `rated`, `ensure` | ident | **kw** | statement starts; recovery syncs on them. Part fields use `rating:` (model.md E8), so no clash |
| `in` | kw, infix | **kw, not an operator** | `for x in r`, `env x: T in r`, `in Mode`. See G2 |
| `port`, `assume` | kw | **removed**, `KwReserved`-like with a migration message | "ports go in `block X { … }`", "`assume` is now `setup = S;` and the setup's fields" |
| `trait`, `impl` | ident, reserved | reserved | not in the MVP; reserve now so `trait` can't become a name |
| `mode`, `event` | ident | **contextual**: at a setup entry start, `mode IDENT {` or `event IDENT :` | `mode:` is a plausible part field (a regulator's `mode: Pfm`); a hard keyword would forbid it forever |
| `observe`, `emits` | ident | **contextual**: at a block entry start, `observe IDENT :` | a port is `IDENT :`, so two tokens decide |
| `with`, `on` | ident | **contextual**: after a spec's relation | an identifier can't continue an expression there, so the Pratt loop has already stopped |
| `window`, `at`, `temp` | ident | ident | they're setup field names, not syntax |

In `parser/mod.rs`, `is_keyword`, `starts_item`, `is_stmt_keyword` and `home_body` change to match. `lexer/mod.rs::keyword()` gets the new arms, and `lexer.md` L9's list and grammar.md §2.3 are updated with it.

### 2.3 Conflicts and ambiguities

Each entry is marked **draft fix** (the §2 text can't be parsed as written; the fix keeps §0) or **rule** (an implementation decision for grammar.md).

**G1. `setup` and `pub` start both items and statements.** (rule)
`ends_body` (`parser/mod.rs:894`) is true for any `starts_item` token, so that an unclosed body ends at the next item. With `setup` and `pub` as item starters, `contract X { setup = Operating;` and `pub spec …` would each close the contract.
- **Rule:** `starts_item(i)` looks at two tokens.
  - `setup` starts an item unless the next token is `=`.
  - `pub` starts an item unless the next token is `spec`.
  - `circuit`, `contract`, `block`, `env`, `const` and `trait` always start one.
  - The same function drives `ends_body`, recovery (A8) and item-level skipping.
- *Precedent:* rustc's `check_keyword` plus `look_ahead(1, …)` for `unsafe fn` vs `unsafe {`, and `union` as a contextual keyword recognized only when an identifier follows it (`parse_item_kind`).

**G2. `in` as a limit, a range binder and a mode filter.** (rule)
v0.1 parses `x in r` as a level-1 operator (`expr.rs:27`). If it stayed one, `dc(vin.i - vout.i) <= 50uA in Sleep` would parse as `(… <= 50uA) in Sleep`, a `Chained` error.
- **Rule:** remove `KwIn` from `infix()` and put `KwWithin` in its place. Then `in` ends any expression, like `;` or `,`.
- `for f in 10Hz..=1MHz in Run` parses with no lookahead: the range stops at the second `in`, which is read as the mode.
- `in` after a spec's relation is a mode filter only as the **last** clause, `in IDENT ;`. Anything else there (`in 4.5V..=6.5V`, `in LO..=HI`) is v0.1's limit. It gets "`in` is now `within`" with a machine-applicable fix, which the migration can rely on.
- *Human reading:* `for f in a..=b in Run` does read as two nested `in`s. The formatter should print clauses in the fixed order `with`, `for`, `on`, `in`, and `--explain` spells the mode out ("in mode Run"). The parser has no ambiguity, so §0 stands.

**G3. `spec name(` vs the missing-name error.** (rule)
`spec_stmt` (`mod.rs:383`) says a spec is named only if `nth(1) == Colon` (or `Ident Ident`). Otherwise it reports `SpecNeedsName`, the grammar.md §7 #17 case `spec dc(out.v) in …;`. `spec cmrr(dm: Diff, cm: Common) {` has the same first tokens.
- **Rule:** after `spec IDENT (`, skip the balanced parentheses. If `{` follows, it's the function form; otherwise the name is missing.
- The skip is bounded by the nesting limit (A11), and no backtracking is needed for the tree: the skip only picks which parse function runs.

**G4. Generic arguments at a placement.** (draft fix, small)
`let amp = GainStage<A = Mcp6001> { … }` is an expression, and grammar.md §3 says "generics appear only in types, so `<` in an expression is always less-than". Draft 4 also has comparisons *inside* expressions (`vin.v.first(vout.v < 3.267V)`), so `<` can't simply be banned there.
- **Fix:** at a placement, generic arguments are **named only** (`<A = Mcp6001, GAIN = 20>`, which every example already uses).
- In `primary`, `path < IDENT =` starts generic arguments. That's three tokens, unambiguous because `=` is never an operator. A positional `GainStage<Mcp6001> {` gets "name the parameter: `GainStage<A = Mcp6001>`".
- Argument values are parsed with `expr_bp(COMPARE)`, so `>` closes the list. A comparison inside needs parentheses.
- The same applies to const defaults in a block header: `const GAIN: f64 = 10.09>` would otherwise read `10.09 > …`.
- *Precedent:* Rust needs the turbofish `::<>` in expressions for exactly this reason, and wraps complex const arguments in braces (`Foo<{ N + 1 }>`). The named form avoids both, because `=` marks the argument.

**G5. `for` as a field name.** (**draft fix**)
`event drop: usb.open { at: 1ms, for: 0us..=10ms }`. `for` is a keyword (reserved since v0.1, as in Rust, which needs `r#for`).
- **Fix:** name the field `lasts:`, or `duration:`.

**G6. Struct literals on a field access, and words used as operators.** (**draft fix**)
- **The draft's forms, and why none of them parse:**
  - `usb.open { … }`: `primary` takes `path [ "{" fields "}" ]`, where a path is `IDENT {:: IDENT}`. So `usb.open` ends at `.open`, and the `{` is unexpected.
  - `emits wake: Step on vdd.i { 5mA -> 30mA, edge: 1us }`: `on` in the middle of an expression, followed by a struct body on a field access.
  - `event: mcu.wake at 1ms`: `at` used as an infix word. It also uses a *different* event syntax from `event drop: …`, which declares a name.
  - `window: until drop.end + 1s`: `until` used as a prefix word.
- **Fix, with no grammar added:**
  ```rust
  emits wake: Step { port: vdd.i, 5mA -> 30mA, edge: 1us },
  event drop: Open { port: usb, at: 1ms, lasts: 0us..=10ms },
  event wake: mcu.wake { at: 1ms },          // one event form: always named
  window: ..=drop.end + 1s,                  // "until" is Rust's `..=` prefix range
  ```
  `mcu.wake { … }` still has a field access before `{`. Allow that only in an `event` entry's value (`event_value = expr_path [ "{" fields "}" ]`), or write `Trigger { event: mcu.wake, at: 1ms }` and keep `primary` unchanged. This review recommends `Trigger`: one rule for every `{`.
- `usb.polarity: reversed` can stay: a setup key with a field path, and a plain identifier as its value.

**G7. `12.0/mV`.** (**draft fix**)
`mV` without a number is an identifier, so this reads `12.0 / (name mV)`, and resolve reports an unknown name.
- **Fix:** `12.0 / 1mV`. A unit-only literal form can come later if it's wanted; it isn't needed for this example.

**G8. Mixed fields in a struct literal: `Step { 5mA -> 30mA, edge: 1us }`.** (rule)
Rust doesn't mix positional and named fields, and a bare `IDENT` field is already the `gnd` shorthand.
- **Rule:** `field = IDENT ":" expr | IDENT | expr "->" expr`. The parser reads `IDENT`. If `:` follows, the field is named. If `,` or `}` follows, it's the shorthand. Otherwise it parses an expression and requires `->`.
- A `->` element must come first. Resolve checks which part kinds and shapes take one (`Step`, `Sweep`, `Ramp`).
- In call arguments the same rule applies: `arg = [ IDENT ":" ] expr [ "->" expr ]`.

**G9. Named call arguments: `h.f_high(-3dB, ref: dc)`.** (rule)
Call arguments today are a plain `list`. They gain `IDENT ":"`, as G8 does.
- Here `dc` is a measure *function* used as a value. Resolve should read `ref:` against a small enum of references (`dc`, `peak`, `at(f)`), not look `dc` up as a value.

**G9b. One function, one spelling.** (small draft fix)
`db(a_dm / a_cm)` in `cmrr` is a free function, while §1 lists `.db()` as a method. P8 asks for one way: write `(a_dm / a_cm).db()`.

**G10. `setup =` inside a contract vs the `setup` item.** Covered by G1. Inside a contract body, `setup` followed by `=` is the only form. A second `setup =` is a resolve error (a duplicate, with both spans, as E9).

**G11. Setup entries: separators, keys, the base.** (rule)
§2 writes `temp: …,` and then `mode Run { … }` with no comma, and `event drop: …,` with one.
- **Rule:** `,` separates entries, except that a `mode … { }` entry may omit its comma. That's the rule for Rust match arms with a block body.
- `..Base` must come last, with no comma after it, exactly as Rust's struct update (Reference §8.2.9 *Functional update syntax*).
- **Keys** are field paths, `IDENT { "." IDENT }`, or a pair, `"(" IDENT "," IDENT ")" { "." IDENT }`. They exist only in setup entries and `with` pins. Struct literals keep plain `IDENT` fields, so none of today's struct-literal parsing changes.
- `..` followed by an identifier at an entry start is the base. `..=` is a different token, so `z: ..=0.5Ω` and `..Operating` never meet.

**G12. The tuple key `(a, b)`.** (rule)
It exists only as a setup key (G11). In expressions `( expr , …` stays an error for now. The day a measure needs a pair's differential value, `dm(inp, inn)` as a function avoids tuple expressions.

**G13. `m[s]`.** (rule)
A postfix `"[" expr "]"`, parsed in the same loop as `.` and `(`. `[a, b]` at the *start* of an operand is still an array (`net x = [a, b];`). Statements end in `;`, so a `[` after an expression can only be an index.
- Resolve accepts only a setup parameter's name inside the brackets (§3.3).

**G14. The limit clauses.** (rule)
`with`, `for`, `on` and `in` go after the relation, in that fixed order, as the table in §1 of the draft lists them. Out of order gives an error with a reorder fix (P8: one way).
- `with` takes `key = expr { , key = expr }`. `=` ends the key, and the expression ends at `,`, a clause word or `;`.
- `for IDENT in expr`: the variable (`f`) is *used* before it's declared (`vout.z(f) … for f in …`). Resolve collects clause bindings before the measure, as E4 collects names before bodies.
- The variable may not shadow a port or net name (`Scope::declare` reports the clash).

**G15. Attribute arguments.** (rule)
`#[outside(reason = "…")]` and `#[check(A = [Mcp6001, Tlv9001])]` use `name = value`, which today's `list` of expressions rejects.
- **Rule:** `attr_arg = [ IDENT "=" ] expr`. `=` is never an operator, so one token of lookahead decides.

**G16. Recovery inside a setup.** (rule)
Setup entries are comma lists, not `;` statements.
- **Rule:** a new recovery level, `Entry`, that stops before a `,` with no bracket open, before the closing `}`, or before `mode`/`event`/`..`.
- A broken entry becomes an `Entry::Error(Reported)`, and the setup keeps its other fields, as a broken statement does (A4, A8).

### 2.4 Precedence (grammar.md §4.1), changed rows only

| Level | Operators | Change |
|---|---|---|
| 1 | `within` `<` `<=` `>` `>=` | `in` → `within`. `in` is no longer an operator |
| 2 | `..=` infix, `..=` prefix, `..` postfix | **new:** `..=b` (an open lower end) and `a..` (an open upper end, legal only when the next token can't start an operand: `,` `}` `)` `;`). `a..b` stays `HalfOpenRange`, with the fix `..=` (Rust's `RangeFrom` rule, Reference §8.2.14) |
| 7 | `.f`, `.m(…)`, calls, **`[s]`** | index postfix added |

§4.3's shape rules extend to cover the new forms: an open range isn't allowed on either side of `<`, `<=`, `>` or `>=`, and a range endpoint can't carry a tolerance.

### 2.5 EBNF for the new items

Notation as grammar.md §3. Productions not shown are unchanged.

```ebnf
file          = { item } EOF ;
item          = { DOC } { attribute } [ "pub" ] item_kind ;
item_kind     = block | circuit | setup | contract | env | const ;   (* trait, impl: reserved *)

(* project level *)
env           = "env" IDENT ":" type "in" expr ";" ;              (* expr: a range or n ± t *)
const         = "const" IDENT ":" type "=" expr ";" ;

(* the interface *)
block         = "block" IDENT [ generics ] [ ":" bounds ] "{" [ block_entries ] "}" ;
generics      = "<" generic { "," generic } [ "," ] ">" ;
generic       = IDENT [ ":" bounds ] [ "=" type ]                   (* A: OpAmp = Mcp6001 *)
              | "const" IDENT ":" type [ "=" gexpr ] ;              (* gexpr = expr_bp(COMPARE): G4 *)
bounds        = path { "+" path } ;
block_entries = block_entry { "," block_entry } [ "," ] ;
block_entry   = { DOC } { attribute } ( port | observe | emits ) ;
port          = IDENT ":" type ;
observe       = "observe" IDENT ":" type ;                         (* contextual: G-table *)
emits         = "emits" IDENT ":" expr ;                           (* contextual *)

(* the implementation *)
circuit       = "circuit" IDENT "{" { circuit_stmt } "}" ;
circuit_stmt  = { DOC } { attribute } ( net | let ) ;              (* today's block_stmt minus port *)

(* environments *)
setup         = "setup" IDENT "for" path "{" setup_body "}" ;
setup_body    = [ entry { sep entry } [ "," ] ] [ [ "," ] base ] ;  (* sep: G11 *)
entry         = { DOC } { attribute } ( field_set | mode | event ) ;
field_set     = key ":" expr ;
key           = ( IDENT | "(" IDENT "," IDENT ")" ) { "." IDENT } ;
mode          = "mode" IDENT "{" [ field_set { "," field_set } [ "," ] ] "}" ;
event         = "event" IDENT ":" expr ;
base          = ".." path ;

(* promises *)
contract      = "contract" IDENT "{" { contract_stmt } "}" ;
contract_stmt = { DOC } { attribute } ( default_setup | rated | let | spec ) ;
default_setup = "setup" "=" path ";" ;
rated         = "rated" relation ";" ;
spec          = [ "pub" ] "spec" IDENT ( spec_line | spec_fn ) ;
spec_line     = ":" relation clauses ";" ;
spec_fn       = "(" [ param { "," param } [ "," ] ] ")" "{" { spec_stmt } "}" ;
param         = IDENT ":" path ;
spec_stmt     = { DOC } { attribute } ( let | ensure ) ;
ensure        = "ensure" relation clauses ";" ;
clauses       = [ "with" pin { "," pin } ] [ "for" IDENT "in" expr ] [ "on" path ] [ "in" IDENT ] ;
pin           = key "=" expr ;
relation      = expr ;                                             (* top must be a rel_op: NotARelation *)

(* expressions: changes *)
rel_op        = "within" | "<=" | ">=" | "<" | ">" ;
range         = tol [ "..=" tol | ".." ]  |  "..=" tol ;
postfix       = primary { "." IDENT | "(" [ args ] ")" | "[" expr "]" } ;
args          = arg { "," arg } [ "," ] ;
arg           = [ IDENT ":" ] expr [ "->" expr ] ;
primary       = QUANTITY | STRING
              | path [ gen_args ] [ "{" [ fields ] "}" ]
              | "(" expr ")" | "[" [ list ] "]" ;
gen_args      = "<" IDENT "=" garg { "," IDENT "=" garg } [ "," ] ">" ;   (* only if `< IDENT =`: G4 *)
garg          = type | gexpr ;
field         = IDENT ":" expr | IDENT | expr "->" expr ;          (* G8 *)
attribute     = "#" "[" path [ "(" [ attr_arg { "," attr_arg } [ "," ] ] ")" ] "]" ;
attr_arg      = [ IDENT "=" ] expr ;
```

The **"no struct literal here" restriction** (grammar.md §3, ast.md "not needed yet") is still not needed. No draft-4 header has `expr {`: `for f in r` is followed by a clause word or `;`, never `{`. It arrives with `for` loops in circuits.

### 2.6 The AST (ast.md §4), changed parts

```rust
pub enum ItemKind<'src> {
    Block(BlockDecl<'src>),          // header only
    Circuit(Body<'src>),             // today's Body: name + stmts (+ broken)
    Setup(SetupDecl<'src>),
    Contract(Body<'src>),
    Env { name: Ident<'src>, ty: Type<'src>, range: Expr<'src> },
    Const { name: Ident<'src>, ty: Type<'src>, value: Expr<'src> },
    Error(Reported),
}
pub struct Item<'src> { /* Node: docs, attrs, */ public: Option<Span>, kind, span }  // `pub`, with its span for fixes

pub struct BlockDecl<'src> {
    pub name: Ident<'src>, pub generics: Vec<Generic<'src>>, pub bounds: Vec<Path<'src>>,
    pub entries: Vec<Node<'src, BlockEntry<'src>>>, pub broken: Option<Reported>,
}
pub enum BlockEntry<'src> { Port { name, ty: Type }, Observe { name, ty }, Emits { name, value: Expr }, Error(Reported) }

pub struct SetupDecl<'src> {
    pub name: Ident<'src>, pub for_block: Path<'src>,
    pub entries: Vec<Node<'src, SetupEntry<'src>>>, pub base: Option<Path<'src>>, pub broken: Option<Reported>,
}
pub enum SetupEntry<'src> {
    Set { key: Key<'src>, value: Expr<'src> },
    Mode { name: Ident<'src>, sets: Vec<(Key<'src>, Expr<'src>)> },
    Event { name: Ident<'src>, value: Expr<'src> },
    Error(Reported),
}
pub struct Key<'src> { pub head: KeyHead<'src>, pub path: Vec<Ident<'src>>, pub span: Span }
pub enum KeyHead<'src> { Name(Ident<'src>), Pair(Ident<'src>, Ident<'src>) }

pub enum StmtKind<'src> {                       // Port and Assume removed
    Net { .. }, Let { .. },                     // unchanged
    DefaultSetup { setup: Path<'src> },
    Rated { relation: Relation<'src> },
    Spec { public: Option<Span>, name: Ident<'src>, form: SpecForm<'src> },
    Ensure { relation: Relation<'src>, clauses: Clauses<'src> },
    Error(Reported),
}
pub enum SpecForm<'src> {
    Line { relation: Relation<'src>, clauses: Clauses<'src> },
    Fn { params: Vec<(Ident<'src>, Path<'src>)>, stmts: Vec<Stmt<'src>> },
}
pub struct Clauses<'src> {
    pub with: Vec<(Key<'src>, Expr<'src>)>,
    pub sweep: Option<(Ident<'src>, Expr<'src>)>,
    pub on: Option<Path<'src>>,
    pub mode: Option<Ident<'src>>,
}
pub enum RelOp { Within, Lt, Le, Gt, Ge }       // In → Within
pub enum ExprKind<'src> { …, Str(Span), Index { base, index }, RangeTo(Box<Expr>), RangeFrom(Box<Expr>),
                          StructLit { path, generics: Vec<(Ident, GenArg)>, fields }, … }
pub enum FieldKind<'src> { Named(Ident, Expr), Shorthand(Ident), Transition(Expr, Expr) }  // G8
pub struct Arg<'src> { pub name: Option<Ident<'src>>, pub value: Expr<'src>, pub to: Option<Expr<'src>> }
```

`BodyKind` gains `Circuit` and `SpecFn`, and `WrongBody` keeps its meaning. `net` belongs in a circuit, `ensure` in a spec body, and `setup =`, `rated` and `spec` in a contract.

### 2.7 What changes in each design note

| Note | Changes |
|---|---|
| `grammar.md` | §1's example becomes `within`. §2.3 keywords per §2.2 here. §2.4 adds `->`. §2.5 adds `Str`. §3 is replaced by §2.5 here. §4.1 per §2.4. §6 adds the entry recovery level and the two-token `starts_item`. §7 rows: #6's hint becomes "`b within a..=c`", #8 narrows to `a..b`, #17 per G3, #20 (`assume` in a block) is replaced by "`assume` was removed". New rows for G4, G5, G11 (base not last), G14 (clause order), "`in` is now `within`", and "ports go in the header". The version goes to **v0.2** |
| `lexer.md` | L9's keyword list; the token enum in §3 (`KwCircuit KwSetup KwEnv KwConst KwPub KwFor KwWithin KwRated KwEnsure`, `Arrow`, `Str`, with `KwPort`/`KwAssume` removed); the `→` look-alike; units `y` and `h` |
| `ast.md` | §4 per §2.6. §5: the `ce_amp.spl` counts become "5 items (env, block, circuit, setup, contract): 4 ports, 8 circuit statements, 5 contract statements". A11's limits also cover setup entry nesting |
| `model.md` | §3.10 below; E16 and E24 rewritten; §5's knob table (`temp` → `ambient`) |
| `language.md` | the owners' job; this review only lists the constructs |

---

## 3. Elaboration

### 3.1 The `Design`

**Recommendation: keep today's `Block` as the merge of `block` and its one `circuit`.** Second implementations are parked (§0), so a separate `Impl` table ([R2] §2.1) would be a seam used by exactly one element, and flatten reads `design.block(id).{nets, instances, merges}` in many places. When second implementations come, splitting `Block` is a mechanical refactor. Until then, flatten doesn't change at all.

```rust
pub struct Design {
    pub blocks: Vec<Block>,               // interface + its circuit's body, as today
    pub setups: Vec<SetupDef>,            // SetupDefId
    pub contracts: Vec<Option<Contract>>, // indexed like blocks, as today
    pub envs: Vec<Env>,                   // EnvId
    pub consts: Vec<Const>,               // ConstId
}
pub struct Block {                        // today's fields, plus:
    pub public: bool,
    pub circuit: CircuitState,            // Written | Missing | Twice(Reported)
    pub observes: Vec<Observe>,           // (later) name, type
    pub emits: Vec<Emit>,                 // (later) name, port quantity, wave
}
pub struct Env   { pub name: String, pub range: Value }          // `ambient`: 263.15 ..= 333.15 K
pub struct Const { pub name: String, pub value: Quantity }       // always Exact (E25)

pub struct SetupDef {
    pub name: String, pub block: BlockId, pub kind: SetupKind,   // Normal | Fault | Outside { reason }
    pub base: Option<Result<SetupDefId, Reported>>,
    pub sets: Vec<SetupSet>,              // only what this item writes
    pub modes: Vec<ModeDef>, pub events: Vec<EventDef>,
}
pub struct SetupSet { pub key: SetupKey, pub value: SetupValue }
pub enum SetupKey {
    Port { port: PortId, path: Vec<Field> },          // vin, vin.wave, vout.i
    Pair { a: PortId, b: PortId, path: Vec<Field> },  // (inp, inn), (inp, inn).dm.ac
    Temp, Window,
    Child { instance: InstanceId, path: Vec<Field> }, // mcu.state: names the circuit (§3.3)
}
pub enum SetupValue { Shape(Shape), Value(Value), Env(EnvId), Wave(Wave), Word(String), Invalid(Reported) }
pub struct ModeDef { pub name: String, pub sets: Vec<SetupSet> }

pub struct Contract {
    pub default_setup: Result<SetupDefId, Reported>,  // `setup = S;`: missing is an error
    pub rated: Vec<Rating>,
    pub measures: Vec<Measure>,                        // contract-level `let`s
    pub specs: Vec<SpecDef>,
}
pub struct SpecDef {
    pub name: String, pub public: bool, pub reach: Reach,   // §3.3
    pub slots: Vec<Slot>,                  // one-liner: one slot, the `on` setup or the default
    pub lets: Vec<(String, MExpr)>, pub ensures: Vec<Ensure>,
}
pub struct Slot { pub name: Option<String>, pub setup: SetupDefId, pub pins: Vec<(SetupKey, Quantity)>,
                  pub mode: Option<ModeRef> }
pub struct Ensure { pub measure: MExpr, pub op: RelOp, pub bound: Value, pub sweep: Option<Sweep> }
pub enum MExpr { Probe { slot: u8, probe: Probe }, Env(EnvId), Const(Quantity), Axis,
                 Call(MeasureFn, Vec<MExpr>), Binary(Op, Box<MExpr>, Box<MExpr>), Local(u16) }
pub enum Probe { Port(PortId, Qty), Net(NetId, Qty), Child(InstanceId, PortId, Qty), Observe(InstanceId, u16) }
```

`DesignSourceMap` gains `SetupSpans`, `ContractSpans` and `BlockSpans::circuit_name: Option<Span>`, so an error in a circuit points into the circuit item.

### 3.2 Resolve, as passes

1. **Names.**
   - Every item's name, in its namespace (E5): blocks and traits in kinds; setups in their own namespace per block (`Operating` exists once *per block*); envs and consts in values, file-wide.
   - Each `circuit`, `setup` and `contract` is matched to its block, as `match_contracts` does today (`resolve/mod.rs:389`). The errors are "circuit/setup/contract without a block", "a second circuit for X (second implementations aren't supported yet)", and a duplicate setup name for one block.
2. **Signatures.** Each block's ports come from its header. It's the same `Signature` table, filled from `BlockEntry::Port` instead of `StmtKind::Port`. `BlockBuilder::push_port` is unchanged.
3. **Circuits.** Each circuit's body goes through today's `BodyResolver::resolve`, started from the header's `BlockBuilder`. It's the same code, reached through a different item.
4. **Envs and consts.** Values are typed against the declared type (`Temperature` → a K point, `Duration` → s, `f64` → dimensionless). A `const` with a spread is an error ("a `const` is fixed; use `env` for a range").
5. **Setups.** Each `SetupDef` is resolved against its block's ports, and against its circuit for `Child` keys. Then bases are resolved, and inheritance cycles are found (one report per cycle, like flatten's recursion pass). Then each *effective* setup is computed (§3.4).
6. **Contracts.** The default setup; lets, as `MExpr`; each spec's slots, clauses, measure, bound and reach (§3.3). `pub` rules are checked here.
7. **Taint.** A block is tainted if its header, its circuit or any error inside them is. A setup or contract with an error taints *its own checks*, not the circuit: the circuit can still be flattened and drawn.

### 3.3 Scopes, and internal vs `pub`

A contract's value scope has two layers, and each name records which layer resolved it.

| Layer | Names | Reach |
|---|---|---|
| Interface | ports and their quantities (`vout.v`, `vin.z(f)`), generic consts, envs, consts, the contract's `let`s, a spec's parameters and its `for` variable | `Interface` |
| Circuit | nets (`base`, `v3v3`), placements and through them child ports and observes (`amp.vout`, `mcu.code`) | `Circuit(first span)` |

**Rule:**
- A spec is **internal** when its measure reaches the circuit layer: through a probe, or through a `let` it uses (a `let`'s reach is the union of its operands'). Otherwise it's an interface spec.
- `pub spec` on an internal spec is an error, `PublishedInternal`, with the offending name's span as related: "`recovers` measures `v3v3`, a net of `circuit SensorBoard`; a parent can't rely on it."
- **Setups don't make a spec internal.** `mode Measuring { mcu.state: Run }` names a placement. If that counted, every spec on the board's `Operating` would be internal, since every spec runs in every mode. A mode's *name* is part of the interface even when its definition reaches into the circuit.
  - The setup is instead marked `circuit_bound`, which goes into the block's cache key ([R2] §6.2) and nothing else.
- **`pub` and setups:** `pub spec … on S` needs S to be the default setup (any of its modes, any `with` pins inside it) or a `#[fault]` setup. An `#[outside]` setup is an error, per §0. On any other setup (`LoadStep`) it's an error: "a parent is checked against `Operating`; `LoadStep` isn't a promise it can use." This is a *clarification* for the lead (§6 Q2). §0 says what parents rely on, but not on which setups.

What a parent may read of a child is the published interface: ports, `observe`, `emits`, modes. It never reads the child's nets. So `mcu.code` is legal in the board's contract, but it names the placement `mcu`, which exists only in the board's circuit. That's why `holdup` is internal.

### 3.4 Setups: the effective setup

- **Merge, field by field** (Modelica §7.2 modifiers, [R2] P2):
  - A derived setup's set on `vin.wave` replaces only that path. A whole-shape set (`vin: Signal { z: … }`) merges field by field into the base's `vin`.
  - The same path written twice in one setup is an error, with both spans (E9).
- **The implied shape.** A field path on a port the base never wrote (`vin.wave` where `Operating` wrote only `vin.z`, or `vout.i` with no `vout`) creates the port's **role default**: `Power<In>` → `Supply`, `Analog<In>` → `Signal`, any `<Out>` → `Load`, `Ground` → none (a key on `gnd` is an error).
- **Unwritten = ideal** (§0): no knob, and a fixed bench element.
- **Stimulus vs envelope.** The envelope check ([R2] §2.2: "a spec's setup must lie inside the default setup") only makes sense for **environment fields**: DC levels, `z`, load `r`/`c`/`i`, `temp`.
  - **Stimulus fields** (`wave`, `ac`, `Step`, `Sweep`, events, `window`) are how a test is run, not where the product lives. They're excluded.
  - Without this rule, `InputStep` (a 0 → 100 mV step on an input whose `v` is unwritten, so ideal 0 V) would fall outside the envelope and need `#[outside]`.
  - A `Step` in a *load* current (`LoadStep`'s 5 → 30 mA) is also a level. Its two levels must lie inside the envelope's `vout.i` for the mode it runs in.
  - This is a proposal (§6 Q3).
- **Values in `DropoutRow`:** `vout.i: 500mA` lies outside every mode's `vout.i`, which is exactly why it is `#[outside]`. The check should say so when the attribute is *missing*: "outside `Operating` (`vout.i` 500 mA ∉ 5…50 mA in `Run`); add `#[outside(reason = …)]` or narrow the setup."

### 3.5 Knobs (E16, rewritten)

| Source | Knob? | Kind | `KnobSource` | Identity (printed path) |
|---|---|---|---|---|
| part field with a spread | yes, per placement | Statistical | `Field { device, field }` (today) | `left.r1.value` |
| effective-setup field with a range or `±` | yes | Range | `SetupField { owner: SetupDefId, mode: Option<ModeId>, key }` | `vcc.v`; owner shown when two setups own one field (`LoadStep::vout.i`) |
| `temp: ambient` | the env's knob | Range | `Env(EnvId)` | `ambient` |
| `temp: -40°C..=125°C` | yes | Range | `SetupField { key: Temp }` | `temp` |
| env referenced by no used setup | no, and a warning ("declared, read by nothing", [R2] §2.4) | — | — | — |
| `const` | never | — | substituted as `Exact` | — |
| `with x = v`, a point value | no: a pin | — | the slot's `pins` | — |
| a range inside a mode | yes | Range | `SetupField { mode: Some(m) }` | `vout.i@Run` |
| a range inside an event (`lasts: 0us..=10ms`) | yes, that setup only | Range | `SetupField { key: event field }` | `Unplug::drop.lasts` |
| `Pair { dm, cm }` | yes, one each | Range | `SetupField { key: Pair … dm }` | `(inp, inn).dm` |
| `Pair { z }` | **two**, one per leg (§3.7) | Range | per port | `inp.z`, `inn.z` |

**Sharing** follows [R2] §3.2–3.3, and draft 4's "sharing rule" states the same thing:
- part knobs are always shared;
- an inherited range field is the same knob;
- a field a derived setup writes is its own knob.

Draft 4 adds one case: **the mode is shared across the slots of a multi-setup spec** (same board, same moment). `sensitivity(zero, full)` runs both slots in `Measuring`, then both in `Idle`, never one of each.

**Where knobs live.** They stay in `FlatDesign.knobs` (decided 2026-09-29). Part knobs come first, from flatten as today. Setup and env knobs are appended by the contract step (§3.9). So adding a contract never renumbers a part knob, and flatten's tests are unaffected.

### 3.6 Modes

- A `ModeDef` is an overlay on its setup. The effective setup of (S, M) is S's merge with M's sets on top.
- **Every spec expands to one slot per mode** of its effective setup, unless it says `in M`. A setup with no modes has one implicit mode. `in M` must name a mode of the spec's *effective* setup: `on LoadStep in Run` is valid because `LoadStep` inherits `Run`.
- **Precedence with derivation** (§0 is silent, and `DropoutRow` and `LoadStep` both hit it).
  - **Proposed rule:** a derived setup's top-level set on path P overrides P in its own top level *and* in every inherited mode. The derived item is written later, and draft 4 says "fields merge one by one".
  - So in `LoadStep`, `vout.i` is the step in both `Run` and `Sleep`. `on LoadStep in Run` then differs from `in Sleep` only in fields the modes set that `LoadStep` didn't.
  - `--explain` prints the effective setup per mode, so nothing is hidden. See §6 Q4.
- **Mode name collisions:** mode names are per setup. A derived setup may add modes, but may not redefine an inherited one (an error with both spans). Changing a mode's field goes through a path: `Run.vout.i: …`, parsed as an ordinary key whose head names a mode. This form isn't in the draft; it's only listed as the way out if it's needed.

### 3.7 Pairs

- `(a, b)` resolves two ports of the same signal type and role (`Analog<In>` for both). Anything else is an error.
- A port written in a pair may not also be written alone (`inp.v` next to `(inp, inn)`): an error with both spans.
- **The `Pair` shape:** `dm`, `cm` and `z`, where `dm` and `cm` are structured like a `Signal` (`v`, `ac`, `wave`). A plain value on `dm` sets `dm.v`, and `(inp, inn).dm.ac: 1V` sets its AC amplitude. That "a value on a structured field sets its primary subfield" rule has to be written down. It's what makes both `dm: -10mV..=10mV` and `.dm.ac` mean something.
- **`z: 350Ω ± 0.1%` must be two knobs, one per leg.** With one knob both legs are always equal, so the source is perfectly balanced at every corner. A bridge's CMRR is limited by exactly that imbalance, so a single knob makes `cmrr` pass where the real board fails: a confident wrong answer, the kind the engine plan forbids.
  - Two knobs, `inp.z` and `inn.z`, both from the one written value, give the imbalance a 2 × 0.1% corner.
  - If the lead wants one knob plus a mismatch, that's a separate field (`z_match: ± 0.05%`). This is a real soundness issue (§6 Q5).

### 3.8 `observe` and `emits`

- **In the model:**
  - `Block.observes` holds (name, type). `Integer` makes any measure over it `MeasureKind::Quantized` ([R2] P5).
  - `Block.emits` holds (name, port quantity, wave).
  - A probe `mcu.code` resolves to `Probe::Observe(instance, index)`. A setup entry `event wake: Trigger { event: mcu.wake, at: 1ms }` resolves `mcu.wake` through the placement to the child's `Emit`.
- **Two gaps in the draft**, for its owners:
  1. `Stm32Adc` and `InAmp` have no `circuit`, so they are model blocks. Nothing in draft 4 says how a model block's observe is computed (`code = floor(ain.v / vref · 4096)`) or where its per-state supply current comes from (§3 open item 3).
     - **MVP rule:** a *placed* block with no circuit is an error ("`Stm32Adc` has no circuit: model blocks aren't supported yet"). An unplaced one is interface-only: its setups and contract resolve but can't be checked.
  2. A block **with** a circuit that declares `observe x` needs a binding in its circuit (`observe x = adc.code;`, a `circuit_stmt`). Otherwise the parent reads a value nothing defines.
- **`mcu.code[full]` names no analysis.** `ac(out.v)[dm]` does. The engine needs to know which run gives the value.
  - **Rule:** a bare quantity in a measure is `dc(…)` of it, and the parser keeps no special case.
  - Or require `dc(mcu.code)[full]`. Either works; the draft should pick one (§6 Q6).

### 3.9 Flatten and the `FlatContract`

Flatten's passes are unchanged. A new function, run after it per root, builds the contract:

```rust
pub fn flatten_contract(flat: Flat, contract: &Contract) -> (FlatContract, Vec<Knob>, Vec<ContractProblem>);

pub struct FlatContract {
    pub setups: Vec<FlatSetup>,     // one per (effective setup, mode, pins) some spec uses
    pub specs: Vec<FlatSpec>,
    pub ratings: Vec<FlatRating>,   // (later) monitors: this root's, plus every placed child's (§5)
}
pub struct FlatSetup {
    pub def: SetupDefId, pub mode: Option<ModeId>, pub pins: Vec<(KnobId, Quantity)>,
    pub knobs: Vec<KnobId>,         // its subset: every part knob + its own and inherited range knobs
    pub bench: Vec<BenchElement>,   // what lowering (M1e) turns into sources and loads
    pub kind: SetupKind,            // Normal | Fault | Outside
}
pub struct FlatSpec {
    pub spec: (BlockId, u16), pub public: bool, pub internal: bool,
    pub slots: Vec<FlatSetupId>,    // one per mode × parameter; the mode shared across slots
    pub measure: MExpr, pub ensures: Vec<Ensure>,
}
```

The probe for an internal net (`base`, `v3v3`) resolves through the flat net map exactly as a port does, so an internal spec costs nothing extra here.

### 3.10 Defects in the §2 example (collected)

| # | Where | Problem | Smallest fix |
|---|---|---|---|
| D1 | `SensorBoard`: `pub spec recovers: tran(v3v3.v)…` | `v3v3` is an internal net, so the spec is internal and can't be `pub` (§0) | drop `pub`, or measure a port |
| D2 | `SensorBoard`: `pub spec holdup: tran(mcu.code)…` | reaches the circuit through the placement `mcu` | drop `pub`, or give the board `observe reading: Integer` bound in its circuit (§3.8) and measure `reading` |
| D3 | `Unplug`: `for: 0us..=10ms` | keyword as a field (G5) | `lasts:` |
| D4 | `Unplug`, `McuWakes`, `Stm32Adc` | `usb.open { … }`, `at 1ms`, `until …`, `Step on vdd.i { … }` (G6) | constructors, `..=` |
| D5 | `sensitivity` | `12.0/mV` (G7) | `12.0 / 1mV` |
| D6 | `sensitivity` | `mcu.code[full]` has no analysis (§3.8) | the `dc` default, or write it |
| D7 | `cmrr` | `db(…)` vs `.db()` (G9b) | `(a_dm / a_cm).db()` |
| D8 | `InAmp` | one `z` knob for both legs hides the imbalance (§3.7) | per-leg knobs |
| D9 | `LoadStep`, `DropoutRow` | derived `vout.i` vs the modes' `vout.i`: no precedence (§3.6) | the proposed rule |
| D10 | `project.spl` | `const confidence` is read by the engine *by its name*, a magic identifier | recognize it by its type (`Confidence`, at most one per project), or use an inner attribute `#![confidence(sigma(3))]` |
| D11 | `project.spl` + `ce_amp.spl` | two files, no `use`: how does `ce_amp.spl` see `ambient`? | a project model (every `.spl` in the project shares env/const) is needed before this works. **MVP: one file**, with `env` in it |

---

## 4. Migration from v0.1

### 4.1 What stays, what changes

| File | Stays | Changes |
|---|---|---|
| `lexer/{quantity,lookalike,check,error}.rs` | everything | `→` look-alike; `Str` scanning; units `y`, `h` |
| `lexer/token.rs`, `lexer/mod.rs::keyword` | layout, L3 one-byte kinds | new kinds (§2.2), `Arrow`, `Str`; `port`/`assume` become removed words |
| `parser/expr.rs` | the Pratt loop, `±` rule, height/nesting limits, shape checks | `infix`: `KwIn` → `KwWithin`; prefix `..=`, postfix `..`; `[s]`; named args, `->` (G8–G9); `gen_args` (G4) |
| `parser/mod.rs` | `file`, recovery (A8), `rhs`, `semi`, `docs_and_attrs`, `list`, `nested`, error plumbing | `item`: the new item kinds; `starts_item`/`ends_body` two-token (G1); `block` header; `setup` entries with an `Entry` recovery level (G16); contract statements; `spec_stmt` (G3, G14); `attribute` args (G15) |
| `parser/ast.rs` | `Node`, `Ident`, `Path`, `Expr`, `Type`, `Attribute`, `Body` | §2.6 |
| `resolve/body.rs`, `value.rs` | **all** (nets, instances, fields, units, spreads, fixes) | nothing: they run on the circuit body |
| `resolve/mod.rs` | the passes' shape, `Scope`, `ErrorStarts`, taint | pass 1 reads headers; bodies come from `circuit` items; `match_contracts` generalizes to `match_items` (circuit, setup, contract) |
| `resolve/` new | — | `setup.rs` (effective setups), `contract.rs` (M1d-5: MExpr, reach, specs) |
| `spicy_model/design.rs` | `Block` and everything in it | `Design` gains setups, envs, consts; `Contract` filled (§3.1) |
| `spicy_model/flat.rs`, `flatten/` | **all** | `KnobSource` gains `SetupField`, `Env`; `flatten_contract` is new, beside it |
| `prelude.rs` | part kinds, signal types | shapes (`Supply`, `Signal`, `Load`, later `Pair`, `Step`, `Sweep`, `Open`); types `Temperature`, `Duration`, `f64`; `FieldType::temperature` keeps serving `temp` |
| `test_data/` (118 files, 235 `port` lines) | cases, layout, coverage rules | every `block` with statements splits into header + `circuit`; `in` → `within`; `assume` → setup. **Done by a throwaway converter** (scratch, not committed), then reviewed |
| `circuits/ce_amp.spl`, fuzz corpora (2 copies) | — | rewritten (§4.2) |

**The regression check that makes step 3 safe:**
- `Design` and `FlatDesign` derive `PartialEq` without spans (E22), and their snapshot dumps print no spans.
- So after converting the test data, every **resolve and flatten dump must be byte-identical** to before. Only rendered diagnostics, which show line and column, and parser tree dumps may change.
- Any dump diff is a migration bug.

### 4.2 `ce_amp.spl` in draft 4

```rust
/// The environment every check shares (one file until projects exist: §3.10 D11).
env ambient: Temperature in -10°C..=60°C;

/// Common-emitter audio stage (walkthrough §1).
pub block CeAmp { vcc: Power<In>, gnd: Ground, input: Analog<In>, output: Analog<Out> }

circuit CeAmp {
    net base;
    net emitter;

    /// Divider holds the base near 2.1 V.
    let r1 = Resistor { a: vcc, b: base, value: 47k ± 1% };
    let r2 = Resistor { a: base, b: gnd, value: 10k ± 1% };
    let rc = Resistor { a: vcc, b: output, value: 4.7k ± 1% };
    let re = Resistor { a: emitter, b: gnd, value: 1k ± 1% };
    let c_in = Electrolytic { p: base, n: input, value: 1uF ± 20% };
    let q1 = Npn { c: output, b: base, e: emitter, beta: 100..=300 };
}

setup Operating for CeAmp { vcc: Supply { v: 12V ± 5% }, temp: ambient }

contract CeAmp {
    setup = Operating;

    let h = ac(output.v / input.v);

    /// Room for the output to swing ±1 V without clipping.
    spec bias: dc(output.v) within 4.5V..=6.5V;
    /// The next stage expects this level.
    spec gain: h.at(1kHz).mag() within 4.6 ± 5%;
    /// Don't cut the bass.
    spec bass: h.f_low(-3dB) <= 30Hz;
}
```

**Kept at three specs on purpose.** Draft 4's `base_bias` (an internal spec) adds two sides, and so extra Inside, Sigma and Band runs, which would move the engine plan's pinned 579. It goes in its own case file, `test_data/contract/ok/internal.spl`, which tests internal detection.

**What the tests pin** (model.md §5, updated):
- **Resolve:**
  - 1 block (4 ports), its circuit (2 nets, 6 parts);
  - 1 env (`ambient`, 263.15 … 333.15 K);
  - 1 setup (`vcc: Supply { v: Given(12 V ± 5%), z: ideal }`, `temp: Env(ambient)`, with `input` and `output` unwritten: an ideal source and open);
  - 1 contract (default `Operating`, 1 measure, 3 specs, all interface specs, none `pub`).
- **Flatten:** 6 devices, 6 nets, as today.
- **Contract:**
  - 8 knobs, in id order: 6 part knobs (as today), then `vcc.v` (Range, 11.4 … 12.6 V) and `ambient` (Range);
  - 1 `FlatSetup`, 3 `FlatSpec`s.
  - This is model.md §5's table with `temp` renamed `ambient`. The suite's case JSON changes that one path, as [R2] §9.3 already noted.

### 4.3 The order of work

Each step ends with every test green. Nothing built in an earlier step is rewritten in a later one.

| Step | Adds | Test data | Proof it's right |
|---|---|---|---|
| **1. Tokens** | `Arrow`, `Str`, the new keyword kinds (parsed as "reserved" until step 4), `→`, units | `lexer/ok/keywords.spl`, a new `lexer/ok/tokens_v4.spl` | lexer snapshots; the round trip |
| **2. `within`** | `KwWithin` at level 1, `KwIn` out of `infix`; the "`in` is now `within`" error with its fix | every spec/assume `in` → `within` (the fix itself, applied by a test) | parser dumps change only `In` → `Within` |
| **3. Header + `circuit`** | `BlockDecl`, `Circuit(Body)`; resolve pass 1 from headers, bodies from circuits, `CircuitState`; `port` gets a migration error | the converter splits all 118 files; `assume` stays for now (still parsed) | **resolve and flatten dumps byte-identical** (§4.1) |
| **4. Items and statements, parsed** | `setup` (full entry grammar, G11–G16), `env`, `const`, `pub`, `setup =`, `rated`, spec clauses and fn form, `ensure`, `m[s]`, prefix/postfix ranges, `gen_args`, attr args; `assume` removed | `ce_amp.spl` per §4.2; new `parser/{ok,err}` cases for every G-rule; draft 4's §2 (with D3–D7 fixed) as a parse-only case | tree snapshots; the fix-applies test covers the new fixes |
| **5. M1d-5: contracts, MVP subset** | resolve: envs, consts, setups without `..`, contracts, specs → `MExpr`, reach and `pub` checks; `flatten_contract`; setup and env knobs; everything else "not supported yet" | `test_data/contract/{ok,err}` | `ce_amp`: 8 knobs, 3 specs, 5 sides (§4.2); the shuffle test extended to setups |
| 6 | derivation (`..`, paths, implied shapes), `on`, `with`, modes | LDO | effective-setup dumps |
| 7 | fn specs, `m[s]` slots, pairs, `rated` | InAmp, two-point calibration | [R2] §10 step 2 |
| 8 | generics, traits, `observe`/`emits`, faults, model blocks | GainStage, the board | [R2] §10 steps 3–5 |

Steps 2 and 3 could be one commit. Kept apart, each diff is readable: one is an operator rename, the other a structural split with a byte-identical proof.

### 4.4 The minimal draft-4 subset for the MVP

**The parser accepts all of draft 4's §1 grammar** (step 4). Adding a feature later then never changes what a file means (grammar.md's rule). **Resolve accepts this subset** and reports "not supported yet" for the rest, at the construct:

| In the MVP | Not yet (parsed, rejected in resolve) |
|---|---|
| `[pub] block X { port: Type, … }`, no generics, no `: Trait` | generics, trait bounds, `trait`, `impl` |
| exactly one `circuit X` per placed block | a block with no circuit when placed; a second circuit |
| `setup S for X { key: value, … }`: keys `port`, `port.field`, `temp`; shapes `Supply { v, z }`, `Signal { v, z }`, `Load { r, c, i }`; values: points, `±`, `a..=b`, `..=b`, `a..`, env names | `..Base`, `mode`, `event`, `window`, pairs, `Step`/`Sweep`, child keys, `#[fault]`, `#[outside]` |
| `env name: Type in range;`, `const NAME: Type = value;` | several files |
| `contract X { setup = S; let m = …; [pub] spec n: m within/<=/>= b; }` | `with`, `for`, `on`, `in` clauses; fn specs; `m[s]`; `rated` |
| measures `dc`, `ac`, `.at`, `.mag`, `.db`, `.f_low`, `.f_high` (E24's table) | `tran`, `.settle`, `.deviation`, `.first`, `.during`, `.after`, port `z(f)` |
| internal-spec detection and the `pub` rules (§3.3) | `observe`, `emits` |

This is draft 4's §3.5 slice plus the internal/`pub` check. That check costs one field per name lookup, and without it `pub` would be accepted without meaning anything.

---

## 5. Engine impact since draft 2

`engine_types.md` was written before draft 2. [R2] §5 listed draft 2's changes. What draft 4 adds or changes on top:

| Draft 4 | Effect on `engine_types.md` / the adapter | New since [R2]? |
|---|---|---|
| **`setup = S;`** | exactly [R2]'s `accepts`: the default setup of every spec and the envelope. No type change | renamed |
| **Stale name** | `from_model(knobs: &KnobTable, …)` (§2.1, §1's diagram): model.md put knobs in `FlatDesign` on 2026-09-29. → `from_model(flat: Flat<'_>, contract: &FlatContract, policy: &Policy)` | a doc fix |
| **Internal vs `pub`** | `Side.public: bool`, `Record.public`. [R2]'s `PortFact` came from "a spec whose only measure is one port quantity". It now comes from **`pub spec`s only**, which is explicit, and resolve has already checked they're interface specs. Internal specs are part of the block's own check and never enter composition. `Needs.probes` may include internal nodes (`v(base)`), which ngspice reads like any node | yes |
| **Modes** | a `Plan.setups` entry per (setup, mode, pins), with `Setup.mode: Option<ModeId>` ([R2]'s placeholder, now used). Every spec gets a side per mode; `Record.mode`. Mode-owned knobs are ordinary Range knobs in their setups' subsets. Cost: × the mode count for every spec without `in M` (the LDO: × 2) | used now |
| **`with x = v`** | an anonymous `Setup` with `pins` ([R2] §2.3) | no |
| **`rated`** | **monitors**: `Needs.monitors: Vec<Monitor { probe, lo, hi }>`, evaluated on **every run of every setup**, faults included, and on setups no spec uses (`Reversed` exists only for this). From the op for DC/AC runs, and min/max over the window for transients. Always `worst_case` (an absolute maximum isn't a yield). A violation is a FAIL record with its row. **Compositional:** each placed child's `rated` lines become monitors in the parent's runs, mapped through the port bindings. `Reversed` then fails at `ldo.vin` (−5 V against `rated vin.v within -0.3V..=6.5V`), which is the intended demonstration | yes |
| **Faults** | `#[fault]` setups are `Tran` analyses. `Open { port, at, lasts }` is a switch (or a PWL resistor) in the setup's deck. `lasts: 0us..=10ms` is a Range knob of that setup only, where interior worst cases are likely, so the E1 interior guard applies. **The stop time depends on a knob** (`..=drop.end + 1s`): run at the longest stop over the knob range, and cut each run's window per point. `.during(ev)` and `.after(ev.end)` are windows computed per run. `usb.polarity: reversed` changes topology, so the setup gets its **own `DeckId`** | yes |
| **Pairs** | two sources from the two knobs `dm` and `cm`: `v(inp) = cm + dm/2`, `v(inn) = cm − dm/2`, as ngspice parameter expressions, with read-back on both. Two `z` knobs (§3.7). **Excitation per setup:** `Diff` and `Common` differ only in AC magnitudes (±0.5 V differential, 1 V both legs), so AC magnitudes must vary per setup: `alter` between runs, or two decks. Since AC is linear, a multi-setup spec like `cmrr` can merge its slots: one run, two AC analyses (inp alone, inn alone), dm and cm by superposition. It's [R2] §4.3's DC-sweep merge for AC | yes |
| **Excitation of `ac(…)`** | draft 4 has both implicit (`ac(output.v / input.v)`) and explicit (`.dm.ac: 1V`) excitation. **Rule:** an explicit `.ac` in the setup wins; otherwise the port in the ratio's denominator is excited; otherwise error "which source is excited?". `Excitation::Source(PortRef)` ([R2] §5.2) carries it | clarified |
| **`observe` / `emits`** | `observe code: Integer` → `MeasureKind::Quantized` ([R2] P5). `emits wake: Step { port: vdd.i, … }` → a PWL current source at the child's port in the parent's deck, placed by the parent setup's trigger at `at` | yes |
| **`Sweep(a -> b)` and `.first(pred)`** | a `DcSweep` over that source with the sweep value as the axis; `.first` is the first crossing along it, a `Crossing` program on the sweep, **undefined** when there's none (so it can't pass) | yes |
| **`for f in band`** | `Band { reduce: Max }` over a grid, [R2] P4; unchanged | no |
| **The CE amp** | unchanged: 8 knobs (`temp` → `ambient`), 1 setup, 1 experiment, 5 sides, 579 runs | — |

---

## 6. Questions for the lead

1. **The event and emits spellings** (G6, D4): constructors (`Open { port: usb, at: 1ms, lasts: … }`, `Step { port: vdd.i, … }`, `Trigger { event: mcu.wake, at: 1ms }`) and `window: ..=t`, instead of `usb.open { … }`, `Step on …`, `at` and `until`?
2. **`pub spec` on which setups** (§3.3): the default setup and `#[fault]` setups only?
3. **Stimulus fields are outside the envelope check** (§3.4): `wave`, `ac`, steps and events describe the test, not the operating range?
4. **Derived fields override the inherited modes' fields** (§3.6, D9)?
5. **A pair's `z` is two knobs, one per leg** (§3.7, D8), or one knob plus an explicit mismatch field?
6. **A bare quantity in a measure means `dc(…)`** (`mcu.code[full]`, D6), or must the analysis always be written?
7. **Keep `ce_amp.spl` at three specs** so the engine's pinned numbers hold, and test `base_bias` in its own case file (§4.2)?
