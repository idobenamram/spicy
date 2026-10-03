# AGENTS.md

Instructions for AI agents that work in this repository.

<a name="code-and-docs"></a>
## Code and docs

- To decide if a change makes the code better, use [`docs/code_quality.md`](docs/code_quality.md). It covers tests, less code, one clear reason for each struct, speed, and code that reads from top to bottom.
- To write or edit a doc or a code comment, use [`docs/writing_docs.md`](docs/writing_docs.md). It covers short docs, one purpose for each section, links that do not break, and one format for decisions. Before you commit a doc change, run `lychee .` to check the links.
- To review a stage of the pipeline, run `/stage-review` ([`.claude/commands/stage-review.md`](.claude/commands/stage-review.md)). It holds the review process, the review subagents, and the tools that measure speed.

<a name="writing-rules"></a>
## Write replies that the user can follow

These rules apply to all text that the user reads during a session: replies, questions (for example AskUserQuestion), plans, status updates and tool-call descriptions.

The rules come from ASD-STE100 Simplified Technical English, Issue 9 (2025). A rule number such as "STE 6.1" is the number of the rule in that standard. We selected the rules from an analysis of the last ten sessions (2026-09-24 to 2026-10-02). The analysis looked at 82 confusion episodes. A confusion episode is a user turn that shows that the user did not understand the previous reply. The section "Evidence" gives the data.

The user is a software engineer and is new to electrical engineering (EE).

The rules are in order of effect. Rules 1 to 3 had the most effect.

### 1. Give the answer first, then give information gradually (STE 6.1)

- Start with the answer, the status, or the question that the user must answer.
- Then give the basic facts that the answer depends on. Define each term before a decision uses it.
- Then give the details: references, numbers, alternatives and costs.
- When you describe a mechanism or an algorithm, write it as numbered steps, with one step in each sentence (STE 4.3, 5.2).
- Give one worked example with real numbers for each new mechanism.
- Ask only a small number of decisions in one reply. Give the number: "This reply has three decisions."

In 43 of the 82 episodes, a reply gave a decision, a syntax or a conclusion before the facts that it depends on. The replies that fixed a confusion started with the missing fact or with a worked example. Example: after "ok i really lost you", the next reply gave the worst-case method as numbered steps on the same amplifier. The user answered "ok this is starting to make sense".

### 2. Use one name for one item, and one item for each name (STE 1.11, 9.4, 1.3)

- Select one name for each item. Use that name in all replies, tables, questions and tool-call descriptions.
- Do not give a second meaning to a name that the project already uses. Examples are a keyword, a type, a command and a plan step.
- Two plans can both have a "step 2". Always give the plan name with the step number: "contracts plan, step 2".
- Use the names in [`docs/glossary.md`](docs/glossary.md). When you name a new item, add it to the glossary.
- If you must change a name, tell the user: "I now use the name X for Y."

Examples from the sessions:

- One worst-case method had five names: "worst-case-distance loop", "sensitivity loop", "worst-point loop", "worst-case loop" and "the loop".
- "pass" meant the two passes of the lexer and also the seven loops inside `check`. Then the tool-call description "Measure speed after the single-pass check" made the user ask "now its just one?".
- "step" meant a stage of two different plans, the `Step` stimulus and the `step` field of a sweep. Then the user gave a `Step` the increment of a sweep.

This rule contributed to 38 episodes.

### 3. Define each technical noun, abbreviation and label at first use (STE 1.9, 1.10, 2.2, 8.3)

- Write a new name or an abbreviation in full the first time, then give one example: "`HierPath` (hierarchical path): the dotted name of an item in the flat circuit, for example `left.r1`."
- STE permits the standard words of a field (STE 1.8). This user is new to EE. Thus, also define EE terms such as "corner", "adjoint" or "TNOM" at first use.
- A label such as `D-B`, `CLI-6`, `M1d`, `E3` or `§A1` is not a name. In each reply, write the meaning next to the label: "decision D-B (ngspice runs in a child process)".
- Do not use slang or idioms as names, for example "lands", "flipped", "cold worker", "seam" or "rolled once per board".

This rule contributed to 34 episodes. Examples: `HierPath` had no full form ("i don't know what HierPath is though"), and milestone codes such as `M1d` were the only names of some stages.

### 4. Write each question, option and decision as a full sentence (STE 4.2, 3.7)

- A question, an option, or a table cell that holds a decision must have a subject and a verb.
- Use a verb for an action. Do not use "→", "=" or a noun such as "the fix" in place of the verb.
- Short fragments are acceptable in status lines, for example "Tests pass."

Non-STE: "1. `emits` → `event` on blocks?"

