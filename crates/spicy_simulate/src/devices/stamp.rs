/// Cached MNA stamp indices for a 2-terminal device (both diagonal and off-diagonal entries).
///
/// The meaning of the stored indices depends on the linear solver:
/// - **KLU (sparse)**: indices refer to the CSC `values` array (nnz indices), computed via the
///   sparsity-pattern builder and an `EntryMapping`.
/// - **BLAS (dense)**: indices refer to a dense row-major linear index into the MNA matrix buffer:
///   `idx = row * dim + col`.
#[derive(Debug, Clone)]
pub struct NodePairStamp {
    pub pos_pos: Option<usize>,
    pub neg_neg: Option<usize>,
    /// Off-diagonal entries: (pos, neg) and (neg, pos).
    pub off_diagonals: Option<(usize, usize)>,
}

impl NodePairStamp {
    /// Create a stamp with no indices assigned yet.
    pub fn uninitialized() -> Self {
        Self {
            pos_pos: None,
            neg_neg: None,
            off_diagonals: None,
        }
    }

    /// Set the temporary indices produced during pattern construction.
    ///
    /// For sparse (KLU), these are COO entry indices returned by the pattern builder.
    /// For dense (BLAS), these can be the final dense linear indices.
    pub fn set_temp_indices(
        &mut self,
        pos_pos: Option<usize>,
        neg_neg: Option<usize>,
        off_diagonals: Option<(usize, usize)>,
    ) {
        self.pos_pos = pos_pos;
        self.neg_neg = neg_neg;
        self.off_diagonals = off_diagonals;
    }

    /// Compute and set temporary indices from node locations.
    ///
    /// The `entry` callback receives (column, row) to match `MatrixBuilder::push`.
    pub fn set_temp_indices_from_nodes<F, E>(
        &mut self,
        pos: Option<usize>,
        neg: Option<usize>,
        mut entry: F,
    ) -> Result<(), E>
    where
        F: FnMut(usize, usize) -> Result<usize, E>,
    {
        let pos_pos = pos.map(|p| entry(p, p)).transpose()?;
        let neg_neg = neg.map(|n| entry(n, n)).transpose()?;
        let off_diagonals = if let (Some(pos), Some(neg)) = (pos, neg) {
            Some((entry(pos, neg)?, entry(neg, pos)?))
        } else {
            None
        };
        self.set_temp_indices(pos_pos, neg_neg, off_diagonals);
        Ok(())
    }

    /// Map temporary indices to their final locations using the provided mapping.
    pub fn set_final_indices<F>(&mut self, mut f: F)
    where
        F: FnMut(usize) -> usize,
    {
        self.pos_pos = self.pos_pos.map(|i| f(i));
        self.neg_neg = self.neg_neg.map(|i| f(i));
        self.off_diagonals = self
            .off_diagonals
            .map(|(pos_neg, neg_pos)| (f(pos_neg), f(neg_pos)));
    }
}

/// Cached MNA stamp indices for a 3-terminal device (base/collector/emitter).
///
/// Field names use row-first notation: `bc` means row=base, col=collector.
#[derive(Debug, Clone)]
pub struct NodeTripletStamp {
    pub bb: Option<usize>,
    pub cc: Option<usize>,
    pub ee: Option<usize>,
    pub bc: Option<usize>,
    pub cb: Option<usize>,
    pub ce: Option<usize>,
    pub ec: Option<usize>,
    pub be: Option<usize>,
    pub eb: Option<usize>,
}

impl NodeTripletStamp {
    /// Create a stamp with no indices assigned yet.
    pub fn uninitialized() -> Self {
        Self {
            bb: None,
            bc: None,
            be: None,
            cb: None,
            cc: None,
            ce: None,
            eb: None,
            ec: None,
            ee: None,
        }
    }

    /// Set the temporary indices produced during pattern construction.
    ///
    /// For sparse (KLU), these are COO entry indices returned by the pattern builder.
    /// For dense (BLAS), these can be the final dense linear indices.
    pub fn set_temp_indices(
        &mut self,
        bb: Option<usize>,
        bc: Option<usize>,
        be: Option<usize>,
        cb: Option<usize>,
        cc: Option<usize>,
        ce: Option<usize>,
        eb: Option<usize>,
        ec: Option<usize>,
        ee: Option<usize>,
    ) {
        self.bb = bb;
        self.bc = bc;
        self.be = be;
        self.cb = cb;
        self.cc = cc;
        self.ce = ce;
        self.eb = eb;
        self.ec = ec;
        self.ee = ee;
    }

    /// Compute and set temporary indices from node locations.
    ///
    /// The `entry` callback receives (row, column) to match the row-first field naming.
    pub fn set_temp_indices_from_nodes<F, E>(
        &mut self,
        b: Option<usize>,
        c: Option<usize>,
        e: Option<usize>,
        mut entry: F,
    ) -> Result<(), E>
    where
        F: FnMut(usize, usize) -> Result<usize, E>,
    {
        let mut maybe = |row: Option<usize>, col: Option<usize>| -> Result<Option<usize>, E> {
            match (row, col) {
                (Some(r), Some(c)) => Ok(Some(entry(r, c)?)),
                _ => Ok(None),
            }
        };

        let bb = maybe(b, b)?;
        let bc = maybe(b, c)?;
        let be = maybe(b, e)?;
        let cb = maybe(c, b)?;
        let cc = maybe(c, c)?;
        let ce = maybe(c, e)?;
        let eb = maybe(e, b)?;
        let ec = maybe(e, c)?;
        let ee = maybe(e, e)?;

        self.set_temp_indices(bb, bc, be, cb, cc, ce, eb, ec, ee);
        Ok(())
    }

