//! Flatten's problems (model.md E18 tier 2, E14): data first, rendered by
//! `spicy_errors` like every stage's.

use spicy_errors::{Diag, DiagKind, Fix, Severity, Text, list};

/// One problem flatten found.
pub type FlattenError = Diag<FlattenErrorKind>;

/// A problem, and which placements have it: what's wrong apart from where, as slang
/// keeps a diagnostic's arguments apart from the instance it's in and its count
/// (model.md E23). The same problem in several placements is one, and is compared
/// without the placements.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FlattenErrorKind {
    pub problem: FlattenProblem,
    /// Where it is, for a problem inside a placed block. `None` for a problem of the
    /// root itself, whose paths are from the root, or of the whole design.
    pub in_block: Option<InBlock>,
}

/// What's wrong, with paths from the block it's in: the same in every placement.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FlattenProblem {
    /// A block that places itself, directly or through others: the chain, from the
    /// block placed again back to itself (`A → B → A`). `related` is its definition.
    RecursivePlacement { chain: Vec<String> },
    /// Power sources that meet on a net: the net and every source, by their paths
    /// from where they meet. `related` is where the first comes in.
    SeveralSources { net: String, sources: Vec<String> },
    /// A net with loads that nothing powers: the net and its innermost loads, by their
    /// paths from the net's top.
    NoSource { net: String, sinks: Vec<String> },
    /// No net carries a `Ground` port.
    NoGround { root: String },
    /// More than one net carries a `Ground` port. `related` is the first.
    SeveralGrounds { root: String, nets: Vec<String> },
    /// A root that flattens to more placements than the limit; it isn't flattened.
    TooManyPlacements { root: String, limit: usize },
    /// A two-pin part with both pins on one net (a warning).
    ShortedPart { part: String },
    /// Nets no part connects to ground (a warning): an isolated part of the circuit.
    IsolatedNets { nets: Vec<String> },
}

/// Where a problem inside a placed block is (model.md E23), as slang says "in 2 of 3
/// instances, e.g. …": the block its paths are from, and how many of the block's
/// placements have it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InBlock {
    pub block: String,
    /// How many of its placements have the problem.
    pub found: usize,
    /// How many placements it has, in every root checked (in the one root simulated,
    /// for a simulation check).
    pub placements: usize,
    /// The path of the first placement with the problem.
    pub example: String,
}

/// A problem in no placed block: of the whole design (a recursive placement), or of a
/// root as a whole (one too large to flatten).
impl From<FlattenProblem> for FlattenErrorKind {
    fn from(problem: FlattenProblem) -> Self {
        FlattenErrorKind {
            problem,
            in_block: None,
        }
    }
}

impl FlattenErrorKind {
    /// Every value [`name`](DiagKind::name) can return; `name`'s match is exhaustive,
    /// so a new variant won't compile until it has a name. Add it here too.
    pub const ALL_NAMES: &'static [&'static str] = &[
        "RecursivePlacement",
        "SeveralSources",
        "NoSource",
        "NoGround",
        "SeveralGrounds",
        "TooManyPlacements",
        "ShortedPart",
        "IsolatedNets",
    ];

    /// One more placement has this problem.
    pub(super) fn add_placement(&mut self) {
        if let Some(in_block) = &mut self.in_block {
            in_block.found += 1;
        }
    }
}

impl InBlock {
    /// `in `Mid``, when every placement of the block has the problem, since it's then
    /// the block's own; `in 2 of 3 placements of `Mid`, e.g. `a``, when some do.
    fn text(&self) -> String {
        let InBlock {
            block,
            found,
            placements,
            example,
        } = self;
        if found == placements {
            format!("in `{block}`")
        } else {
            format!("in {found} of {placements} placements of `{block}`, e.g. `{example}`")
        }
    }
}

impl DiagKind for FlattenErrorKind {
    fn name(&self) -> &'static str {
        use FlattenProblem::*;
        match self.problem {
            RecursivePlacement { .. } => "RecursivePlacement",
            SeveralSources { .. } => "SeveralSources",
            NoSource { .. } => "NoSource",
            NoGround { .. } => "NoGround",
            SeveralGrounds { .. } => "SeveralGrounds",
            TooManyPlacements { .. } => "TooManyPlacements",
            ShortedPart { .. } => "ShortedPart",
            IsolatedNets { .. } => "IsolatedNets",
        }
    }

    fn code(&self) -> &'static str {
        use FlattenProblem::*;
        match self.problem {
            RecursivePlacement { .. } => "E-recursion",
            SeveralSources { .. } | NoSource { .. } => "E-power",
            NoGround { .. } | SeveralGrounds { .. } => "E-ground",
            TooManyPlacements { .. } => "E-size",
            ShortedPart { .. } => "W-short",
            IsolatedNets { .. } => "W-isolated",
        }
    }

    fn severity(&self) -> Severity {
        match self.problem {
            FlattenProblem::ShortedPart { .. } | FlattenProblem::IsolatedNets { .. } => {
                Severity::Warning
            }
            _ => Severity::Error,
        }
    }

    fn related_label(&self) -> &'static str {
        match self.problem {
            FlattenProblem::RecursivePlacement { .. } => "defined here",
            FlattenProblem::SeveralSources { .. } => "the first source",
            FlattenProblem::SeveralGrounds { .. } => "the first ground",
            _ => "",
        }
    }

    fn text(&self, _fix: Option<&Fix>) -> Text {
        let (message, label, notes) = self.problem.text();
        match &self.in_block {
            None => (message, label, notes),
            Some(in_block) => (format!("{message} ({})", in_block.text()), label, notes),
        }
    }
}

