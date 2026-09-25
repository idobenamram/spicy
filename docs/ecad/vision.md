# AI-Native Schematic & Simulation Editor: Concept Spec v0.1

## Vision
A KiCad-style schematic editor built around two first-class capabilities: **working with AI** and **working with simulation**.

**In scope:** schematic capture and simulation.
**Out of scope (future):** PCB layout, placement, routing.

## 1. AI-Native
- **Select to ask:** drag-select any region of a schematic and ask about it. The selection and a rendered visual of it are sent to the AI as context.
- **Project-bound chats:** conversations are stored with the project and persist across sessions.
- **Full project visibility:** the AI can see everything it needs to give valid answers, including the schematic, parameters and equations, simulation results, and part models.

## 2. Vim-Style Modal Workflow
- **Edit mode:** change the design.
- **Simulate mode:** configure, run, and inspect simulations.
- Keyboard-first navigation within modes and fast switching between them.

## 3. Integrated Simulation

### Engine
- Start with the in-house KLU-based SPICE simulator.
- Provide an integration layer so any other SPICE simulator can be plugged in.

### Hierarchical Blocks
- Each schematic is a block.
- **Block-level simulation:** run one block in isolation.
- **System-level simulation:** connect blocks through their defined interfaces and run the whole system.

### Block I/O Definition Page
- **Valid ranges:** allowed ranges for specific values and nodes.
- **Required inputs:** stimulus definitions a block needs, such as a sine wave over a given voltage range.
- **Expected outputs:** what the block should produce, such as GPIO logic levels or analog signals.
- **Signal driving:** set and drive digital (GPIO) and analog signals at the interface.
- **Composition:** blocks connect through these interfaces to build larger simulations.
- **Simulation config:** analysis type, simulator choice, and the usual settings.

### In-Editor Results
- Results appear on the schematic itself (node voltages, currents, values in place), not in a separate plot window where they get lost.

## 4. Parametric Values
- Component values are expressions, not magic numbers.
- Each value can carry:
  - its equation,
  - a comment explaining the choice,
  - design margins (offsets and buffer zones).
- **Project-wide parameters** drive dependent values.
- **Change propagation**, user-selectable:
  - **Cascade:** dependent values update automatically, with a diff of what changed.
  - **Flag:** values stay as they are, and any whose equation no longer holds are marked.

## 5. AI-Assisted Part Modeling
- Given a real manufacturer part, the AI extracts the data needed to build its simulation model.
- The goal is accurate, realistic simulations rather than ideal-component approximations.

## Open Items
- **File format:** prefer existing standard formats, but a custom format may be needed. Decide after surveying what exists and what's supported.
- **Parametric system:** mechanics still to be made concrete.
- **I/O page schema:** needs detailed definition.

---

I'll start the research now. Several parallel threads will survey existing tools pillar by pillar, others will hunt for gaps, and everything will be combined into one picture of where the idea stands.

Your competitive landscape and gap analysis for the AI-native schematic capture + SPICE simulation editor is ready. It surveys the fast-moving AI-ECAD category (including Flux.ai's March 2026 launch of AI-driven SPICE simulation) and maps each of your five pillars against shipping and research tools. The report includes a per-pillar survey, a full competitor matrix across 13 products, your closest competitors ranked (Flux.ai, Qucs-S, tscircuit, Cadence Virtuoso ADE, atopile), a gap analysis with KiCad user-pain evidence, a file-format recommendation (.kicad_sch interop + custom diff-able native format), incumbent risk assessment, and six strategic recommendations. Key takeaway: your defensible differentiation lies in the union of Vim-modal UX, formal block-I/O contracts driving SPICE, parametric values with rationale/margins and cascade-or-flag propagation, and in-place results — not "AI + simulation" alone, which is now table stakes.
