// SPDX-License-Identifier: BSD-3-Clause
//
// This file is a Rust port / adaptation based on the SuiteSparse AMD sources by
// Timothy A. Davis and collaborators.
//
// AMD, Copyright (c) 1996-2022, Timothy A. Davis, Patrick R. Amestoy, and
// Iain S. Duff.  All Rights Reserved.
//
// Modifications/porting for this project:
// Copyright (c) 2025 Ido Ben Amram

/// Approximate Minimum Degree (AMD)
/// the algorithm is described in the paper:
/// An Approximate Minimum Degree Ordering Algorithm
/// by Timothy A. Davis and Iain S. Duff
///
/// Timothy A. Davis implements the algorithm
/// here: https://github.com/DrTimothyAldenDavis/SuiteSparse/blob/dev/AMD/Source/amd_2.c
/// the code is extensively documented but is not very easy to understand.
///
use crate::solver::utils::{flip, inverse_permutation};

pub struct AmdControl {
    /// If true, then aggressive absorption is performed.
    aggressive: bool,

    /// base multipler for sqrt(n) to determine the dense threshold
    dense: usize,
}

impl Default for AmdControl {
    fn default() -> Self {
        Self {
            aggressive: true,
            dense: 10,
        }
    }
}

impl AmdControl {
    /// A row is "dense" if the number of entries exceeds the value
    /// returned by get_dense. A row with 16 or fewer entries is never considered "dense".
    fn get_dense(&self, n: usize) -> usize {
        let dense = (self.dense as f64 * (n as f64).sqrt()) as usize;
        dense.max(16).min(n)
    }
}

pub struct AmdInfo {
    // number of non-zero entries in L (excluding the diagonal)
    pub lnz: f64,
    // number of divide operations for LDL' and for LU
    pub ndiv: f64,
    // number of multiply-subtract pairs for LDL'
    pub nms_ldl: f64,
    // number of multiply-subtract pairs for LU
    pub nms_lu: f64,
    // number of "dense" rows/columns
    pub ndense: usize,
    // largest front is dmax-by-dmax
    pub dmax: f64,
    // number of garbage collections in AMD
    // TODO: support this
    pub ncmpa: usize,
}

impl AmdInfo {
    pub fn new() -> Self {
        Self {
            lnz: 0.0,
            ndiv: 0.0,
            nms_ldl: 0.0,
            nms_lu: 0.0,
            ndense: 0,
            dmax: 1.0,
            ncmpa: 0,
        }
    }
}

impl Default for AmdInfo {
    fn default() -> Self {
        Self::new()
    }
}

const EMPTY: isize = -1;

fn clear_flag(mut wflg: isize, wbig: isize, w: &mut [isize], n: usize) -> isize {
    if wflg < 2 || wflg >= wbig {
        for i in 0..n {
            if w[i] != 0 {
                w[i] = 1;
            }
        }
        wflg = 2;
    }
    // at this point, W [0..n-1] < wflg holds
    wflg
}

fn add_to_degree_list(
    i: usize,
    n: usize,
    deg: usize,
    head: &mut [isize],
    last: &mut [isize],
    next: &mut [isize],
) {
    let inext = head[deg];
    debug_assert!(inext >= EMPTY && inext < n as isize);
    if inext != EMPTY {
        last[inext as usize] = i as isize;
    }
    next[i] = inext;
    last[i] = EMPTY;
    head[deg] = i as isize;
}

fn remove_head_from_degree_list(
    i: usize,
    n: usize,
    deg: usize,
    head: &mut [isize],
    last: &mut [isize],
    next: &mut [isize],
) {
    let inext = next[i];
    debug_assert!(inext >= EMPTY && inext < n as isize);
    if inext != EMPTY {
        last[inext as usize] = EMPTY;
    }
    head[deg] = inext;
}

fn remove_from_degree_list(
    i: usize,
    n: usize,
    deg: usize,
    head: &mut [isize],
    last: &mut [isize],
    next: &mut [isize],
) {
    let inext = next[i];
    let ilast = last[i];
    debug_assert!(inext >= EMPTY && inext < n as isize);
    debug_assert!(ilast >= EMPTY && ilast < n as isize);
    if inext != EMPTY {
        last[inext as usize] = ilast;
    }
    if ilast != EMPTY {
        next[ilast as usize] = inext;
    } else {
        // i is at the head of the degree list
        debug_assert!(deg < n);
        head[deg] = inext;
    }
}

// temporarly use the degree list to store the hash list for variables
// same hash means the might be able to be joined into a single supervariable
fn add_hash_to_degree_list(
    i: usize,
    hash: usize, // a "hash" key for a variables,
    //  basically if two variables have the same incident elements and variables
    // they will have the same hash key
    head: &mut [isize],
    last: &mut [isize],
    next: &mut [isize],
) {
    let j = head[hash];
    // if the degree list for jis empty we can use it
    if j <= EMPTY {
        // degree list is empty, hash head is FLIP (j)
        next[i] = flip(j);
        head[hash] = flip(i as isize);
    } else {
        // degree list is not empty, use Last[Head[hash]] as hash head.
        next[i] = last[j as usize];
        last[j as usize] = i as isize;
    }
    // store the hash key for i in Last[i]
    last[i] = hash as isize;
}

