
/// Cached MNA stamp indices for a 2-terminal device (both diagonal and off-diagonal entries).
///
/// Note: During sparsity-pattern building this temporarily stores *builder entry indices*, and is
/// later "finalized" to hold *CSC nnz indices* using an `EntryMapping`.
#[derive(Debug, Clone)]
pub struct NodePairStamp {
    pub pos_pos: Option<usize>,
    pub neg_neg: Option<usize>,
    /// Off-diagonal entries: (pos, neg) and (neg, pos).
    pub off_diagonals: Option<(usize, usize)>,
}

impl NodePairStamp {
    pub fn unitialized() -> Self {
        Self {
            pos_pos: None,
            neg_neg: None,
            off_diagonals: None,
        }
    }

    pub fn temp_entries(
        &mut self,
        pos_pos: Option<usize>,
        neg_neg: Option<usize>,
        off_diagonals: Option<(usize, usize)>,
    ) {
        self.pos_pos = pos_pos;
        self.neg_neg = neg_neg;
        self.off_diagonals = off_diagonals;
    }

    pub fn finialize(
        &mut self,
        pos_pos: Option<usize>,
        neg_neg: Option<usize>,
        off_diagonals: Option<(usize, usize)>,
    ) {
        self.pos_pos = pos_pos;
        self.neg_neg = neg_neg;
        self.off_diagonals = off_diagonals;
    }
}

#[derive(Debug, Clone)]
pub struct NodeBranchPairStamp {
    // (pos, branch), (branch, pos)
    pub pos_branch: Option<(usize, usize)>,
    // (neg, branch), (branch, neg)
    pub neg_branch: Option<(usize, usize)>,
    pub branch_branch: usize,
}

impl NodeBranchPairStamp {
    pub fn unitialized() -> Self {
        Self {
            pos_branch: None,
            neg_branch: None,
            branch_branch: usize::MAX,
        }
    }

    pub fn temp_entries(
        &mut self,
        pos_branch: Option<(usize, usize)>,
        neg_branch: Option<(usize, usize)>,
        branch_branch: usize,
    ) {
        self.pos_branch = pos_branch;
        self.neg_branch = neg_branch;
        self.branch_branch = branch_branch;
    }

    pub fn finialize(
        &mut self,
        pos_branch: Option<(usize, usize)>,
        neg_branch: Option<(usize, usize)>,
        branch_branch: usize,
    ) {
        self.pos_branch = pos_branch;
        self.neg_branch = neg_branch;
        self.branch_branch = branch_branch;
    }
}

#[derive(Debug, Clone)]
pub struct NodeVoltageSourceStamp {
    // (pos, branch), (branch, pos)
    pub pos_branch: Option<(usize, usize)>,
    // (neg, branch), (branch, neg)
    pub neg_branch: Option<(usize, usize)>,
}

impl NodeVoltageSourceStamp {
    pub fn unitialized() -> Self {
        Self {
            pos_branch: None,
            neg_branch: None,
        }
    }

    pub fn temp_entries(
        &mut self,
        pos_branch: Option<(usize, usize)>,
        neg_branch: Option<(usize, usize)>,
    ) {
        self.pos_branch = pos_branch;
        self.neg_branch = neg_branch;
    }
    pub fn finialize(
        &mut self,
        pos_branch: Option<(usize, usize)>,
        neg_branch: Option<(usize, usize)>,
    ) {
        self.pos_branch = pos_branch;
        self.neg_branch = neg_branch;
    }
}