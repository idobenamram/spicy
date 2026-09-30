//! Pass 1: every placement cycle in the design, before anything is expanded.
//!
//! A pass of its own, over every block, not a walk from the roots: if `A` and `B` place
//! each other, neither is unplaced, so there is no root to walk from, and the cycle
//! would never be seen (`research/flatten_circuit.md` §3.1). Each cycle is reported
//! once, at the placement that closes it, with the whole chain (CIRCT's
//! `CheckRecursiveInstantiation` reports each cycle once; Spade's traceback shows the
//! chain). A block placed twice on the way (a diamond) is not a cycle: only a block
//! still on the current path is.
//!
//! The search takes the blocks, and each block's placements, in name order: which
//! placement closes a cycle, and so what is reported and cut, depends only on the
//! names, never on statement or block order (model.md E4).

use std::collections::HashMap;

use super::{FlattenError, FlattenProblem};
use crate::design::{BlockId, Design, DesignSourceMap, InstanceId};
use spicy_errors::Reported;

/// Where the search is with a block: the white, grey and black of a depth-first search.
#[derive(Clone, Copy, PartialEq)]
enum Visit {
    New,
    /// On the current path: placing it again closes a cycle.
    OnPath,
    Done,
}

/// Reports every placement cycle once; returns the placements on a cycle, which
/// flatten doesn't expand, each with the proof its cycle was reported. `blocks` is every
/// block, and `children` each block's placements of blocks, in name order.
pub(super) fn find_cycles(
    design: &Design,
    map: &DesignSourceMap,
    blocks: &[BlockId],
    children: &[Vec<(InstanceId, BlockId)>],
    errors: &mut Vec<FlattenError>,
) -> HashMap<(BlockId, InstanceId), Reported> {
    let mut visit = vec![Visit::New; design.blocks.len()];
    let mut cut = HashMap::new();
    for &start in blocks {
        if visit[start.index()] != Visit::New {
            continue;
        }
        // The current path, from `start`: each block on it, and the placement it goes
        // on through.
        let mut path = vec![Step::new(start)];
        visit[start.index()] = Visit::OnPath;
        while let Some(step) = path.last_mut() {
            let Some(child) = step.next_block(children) else {
                visit[step.block.index()] = Visit::Done;
                path.pop();
                continue;
            };
            match visit[child.index()] {
                Visit::New => {
                    visit[child.index()] = Visit::OnPath;
                    path.push(Step::new(child));
                }
                Visit::OnPath => {
                    // The cycle: from `child` on the path to here, then back to `child`.
                    let from = path.iter().position(|s| s.block == child).expect("on path");
                    let cycle = &path[from..];
                    let reported = cycle_error(design, map, children, cycle).report(errors);
                    let steps = cycle.iter().map(|step| step.placement(children));
                    cut.extend(steps.map(|placement| (placement, reported)));
                }
                Visit::Done => {}
            }
        }
    }
    cut
}

/// A block on the current path, and how far through its placements the search is.
struct Step {
    block: BlockId,
    /// The placements looked at so far, in name order. The last of them is the one the
    /// path goes on through, or the one that closes a cycle.
    looked_at: usize,
}

impl Step {
    fn new(block: BlockId) -> Self {
        Step {
            block,
            looked_at: 0,
        }
    }

    /// Moves on to the block's next placement, and returns the block it places. `None`
    /// when there are no more.
    fn next_block(&mut self, children: &[Vec<(InstanceId, BlockId)>]) -> Option<BlockId> {
        let &(_, child) = children[self.block.index()].get(self.looked_at)?;
        self.looked_at += 1;
        Some(child)
    }

    /// The placement the path goes on through: the last one looked at.
    fn placement(&self, children: &[Vec<(InstanceId, BlockId)>]) -> (BlockId, InstanceId) {
        (
            self.block,
            children[self.block.index()][self.looked_at - 1].0,
        )
    }
}

/// A cycle, reported at the placement that closes it, with the chain from its first
/// block back to itself (`A → B → A`) and the first block's definition as related.
fn cycle_error(
    design: &Design,
    map: &DesignSourceMap,
    children: &[Vec<(InstanceId, BlockId)>],
    cycle: &[Step],
) -> FlattenError {
    let first = cycle[0].block;
    let blocks = cycle.iter().map(|step| step.block).chain([first]);
    let chain = blocks.map(|b| design.block(b).name.clone()).collect();
    let (block, i) = cycle
        .last()
        .expect("a cycle has a block")
        .placement(children);
    let at = map.blocks[block.index()].instances[i.index()].kind;
    let problem = FlattenProblem::RecursivePlacement { chain };
    FlattenError::new(problem.into(), at).with_related(map.blocks[first.index()].name)
}
