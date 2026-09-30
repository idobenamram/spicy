# Grammar v0.2: the MVP subset of v5

> 2026-09-30 · Design note for roadmap M1a. **v0.2:** the syntax of `research/contract_syntax_v5.md`, as decided in `syntax_v5_plan.md` (v0.1, 2026-09-26, had `port` statements inside `block`, `assume`, and `in` as the limit operator; none of it is accepted any more).
> The exact grammar `spicy_lang` parses for the MVP: tokens, keywords, the syntax in EBNF, operator precedence, and how unit literals are read. The meaning of each construct is in `language.md`. Why an AST and a hand-written lexer: roadmap §4.4.

Everything here must parse `circuits/ce_amp.spl` and `test_data/parser/ok/v5_mvp.spl`. v5 constructs outside the MVP (`mode`, `event`, `..Base`, spec clauses, function-form specs, `rated`, `observe`, `emits`, generic parameters) aren't parsed yet: they're syntax errors today, or reserved words, so adding them later only turns errors into valid code and never changes what an existing file means.

---

## 1. One line, all the way through

```rust
spec gain: h.at(1kHz).mag() within 4.6 ± 5%;
```

**Tokens** (whitespace tokens are kept in the token list but left out here):

```
KwSpec "spec"  Ident "gain"  Colon  Ident "h"  Dot  Ident "at"  LParen
Quantity "1kHz"  RParen  Dot  Ident "mag"  LParen  RParen  KwWithin "within"
Quantity "4.6"  PlusMinus "±"  Quantity "5%"  Semi
```

**AST:**

```
Spec { public: None,
       name: "gain",
       relation: Relation { lhs: Call(Field(Call(Field(h, at), [1kHz]), mag), []),
                            op:  Within,
                            rhs: Binary(Tol, 4.6, 5%) } }
```

Two rules decide that shape:
- **`within` binds loosest** (§4), so the relation splits into `h.at(1kHz).mag()` and `4.6 ± 5%`.
- **`1kHz`** is one token. It's split into the prefix `k` and the unit `Hz` after lexing (§5).

---

## 2. Tokens

### 2.1 Kept but skipped by the parser ("trivia")

| Token | Text | Notes |
|---|---|---|
| `Whitespace` | runs of space, tab, `\r`, `\n` | |
| `LineComment` | `//…` to end of line | `////…` is also a plain comment, as in Rust |
| `BlockComment` | `/* … */` | **Nests**, as in Rust. Unterminated is an error |

Trivia stays in the token list so the file can be rebuilt byte for byte (roadmap §4.4). The parser reads past it.

### 2.2 Doc comments

| Token | Text | Notes |
|---|---|---|
| `DocComment` | `///…` to end of line | Attaches to the next item, statement, port or setup entry. That's the rationale shown on value cards and spec rows (language §5.6) |

A doc comment followed by nothing it can attach to (e.g. just before `}`) is a warning, as in Rust.

### 2.3 Identifiers and keywords

- **Identifiers:** ASCII only for now, `[A-Za-z_][A-Za-z0-9_]*`.
  - *Why:* Unicode identifiers need Unicode tables, which means a crate or a large hand-written table. A non-ASCII identifier gets a clear error, so allowing them later won't change any file that already parses.
  - Unicode *is* accepted in unit suffixes (`µ`, `Ω`, `°`), as `±` (§5), and inside strings.