fn initialize_amd(
    n: usize,
    control: &AmdControl,
    last: &mut [isize],
    head: &mut [isize],
    next: &mut [isize],
    nv: &mut [isize],
    w: &mut [isize],
    elen: &mut [isize],
    degree: &mut [isize],
    len: &mut [usize],
    pe: &mut [isize],
) -> (usize, usize) {
    // all lists are empty at the start
    last.fill(EMPTY);
    head.fill(EMPTY);
    next.fill(EMPTY);

    // the size of all supervariables at the start is just 1 (simple variable)
    nv.fill(1);
    // no variables have been marked yet
    w.fill(1);
    // no variables are part of elements at the start
    elen.fill(0);

    // number of eliminated variables
    let mut nel = 0;
    // number of dense rows
    let mut ndense = 0;
    let dense = control.get_dense(n);

    /*
    * 	Principal supervariable: live representative of a group. It can be chosen as the next pivot.

        Code test: Nv[i] > 0. It sits in degree buckets; Pe[i] points to its structure; Elen[i] ≥ 0 (count of incident elements).

        Non-principal variable: not eligible to pivot and ignored by the kernel. Two cases:
        1.	Absorbed/merged: i was merged into another principal.
            Code: Nv[i] = 0, Pe[i] = FLIP(parent). It will be numbered with its parent’s supernode; it has a parent in the assembly tree.
        2.	Dense parked: i exceeded the dense threshold and is excluded from ordering.
            Code: Nv[i] = 0, Pe[i] = EMPTY, Elen[i] = EMPTY. It has no parent and is appended at the end of the final permutation; it is not postordered.
    */
    for i in 0..n {
        degree[i] = len[i] as isize;
        debug_assert!(degree[i] >= 0 && degree[i] < n as isize);
        let deg = degree[i] as usize;
        if deg == 0 {
            // we have a variable that can be eliminated at once because
            // there is no off-diagonal non-zero in its row.  Note that
            // Nv [i] = 1 for an empty variable i.  It is treated just
            // the same as an eliminated element i.
            elen[i] = flip(1);
            nv[i] = 1;
            nel += 1;
            pe[i] = EMPTY;
            w[i] = 0;
        } else if deg > dense {
            // dense variable, treat as unordered non-principal variables that have no parent.
            // they do not take part in the postorder, since Nv [i] = 0.
            ndense += 1;
            // non-principal
            nv[i] = 0;
            elen[i] = EMPTY;
            nel += 1;
            pe[i] = EMPTY;
        } else {
            // place i in the degree list corresponding to its degree
            add_to_degree_list(i, n, deg, head, last, next);
        }
    }

    (nel, ndense)
}

// find next supervariable for elimination
fn get_pivot_of_minimum_degree(mindeg: &mut usize, n: usize, head: &[isize]) -> usize {
    let mut me = EMPTY;

    debug_assert!(*mindeg < n);

    let mut deg = *mindeg;
    while deg < n {
        me = head[deg];
        if me != EMPTY {
            break;
        }
        deg += 1;
    }
    *mindeg = deg;
    debug_assert!(me >= 0 && me < n as isize);
    me as usize
}

/// search for different supervariables and add them to the
/// new list, compressing when necessary. this loop is
/// executed once for each element in the list and once for
/// all the supervariables in the list.
fn add_neighboring_supervariables_to_pivot(
    // element we are searching
    e: usize,
    // start of the current element
    pme1: &mut usize,
    knt1: usize,
    // length of the element
    ln: usize,
    p: &mut usize,
    // index of the first supervariable in the element
    pj: &mut usize,
    // degree of the new element (or |Lme|)
    degme: &mut usize,

    // index of the first free position in Iw
    pfree: &mut usize,

    n: usize,
    me: usize,
    iwlen: usize,
    elen: &mut [isize],
    nv: &mut [isize],
    pe: &mut [isize],
    len: &mut [usize],
    iw: &mut [isize],
    head: &mut [isize],
    last: &mut [isize],
    next: &mut [isize],
    degree: &mut [isize],
) {
    for knt2 in 1..=ln {
        debug_assert!(iw[*pj] >= 0 && iw[*pj] < n as isize);
        let i = iw[*pj] as usize;
        *pj += 1;
        debug_assert!(i == me || elen[i] >= EMPTY);
        let nvi = nv[i];
        // if we have already seen this supervariable
        // or it is not a principal variable, skip it
        if nvi > 0 {
            // compress Iw, if necessary
            if *pfree >= iwlen {
                // prepare for compressing Iw by adjusting pointers
                // and lengths so that the lists being searched in
                // the inner and outer loops contain only the
                // remaining entries.
                pe[me] = (*p) as isize;
                len[me] -= knt1;
                if len[me] == 0 {
                    pe[me] = EMPTY;
                }
                pe[e] = (*pj) as isize;
                len[e] = ln - knt2;
                // nothing left of element e
                if len[e] == 0 {
                    pe[e] = EMPTY;
                }

                for j in 0..n {
                    let pn = pe[j];
                    if pn >= 0 {
                        debug_assert!(pn >= 0 && pn < iwlen as isize);
                        pe[j] = iw[pn as usize];
                        iw[pn as usize] = flip(j as isize);
                    }
                }

                let mut psrc = 0;
                let mut pdst = 0;
                let pend = (*pme1 as isize) - 1;

                while psrc <= pend {
                    let j = flip(iw[psrc as usize]);
                    psrc += 1;
                    if j >= 0 {
                        iw[pdst] = pe[j as usize];
                        pe[j as usize] = pdst as isize;
                        pdst += 1;
                        let lenj = len[j as usize] as isize;
                        // copy from source to destination
                        for _ in 0..=(lenj - 2) {
                            iw[pdst] = iw[psrc as usize];
                            psrc += 1;
                            pdst += 1;
                        }
                    }
                }

                // move the new partially-constructed element
                let p1 = pdst;
                for psrc in (*pme1)..=((*pfree) - 1) {
                    iw[pdst] = iw[psrc];
                    pdst += 1;
                }
                *pme1 = p1;
                *pfree = pdst;
                *pj = pe[e] as usize;
                *p = pe[me] as usize;
            }

            // add supervariable i to Lme
            *degme += nvi as usize;
            nv[i] = -nvi;
            iw[*pfree] = i as isize;
            *pfree += 1;

            let deg = degree[i] as usize;
            remove_from_degree_list(i, n, deg, head, last, next);
        }
    }
}