impl FlattenProblem {
    /// The words, the same in every placement.
    fn text(&self) -> Text {
        use FlattenProblem::*;
        // The verb that agrees with a `list`: "`a` is", "`a` and `b` are".
        let verb = |items: &[String], one: &'static str, many: &'static str| match items {
            [_] => one,
            _ => many,
        };
        match self {
            RecursivePlacement { chain } => (
                format!("`{}` places itself: {}", chain[0], chain.join(" → ")),
                format!("places `{}` again", chain[0]),
                vec!["note: a block can't contain itself, directly or through others".to_string()],
            ),
            SeveralSources { net, sources } => (
                format!("net `{net}` has more than one power source"),
                "a second source".to_string(),
                vec![format!("note: the sources are {}", list_capped(sources))],
            ),
            NoSource { net, sinks } => (
                format!("nothing powers net `{net}`"),
                "needs a power source".to_string(),
                vec![
                    format!(
                        "note: {} {} power from it",
                        list_capped(sinks),
                        verb(sinks, "takes", "take")
                    ),
                    format!(
                        "help: connect `{net}` to a `Power<In>` port of this block, or to a \
                         placed block's `Power<Out>`"
                    ),
                ],
            ),
            NoGround { root } => (
                format!("`{root}` has no ground net"),
                "no net here carries a `Ground` port".to_string(),
                vec![
                    "help: add `port gnd: Ground;` and bind the parts' return pins to it"
                        .to_string(),
                ],
            ),
            SeveralGrounds { root, nets } => (
                format!("`{root}` has more than one ground net"),
                "a second ground".to_string(),
                vec![
                    format!("note: the ground nets are {}", list_capped(nets)),
                    "help: bind every `Ground` port to the same net".to_string(),
                ],
            ),
            TooManyPlacements { root, limit } => (
                format!("`{root}` flattens to more than {limit} placements"),
                "past the limit here".to_string(),
                vec![
                    "note: each placement places everything inside its block again: a block \
                     placed twice inside a block placed twice is four placements"
                        .to_string(),
                ],
            ),
            ShortedPart { part } => (
                format!("both pins of `{part}` are on one net"),
                "shorted".to_string(),
                vec![],
            ),
            IsolatedNets { nets } => (
                format!(
                    "{} {} connected to nothing that reaches ground",
                    list_capped(nets),
                    verb(nets, "is", "are")
                ),
                "isolated".to_string(),
                vec!["note: a simulation can't solve an isolated net".to_string()],
            ),
        }
    }
}

/// `list`, with only the first few shown past a handful, in rustc's style: "`a`, `b`,
/// `c`, `d`, `e` and 95 others". Every list of paths is capped, since each can grow
/// with the design (a bus of 3000 regulators, 100 000 isolated nets); the problem's
/// data keeps them all.
fn list_capped(items: &[String]) -> String {
    const SHOWN: usize = 5;
    let rest = items.len().saturating_sub(SHOWN);
    match rest {
        0 => list(items),
        1 => format!("{} and 1 other", list_items(&items[..SHOWN])),
        _ => format!("{} and {rest} others", list_items(&items[..SHOWN])),
    }
}

/// Each item in backticks, joined with commas.
fn list_items(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| format!("`{s}`")).collect();
    quoted.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    /// A list of several reads `a, b and c`, and its verb agrees (the case files have
    /// lists of one and two).
    #[test]
    fn lists_agree_with_their_verb() {
        let sinks = FlattenProblem::NoSource {
            net: "rail".to_string(),
            sinks: paths(&["a.vcc", "b.vcc"]),
        };
        let (_, _, notes) = sinks.text();
        assert_eq!(notes[0], "note: `a.vcc` and `b.vcc` take power from it");
        let isolated = FlattenProblem::IsolatedNets {
            nets: paths(&["x", "y", "z"]),
        };
        let (message, _, _) = isolated.text();
        assert_eq!(
            message,
            "`x`, `y` and `z` are connected to nothing that reaches ground"
        );
    }

    /// A long list shows the first five and counts the rest (the 100 000-part benchmark
    /// had a message of about 100 000 nets), in every problem that lists paths.
    #[test]
    fn a_long_list_is_capped() {
        let nets = |n: usize| (0..n).map(|i| format!("n{i}")).collect::<Vec<_>>();
        let message = |n| {
            let problem = FlattenProblem::IsolatedNets { nets: nets(n) };
            problem.text().0
        };
        assert!(message(5).starts_with("`n0`, `n1`, `n2`, `n3` and `n4` are"));
        assert!(message(6).starts_with("`n0`, `n1`, `n2`, `n3`, `n4` and 1 other are"));
        assert!(message(100).starts_with("`n0`, `n1`, `n2`, `n3`, `n4` and 95 others are"));
        let capped = "`n0`, `n1`, `n2`, `n3`, `n4` and 95 others";
        let lists = [
            FlattenProblem::SeveralSources {
                net: "bus".to_string(),
                sources: nets(100),
            },
            FlattenProblem::NoSource {
                net: "rail".to_string(),
                sinks: nets(100),
            },
            FlattenProblem::SeveralGrounds {
                root: "T".to_string(),
                nets: nets(100),
            },
        ];
        for problem in lists {
            let (_, _, notes) = problem.text();
            assert!(notes[0].contains(capped), "{}", notes[0]);
        }
    }
}
