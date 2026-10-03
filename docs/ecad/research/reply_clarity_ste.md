# Reply clarity: ASD-STE100 applied to past sessions

> Research report, 2026-10-02. It is the evidence for the [writing rules in `AGENTS.md`](../../../AGENTS.md#writing-rules). It does not change after the decision.

**Question:** which rules of ASD-STE100 Simplified Technical English (Issue 9, 2025) would have made the assistant's replies clearer, and caused less confusion?

**Answer:** the rules about the order of information and about names. These are: give information gradually (6.1), use one name for each item (1.11, 9.4), and define each term at first use (1.9, 1.10, 2.2, 8.3). The sentence rules (length, semicolons, contractions) made no difference, because the replies already followed them.

<a name="method"></a>
## Method

- **Sessions.** The last ten sessions of this project, 2026-09-24 to 2026-10-02. Six were only a login. One (`f866c681`) is the start of `443154a8`. Three work sessions remain: `443154a8` (97 user turns), `351b53f1` (76 user turns) and `aa19c466` (7 user turns).
- **Text.** All user prompts and all assistant text, without tool calls. The inputs of the question tool stayed in, because the user read them.
- **Agents.** Eight subagents each read one part of about 150,000 characters. Each agent did four things:
  - found the confusion episodes;
  - quoted the reply, named the STE rules that it broke, and wrote an STE version;
  - judged if STE would have helped;
  - read at least ten replies with no confusion after them, as a baseline.
- **Confusion episode.** A user turn that shows that the user did not understand the previous reply: "I didn't get", "lost you", "what is X", "expand", a correction such as "I meant…", or a question asked again.
- **Counts.** A script measured three groups of reply blocks: the blocks before a confusion, the blocks that answered a confusion (the repairs), and all other blocks. Rule 8.5 and rule 8.6 set the word count: text in parentheses, code, numbers and identifiers count as one word.
- **Checks.** The quotes used as examples were checked against the transcripts. All of them matched.

<a name="verdicts"></a>
## Verdicts

The agents found 82 confusion episodes. For each one, they judged what STE alone would have done:

| STE alone would have… | Episodes |
|---|---|
| prevented the confusion | 11 |
| reduced the confusion | 50 |
| not helped | 21 |

<a name="rules"></a>
## The rules that contributed

The counts are the sums of the eight agents' tallies. One episode can have several rules.

| STE rule | Episodes | Typical failure |
|---|---|---|
| 6.1: give information gradually | 43 | A decision, a syntax or a conclusion came before the facts that it depends on |
| 1.11, 9.4: one name for each item | 38 | One worst-case method had five names. "step" had three meanings |
| 1.9, 1.10, 2.2, 8.3: clear terms, no jargon, abbreviations in full | 34 | `HierPath`, `TNOM`, milestone codes as the names of stages, "cold worker" |
| 4.2, 3.7: no omitted words, a verb for each action | 22 | Questions written as fragments, "→" in place of "rename" |
| 6.5: one topic in each paragraph | 13 | One decision item with two topics |
| GR-3, GR-4: clear pronouns | 13 | "decks it gets wrong" |
| 4.4: connecting words | 12 | A removal with no "because" |
| 3.6: active voice | 11 | "moved over from the parser" hid the destination file |
| 4.3 and section 5: numbered steps | 4 | An algorithm in one long sentence |
| 6.2: key words for structure | 3 | No "step N of M" in status updates |
| 1.3: one meaning for each word | 3 | "linear" for a circuit and for an estimate |
| 6.3, 4.1: sentence length | 13 (never the main cause) | Long sentences also appeared in the replies that worked |
| 8.1: no semicolons | 7 (never the main cause) | |
| 4.2: no contractions | 0 | |

`AGENTS.md` quotes the clearest episodes next to each rule.

<a name="counts"></a>
## Surface counts

| Group | Reply blocks | Words per block | Mean sentence (words) | Sentences over 25 words | Semicolons per 1,000 words | Contractions per 1,000 words | Passive sentences |
|---|---|---|---|---|---|---|---|
| Before a confusion | 81 | 268 | 11.9 | 4% | 11.7 | 26.2 | 10% |
| Repairs | 30 | 420 | 12.0 | 3% | 7.8 | 26.9 | 8% |
| All other | 233 | 273 | 12.5 | 4% | 10.4 | 24.0 | 12% |

No count separates the replies before a confusion from the other replies. The repairs had more words, not fewer. They added the missing fact, a definition, numbered steps, a worked example with numbers, a table or a diagram. After the user wrote "i really lost you", the next reply gave the method as numbered steps on the same amplifier. The user answered "ok this is starting to make sense".

<a name="not-adopted"></a>
## Rules that do not fit

All eight agents found the same misfits:

- **The approved dictionary (1.1–1.4).** It does not approve the project's verbs: check, run, pass, fix, parse, lower, commit, merge. It replaces "would" and "should", but an estimate must stay an estimate.
- **The permitted tenses (3.2, 3.4).** A status update needs "is running" and "has finished".
- **Contractions, semicolons, spelling and noun clusters (4.2, 8.1, 1.14, 2.1).** The replies had about 3,100 contractions and 1,100 semicolons, and none of them caused an episode.
- **Procedures and safety instructions (sections 5 and 7).** The replies seldom give the user steps to do, and the work has no physical hazards.

<a name="outside-ste"></a>
## Causes outside STE

These causes come from the agents' reports. The counts are approximate.

| Cause | Episodes |
|---|---|
| No worked example, or no background in EE or compilers | about 18 |
| Too many topics or decisions in one reply | about 14 |
| The assistant misread the user, or made the scope wider | about 11 |
| A design change or a reversed decision that the reply did not announce | about 6 |
| A wrong or unverified claim, stated as a fact | about 6 |
| A decision question with no options, references or example | about 6 |
| The status was lost over long turns with many agent reports | about 5 |
| A pointer to a document in place of an explanation | 3 |
| An empty reply after a context compaction | 2 |
| A promised answer that the reply never gave | 1 |

<a name="limits"></a>
## Limits

- The agents decided what counts as an episode and which rules contributed. Other readers can count differently.
- The analysis has one user, a software engineer who is new to electrical engineering.
- The text missed 13 prompts that the user typed while a turn was running. One of the two that showed confusion (the "two passes" question) is in the analysis.
- A first count of labels such as `D-B` and `E3` also counted part names such as `R1` and `Q1`. That count showed a false difference. A count without part names shows no difference (0.53, 0.53 and 0.46 labels per 100 words).