// the supervariable me will be converted into the current element.
// at the end Lme (list of supervariables neighboring the **element** me) will be
// contained in Iw [pme1 .. pme2]
// also degme holds the external degree |Lme| of new element
fn construct_new_element(
    me: usize,
    nel: &mut usize,
    n: usize,
    pfree: &mut usize,
    iwlen: usize,
    elen: &mut [isize],
    // degme holds the external degree of new element (or |Lme|)
    degme: &mut usize,
    nv: &mut [isize],
    pe: &mut [isize],
    iw: &mut [isize],
    len: &mut [usize],
    degree: &mut [isize],
    w: &mut [isize],
    // degree list
    head: &mut [isize],
    last: &mut [isize],
    next: &mut [isize],
) -> (usize, usize, usize, isize) {
    let elenme = elen[me];
    debug_assert!(nv[me] > 0);
    let nvpiv = nv[me] as usize;
    *nel += nvpiv;

    // flag the variable "me" as being in Lme by negating Nv [me]
    nv[me] = -(nvpiv as isize);
    debug_assert!(pe[me] >= 0 && pe[me] < iwlen as isize);
    let mut pme1: usize;
    let mut pme2: isize;

    if elenme == 0 {
        // construct the new element in place
        pme1 = pe[me] as usize;
        pme2 = (pme1 as isize) - 1;
        for p in pme1..=(pme1 + len[me] - 1) {
            debug_assert!(iw[p] >= 0 && iw[p] < n as isize);
            let i = iw[p] as usize;
            let nvi = nv[i];

            // i is a principal variable not yet placed in Lme.
            if nvi > 0 {
                *degme += nvi as usize;
                // flag i as being in Lme by negating Nv [i]
                nv[i] = -nvi;
                pme2 += 1;
                iw[pme2 as usize] = i as isize;

                let deg = degree[i] as usize;
                remove_from_degree_list(i, n, deg, head, last, next);
            }
        }
    } else {
        // construct the new element in empty space, Iw [pfree ...]
        let mut p = pe[me] as usize;
        pme1 = *pfree;
        // length of neighboring supervariables of me
        let slenme = len[me] - elenme as usize;

        for knt1 in 1..=elenme + 1 {
            let e: usize;
            let mut pj: usize;
            let ln: usize;

            if knt1 > elenme {
                // search the supervariables in me.
                e = me;
                pj = p;
                ln = slenme;
            } else {
                // search the elements in me.
                debug_assert!(iw[p] >= 0 && iw[p] < n as isize);
                e = iw[p] as usize;
                p += 1;
                debug_assert!(pe[e] >= 0);
                pj = pe[e] as usize;
                // debug_assert!(len[e] >= 0);
                ln = len[e];
                debug_assert!(elen[e] < EMPTY && w[e] > 0);
            }
            debug_assert!(ln == 0 || (pj < iwlen));

            add_neighboring_supervariables_to_pivot(
                e,
                &mut pme1,
                knt1 as usize,
                ln,
                &mut p,
                &mut pj,
                degme,
                pfree,
                n,
                me,
                iwlen,
                elen,
                nv,
                pe,
                len,
                iw,
                head,
                last,
                next,
                degree,
            );

            if e != me {
                // element e is absorbed into me, as all supervariables in e
                // are now in Lme
                // set me as the parent of e (a negative value in pe)
                pe[e] = flip(me as isize);
                // flag e as being absorbed by me
                w[e] = 0;
            }
        }
        pme2 = (*pfree - 1) as isize;
    }

    degree[me] = *degme as isize;
    debug_assert!(pme1 < iwlen);
    pe[me] = pme1 as isize;
    len[me] = (pme2 - pme1 as isize + 1) as usize;

    // flip(elen[me]) is now the degree of pivot (including
    // diagonal part).
    elen[me] = flip((nvpiv + *degme) as isize);

    (pme1, pme2 as usize, nvpiv, elenme)
}

