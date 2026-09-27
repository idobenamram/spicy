# Research: Modelica (Spec, OpenModelica, MSL) for the Elaboration Stage

> 2026-09-27 · Research report for `model.md` (M1d).
> Revisions: Modelica spec LaTeX `18f00b1` (2026-09-03; section numbers are MLS 3.7-dev, where units are Ch. 19), OpenModelica `41a658f` (2026-09-26: `OMCompiler/Compiler/NFFrontEnd`, `Util`, `testsuite/flattening`), Modelica Standard Library `4c40388`.
> Verified facts in §1–8; lessons in §9 are opinion.

Running example (Modelica syntax):
```modelica
model Divider
  parameter Real R1 = 47e3;
  Resistor r1(R = R1);  Resistor r2(R = 10e3);  Ground g;
equation
  connect(r1.n, r2.p);  connect(r2.n, g.p);
end Divider;
```

## 1. Stages and names

**The spec (§5.6)** defines flattening in two steps:
1. **Instantiation** turns the *class tree* into an *instance tree*. The class tree is the AST of all loaded libraries (§5.6.1.1); the instance tree is described in §5.6.1.2.
2. **Generation of the flat equation system** (§5.6.2):
   - every name becomes a "globally unique identifier";
   - modifiers become equations (`r1.R = R1`);
   - `connect` equations are expanded per Ch. 9.

   "No other transformations are performed." The result is lists of variables, equations and functions.

**OpenModelica's new frontend (NF)** runs these stages in `instClassInProgram2` (`NFInst.mo:177–300`):
- `lookupRootClass`;
- `instantiateRootClass` (`:211`):
  - **expand**: resolve `extends` into a ClassTree;
  - **instantiate**: clone the tree per component and merge the modifiers;
- `instExpressions` (`:221`), a separate pass "to make sure that lookup is able to find the correct nodes";
- `updateImplicitVariability` (`:225`), which marks structural parameters;
- `Typing.typeClass` (`:230`);
- `Flatten.flatten` (`:234`), including `resolveConnections` (`NFFlatten.mo:2477`);
- `EvalConstants`;
- `UnitCheck` (`:240`);
- `SimplifyModel`;
- `VerifyModel` (`:295`).

A class tree moves through the states `PARTIAL_TREE → EXPANDED_TREE → INSTANTIATED_TREE → FLAT_TREE` (`NFClassTree.mo:74–117`). The output is `FLAT_MODEL{name, variables, equations, initialEquations, algorithms, …}` (`NFFlatModel.mo:85`).

## 2. Definitions vs instances

- A **class** is a definition (`Resistor`); a **component** is a placement (`Resistor r1(...)`).
- Each component gets an instance-tree node that "knows its source class definition … and its modification environment" (§5.6.1.2).

**Modifiers** are the parameter values given at placement: `r1(R = 47e3)` overrides the class's declaration equation (§7.2).
- **Outer overrides inner** (§7.2.3). In the spec's example, `extends C3(x2 = 22, …)` beats `x2 = 2` in C3.
- **The value is resolved where the modifier is written** (§7.2). NF's test `ModScope1` expects `parameter Real c.a.x = c.x;`, i.e. the value refers to the parent's `x`.
- `final` forbids further modification (§7.2.6).
- **Errors:**
  - an unknown element: `"Modified element %s not found in class %s"` (`Util/Error.mo:506`);
  - a duplicate: `DUPLICATE_MODIFICATIONS`, reported with both spans (`NFModifier.mo:780`, test `DuplicateMod1`).

**What two instances share:** in the spec, nothing; each gets its own subtree. Tools may reuse "as long as the resulting flat equation system is identical" (§5.6.1.4). NF expands a class once and clones it per instance (`ClassTree.instantiate`, `NFClassTree.mo:433–471`).

