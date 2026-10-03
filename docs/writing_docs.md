# Writing docs and code comments

These rules apply to all Markdown docs and all code comments in this repository. Use them for new docs and for each edit. Older docs keep their form, except the live docs (see [Live docs](#live-docs)). For the wording of each sentence, use the writing rules in [`AGENTS.md`](../AGENTS.md#writing-rules). Those rules apply to docs too.

<a name="short"></a>
## Keep each doc short

- Write only what the reader needs. "A small set of fresh and accurate docs is better than a large assembly of 'documentation' in various states of disrepair." ([Google docguide](https://google.github.io/styleguide/docguide/best_practices.html))
- Change a doc in the same commit as the code that it describes.
- Delete text that is no longer true. Git keeps the history, so do not keep old versions in the doc.

<a name="one-purpose"></a>
## Give each doc and each section one purpose

- Select one type for each doc. Do not mix two types in one doc ([Diátaxis](https://diataxis.fr/start-here/)).

  | Type | Holds | Examples |
  |---|---|---|
  | Reference | Facts that the code must match | `grammar.md`, `language.md` |
  | Design note | Why the design has this form, and its decisions | `model.md`, `ast.md` |
  | Plan | The steps of a piece of work, and their status | `contracts_plan.md` |
  | Research report | The evidence for a decision. It does not change after the decision. | `research/*.md` |
  | Tutorial | A worked example that teaches | `walkthrough.md` |
  | Live doc | The current state and the index | see [Live docs](#live-docs) |

- Keep each fact in one place. In other places, write a one-line summary of the fact and link to it. A link with no summary is not sufficient: the reader must know what the link holds before they follow it.
- Give each section a heading that says what it holds. Make each heading unique in its doc.

<a name="linear"></a>
## Write in a straight line

- Start with the purpose of the doc in one or two sentences.
- Then put the sections in the order that the reader needs them. Define each term before a section uses it.
- Use the names in the [glossary](glossary.md). When you name a new item, add it to the glossary.
- Put a decision after the facts and the options that it depends on.

<a name="decisions"></a>
## Write decisions in one format

Use the sections of [MADR 4.0](https://github.com/adr/madr/blob/4.0.0/template/adr-template.md) for each decision:

1. **Context and problem:** the problem, with one worked example.
2. **Options:** each option with its pros and cons, its cost, and what the references do.
3. **Decision:** the selected option and the reason.
4. **Consequences:** what changes, and what becomes harder.

Each decision has a status: proposed, accepted, rejected, or replaced by another decision.

<a name="links"></a>
## Make links and numbers that do not break

A section number changes when someone adds a section. A line number changes with each edit. A heading changes when someone renames it. Thus, link to fixed names, not to positions.

- **Anchors.** Put an anchor on the line above each heading that other text links to: `<a name="transport"></a>`. A link uses that anchor: `[engine plan, transport](engine_plan.md#transport)`. Do not change an anchor after other text links to it.
- **IDs.** Give a decision, a plan step and an open question an ID with the prefix of its doc, for example `CLI-4`. Use the ID as its anchor too: `<a name="cli-4"></a>`. Do not change an ID and do not use it again. When a decision is replaced, keep it and set its status to "replaced by `<new ID>`" ([Nygard](https://www.cognitect.com/blog/2011/11/15/documenting-architecture-decisions)). Before you select a prefix for a new doc, search the docs to make sure that no other doc uses it.
- **IDs with names.** Always write an ID with its short name: "`CLI-4` (exit codes)". An ID alone does not tell the reader anything.
- **Code from docs.** Name the item, for example `spicy_lang::resolve::Scope::declare`, so that a reader can find it with symbol search. Do not link to a line of a file, because line links go stale ([matklad](https://matklad.github.io/2021/02/06/ARCHITECTURE.md.html)).
- **Docs from code.** Write the path, the anchor and the short name, for example `// See docs/ecad/model.md#e12 (unit inference).`
- **Older docs.** Most older docs have no anchors. To refer to one of their sections, name the doc and the section number or the decision ID, for example `model.md` §4 or `model.md` E16.
- **Section numbers.** New docs do not number their headings. If a long doc must number its headings, use the [ISO 2145](https://en.wikipedia.org/wiki/ISO_2145) form, with no period after the last number: `2`, `2.1`, `2.1.3`. Links still use anchors.
- **Steps and lists.** Number a list only when the order matters.
- **Check the links.** Before you commit a doc change, run `lychee .` from the repository root ([lychee](https://github.com/lycheeverse/lychee) 0.24.2). It fails when a link points to a missing file or to a missing heading or anchor. CI runs the same check, with the settings in `lychee.toml`.

<a name="code-comments"></a>
## Code comments

- Write why the code does something. Do not write what the code does, because the code shows that.
- Start a doc comment with one line that summarizes the item ([rustdoc](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html)). Add more lines only when the reader needs them.
- Do not repeat an explanation that a doc gives. Write one line and the link to the doc.
- In doc comments, refer to items with intra-doc links, for example ``[`Scope::declare`]``. `cargo doc --document-private-items` warns when such a link breaks.
- Do not write the history of a change in a comment. Git keeps it.
- A `TODO` names the roadmap item that tracks it.

<a name="live-docs"></a>
## Live docs

These docs always show the current state: [`README.md`](../README.md), [`docs/ecad/README.md`](ecad/README.md) (the index of the design docs) and [`docs/ecad/roadmap.md`](ecad/roadmap.md). When a change makes one of them wrong, correct it in the same commit. When you add, delete or rename a doc, update the index.