// this is the "algorithm 2" from the paper.
// for each supervariable in Lme, we want to compute the upper bound of the external degree.
// to do this, we need to compute the "outside degree" of the supervariable.
// outside degree == all of the neighboring supervariables to i that are not in lme
// or |Le\Lme| for element e in the E_i set
// the value will be stored in the w array, as |Le\Lme| = w[e] - wflg
// The notation Le refers to the pattern (list of supervariables) of a
// previous element e, where e is not yet absorbed, stored in
// iw[pe[e] + 1 ... pe[e] + len[e]].  The notation Lme
// refers to the pattern of the current element (stored in
// iw[pme1..pme2]).   If aggressive absorption is enabled, and
// (w[e] - wflg) becomes zero, then the element e will be absorbed
// in Scan 2.
fn compute_outside_degrees(
    pme1: usize,
    pme2: usize,
    n: usize,
    iwlen: usize,
    pe: &[isize],
    iw: &[isize],
    elen: &[isize],
    nv: &[isize],
    degree: &[isize],
    wflg: isize,
    w: &mut [isize],
) {
    for pme in pme1..=pme2 {
        // index of the supervariable in Lme
        debug_assert!(iw[pme] >= 0 && iw[pme] < n as isize);
        let i = iw[pme] as usize;

        // if the supervariable has elements in E_i
        if elen[i] > 0 {
            let eln = elen[i] as usize;
            // note that nv[i] has been negated to denote i in Lme
            debug_assert!(-nv[i] > 0);
            let nvi = -nv[i];
            debug_assert!(pe[i] >= 0 && pe[i] < iwlen as isize);

            let wnvi = wflg - nvi;

            // loop through the elements in E_i
            for p in pe[i] as usize..=(pe[i] as usize + eln - 1) {
                debug_assert!(iw[p] >= 0 && iw[p] < n as isize);
                let e = iw[p] as usize;
                let mut we = w[e];
                // we is 0 for absorbed elements
                // we >= wflg if we have already seen e in the scan
                // otherwise this is the first time we have seen e in the scan

                // each time we are going to remove nvi from the degree of e
                // because i is already accounted for in Lme
                if we >= wflg {
                    // unabsorbed element e has been seen in this loop
                    we -= nvi;
                } else if we != 0 {
                    // element e is not absorbed by i
                    // this is the first we have seen e in all of Scan 1
                    we = degree[e] + wnvi;
                }
                w[e] = we;
            }
        }
    }
}

fn update_degrees(
    me: usize,
    pme1: usize,
    pme2: usize,
    nvpiv: &mut usize,
    nel: &mut usize,
    n: usize,
    iwlen: usize,
    degme: &mut usize,
    pe: &mut [isize],
    iw: &mut [isize],
    nv: &mut [isize],
    elen: &mut [isize],
    len: &mut [usize],
    degree: &mut [isize],
    w: &mut [isize],
    wflg: isize,
    head: &mut [isize],
    last: &mut [isize],
    next: &mut [isize],
    aggressive: bool,
) {
    for pme in pme1..=pme2 {
        debug_assert!(iw[pme] >= 0 && iw[pme] < n as isize);
        let i = iw[pme] as usize;
        // i is in Lme and has an element list associated with it
        debug_assert!(nv[i] < 0 && elen[i] >= 0);
        debug_assert!(pe[i] >= 0 && pe[i] < iwlen as isize);
        let p1 = pe[i] as usize;
        let p2 = p1 + elen[i] as usize;
        debug_assert!(p2 < iwlen);
        let mut pn = p1;
        let mut hash = 0;
        let mut deg = 0;

        // go over all elements in i and update the degree of i (absorbing when possible)
        if aggressive {
            for p in p1..p2 {
                debug_assert!(iw[p] >= 0 && iw[p] < n as isize);
                let e = iw[p] as usize;
                let we = w[e];
                if we != 0 {
                    // e is an unabsorbed element
                    // dext = | Le \ Lme |
                    let dext = we - wflg;
                    // dext > 0 means the element still has
                    // some supervairables that are not in Lme
                    if dext > 0 {
                        deg += dext as usize;
                        iw[pn] = e as isize;
                        pn += 1;
                        hash += e;
                    } else {
                        // external degree of e is zero, aggressive absorb e into me
                        // set me as the parent of e (a negative value in pe)
                        pe[e] = flip(me as isize);
                        // flag e as being absorbed by me
                        w[e] = 0;
                    }
                }
            }
        } else {
            for p in p1..p2 {
                debug_assert!(iw[p] >= 0 && iw[p] < n as isize);
                let e = iw[p] as usize;
                let we = w[e];
                if we != 0 {
                    // e is an unabsorbed element
                    let dext = we - wflg;
                    debug_assert!(dext >= 0);
                    deg += dext as usize;
                    // this will essentially remove absorbed elements from
                    // the element list of i
                    iw[pn] = e as isize;
                    pn += 1;
                    hash += e;
                }
            }
        }

        // count the number of elements in i (including me):
        elen[i] = (pn - p1 + 1) as isize;

        // scan the supervariables in the list associated with i
        let p3 = pn;
        let p4 = p1 + len[i];
        for p in p2..p4 {
            debug_assert!(iw[p] >= 0 && iw[p] < n as isize);
            let j = iw[p] as usize;
            let nvj = nv[j];

            if nvj > 0 {
                // j is unabsorbed, and not in Lme.
                // add to degree and add to new list
                deg += nvj as usize;
                iw[pn] = j as isize;
                pn += 1;
                hash += j;
            }
        }

        // update the degree and check for mass elimination
        debug_assert!(!aggressive || ((deg == 0) == (elen[i] == 1 && p3 == pn)));

        if elen[i] == 1 && p3 == pn {
            // mass elimination
            // there is nothing left of this node except for an edge to
            // the pivot (me).
            // see original code for an in depth explanation

            // set me as the parent of i (a negative value in pe)
            pe[i] = flip(me as isize);
            let nvi = (-nv[i]) as usize;
            *degme -= nvi;
            *nvpiv += nvi;
            *nel += nvi;
            nv[i] = 0;
            elen[i] = EMPTY;
        } else {
            // update the upper-bound degree of i

            // the following degree does not yet include the size
            // of the current element, which is added later:
            degree[i] = isize::min(degree[i], deg as isize);

            // add me to the list for i

            // move first supervariable to end of list
            iw[pn] = iw[p3];
            //move first element to end of element part of list
            iw[p3] = iw[p1];
            // add new element, me, to front of list.
            iw[p1] = me as isize;
            // store the new length of the list in Len [i]
            len[i] = pn - p1 + 1;

            // place in hash bucket.  Save hash key of i in Last [i].

            hash %= n;
            debug_assert!((hash as isize) >= 0 && (hash as isize) < n as isize);

            add_hash_to_degree_list(i, hash, head, last, next);
        }

        degree[me] = (*degme) as isize;
    }
}