**Names:** the path of component names from the root, dot-separated, leaving out the unnamed `extends` nodes (§5.6.2): `amp.r1.R`. NF stores it as a linked list of instance nodes that shares prefixes (`NFComponentRef.mo:70–78`, `prefixCref` at `:107`).

## 3. Connections

**A connector** declares a potential and a flow variable. The MSL `Pin` (`Interfaces/Pin.mo:3,9`):
```modelica
connector Pin
  Real v;        // potential
  flow Real i;   // flow
end Pin;
```

**Parts are built from pins:**
- `TwoPin` defines `v = p.v - n.v`;
- `OnePort` adds `0 = p.i + n.i; i = p.i` (`OnePort.mo:7–8`);
- `Resistor` extends `OnePort` with `v = R_actual*i` (`Resistor.mo:18`).

**`Ground` is an ordinary model** whose only equation is `p.v = 0` (`Basic/Ground.mo:9`). There is no special ground node.

**From connect equations to equations (§9.2):**
- Each `connect` merges the *connection sets* of the primitive variables on both sides. Each member is tagged *inside* or *outside*.
- For each resulting set:
  - the potential variables become equal: `a1 = a2 = …`;
  - the flow variables sum to zero, `Σ ±z = 0`, with +1 for inside and −1 for outside members. That is Kirchhoff's current law.
- Every flow variable also starts in a set of its own, so **an unconnected pin silently gets `i = 0`** (an open pin).

For the example:
```
r1.n.v = r2.p.v;  r1.n.i + r2.p.i = 0;
r2.n.v = g.p.v;   r2.n.i + g.p.i = 0;
r1.p.i = 0;       // r1.p is unconnected
```

**Inside vs outside (§9.1.2):** a block's own connectors are *outside* with respect to that block; connectors of its sub-components are *inside*. This is exactly our "a block's own ports are seen flipped inside it" (language §3.3).

**In NF**, connections go through three steps:
- `Connections.collectConnections`;
- `ConnectionSets.fromConnections`, which `extends DisjointSets` (`NFConnectionSets.mo:50,67,121`), a union-find with union-by-rank and path compression (`Util/DisjointSets.mo:249–300`);
- `extractSets`, then `ConnectEquations.generateEquations` (`NFConnectEquations.mo:87`): potential equations at `:245`, flow equations at `:407`, the face sign at `:451`.

**Checks:**
- **connector balance:** as many flow variables as potential variables (§9.3.1; `UNBALANCED_CONNECTOR`, `NFTyping.mo:400`);
- **type compatibility:** test `ConnectInvalidType1` expects `[…ConnectInvalidType1.mo:14:3-14:18] Error: The connectors in connect(c1, c2) are not type compatible.`;
- one `quantity` per set, and **at most one signal source per causal set** (§9.3). This is the analog of our "two sources on a rail";
- **model balance:** equations = unknowns, both locally per class and globally (§4.8).

**A missing ground is not a frontend error.** It surfaces later as a singular system. The MSL's only help is an `unassignedMessage` on `Pin.v`: "a ground object is missing … or a connector … is not connected" (`Pin.mo:4–8`).

**How this maps to us:**
- our **net** is a *named* connection set;
- a pin binding `a: vcc` is `connect(r1.a, vcc)`;
- a block's **port** is an outside member of a set;
- **Ground** is the set that must become node 0.

## 4. Parameters, variability and units

**Variability** is ordered (§4.5): constant < evaluable parameter < non-evaluable parameter < discrete < continuous.
- NF's enum is `CONSTANT, STRUCTURAL_PARAMETER, PARAMETER, NON_STRUCTURAL_PARAMETER, DISCRETE, IMPLICITLY_DISCRETE, CONTINUOUS` (`NFPrefixes.mo:276`).
- A binding more variable than its slot is `HIGHER_VARIABILITY_BINDING` (`NFTyping.mo:1154`).
- **Structure may depend only on evaluable values:** array sizes, `if`/`for` around a `connect` (`IN_NON_EVALUABLE_IF_OR_FOR`, `Error.mo:310`), and conditional components (`heatPort … if useHeatPort`, `ConditionalHeatPort.mo:9`). NF marks those parameters *structural*.
- Bindings must be acyclic: `p = 2*q; q = sin(p)` is illegal (§4.4.4).
- This is our exact-vs-design split (language §5.5): *exact* ≈ constant or evaluable parameter, *design* ≈ parameter. **But Modelica parameters have no spread.**