- **Keywords** (can't be identifiers): `block` `circuit` `setup` `contract` `env` `const` `pub` `net` `let` `spec` `within` `for` `in`.
  - `in` is a keyword but **not an operator**: it appears in `env ambient: Temperature in -10°C..=60°C;`, and later in `for f in r` and `in Mode`. Where it follows an expression, it ends it.
  - `for` is read only in `setup S for X`. Anywhere else it says "`for` isn't supported yet", until loops arrive.
- **Reserved** (can't be identifiers; using one says "not supported yet"):
  - `use` `mod` `fn` `enum` `type` `trait` `impl` `if` `else` `match` `where` `as` `true` `false` `self` `super` `crate`;
  - `signal` `interface` `family`;
  - `rated` `ensure`: v5 statements outside the MVP.

  *Why:* these are Rust keywords or constructs planned in `language.md` and v5. Reserving them now means no file written today breaks when they arrive. `bench` and `param` were reserved in v0.1 and aren't any more: v5 writes a test bench as a `setup`, and a parameter as a generic.
- **Contextual words** stay ordinary identifiers, recognized only in their positions once their constructs arrive: `mode`, `event` (setup entries), `observe`, `emits` (block entries), `with`, `on` (spec clauses), and `part`, `from`. `from:` is a field name today (`Step { from: 5mA, … }`), and so is `part:` (`Resistor { part: … }`).
- **Case matters.** `in` is a keyword; `In` (the role in `Power<In>`) is an identifier.

### 2.4 Punctuation

| Token | Text | Token | Text |
|---|---|---|---|
| `LBrace` `RBrace` | `{` `}` | `Plus` `Minus` | `+` `-` |
| `LParen` `RParen` | `(` `)` | `Star` `Slash` | `*` `/` |
| `LBracket` `RBracket` | `[` `]` | `PlusMinus` | `±` or `+/-` |
| `Lt` `Gt` | `<` `>` | `DotDotEq` | `..=` |
| `Le` `Ge` | `<=` `>=` | `DotDot` | `..`: an open range's end (`10kΩ..`, §4) |
| `Comma` `Semi` `Colon` | `,` `;` `:` | `Dot` | `.` |
| `ColonColon` | `::` | `Pound` | `#` |
| `Eq` | `=` | `Question` | `?` (reserved for solver values, language §5.3) |

**What's deliberately not a token:**

| Text | Why |
|---|---|
| `>>` | Rust and Spade must split it into `> >` to close nested generics (`Foo<Tol<Ohm>>`). We have no shift operator, so `>` is always a single token and the problem never appears |
| `==`, `!=` | Not in the MVP. `==` arrives with `where temp == 25°C` |
| `%` on its own | `%` exists only glued to a number (`5%`). There is no modulo. A lone `%` is an error |
| `->` | v5's drafts wrote a step's levels as `5mA -> 30mA`. They're named fields instead (decided 2026-09-30): `Step { from: 5mA, to: 30mA, … }`, so a struct literal has no positional parts and needs no arrow. `a->b` is `a - > b`, an error |

**`+/-`** is read as one token only when the three characters touch. It's invalid Rust, so accepting it can't change the meaning of Rust-looking code (P1).

### 2.5 Quantities (numbers with an optional unit)

One token, `Quantity`, read greedily:

```
quantity  = mantissa [ suffix ] ;
mantissa  = digits [ "." digits ] [ exponent ]
          | digits prefix_letter digits ;          (* 4k7 = 4.7k *)
digits    = digit { digit | "_" } ;
exponent  = ( "e" | "E" ) [ "+" | "-" ] digit { digit | "_" } ;   (* `_` allowed, as in Rust: `1e1_0` *)
suffix    = unit_char { unit_char } ;              (* letters, µ, μ, Ω, Ω, °, % *)
```

- **A `.` belongs to the number only if a digit follows it.** So `100..=300` reads as `100`, `..=`, `300`; `1.` and `.5` are errors ("write `1.0` / `0.5`"). *Why:* the same rule keeps Rust's `0..N` working.
- **`e`/`E` is an exponent only when a digit (or a sign and a digit) follows it.** No MVP unit starts with `e`, and exa (`E`) isn't in the prefix table.
- **The unit must touch the number.** `10 V` is two tokens and an error with a fix (§7).
- **The lexer doesn't interpret the suffix.** It records the suffix's span; §5 splits it into prefix and unit.

### 2.6 Strings

`Str`: `"…"` on one line. The only escapes are `\"` and `\\`; any other `\` is an error ("unknown escape"), and a string with no closing `"` before the end of its line is one too ("unterminated string", at the opening `"`). Nothing inside a string is a comment, a number or a look-alike.

Strings are values (`Note { text: "…" }`). Their first real use is attribute arguments (`#[outside(reason = "…")]`), which arrive with v5's `#[outside]` setups.

---

## 3. Syntax (EBNF)

`{ x }` is zero or more, `[ x ]` is optional, `","` is a token. Trivia may appear between any two tokens.

```ebnf
file          = { item } EOF ;
item          = { DOC } { attribute } [ "pub" ] item_kind ;   (* `pub` only on a block *)
item_kind     = block | circuit | setup | contract | env | const ;

(* the interface: what a placement binds *)
block         = "block" IDENT "{" [ port { "," port } [ "," ] ] "}" ;
port          = { DOC } { attribute } IDENT ":" type ;

(* the implementation: the nets and parts inside *)
circuit       = "circuit" IDENT "{" { circuit_stmt } "}" ;
circuit_stmt  = { DOC } { attribute } ( net | let ) ;
net           = "net" IDENT [ "=" expr ] ";" ;               (* net x = [a, b]; merges nets *)
let           = "let" IDENT "=" expr ";" ;

(* the world around a block *)
setup         = "setup" IDENT "for" path "{" [ entry { "," entry } [ "," ] ] "}" ;
entry         = { DOC } { attribute } key ":" expr ;
key           = IDENT { "." IDENT } ;                         (* vcc, vin.v, temp *)

(* what the block promises *)
contract      = "contract" IDENT "{" { contract_stmt } "}" ;
contract_stmt = { DOC } { attribute } ( default_setup | let | spec ) ;
default_setup = "setup" "=" path ";" ;
spec          = [ "pub" ] "spec" IDENT ":" expr ";" ;         (* the top must be a relation *)

(* conditions and values of the whole project *)
env           = "env" IDENT ":" type "in" expr ";" ;
const         = "const" IDENT ":" type "=" expr ";" ;

type          = path [ "<" type { "," type } [ "," ] ">" ] ;
attribute     = "#" "[" path [ "(" [ list ] ")" ] "]" ;       (* #[confidence(sigma(3))] *)

expr          = relation ;                                    (* precedence: §4 *)
relation      = range [ rel_op range ] ;                      (* no chaining *)
rel_op        = "within" | "<=" | ">=" | "<" | ">" ;
range         = tol [ "..=" tol | ".." ] | "..=" tol ;        (* `a..` only when no operand follows *)
tol           = sum [ "±" sum ] ;                             (* operand rule: §4.2 *)
sum           = product { ( "+" | "-" ) product } ;
product       = unary { ( "*" | "/" ) unary } ;
unary         = "-" unary | postfix ;
postfix       = primary { "." IDENT | "(" [ args ] ")" | "[" expr "]" } ;
primary       = QUANTITY | STRING
              | path [ generics ] [ "{" [ fields ] "}" ]      (* Resistor { a: vcc, … } *)
              | "(" expr ")"
              | "[" [ list ] "]" ;
generics      = "<" IDENT "=" range { "," IDENT "=" range } [ "," ] ">" ;
                                                              (* GainStage<A = Mcp6001> { … } *)
fields        = field { "," field } [ "," ] ;
field         = IDENT [ ":" expr ] ;                          (* `gnd` is shorthand for `gnd: gnd` *)
args          = arg { "," arg } [ "," ] ;
arg           = [ IDENT ":" ] expr ;                          (* h.f_high(-3dB, ref: dc) *)
list          = expr { "," expr } [ "," ] ;
path          = IDENT { "::" IDENT } ;
```

**Choices in this grammar, and why:**

- **A block is its interface; its `circuit` is its inside** (v5 §1). The header `pub block CeAmp { vcc: Power<In>, … }` is what a parent sees and binds; `circuit CeAmp { … }` holds the nets and parts. A circuit, contract or setup names its block, and resolve matches them by name. A block with no circuit is only an interface: declaring it is fine, placing it is an error.
- **One `let` for everything.** `let r1 = Resistor {…}` (a part), `let amp = CeAmp {…}` (a block placement) and `let h = ac(…)` (a measure) all parse the same way. Name resolution decides later what the path refers to. *Why:* the parser needs no symbol table, and the placement syntax is just an expression, as in Spade.
- **`spec` takes an ordinary expression.** The parser then checks its shape: the top node must be a relation, otherwise the error is "expected a relation like `x within a..=b` or `x <= b`". *Why:* the Pratt loop handles all expressions, and the AST still gets a structured `Spec { public, name, relation: Relation { lhs, op, rhs } }`.
- **`setup` and `pub` start both items and statements** (`setup S for X` vs a contract's `setup = S;`, `pub block` vs `pub spec`), so the token after them decides. They start an item only when that token is an item's own: a name after `setup`, an item keyword after `pub`. Anything else is a statement, and a mistake in it is that statement's error, so a typo doesn't end the body (decided 2026-09-30, after the step 4 review; rustc reads `union` as an item only before a name the same way). `pub` goes only on a block or a spec; anywhere else it's reported, with its removal as the fix.
- **Every field of a struct literal is named.** A step's or a sweep's levels are fields like any other: `Step { from: 5mA, to: 30mA, edge: 1us, at: 50us }`, `Sweep { from: 4.5V, to: 3.0V, step: 10mV }` (decided 2026-09-30). A bare name is the shorthand `gnd` for `gnd: gnd`.
- **A setup entry's key is a dotted name, not an expression:** it names a port, a field of what's on the port, or `temp`; nothing is computed.
- **Parentheses are kept in the AST** as a `Paren` node. They're needed for the `±` rule (§4.2), and the formatter needs them.
- **Struct literals are allowed in every expression.** No MVP construct has `expr {` followed by a body. Once `for` loops and `if` arrive, their headers will forbid struct literals, as in Rust and Spade (roadmap §4.5).
- **Generic arguments at a placement are always named** (`GainStage<A = Mcp6001>`). `<` followed by any token and then `=` starts them, since `=` is never an operator; anywhere else `<` is less-than, so no turbofish is needed. Each value is read above the comparisons, so a `>` ends it; a comparison inside one needs parentheses (`G<N = (a > b)>`). Resolve doesn't take generic arguments yet ("not supported yet").
- **`m[s]` is an index**, a postfix like `.f` and `(…)`: measure `m` in setup `s`, for a spec that uses several setups. A statement ends in `;`, so `[` after a value can only be an index; `[a, b]` at the start of a value is a list.
- **Trailing commas** are allowed in every list, as in Rust. The formatter adds them in multi-line lists.

---

## 4. Operator precedence

### 4.1 The table

Loosest first:

| Level | Operators | Associativity | Example |
|---|---|---|---|
| 1 | `within` `<` `<=` `>` `>=` | none: `a < b < c` is an error | `dc(output.v) within 4.5V..=6.5V` |
| 2 | `..=` between two ends, `..=b` with no lower end, `a..` with no upper end | none | `-10°C..=60°C`, `..=0.5Ω`, `10kΩ..`, `v - 0.5V..=v + 0.5V` |
| 3 | `±` `+/-` | none; plus the operand rule (§4.2) | `12V ± 5%` |
| 4 | `+` `-` | left | `vcc.v - dc(output.v)` |
| 5 | `*` `/` | left | `output.v / input.v` |
| 6 | unary `-` | prefix | `-3dB`, `-10°C` |
| 7 | `.field`, `.method(…)`, calls, `[s]` | left | `h.at(1kHz).mag()`, `ac(out.v)[dm]` |

**What each level means for our file:**
- **Level 1 is loosest**, so every `spec` splits into a measure and a bound, whatever arithmetic is inside them.
- **Level 2 follows Rust.** Ranges bind looser than arithmetic, so an endpoint can be a formula without parentheses: `v - 0.5V..=v + 0.5V` means `(v - 0.5V)..=(v + 0.5V)`, and loop ranges like `0..=n - 1` will keep their Rust meaning. There's only one sensible reading, so no parentheses are required.
  - **An open range** leaves out one end. `..=b` has no lower end and can start any value that may be a range (`z: ..=0.5Ω`, `x within ..=2V`). `a..` has no upper end when nothing that could be one follows: before `,` `}` `)` `]` `;` or a looser operator (rustc's rule, `is_at_start_of_range_notation_rhs`). A tighter operator after `a..` (`1.. ± 5%`, `10k.. * 2`) reads as a missing upper end, an error.
  - **`a..b` and `..b`** are errors, with the fix `..=`: a range's end is written after `..=`. (`a..` with nothing after it is the open range.)
- **Level 3 sits above ranges and below arithmetic,** so `4.6 ± 5%` is a single operand of `within`.

### 4.2 The `±` operand rule

**An operand of `±` that is an unparenthesized `+ - * /` expression is an error.** The parser sees it through the `Paren` node.

This is where the precedence question really bites. atopile's own example (`examples/led_badge/led_badge.ato:42`, with `capacity = 300mAh` on line 67):

```
charger.charge_current_limit = battery.model.capacity / 2hour +/- 10%
```

In atopile, `±` is part of the number literal, so this means `300mAh / (2h ± 10%)`:

| Reading | Result |
|---|---|
| atopile's: `300mAh / (2h ± 10%)` = 300 mAh / [1.8 h, 2.2 h] | **136.4 … 166.7 mA**, lopsided (−9.1% / +11.1%) |
| What the author almost certainly meant: `(300mAh / 2h) ± 10%` | **135 … 165 mA** |

At ±50% the readings would be 100…300 mA against 75…225 mA. Either fixed precedence silently gives one reader the wrong answer, so we refuse to guess:

```
error: ambiguous tolerance
  |
4 |     let i_chg = capacity / 2h ± 10%;
  |                 ^^^^^^^^^^^^^ does `±` apply to this whole expression?
  = help: `(capacity / 2h) ± 10%` for a spread on the result
  = help: `capacity / (2h ± 10%)` for a spread on `2h`
```

**Unaffected:**
- `12V ± 5%`, `47k ± 1%`, `4.6 ± 5%`: single-value operands;
- `r_nom ± 1%`: a named constant. atopile can't express this, since its `±` accepts only literal numbers;
- `vcc.v ± 50mV` and `-3V ± 1%`: field access and unary minus bind tighter, so these aren't ambiguous.

*Why this rule and not just a precedence level:* Rust does the same with `a < b < c`, rejecting it at parse time instead of picking a meaning. It also fits P8: one way to write each thing, and a fix-it instead of a silent reading.

### 4.3 Mixing spreads and relations

These parse, but the parser rejects their shape:
- **A tolerance on a range endpoint:** `1V ± 1% ..= 2V`, `..=2V ± 1%` and `1V ± 1%..` give "a range endpoint can't carry a tolerance".
- **A range (closed or open) or a tolerance on either side of `<`, `<=`, `>`, `>=`, or on the left of `within`:** `x <= 1V..=2V`, `x <= ..=2V` and `1..=2 within x` give "compare with a single value", with the help "to require a value inside a range, use `within`".

**The Rust check (P1):** Rust puts `..=` *below* comparisons, so `a < b..=c` means `(a < b)..=c` there. Ours would put the range inside the comparison, but that shape is always an error (the second rule above). So we accept less than Rust here; we never give Rust-looking code a different meaning.

**Why `within` and not `in`** (v5): `in` is needed for binding (`env x: T in r`, later `for f in r`) and for the mode filter (`… in Sleep`). As an operator it would make `dc(i) <= 50uA in Sleep` read as `(… <= 50uA) in Sleep`, a chained comparison. With `within` as the limit, `in` simply ends an expression.

---

## 5. Reading a unit suffix

After lexing, the suffix of each `Quantity` is split into an optional prefix and an optional unit. The dimension is checked later, in elaboration (`47k` as a `Resistor`'s `value:` becomes ohms).

### 5.1 Tables

**Units (MVP):** `V`, `A`, `Ω` (or `ohm`), `F`, `H`, `Hz`, `s`, `h` (hour), `y` (a Julian year, 365.25 days), `W`, `K`, `°C`, `%`, `dB`. `°C`, `%`, `dB`, `h` and `y` take no prefix.

**Prefixes** (case-sensitive): `f` 1e-15 · `p` 1e-12 · `n` 1e-9 · `u` / `µ` 1e-6 · `m` 1e-3 · `k` 1e3 · `M` 1e6 · `G` 1e9 · `T` 1e12.

Look-alike characters are treated as the same: `µ` (U+00B5) and `μ` (U+03BC); `Ω` (U+03A9) and `Ω` (U+2126).

### 5.2 Splitting rule

Try each in order and take the first that matches:
1. **the whole suffix is a unit:** `F`, `Hz`, `dB`, `°C`, `%`;
2. **the whole suffix is a prefix:** `47k`, `100n`, `1m` (a bare value);
3. **a prefix followed by a unit:** `kHz`, `mV`, `uF`, `kΩ`.

Otherwise the suffix is unknown, and the error lists the closest valid spellings.

Worked through:

| Literal | Rule | Value |
|---|---|---|
| `47k` | 2 | 47 000, unit from context |
| `4k7` | infix form | 4 700 |
| `1kHz` | 3 | 1 000 Hz |
| `100nF` | 3 | 1e-7 F |
| `1F` / `1f` | 1 / 2 | 1 farad / 1e-15 (femto): case matters |
| `1m` / `1M` | 2 | 1e-3 / 1e6 |
| `1K` | 1 | 1 kelvin. Used as a resistance, elaboration says "expected Ohm, found Kelvin; kilo is lowercase `k`" |
| `1Meg` | none | error: "SPICE's `Meg` is `M` here" |
| `k°C`, `m%`, `kdB`, `3ky` | none | error: these units take no prefix |
| `10mh`, `4u7h` | none | error: `h` takes no prefix, and no fix is offered, since `mH`/`uH` (henry) is as likely as removing the prefix (hours) |
| `5%` | 1 | 5, unit `%` (the tolerance reads it as a fraction later) |

*Why split after lexing, not in the lexer:* the lexer stays a simple scanner, and each suffix mistake gets its own diagnostic pointing at just the suffix.

---

## 6. Error recovery

- **At statement level:** after an error, skip tokens (scanning again from the statement's start, so brackets opened before the error are known) until one of:
  - a `;` with no bracket open, which is consumed;
  - the `}` that closes the current body;
  - a statement or item start at or after the error, even inside brackets opened *before* the error (an unclosed `Resistor {` doesn't swallow the rest of the circuit). Brackets opened *after* the error are skipped whole (`for i in 0..N { … }`);
  - `///` or `#` strictly after the error: the next statement's doc comments and attributes, which it keeps.
- **At value level** (decided 2026-09-29): when a right-hand side doesn't parse (the value of a `let`, a `net x =`, an `env` or a `const`), or something other than `;` follows it, only the value becomes an error node, over the text up to the `;` (the same stopping rules, but a value may be empty). The statement keeps its name, so `net base = [g g];` still declares `base` and its uses aren't reported again (rustc: "we still want a field even if its expr didn't parse").
- **At entry level** (a block's port, a setup's entry; step 3–4): a broken entry is skipped to the next `,` with no bracket open, the list's `}`, or the next item, and becomes an error entry; the entries after it are kept. A `;` inside the list is skipped. A port whose type doesn't parse keeps its name, and an entry whose value doesn't parse keeps its key, as a statement keeps its name.
- **At item level:** skip to the next item start (by the two-token rule of §3 for `setup` and `pub`) or the end of file. The next item keeps its doc comments and attributes.
- **Missing `;`** before the next statement or the end of the body, and **missing `,`** before the next entry: reported with an insertion fix, and parsing continues as if it were there.
- **Missing `{`** after `block Name`, `circuit Name`, `contract Name` or `setup S for X`, when the item's contents or its `}` follow: reported with an insert-`{` fix, and the item is read as if it were there, so it keeps its name (decided 2026-09-29).
- **The result:** each broken statement or entry gives one error, the rest of the file still parses, and later stages run on what parsed (roadmap §4.5).

---

## 7. Syntax errors in the MVP

Each gets a snapshot test of its rendered diagnostic. The lexer reports #2–3, #9–15, #22–24 and #29–30 (`lexer.md` §6); the parser reports the rest.

| # | Input | Diagnostic |
|---|---|---|
| 1 | `let r1 = Resistor {…}` then a new line with no `;` | expected `;`, with an insert-`;` fix |
| 2 | `12V +- 5%` | `+-` is `+ (-…)` in Rust; did you mean `±` or `+/-`? |
| 3 | `--x` | `--` is `-(-x)` in Rust; write `x` or `-(-x)` |
| 4 | `capacity / 2h ± 10%` | ambiguous tolerance (§4.2), both readings offered |
| 5 | `1 ± 2% ± 1%`, `1..=2..=3` | tolerances / ranges can't be chained |
| 6 | `a < b < c` | comparisons can't be chained; for a range, write `b within a..=c` |
| 7 | `1V ± 1% ..= 2V`, `x <= 1V..=2V`, `x <= ..=2V` | §4.3 |
| 8 | `100..300`, `..300` | a range's end is written after `..=`, with the fix `..=`; `a..` with nothing after it is a range with no upper end |
| 9 | `47q` | unknown unit suffix `q`, with the closest spellings |
| 10 | `1Meg` | use `M` |
| 11 | `k°C`, `10mh` | this unit takes no prefix (no fix where `h` may be `H`) |
| 12 | `4.7k7` | a number uses either a decimal point or an infix prefix (`4k7`), not both |
| 13 | `10 V` | the unit must touch the number: `10V` |
| 14 | `.5`, `1.` | write `0.5` / `1.0` |
| 15 | `a % 3`, a lone `%` | `%` only means percent, glued to a number (`1 %` after a number is #13 instead) |
| 16 | `Resistor { a = vcc }` | fields use `:` (a replacement fix, as in Spade) |
| 17 | `spec dc(out.v) within …;` | a spec needs a name: `spec name: …;` |
| 18 | `let net = …`, a port or key named `setup` | `net` is a keyword |
| 19 | `fn f() {}` at top level, `rated …;` in a contract | `fn` isn't supported yet (reserved) |
| 20 | `net` in a `contract`; `spec` or `setup = S;` in a `circuit` | `net` doesn't belong in a `contract`: it belongs in the circuit |
| 21 | Unclosed `(`, `[`, `{` or `<…>` | expected the closer, with a secondary label on the opener |
| 22 | Unterminated `/*` | expected `*/`, with a label on the opener |
| 23 | Look-alikes pasted from datasheets: `−` (U+2212 minus), `–` (en dash), `≤` `≥`, `;` (Greek question mark), `×` | "this is `−` (U+2212), not `-`", with a replacement. *Why:* datasheet PDFs are full of these, and the AI will paste from them |
| 24 | `r1_α` | identifiers are ASCII for now |
| 25 | `///` just before `}` | warning: the doc comment isn't attached to anything |
| 26 | `pub circuit A {…}`, `pub let x = 1;` | only a block or a spec can be `pub`, with its removal as the fix |
| 27 | `block A { a: Pin b: Pin }`, two setup entries with no `,` between | expected `,`, with an insert-`,` fix; both entries are kept |
| 28 | `spec s: dc(out.v) in 1V..=2V;` | expected a relation (the measure ends before `in`), then expected `;`. `in` as a limit is v0.1 syntax; there's no migration message (plan decision 1) |
| 29 | `"never closed` | unterminated string, at the opening `"` |
| 30 | `"a\nb"` | unknown escape `\n`: the only escapes are `\"` and `\\` |

Resolve's errors (a circuit, contract or setup for an unknown block; placing a block with no circuit; two circuits for one block; two setups of one name for one block) are in `model.md`.

---

## 8. Decisions and open questions

| # | Question | Answer |
|---|---|---|
| 1 | ASCII spellings `ohm` / `u` next to `Ω` / `µ` | **Both are accepted (decided 2026-09-26).** The formatter keeps whichever you wrote. *Why:* `Ω` and `µ` are hard to type; rewriting them would turn every format into a diff |
| 2 | Digit separators | **`1_000` and `1000` are both accepted (decided 2026-09-26),** as in Rust |
| 3 | Unicode identifiers | **Recommended: no, ASCII only**, unless a real need shows up. *Why:* names become SPICE node and element names in the netlist export, KiCad references and BOM rows, and those tools expect ASCII. Look-alike letters (Cyrillic `а` vs Latin `a`) would make two different nets look identical. And it needs no Unicode tables. Unicode stays where it earns its place: `µ`, `Ω`, `°`, `±` |
| 4 | The old syntax | **Not accepted, and no migration messages (decided 2026-09-30).** Nothing outside this repo used v0.1 |
| 5 | Transitions (`5mA -> 30mA`) | **Named fields instead (decided 2026-09-30):** `Step { from, to, edge, at }`, `Sweep { from, to, step }`. No `->` token |
| 6 | When `setup` and `pub` start an item | **Only before an item's own next token (decided 2026-09-30):** §3 |
| 7 | The `,` before a setup's `..Base`, when `..Base` lands | **Required (decided 2026-09-30),** as in Rust's struct update. Without it, `vcc: 1V` then `..Operating` on the next line would read as the range `1V..Operating` |
| 8 | Positional generic arguments (`G<Mcp6001> {}`) | Today they read as a chained comparison. A dedicated "name the parameter" error comes when generics are resolved, since resolve knows the parameter's name then |