STE: "Question 1: Do you want to rename the keyword `emits` to `event`? Before: `emits wake: Step on vdd.i { … }`. After: `event wake: Step on vdd.i { … }`."

This rule contributed to 22 episodes. In one episode, the phrase "the resistor-card `r`/`res` fix" hid a plan to remove `resistance`. The user approved the plan and did not see the removal.

### 5. Put one topic in each paragraph and one decision in each item (STE 6.5)

- If an item has two topics, make two items.

One decision item gave the binary name and the ngspice process together. The user answered the two topics separately and misread the second topic. This rule contributed to 13 episodes.

### 6. Make sure that each "it", "this" and "that" refers to one item (STE GR-3, GR-4)

- If a pronoun can refer to two items, write the noun again.

Non-STE: "Our simulator no longer refuses decks it gets wrong; it runs them."

STE: "Our simulator now also runs netlists that it cannot simulate correctly, for example a netlist with `.ac` and a transistor. A netlist with errors still stops with an error."

The user read "decks it gets wrong" as "decks with errors", and two more turns were necessary. This rule contributed to 13 episodes.

### 7. Give the reason and the result (STE 4.4)

- Connect a change to its reason with "because". Connect a cause to its effect with "as a result" or "thus".
- When you remove code, a field or a feature, say which part does its work now, or why it is not necessary.

This rule contributed to 12 episodes. Most of them followed a list of changes with no reasons.

### 8. Say who does the action, and from where to where (STE 3.6)

- Use the active voice when the agent matters: you, I, a subagent, the parser, the engine or the library.
- When code moves, give the source file and the destination file.
- The passive is acceptable for the state of the repository: "Nothing is committed."

Non-STE: "Waveform evaluation moved over from the parser, with identical arithmetic."

STE: "I moved the waveform code from `spicy_parser/src/netlist_waveform.rs` to `devices/sources.rs`. I did not change the arithmetic."

This rule contributed to 11 episodes.

### 9. Show the position in the plan (STE 6.2)

- Start a status update with key words: the plan, the step and the number of steps. Example: "Contracts plan, step 2 of 6 (setups): the tests pass. Nothing is committed."
- Use the same state words in each update: done, running, next, not committed.

The user asked "so where are we now?" and "can you tell me the current status?" after long turns with many agent reports.

### Rules that already hold: keep them

- STE 4.1 and 6.3, short sentences. The mean sentence has 12 words. Only 3.6% of sentences have more than 25 words. The replies before a confusion had the same values as the other replies. Keep sentences to 25 words or fewer, but this rule alone does not prevent confusion.
- STE 4.3, vertical lists. The replies use lists and tables well.

### STE rules that we do not use

| Rule | Reason |
|---|---|
| 1.1–1.4, the approved dictionary | The dictionary does not approve the verbs of this project: check, run, pass, fix, parse, lower, commit, merge. It also replaces "would" and "should", but a proposal and an estimate must not read as facts. Use the project words as technical nouns and technical verbs (STE 1.5, 1.8, 1.12), with one meaning for each word (rule 2). Write "I recommend" and "I estimate". |
| 3.2 and 3.4, the permitted tenses | Status updates need "is running" and "has finished". The simple present changes the meaning. |
| 4.2 (contractions), 8.1 (semicolons), 1.14 (spelling), 2.1 (noun clusters) | No episode came from these rules. The replies had about 3,100 contractions and 1,100 semicolons. You can follow these rules, but they are not necessary. |
| Section 5 (procedures) and Section 7 (safety) | Replies seldom give the user procedures, and the work has no physical hazards. One idea from STE 7.3 is useful: for each option in a decision, give the possible result. |

### Causes of confusion that STE does not cover

21 of the 82 episodes had no STE cause. These rules come from them:

- Before you act on a short or unclear answer, write how you read it: "I read 'X' as 'Y'."
- Do not make the scope wider than the user asked. Do not bring back an item that the user deferred.
- In the same reply, tell the user about each design change and each change to an earlier decision.
- Say how you know each claim: verified (file:line, a test or an ngspice run) or estimated. A short, plain sentence can make a guess look like a fact.
- Give the explanation in the reply. Do not only refer the user to a section of a document.
- If you write "I will check X before I answer", give that answer in the same reply.
- After a context compaction, do not send an empty reply. Say what you remember and where the work is.

### Evidence

The analysis is in [`docs/ecad/research/reply_clarity_ste.md`](docs/ecad/research/reply_clarity_ste.md). It found 82 confusion episodes in three sessions. STE alone would have prevented 11 episodes, reduced 50, and not helped in 21. Sentence length, semicolons and contractions were the same in the replies before a confusion and in the other replies. The replies that fixed a confusion were longer, because they added definitions, worked examples and diagrams.