**Units:**
- A unit is a string attribute on the type: `type Resistance = Real(final quantity="Resistance", final unit="Ohm")` (MSL `Units.mo:575–577`).
- The unit grammar allows rational exponents (§19.1).
- New unitful literals (`9.8'm/s2'`) are converted to the slot's unit only when the binding is a bare literal (§19.2).
- `displayUnit` is only a display default (§4.9.1).
- **Checking unit consistency is explicitly left to tools** (§19.2, non-normative).

**NF's unit checker:**
- runs on the *flat* model, after flattening;
- is off unless `--unitChecking` is set (default `false`, `Util/Flags.mo:819–821`) or `checkModel` runs;
- infers missing units through `MASTER` groups;
- reports **warnings**, not errors: "equation is INCONSISTENT…" (`NFUnitCheck.mo:873–876`);
- stores a unit as *integer* exponents over the 7 SI base units, plus a factor and an offset (`NFUnit.mo:62–74`). So V/√Hz can't be represented, even though the grammar allows it.

**`min` / `max` / `nominal`** (§4.9.1, §4.9.6):
- `min`/`max` is an assertion checked during simulation, `assert(min <= value <= max)`;
- `nominal` is only for scaling solver tolerances;
- there is no tolerance or distribution concept. Parameter sweeps are experiment-environment modifications, outside the language (§7.2.6 note).

## 5. Scoping and lookup

- **Simple names (§5.3.1):** search iteration variables first, then each lexically enclosing *instance* scope, up to an `encapsulated` class. In each scope: its elements (inherited ones included), then qualified imports, then unqualified imports (two matches is an error).
- **Only constants** may be referenced from an enclosing class (§5.3.1).
- **Composite names `A.B` (§5.3.2):** if `A` is a component, look `B` up among its components. If `A` is a class, only package or encapsulated elements are visible.
- **Classes and components share one namespace:** `M M;` is illegal.
- **Lookup is lexical:** it continues "at the instance of the lexically enclosing class … normally not equal to the parent of the current instance" (§5.6.2). `inner`/`outer` is the one dynamic lookup up the *instance* tree (for ambient temperature and similar, §5.4). Compare our `env temp`.
- **Order independence:** "Variables and classes can be used before they are declared" (§4.3). The spec guarantees it with the two-step flattening; NF with its separate `instExpressions` pass.

## 6. Diagnostics

**When errors are found:**
- **instantiation:** lookup (`"Variable %s not found in scope %s"`, `Error.mo:88`), missing or duplicate modifiers, extends loops, recursive definitions, partial components;
- **typing:** binding types, variability, connector compatibility and balance;
- **flattening:** connect restrictions, disabled conditional components;
- **afterwards:** model balance, and unit warnings.

**How they refer to source:**
- every message carries `file:line:col-line:col`;
- messages with several locations use `addMultiSourceMessage`, which prints "Notification: From here:" and the second location;
- every flat equation keeps an `ElementSource` (`FrontEnd/DAE.mo:110–120`): its source info, the instance prefix, **which `connect`s produced it**, and the symbolic operations applied since.

A "relaxed" context lets `checkModel` and the API keep going after errors (`NFInst.mo:193`).

## 7. What stays hierarchical vs flat