fn supervairable_detection(
    pme1: usize,
    pme2: usize,
    n: usize,
    iwlen: usize,
    pe: &mut [isize],
    elen: &mut [isize],
    len: &[usize],
    iw: &[isize],
    nv: &mut [isize],
    head: &mut [isize],
    next: &mut [isize],
    last: &mut [isize],
    w: &mut [isize],
    wflg: &mut isize,
) {
    for pme in pme1..=pme2 {
        debug_assert!(iw[pme] >= 0 && iw[pme] < n as isize);
        let mut i = iw[pme];

        // we set all supervariables to negative in nv
        // to denote that they are in Lme
        if nv[i as usize] < 0 {
            // i is a principal variable in Lme

            // examine all hash buckets with 2 or more variables.

            debug_assert!(last[i as usize] >= 0 && last[i as usize] < n as isize);
            // the hash key for i is stored in Last[i]
            let hash = last[i as usize] as usize;

            let mut j = head[hash];
            if j == EMPTY {
                // hash bucket and degree list are both empty
                i = EMPTY;
            } else if j < EMPTY {
                // degree list is empty
                i = flip(j);
                head[hash] = EMPTY;
            } else {
                i = last[j as usize];
                last[j as usize] = EMPTY;
            }

            let mut jlast;
            debug_assert!(i >= EMPTY && i < n as isize);
            while i != EMPTY && next[i as usize] != EMPTY {
                let ln = len[i as usize];
                debug_assert!(elen[i as usize] >= 0);
                let eln = elen[i as usize];
                debug_assert!(pe[i as usize] >= 0 && pe[i as usize] < iwlen as isize);
                let p1 = pe[i as usize] as usize;
                let p2 = (p1 + ln - 1) as isize;

                // skip the first element in the list (me)
                for p in (p1 + 1) as isize..=p2 {
                    debug_assert!(iw[p as usize] >= 0 && iw[p as usize] < n as isize);
                    w[iw[p as usize] as usize] = *wflg;
                }

                jlast = i;
                j = next[i as usize];
                debug_assert!(j >= EMPTY && j < n as isize);

                while j != EMPTY {
                    // check if j and i have identical nonzero pattern

                    debug_assert!(elen[j as usize] >= 0);
                    debug_assert!(pe[j as usize] >= 0 && pe[j as usize] < iwlen as isize);
                    let mut ok = (len[j as usize] == ln) && (elen[j as usize] == eln);
                    let p1 = pe[j as usize] as usize;
                    let p2 = (p1 + ln - 1) as isize;

                    // skip the first element in the list (me)
                    for p in (p1 + 1) as isize..=p2 {
                        debug_assert!(iw[p as usize] >= 0 && iw[p as usize] < n as isize);
                        if w[iw[p as usize] as usize] != *wflg {
                            ok = false;
                            break;
                        }
                    }

                    if ok {
                        // j can be absorbed into i
                        pe[j as usize] = flip(i);
                        // both Nv [i] and Nv [j] are negated since they
                        // are in Lme, and the absolute values of each
                        // are the number of variables in i and j:
                        nv[i as usize] += nv[j as usize];
                        nv[j as usize] = 0;
                        elen[j as usize] = EMPTY;
                        // delete j from hash bucket
                        debug_assert!(j != next[j as usize]);
                        j = next[j as usize];
                        next[jlast as usize] = j;
                    } else {
                        jlast = j;
                        debug_assert!(j != next[j as usize]);
                        j = next[j as usize];
                    }
                    debug_assert!(j >= EMPTY && j < n as isize);
                }

                // no more variables can be absorbed into i
                // go to next i in bucket and clear flag array

                *wflg += 1;
                i = next[i as usize];
                debug_assert!(i >= EMPTY && i < n as isize);
            }
        }
    }
}

