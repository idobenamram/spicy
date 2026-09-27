# Grammar v0.1: the MVP subset

> 2026-09-26 · Design note for roadmap M1a. **Status:** draft for review.
> The exact grammar `spicy_lang` parses for the MVP (roadmap §4.1): tokens, keywords, the syntax in EBNF, operator precedence, and how unit literals are read. The meaning of each construct is in `language.md`. Why an AST and a hand-written lexer: roadmap §4.4.

Everything here must parse `circuits/ce_amp.spl` (roadmap §4.2). Anything outside the MVP is either rejected with a "not supported yet" diagnostic or reserved, so adding it later can't change what existing files mean.

---

## 1. One line, all the way through

```rust
spec gain: h.at(1kHz).mag() in 4.6 ± 5%;
```

**Tokens** (whitespace tokens are kept in the token list but left out here):

```
KwSpec "spec"  Ident "gain"  Colon  Ident "h"  Dot  Ident "at"  LParen
Quantity "1kHz"  RParen  Dot  Ident "mag"  LParen  RParen  KwIn "in"
Quantity "4.6"  PlusMinus "±"  Quantity "5%"  Semi
```

**AST:**

```
Spec { name: "gain",
       relation: Relation { lhs: Call(Field(Call(Field(h, at), [1kHz]), mag), []),
                            op:  In,
                            rhs: Binary(Tol, 4.6, 5%) } }
```

Two rules decide that shape:
- **`in` binds loosest** (§4), so the relation splits into `h.at(1kHz).mag()` and `4.6 ± 5%`.
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
| `DocComment` | `///…` to end of line | Attaches to the next item or statement. That's the rationale shown on value cards and spec rows (language §5.6) |

A doc comment followed by no item or statement (e.g. just before `}`) is a warning, as in Rust.

### 2.3 Identifiers and keywords

- **Identifiers:** ASCII only for now, `[A-Za-z_][A-Za-z0-9_]*`.
  - *Why:* Unicode identifiers need Unicode tables, which means a crate or a large hand-written table. A non-ASCII identifier gets a clear error, so allowing them later won't change any file that already parses.
  - Unicode *is* accepted in unit suffixes (`µ`, `Ω`, `°`) and as `±` (§5).