    /// Map temporary indices to their final locations using the provided mapping.
    pub fn set_final_indices<F>(&mut self, mut f: F)
    where
        F: FnMut(usize) -> usize,
    {
        self.bb = self.bb.map(|i| f(i));
        self.bc = self.bc.map(|i| f(i));
        self.be = self.be.map(|i| f(i));
        self.cb = self.cb.map(|i| f(i));
        self.cc = self.cc.map(|i| f(i));
        self.ce = self.ce.map(|i| f(i));
        self.eb = self.eb.map(|i| f(i));
        self.ec = self.ec.map(|i| f(i));
        self.ee = self.ee.map(|i| f(i));
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
    /// Create a stamp with no indices assigned yet.
    pub fn uninitialized() -> Self {
        Self {
            pos_branch: None,
            neg_branch: None,
            branch_branch: usize::MAX,
        }
    }

    /// Set the temporary indices produced during pattern construction.
    ///
    /// For sparse (KLU), these are COO entry indices returned by the pattern builder.
    /// For dense (BLAS), these can be the final dense linear indices.
    pub fn set_temp_indices(
        &mut self,
        pos_branch: Option<(usize, usize)>,
        neg_branch: Option<(usize, usize)>,
        branch_branch: usize,
    ) {
        self.pos_branch = pos_branch;
        self.neg_branch = neg_branch;
        self.branch_branch = branch_branch;
    }

    /// Compute and set temporary indices from node and branch locations.
    ///
    /// The `entry` callback receives (column, row) to match `MatrixBuilder::push`.
    pub fn set_temp_indices_from_nodes<F, E>(
        &mut self,
        pos: Option<usize>,
        neg: Option<usize>,
        branch: usize,
        mut entry: F,
    ) -> Result<(), E>
    where
        F: FnMut(usize, usize) -> Result<usize, E>,
    {
        let pos_branch = pos
            .map(|p| Ok((entry(p, branch)?, entry(branch, p)?)))
            .transpose()?;
        let neg_branch = neg
            .map(|n| Ok((entry(n, branch)?, entry(branch, n)?)))
            .transpose()?;
        let branch_branch = entry(branch, branch)?;

        self.set_temp_indices(pos_branch, neg_branch, branch_branch);
        Ok(())
    }

    /// Map temporary indices to their final locations using the provided mapping.
    pub fn set_final_indices<F>(&mut self, mut f: F)
    where
        F: FnMut(usize) -> usize,
    {
        self.pos_branch = self
            .pos_branch
            .map(|(pos_branch, branch_pos)| (f(pos_branch), f(branch_pos)));
        self.neg_branch = self
            .neg_branch
            .map(|(neg_branch, branch_neg)| (f(neg_branch), f(branch_neg)));
        if self.branch_branch != usize::MAX {
            self.branch_branch = f(self.branch_branch);
        }
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
    /// Create a stamp with no indices assigned yet.
    pub fn uninitialized() -> Self {
        Self {
            pos_branch: None,
            neg_branch: None,
        }
    }

    /// Set the temporary indices produced during pattern construction.
    ///
    /// For sparse (KLU), these are COO entry indices returned by the pattern builder.
    /// For dense (BLAS), these can be the final dense linear indices.
    pub fn set_temp_indices(
        &mut self,
        pos_branch: Option<(usize, usize)>,
        neg_branch: Option<(usize, usize)>,
    ) {
        self.pos_branch = pos_branch;
        self.neg_branch = neg_branch;
    }

    /// Compute and set temporary indices from node and branch locations.
    ///
    /// The `entry` callback receives (column, row) to match `MatrixBuilder::push`.
    pub fn set_temp_indices_from_nodes<F, E>(
        &mut self,
        pos: Option<usize>,
        neg: Option<usize>,
        branch: usize,
        mut entry: F,
    ) -> Result<(), E>
    where
        F: FnMut(usize, usize) -> Result<usize, E>,
    {
        let pos_branch = pos
            .map(|p| Ok((entry(p, branch)?, entry(branch, p)?)))
            .transpose()?;
        let neg_branch = neg
            .map(|n| Ok((entry(n, branch)?, entry(branch, n)?)))
            .transpose()?;

        self.set_temp_indices(pos_branch, neg_branch);
        Ok(())
    }

    /// Map temporary indices to their final locations using the provided mapping.
    pub fn set_final_indices<F>(&mut self, mut f: F)
    where
        F: FnMut(usize) -> usize,
    {
        self.pos_branch = self
            .pos_branch
            .map(|(pos_branch, branch_pos)| (f(pos_branch), f(branch_pos)));
        self.neg_branch = self
            .neg_branch
            .map(|(neg_branch, branch_neg)| (f(neg_branch), f(branch_neg)));
    }
}