fn restore_degree_list(
    pme1: usize,
    pme2: usize,
    n: usize,
    nel: usize,
    degme: usize,
    nv: &mut [isize],
    iw: &mut [isize],
    head: &mut [isize],
    last: &mut [isize],
    next: &mut [isize],
    degree: &mut [isize],
    aggressive: bool,
    mindeg: &mut usize,
) -> usize {
    let mut p = pme1;
    let nleft = n - nel;
    for pme in pme1..=pme2 {
        debug_assert!(iw[pme] >= 0 && iw[pme] < n as isize);
        let i = iw[pme] as usize;

        let nvi = -nv[i];
        if nvi > 0 {
            // i is a principal variable in Lme
            // restore Nv [i] to signify that i is principal
            nv[i] = nvi;

            // compute the external degree (ass the size of current element)
            let mut deg = degree[i] + degme as isize - nvi;
            deg = isize::min(deg, nleft as isize - nvi);
            debug_assert!((!aggressive || (deg > 0)) && deg >= 0 && deg < n as isize);
            let deg = deg as usize;

            // place the supervariable at the head of the degree list
            add_to_degree_list(i, n, deg, head, last, next);

            // save the new degree, and find the minimum degree
            degree[i] = deg as isize;
            *mindeg = usize::min(*mindeg, deg);

            // place the supervariable in the element pattern
            iw[p] = i as isize;
            p += 1;
        }
    }
    p
}

fn finalize_new_element(
    me: usize,
    nvpiv: usize,
    pme1: usize,
    p: usize,
    pfree: &mut usize,
    nv: &mut [isize],
    len: &mut [usize],
    pe: &mut [isize],
    w: &mut [isize],
    elenme: isize,
    _info: &mut AmdInfo,
) {
    nv[me] = nvpiv as isize;
    len[me] = p - pme1;
    if len[me] == 0 {
        // there is nothing left of the current pivot element
        // it is a root of the assembly tree
        pe[me] = EMPTY;
        w[me] = 0;
    }
    if elenme != 0 {
        // element was not constructed in place: deallocate part of
        // it since newly nonprincipal variables may have been removed
        *pfree = p;
    }
}

/*
 * Variables at this point:
 * - pe: holds the elimination tree.  The parent of j is flip(pe[j]),
 *	or EMPTY if j is a root.  The tree holds both elements and
 *	non-principal (unordered) variables absorbed into them.
 *	Dense variables are non-principal and unordered.
 *
 * elen: holds the size of each element, including the diagonal part.
 *	flip(elen[e]) > 0 if e is an element.  For unordered
 *	variables i, elen[i] is EMPTY.
 *
 * nv: nv[e] > 0 is the number of pivots represented by the element e.
 *	For unordered variables i, nv[i] is zero.
 *
 * Contents no longer needed:
 *	w, iw, len, degree, head, next, last.
 *
 * The matrix itself has been destroyed.
 *
 * n: the size of the matrix.
 * No other scalars needed (pfree, iwlen, etc.)
 */
fn compress_paths(n: usize, pe: &mut [isize], elen: &mut [isize], nv: &[isize]) {
    // restore pe
    for i in 0..n {
        pe[i] = flip(pe[i]);
    }

    // restore elen,
    for i in 0..n {
        elen[i] = flip(elen[i]);
    }

    // compress the paths of the variables
    for i in 0..n {
        if nv[i] == 0 {
            // i is an un-ordered row. Traverse the tree from i until
            // reaching an element, e. The element, e, was the principal
            // supervariable of i and all nodes in the path from i to when e
            // was selected as pivot.

            let mut j = pe[i];
            debug_assert!(j >= EMPTY && j < n as isize);
            if j == EMPTY {
                // i is a dense variable. It has no parent.
                continue;
            }

            // while j is a variable
            while nv[j as usize] == 0 {
                j = pe[j as usize];
                debug_assert!(j >= 0 && j < n as isize);
            }

            // got to an element e
            let e = j;

            // traverse the path again from i to e, and compress the path
            // (all nodes point to e).  Path compression allows this code to
            // compute in O(n) time.

            j = i as isize;
            // while j is a variable
            while nv[j as usize] == 0 {
                let jnext = pe[j as usize];
                pe[j as usize] = e;
                j = jnext;
                debug_assert!(j >= 0 && j < n as isize);
            }
        }
    }
}