- **MVP keywords** (can't be identifiers): `block` `contract` `port` `net` `let` `assume` `spec` `in`.
- **Reserved** (can't be identifiers; using one says "not supported yet"):
  - `use` `mod` `pub` `fn` `const` `enum` `type` `for` `if` `else` `match` `where` `as` `true` `false` `self` `super` `crate`;
  - `signal` `interface` `family` `env` `param` `bench`.

  *Why:* these are Rust keywords or items planned in `language.md`. Reserving them now means no file written today breaks when they arrive.
- **Contextual words** stay ordinary identifiers: `part`, `from`, `on`. `part:` is a field name (`Resistor { part: … }`), so it can't be a keyword. When `from` (§5.6) and `on bench` (§8.3) arrive, they'll be recognized only in their positions.
- **Case matters.** `in` is a keyword; `In` (the role in `Power<In>`) is an identifier.

### 2.4 Punctuation

| Token | Text | Token | Text |
|---|---|---|---|
| `LBrace` `RBrace` | `{` `}` | `Plus` `Minus` | `+` `-` |
| `LParen` `RParen` | `(` `)` | `Star` `Slash` | `*` `/` |
| `LBracket` `RBracket` | `[` `]` | `PlusMinus` | `±` or `+/-` |
| `Lt` `Gt` | `<` `>` | `DotDotEq` | `..=` |
| `Le` `Ge` | `<=` `>=` | `DotDot` | `..` (only for a diagnostic in the MVP, §7) |
| `Comma` `Semi` `Colon` | `,` `;` `:` | `Dot` | `.` |
| `ColonColon` | `::` | `Pound` | `#` |
| `Eq` | `=` | `Question` | `?` (reserved for solver values, language §5.3) |

**What's deliberately not a token:**

| Text | Why |
|---|---|
| `>>` | Rust and Spade must split it into `> >` to close nested generics (`Foo<Tol<Ohm>>`). We have no shift operator, so `>` is always a single token and the problem never appears |
| `==`, `!=` | Not in the MVP. `==` arrives with `where temp == 25°C` |
| `%` on its own | `%` exists only glued to a number (`5%`). There is no modulo. A lone `%` is an error |

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

---

## 3. Syntax (EBNF)

`{ x }` is zero or more, `[ x ]` is optional, `","` is a token. Trivia may appear between any two tokens.

```ebnf
file          = { item } EOF ;
item          = { DOC } { attribute } ( block | contract ) ;

block         = "block" IDENT "{" { block_stmt } "}" ;
block_stmt    = { DOC } { attribute } ( port | net | let ) ;
port          = "port" IDENT ":" type ";" ;
net           = "net" IDENT [ "=" expr ] ";" ;            (* net x = [a, b]; merges nets *)
let           = "let" IDENT "=" expr ";" ;

contract      = "contract" IDENT "{" { contract_stmt } "}" ;
contract_stmt = { DOC } { attribute } ( assume | let | spec ) ;
assume        = "assume" expr ";" ;
spec          = "spec" IDENT ":" expr ";" ;

type          = path [ "<" type { "," type } [ "," ] ">" ] ;
attribute     = "#" "[" path [ "(" [ list ] ")" ] "]" ;   (* #[confidence(sigma(3))] *)

expr          = relation ;                                (* precedence: §4 *)
relation      = range [ rel_op range ] ;                  (* no chaining *)
rel_op        = "in" | "<=" | ">=" | "<" | ">" ;
range         = tol [ "..=" tol ] ;
tol           = sum [ "±" sum ] ;                         (* operand rule: §4.2 *)
sum           = product { ( "+" | "-" ) product } ;
product       = unary { ( "*" | "/" ) unary } ;
unary         = "-" unary | postfix ;
postfix       = primary { "." IDENT | "(" [ list ] ")" } ;
primary       = QUANTITY
              | path [ "{" [ fields ] "}" ]               (* Resistor { a: vcc, … } *)
              | "(" expr ")"
              | "[" [ list ] "]" ;
fields        = field { "," field } [ "," ] ;
field         = IDENT [ ":" expr ] ;                      (* `gnd` is shorthand for `gnd: gnd` *)
list          = expr { "," expr } [ "," ] ;
path          = IDENT { "::" IDENT } ;
```

**Choices in this grammar, and why:**

- **One `let` for everything.** `let r1 = Resistor {…}` (a part), `let amp = CeAmp {…}` (a block placement) and `let h = ac(…)` (a measure) all parse the same way. Name resolution decides later what the path refers to. *Why:* the parser needs no symbol table, and the placement syntax is just an expression, as in Spade.
- **`assume` and `spec` take an ordinary expression.** The parser then checks its shape: the top node must be a relation, otherwise the error is "expected a relation like `x in a..=b` or `x <= b`". *Why:* the Pratt loop handles all expressions, and the AST still gets a structured `Spec { name, relation: Relation { lhs, op, rhs } }`.
- **Parentheses are kept in the AST** as a `Paren` node. They're needed for the `±` rule (§4.2), and the formatter needs them.
- **Struct literals are allowed in every expression.** No MVP statement has `expr {` followed by a body. Once `for` and `if` arrive, their headers will forbid struct literals, as in Rust and Spade (roadmap §4.5).
- **Generics appear only in types** (after `port x:`), so `<` in an expression is always less-than. No turbofish is needed.
- **Trailing commas** are allowed in every list, as in Rust. The formatter adds them in multi-line lists.

---

## 4. Operator precedence

### 4.1 The table

Loosest first:

| Level | Operators | Associativity | Example |
|---|---|---|---|
| 1 | `in` `<` `<=` `>` `>=` | none: `a < b < c` is an error | `dc(output.v) in 4.5V..=6.5V` |
| 2 | `..=` | none | `-10°C..=60°C`, `v - 0.5V..=v + 0.5V` |
| 3 | `±` `+/-` | none; plus the operand rule (§4.2) | `12V ± 5%` |
| 4 | `+` `-` | left | `vcc.v - dc(output.v)` |
| 5 | `*` `/` | left | `output.v / input.v` |
| 6 | unary `-` | prefix | `-3dB`, `-10°C` |
| 7 | `.field`, `.method(…)`, calls | left | `h.at(1kHz).mag()` |

**What each level means for our file:**
- **Level 1 is loosest**, so every `assume` and `spec` splits into a measure and a bound, whatever arithmetic is inside them.
- **Level 2 follows Rust.** Ranges bind looser than arithmetic, so an endpoint can be a formula without parentheses: `v - 0.5V..=v + 0.5V` means `(v - 0.5V)..=(v + 0.5V)`, and loop ranges like `0..=n - 1` will keep their Rust meaning. There's only one sensible reading, so no parentheses are required.
- **Level 3 sits above ranges and below arithmetic,** so `4.6 ± 5%` is a single operand of `in`.

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
- **A tolerance inside a range endpoint:** `1V ± 1% ..= 2V` gives "a range endpoint can't carry a tolerance".
- **A range or tolerance on either side of `<`, `<=`, `>`, `>=`, or on the left of `in`:** `x <= 1V..=2V` and `1..=2 in x` give "compare with a single value, or use `in` for a range".

**The Rust check (P1):** Rust puts `..=` *below* comparisons, so `a < b..=c` means `(a < b)..=c` there. Ours would put the range inside the comparison, but that shape is always an error (the second rule above). So we accept less than Rust here; we never give Rust-looking code a different meaning.

---

## 5. Reading a unit suffix

After lexing, the suffix of each `Quantity` is split into an optional prefix and an optional unit. The dimension is checked later, in elaboration (`47k` as a `Resistor`'s `value:` becomes ohms).

### 5.1 Tables

**Units (MVP):** `V`, `A`, `Ω` (or `ohm`), `F`, `H`, `Hz`, `s`, `W`, `K`, `°C`, `%`, `dB`.

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
| `k°C`, `m%`, `kdB` | none | error: these units take no prefix |
| `5%` | 1 | 5, unit `%` (the tolerance reads it as a fraction later) |

*Why split after lexing, not in the lexer:* the lexer stays a simple scanner, and each suffix mistake gets its own diagnostic pointing at just the suffix.

---

## 6. Error recovery

- **At statement level:** after an error, skip tokens (scanning again from the statement's start, so brackets opened before the error are known) until one of:
  - a `;` with no bracket open, which is consumed;
  - the `}` that closes the current body;
  - a statement or item keyword at or after the error, even inside brackets opened *before* the error (an unclosed `Resistor {` doesn't swallow the rest of the block). Brackets opened *after* the error are skipped whole (`for i in 0..N { … }`).
  - `///` or `#` at the top level, strictly after the error.
- **At item level:** skip to `block`, `contract` or end of file.
- **Missing `;`:** reported with an insert-`;` suggestion, and parsing continues as if it were there.
- **The result:** each broken statement gives one error, the rest of the file still parses, and later stages run on the statements that parsed (roadmap §4.5).

---

## 7. Syntax errors in the MVP

Each gets a snapshot test of its rendered diagnostic. The lexer reports #2–3, #9–15 and #22–24 (`lexer.md` §6); the parser reports the rest.

| # | Input | Diagnostic |
|---|---|---|
| 1 | `let r1 = Resistor {…}` then a new line with no `;` | expected `;`, with an insert-`;` suggestion |
| 2 | `12V +- 5%` | `+-` is `+ (-…)` in Rust; did you mean `±` or `+/-`? |
| 3 | `--x` | `--` is `-(-x)` in Rust; write `x` or `-(-x)` |
| 4 | `capacity / 2h ± 10%` | ambiguous tolerance (§4.2), both fixes offered |
| 5 | `1 ± 2% ± 1%` | `±` can't be chained |
| 6 | `a < b < c` | comparisons can't be chained; use `b in a..=c` |
| 7 | `1V ± 1% ..= 2V`, `x <= 1V..=2V` | §4.3 |
| 8 | `100..300` as a value | a value range is closed: use `100..=300` |
| 9 | `47q` | unknown unit suffix `q`, with the closest spellings |
| 10 | `1Meg` | use `M` |
| 11 | `k°C` | this unit takes no prefix |
| 12 | `4.7k7` | a number uses either a decimal point or an infix prefix (`4k7`), not both |
| 13 | `10 V` | the unit must touch the number: `10V` |
| 14 | `.5`, `1.` | write `0.5` / `1.0` |
| 15 | `a % 3`, a lone `%` | `%` only means percent, glued to a number (`1 %` after a number is #13 instead) |
| 16 | `Resistor { a = vcc }` | fields use `:` (a replacement suggestion, as in Spade) |
| 17 | `spec dc(out.v) in …;` | a spec needs a name: `spec name: …;` |
| 18 | `let net = …` | `net` is a keyword |
| 19 | `fn f() {}` at top level | `fn` isn't supported yet (reserved) |
| 20 | `assume …` inside a `block` | `assume` belongs in the contract |
| 21 | Unclosed `(`, `[` or `{` | expected the closer, with a secondary label on the opener |
| 22 | Unterminated `/*` | expected `*/`, with a label on the opener |
| 23 | Look-alikes pasted from datasheets: `−` (U+2212 minus), `–` (en dash), `≤` `≥`, `;` (Greek question mark), `×` | "this is `−` (U+2212), not `-`", with a replacement. *Why:* datasheet PDFs are full of these, and the AI will paste from them |
| 24 | `r1_α` | identifiers are ASCII for now |
| 25 | `///` just before `}` | warning: the doc comment isn't attached to anything |

---

## 8. Decisions and open questions

| # | Question | Answer |
|---|---|---|
| 1 | ASCII spellings `ohm` / `u` next to `Ω` / `µ` | **Both are accepted (decided 2026-09-26).** The formatter keeps whichever you wrote. *Why:* `Ω` and `µ` are hard to type; rewriting them would turn every format into a diff |
| 2 | Digit separators | **`1_000` and `1000` are both accepted (decided 2026-09-26),** as in Rust |
| 3 | Unicode identifiers | **Recommended: no, ASCII only**, unless a real need shows up. *Why:* names become SPICE node and element names in the netlist export, KiCad references and BOM rows, and those tools expect ASCII. Look-alike letters (Cyrillic `а` vs Latin `a`) would make two different nets look identical. And it needs no Unicode tables. Unicode stays where it earns its place: `µ`, `Ω`, `°`, `±` |