- **Graphics live in the source as annotations:** `Placement` on components, `Line(points=…)` on each `connect` (§18.9; `Examples/ChuaCircuit.mo:33–48`). The editor draws the class-level diagram.
- **The simulator gets the flat model.** Results are named by flat paths (`amp.r1.v`), which map plots back to the diagram.
- **The trees stay alive after flattening,** for functions instantiated on demand (§5.6.2, end).
- NF prints the flat model as "flat Modelica" / Base Modelica (`BaseModelica.mo`, `NFFlatModel.toFlatStream`).

## 8. Testing

- `testsuite/flattening/modelica/scodeinst` has 1,344 files.
- Each file has a header `// status: correct|incorrect`, the model, and a `// Result: … // endResult` block. The block holds either the expected flat model or the exact error text with `file:line:col`.
- `rtest` compares the output, and its baseline mode rewrites the block (`testsuite/rtest:348–427`; `fix-tests.sh` runs `runtests.pl -b`).
- There is one test per error kind: `DuplicateMod1`, `ConnectInvalidType1–2`, `ConnectNonConnector1–6`, …

## 9. Lessons (opinion)

1. **`Design` is the class tree; `FlatDesign` is the instance tree plus the flat system.** Define once, expand per placement (§5.6; NF expands once, then clones).
2. **Nets are a union-find over `(instance path, local net or port)` entries.**
   - Each set root becomes one `FlatNet`, later one `NodeId`; the ground set is 0.
   - No equality equations are needed: the set *is* the node.
   - Store each member's **face** (inside or outside) and its pin or port. The face drives the role flip for `Power<In>` and the per-net checks (§9.2, `DisjointSets`, language §3.3).
3. **Check ground and floating nets at flatten time, with spans.** Every connected group must reach a `Ground` set. Keep "every pin bound exactly once" as our structural balance check, rather than Modelica's silent `i = 0`.
4. **Modifiers are field bindings:** `fields: name → Binding { expr, scope_where_written, span }`. A placement's value overrides the default. Unknown and duplicate fields are errors, the duplicate with both spans (§7.2, §7.2.3; `ModScope1`, `DuplicateMod1`).
5. **Don't copy Modelica's parameter binding for tolerances.** `c.a.x = c.x` makes the two values identical, i.e. fully correlated. Our `param r: Tol<Ohm>` must give *each* part its own knob (language §7.1). Knobs are created at leaf part fields, and the knob's identity is the flat field path, linked back to the `param`.
6. **Variability as an ordered enum:** `Exact < Design (knobbed) < Solver/Simulated`, with the rule "binding ≤ slot". `for`/`if`/sizes require `Exact`, plus the acyclic-binding check (§4.5, §4.4.4).
7. **Check units while typing the `Design`, as errors, not after flattening.** Give `Dimension` rational exponents, for V/√Hz. Keep the literal as written apart from its SI value (`unit` vs `displayUnit`). NF's checker is late, optional, warning-only and integer-only.
8. **Intern paths as `(parent PathId, segment)`,** building dotted strings only at the edges (NF's shared-prefix `ComponentRef`).
9. **Provenance on every flat item:** the placement span, the instance chain, and for nets, the bindings that merged them. That answers "why are `base` and `x` one net?" (`ElementSource.connectEquationOptLst`).
10. **Two-pass name resolution** (§4.3; `instExpressions`).
11. **Lookup:** a block body sees its own ports, nets and params, plus constants and items from modules. Environment quantities are seen only through explicit `env` (§5.3.1's constants-only rule; `inner`/`outer` as the controlled exception).
12. **Tests:** one `.spl` per construct or error, with an insta snapshot of the FlatDesign dump or the diagnostic. `cargo insta review` does what `rtest -b` does.
13. **Keep geometry out of the text,** in our layout sidecar. Modelica mixes `Placement`/`Line` annotations into the source, so every drag shows up in diffs.

**Where Modelica doesn't help:** tolerances, knobs, contracts, and roles beyond input/output causality. Modelica has no concept for these.