// postordering of a supernodal elimination tree
fn post_tree(
    // root of a tree
    root: usize,
    // start numbering at k
    mut k: usize,
    // input argument of size nn, undefined on output. child[i] is the head of a link list of all nodes that are children of node i in the tree.
    child: &mut [isize],
    // input argument of size nn, not modified. If f is a node in the link list of the children of node i, then sibiling[f] is the next child of node i.
    sibling: &[isize],
    // output order, of size nn. order[i] = k if node i is the kth node of the reordered tree.
    order: &mut [isize],
    stack: &mut [isize],
    n: usize,
) -> usize {
    let mut head: isize = 0;
    stack[head as usize] = root as isize;

    while head >= 0 {
        debug_assert!(head >= 0 && head < n as isize);
        debug_assert!(stack[head as usize] >= 0 && stack[head as usize] < n as isize);
        let i = stack[head as usize] as usize;

        if child[i] != EMPTY {
            // the children of i are not yet ordered
            // push each child onto the stack in reverse order
            // so that small ones at the head of the list get popped first
            // and the biggest one at the end of the list gets popped last
            let mut f = child[i];
            while f != EMPTY {
                head += 1;
                debug_assert!((head as usize) < n);
                debug_assert!(f >= 0 && f < n as isize);
                f = sibling[f as usize];
            }

            let mut h = head;
            debug_assert!((head as usize) < n);
            f = child[i];
            while f != EMPTY {
                debug_assert!(h > 0);
                stack[h as usize] = f;
                h -= 1;
                debug_assert!(f >= 0 && f < n as isize);
                f = sibling[f as usize];
            }

            debug_assert!(stack[h as usize] == i as isize);

            // delete child list so that i gets ordered next time we see it
            child[i] = EMPTY;
        } else {
            // the children of i (if there were any) are already ordered
            // remove i from the stack and order it.  Front i is kth front
            head -= 1;
            order[i] = k as isize;
            k += 1;
            debug_assert!(k <= n);
        }
    }

    k
}

// perform a postordering (via depth-first search) of an assembly tree
fn postorder_assembly_tree(
    // inputs, not modified on output:
    n: usize,
    parent: &[isize],
    nv: &[isize],
    fsize: &[isize],

    // output, not defined on input:
    order: &mut [isize],

    // workspaces of size nn:
    child: &mut [isize],
    sibling: &mut [isize],
    stack: &mut [isize],
) {
    for j in 0..n {
        child[j] = EMPTY;
        sibling[j] = EMPTY;
    }

    // place the children in link lists - bigger elements tend to be last

    let mut j = (n - 1) as isize;
    while j >= 0 {
        if nv[j as usize] > 0 {
            let parent = parent[j as usize];
            if parent != EMPTY {
                // place the element in link list of the children its parent
                sibling[j as usize] = child[parent as usize];
                child[parent as usize] = j;
            }
        }
        j -= 1;
    }

    // place the larget child last in the list of children for each node
    for i in 0..n {
        if nv[i] > 0 && child[i] != EMPTY {
            let mut fprev = EMPTY;
            let mut maxfrsize = EMPTY;
            let mut bigfprev = EMPTY;
            let mut bigf = EMPTY;

            let mut f = child[i];
            while f != EMPTY {
                debug_assert!(f >= 0 && f < n as isize);
                let frsize = fsize[f as usize];

                if frsize >= maxfrsize {
                    // this is the biggest seen so far
                    maxfrsize = frsize;
                    bigfprev = fprev;
                    bigf = f
                }
                fprev = f;
                f = sibling[f as usize];
            }
            debug_assert!(bigf != EMPTY);

            let fnext = sibling[bigf as usize];

            if fnext != EMPTY {
                // if fnext is EMPTY then bigf is already at the end of list

                if bigfprev == EMPTY {
                    // delete bigf from the element of the list
                    child[i] = fnext;
                } else {
                    // delete bigf from the middle of the list
                    sibling[bigfprev as usize] = fnext;
                }

                // put bigf at the end of the list
                sibling[bigf as usize] = EMPTY;
                debug_assert!(child[i] != EMPTY);
                debug_assert!(fprev != bigf);
                debug_assert!(fprev != EMPTY);
                sibling[fprev as usize] = bigf;
            }
        }
    }

    // postorder the assembly tree

    order.fill(EMPTY);

    let mut k = 0;
    for i in 0..n {
        if parent[i] == EMPTY && nv[i] > 0 {
            k = post_tree(i, k, child, sibling, order, stack, n);
        }
    }
}

fn compute_output_permutation(
    n: usize,
    ndense: usize,
    nv: &[isize],
    pe: &[isize],
    head: &mut [isize],
    next: &mut [isize],
    last: &mut [isize],
    w: &[isize],
) {
    for k in 0..n {
        head[k] = EMPTY;
        next[k] = EMPTY;
    }

    for e in 0..n {
        let k = w[e];
        debug_assert!((k == EMPTY) == (nv[e] == 0));
        if k != EMPTY {
            debug_assert!(k >= 0 && k < n as isize);
            head[k as usize] = e as isize;
        }
    }

    // construct output inverse permutation in next,
    // and permutation in last
    let mut nel = 0usize;
    for k in 0..n {
        let e = head[k];
        if e == EMPTY {
            break;
        }
        debug_assert!(e >= 0 && e < n as isize && nv[e as usize] > 0);
        next[e as usize] = nel as isize;
        nel += nv[e as usize] as usize;
    }
    debug_assert!(nel == n - ndense);

    // order non-principal variables (dense, & those merged into supervar's)
    for i in 0..n {
        if nv[i] == 0 {
            let e = pe[i];
            debug_assert!(e >= EMPTY && e < n as isize);
            if e != EMPTY {
                // This is an unordered variable that was merged
                // into element e via supernode detection or mass
                // elimination of i when e became the pivot element.
                // Place i in order just before e.
                debug_assert!(next[i] == EMPTY && nv[e as usize] > 0);
                next[i] = next[e as usize];
                next[e as usize] += 1;
            } else {
                // this is a dense unordered variable, with no parent.
                // Place it last in the output order.
                next[i] = nel as isize;
                nel += 1;
            }
        }
    }
    debug_assert!(nel == n);

    inverse_permutation(n, next, last);
}

pub fn amd(
    n: usize,          // A is n-by-n, where n > 0
    pe: &mut [isize],  // Pe[0..n-1]: index in Iw of row i on input
    iw: &mut [isize],  // workspace of size iwlen. Iw[0..pfree-1] holds the matrix on input
    len: &mut [usize], // Len[0..n-1]: length for row/column i on input
    iwlen: usize,      // length of Iw. iwlen >= pfree + n
    mut pfree: usize,  // Iw[pfree .. iwlen-1] is empty on input

    // 7 size-n workspaces, not defined on input:
    nv: &mut [isize],   // the size of each supernode on output
    next: &mut [isize], // the output inverse permutation
    last: &mut [isize], // the output permutation
    head: &mut [isize],
    elen: &mut [isize], // the size (in columns of L) for each supernode
    degree: &mut [isize],
    w: &mut [isize],

    control: AmdControl,
) -> AmdInfo {
    /* Note that this restriction on iwlen is slightly more restrictive than
     * what is actually required in AMD_2.  AMD_2 can operate with no elbow
     * room at all, but it will be slow.  For better performance, at least
     * size-n elbow room is enforced. */
    debug_assert!(iwlen >= pfree + n);
    debug_assert!(n > 0);
    let mut info = AmdInfo::new();

    let (mut nel, ndense) =
        initialize_amd(n, &control, last, head, next, nv, w, elen, degree, len, pe);
    info.ndense = ndense;

    let wbig = isize::MAX - n as isize;
    let mut wflg = clear_flag(0, wbig, w, n);
    // largest |Le| seen so far
    let mut lemax = 0;

    let mut mindeg = 0;

    while nel < n {
        // the current pivot(supervariable) to be eliminated
        // or the current element being created by eliminating that supervariable
        let me = get_pivot_of_minimum_degree(&mut mindeg, n, head);
        let mut degme = 0usize;
        remove_head_from_degree_list(me, n, mindeg, head, last, next);
        let (pme1, pme2, mut nvpiv, elenme) = construct_new_element(
            me, &mut nel, n, &mut pfree, iwlen, elen, &mut degme, nv, pe, iw, len, degree, w, head,
            last, next,
        );

        // make sure that wflg is not too large.
        wflg = clear_flag(wflg, wbig, w, n);
        compute_outside_degrees(pme1, pme2, n, iwlen, pe, iw, elen, nv, degree, wflg, w);
        update_degrees(
            me,
            pme1,
            pme2,
            &mut nvpiv,
            &mut nel,
            n,
            iwlen,
            &mut degme,
            pe,
            iw,
            nv,
            elen,
            len,
            degree,
            w,
            wflg,
            head,
            last,
            next,
            control.aggressive,
        );

        lemax = usize::max(lemax, degme);
        wflg += lemax as isize;
        wflg = clear_flag(wflg, wbig, w, n);
        // at this point, W [0..n-1] < wflg holds

        supervairable_detection(
            pme1, pme2, n, iwlen, pe, elen, len, iw, nv, head, next, last, w, &mut wflg,
        );

        let p = restore_degree_list(
            pme1,
            pme2,
            n,
            nel,
            degme,
            nv,
            iw,
            head,
            last,
            next,
            degree,
            control.aggressive,
            &mut mindeg,
        );
        finalize_new_element(
            me, nvpiv, pme1, p, &mut pfree, nv, len, pe, w, elenme, &mut info,
        );

        let f = nvpiv as f64;
        let r = degme as f64 + info.ndense as f64;
        info.dmax = f64::max(info.dmax, f + r);

        let lnzme = f * r + (f - 1.) * f / 2.;
        // number of nonzeros in L (excluding the diagonal)
        info.lnz += lnzme;
        // number of divide operations for LDL' and for LU
        info.ndiv += lnzme;
        // number of multiply-subtract pairs for LU
        let s = f * r * r + r * (f - 1.) * f + (f - 1.) * f * (2. * f - 1.) / 6.;
        info.nms_lu += s;
        // number of multiply-subtract pairs for LDL'
        info.nms_ldl += (s + lnzme) / 2.;
    }

    let f = info.ndense as f64;
    info.dmax = f64::max(info.dmax, f);
    let lnzme = (f - 1.) * f / 2.;
    info.lnz += lnzme;
    info.ndiv += lnzme;
    let s = (f - 1.) * f * (2. * f - 1.) / 6.;
    info.nms_lu += s;
    info.nms_ldl += (s + lnzme) / 2.;

    compress_paths(n, pe, elen, nv);
    postorder_assembly_tree(n, pe, nv, elen, w, head, next, last);
    compute_output_permutation(n, ndense, nv, pe, head, next, last, w);

    info
}
