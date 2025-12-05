mod analyze;
mod btf;
mod amd;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KluScale {
    None,
    Sum,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KluOrdering {
    Amd,
}

#[derive(Debug, Clone, Copy)]
struct KluConfig {
    /* pivot tolerance for diagonal preference */
    tol: f64,
    /* realloc memory growth size for LU factors */
    memgrow: f64,
    /* init. memory size with AMD: c*nnz(L) + n */
    initmem_amd: f64,
    /* init. memory size: c*nnz(A) + n */
    initmem: f64,
    /* use BTF pre-ordering, or not */
    btf: bool,
    ordering: KluOrdering,
    scale: KluScale,
    // how to handle a singular matrix:
    // FALSE: keep going.  Return a Numeric object with a zero U(k,k).  A
    //   divide-by-zero may occur when computing L(:,k).  The Numeric object
    //   can be passed to klu_solve (a divide-by-zero will occur).  It can
    //   also be safely passed to klu_refactor.
    // TRUE: stop quickly.  klu_factor will free the partially-constructed
    //   Numeric object.  klu_refactor will not free it, but will leave the
    //   numerical values only partially defined.  This is the default.
    halt_if_singular: bool,
}

struct KluSymbolic {
    ordering: KluOrdering,

    n: usize,
    nz: usize,
    nzoff: usize,
    nblocks: usize,
    maxblock: usize,
    structural_rank: usize,
    symmetry: f64,
    lnz: f64,
    unz: f64,

    lower_nz: Vec<f64>,
    row_permutation: Vec<isize>,
    column_permutation: Vec<isize>,
    // used in btf to hold block boundaries
    row_scaling: Vec<isize>,
}

struct KluNumeric {
    n: usize,
    nblocks: usize,
    lnz: usize,
    unz: usize

}

pub(crate) fn klu_valid(n: usize, column_pointers: &[usize], row_indices: &[usize]) -> bool {
    if n == 0 {
        return false;
    }

    // column pointers must start at column_pointers[0] = 0, and column_pointers[n] must be >= 0
    if column_pointers[0] != 0 {
        return false;
    }

    for j in 0..n {
        let p1 = column_pointers[j];
        let p2 = column_pointers[j + 1];

        // column pointers must be ascending
        if p1 > p2 {
            return false;
        }
        for p in p1..p2 {
            let i = row_indices[p];
            // row index out of range
            if i >= n {
                return false;
            }
        }
    }

    true
}

/*
Int KLU_valid (Int n, Int Ap [ ], Int Ai [ ], Entry Ax [ ])
{
    Int nz, j, p1, p2, i, p ;
    PRINTF (("\ncolumn oriented matrix, n = %d\n", n)) ;
    if (n <= 0)
    {
        PRINTF (("n must be >= 0: %d\n", n)) ;
        return (FALSE) ;
    }
    nz = Ap [n] ;
    if (Ap [0] != 0 || nz < 0)
    {
        /* column pointers must start at Ap [0] = 0, and Ap [n] must be >= 0 */
        PRINTF (("column 0 pointer bad or nz < 0\n")) ;
        return (FALSE) ;
    }
    for (j = 0 ; j < n ; j++)
    {
        p1 = Ap [j] ;
        p2 = Ap [j+1] ;
        PRINTF (("\nColumn: %d p1: %d p2: %d\n", j, p1, p2)) ;
        if (p1 > p2)
        {
            /* column pointers must be ascending */
            PRINTF (("column %d pointer bad\n", j)) ;
            return (FALSE) ;
        }
        for (p = p1 ; p < p2 ; p++)
        {
            i = Ai [p] ;
            PRINTF (("row: %d", i)) ;
            if (i < 0 || i >= n)
            {
                /* row index out of range */
                PRINTF (("index out of range, col %d row %d\n", j, i)) ;
                return (FALSE) ;
            }
            if (Ax != (Entry *) NULL)
            {
                PRINT_ENTRY (Ax [p]) ;
            }
            PRINTF (("\n")) ;
        }
    }
    return (TRUE) ;
}
 */

/*
typedef struct
{
    /* A (P,Q) is in upper block triangular form.  The kth block goes from
     * row/col index R [k] to R [k+1]-1.  The estimated number of nonzeros
     * in the L factor of the kth block is Lnz [k].
     */

    /* only computed if the AMD ordering is chosen: */
    double symmetry ;   /* symmetry of largest block */
    double est_flops ;  /* est. factorization flop count */
    double lnz, unz ;   /* estimated nz in L and U, including diagonals */
    double *Lnz ;       /* size n, but only Lnz [0..nblocks-1] is used */

    /* computed for all orderings: */
    int32_t
        n,              /* input matrix A is n-by-n */
        nz,             /* # entries in input matrix */
        *P,             /* size n */
        *Q,             /* size n */
        *R,             /* size n+1, but only R [0..nblocks] is used */
        nzoff,          /* nz in off-diagonal blocks */
        nblocks,        /* number of blocks */
        maxblock,       /* size of largest block */
        ordering,       /* ordering used (0:AMD, 1:COLAMD, 2:given, ... */
        do_btf ;        /* whether or not BTF preordering was requested */

    /* only computed if BTF preordering requested */
    int32_t structural_rank ;   /* 0 to n-1 if the matrix is structurally rank
                        * deficient.  -1 if not computed.  n if the matrix has
                        * full structural rank */

} klu_symbolic ;


typedef struct klu_common_struct
{

    /* ---------------------------------------------------------------------- */
    /* parameters */
    /* ---------------------------------------------------------------------- */

    double tol ;            /* pivot tolerance for diagonal preference */
    double memgrow ;        /* realloc memory growth size for LU factors */
    double initmem_amd ;    /* init. memory size with AMD: c*nnz(L) + n */
    double initmem ;        /* init. memory size: c*nnz(A) + n */
    double maxwork ;        /* maxwork for BTF, <= 0 if no limit */

    int btf ;               /* use BTF pre-ordering, or not */
    int ordering ;          /* 0: AMD, 1: COLAMD, 2: user P and Q,
                             * 3: user function */
    int scale ;             /* row scaling: -1: none (and no error check),
                             * 0: none, 1: sum, 2: max */

    /* pointer to user ordering function */
    int32_t (*user_order) (int32_t, int32_t *, int32_t *, int32_t *,
        struct klu_common_struct *) ;

    /* pointer to user data, passed unchanged as the last parameter to the
     * user ordering function (optional, the user function need not use this
     * information). */
    void *user_data ;

    int halt_if_singular ;      /* how to handle a singular matrix:
        * FALSE: keep going.  Return a Numeric object with a zero U(k,k).  A
        *   divide-by-zero may occur when computing L(:,k).  The Numeric object
        *   can be passed to klu_solve (a divide-by-zero will occur).  It can
        *   also be safely passed to klu_refactor.
        * TRUE: stop quickly.  klu_factor will free the partially-constructed
        *   Numeric object.  klu_refactor will not free it, but will leave the
        *   numerical values only partially defined.  This is the default. */

    /* ---------------------------------------------------------------------- */
    /* statistics */
    /* ---------------------------------------------------------------------- */

    int status ;                /* KLU_OK if OK, < 0 if error */
    int nrealloc ;              /* # of reallocations of L and U */

    int32_t structural_rank ;       /* 0 to n-1 if the matrix is structurally rank
        * deficient (as determined by maxtrans).  -1 if not computed.  n if the
        * matrix has full structural rank.  This is computed by klu_analyze
        * if a BTF preordering is requested. */

    int32_t numerical_rank ;        /* First k for which a zero U(k,k) was found,
        * if the matrix was singular (in the range 0 to n-1).  n if the matrix
        * has full rank. This is not a true rank-estimation.  It just reports
        * where the first zero pivot was found.  -1 if not computed.
        * Computed by klu_factor and klu_refactor. */

    int32_t singular_col ;          /* n if the matrix is not singular.  If in the
        * range 0 to n-1, this is the column index of the original matrix A that
        * corresponds to the column of U that contains a zero diagonal entry.
        * -1 if not computed.  Computed by klu_factor and klu_refactor. */

    int32_t noffdiag ;      /* # of off-diagonal pivots, -1 if not computed */

    double flops ;      /* actual factorization flop count, from klu_flops */
    double rcond ;      /* crude reciprocal condition est., from klu_rcond */
    double condest ;    /* accurate condition est., from klu_condest */
    double rgrowth ;    /* reciprocal pivot rgrowth, from klu_rgrowth */
    double work ;       /* actual work done in BTF, in klu_analyze */

    size_t memusage ;   /* current memory usage, in bytes */
    size_t mempeak ;    /* peak memory usage, in bytes */

} klu_common ;
 
typedef struct
{
    /* LU factors of each block, the pivot row permutation, and the
     * entries in the off-diagonal blocks */

    int32_t n ;             /* A is n-by-n */
    int32_t nblocks ;       /* number of diagonal blocks */
    int32_t lnz ;           /* actual nz in L, including diagonal */
    int32_t unz ;           /* actual nz in U, including diagonal */
    int32_t max_lnz_block ; /* max actual nz in L in any one block, incl. diag */
    int32_t max_unz_block ; /* max actual nz in U in any one block, incl. diag */
    int32_t *Pnum ;         /* size n. final pivot permutation */
    int32_t *Pinv ;         /* size n. inverse of final pivot permutation */

    /* LU factors of each block */
    int32_t *Lip ;          /* size n. pointers into LUbx[block] for L */
    int32_t *Uip ;          /* size n. pointers into LUbx[block] for U */
    int32_t *Llen ;         /* size n. Llen [k] = # of entries in kth column of L */
    int32_t *Ulen ;         /* size n. Ulen [k] = # of entries in kth column of U */
    void **LUbx ;       /* L and U indices and entries (excl. diagonal of U) */
    size_t *LUsize ;    /* size of each LUbx [block], in sizeof (Unit) */
    void *Udiag ;       /* diagonal of U */

    /* scale factors; can be NULL if no scaling */
    double *Rs ;        /* size n. Rs [i] is scale factor for row i */

    /* permanent workspace for factorization and solve */
    size_t worksize ;   /* size (in bytes) of Work */
    void *Work ;        /* workspace */
    void *Xwork ;       /* alias into Numeric->Work */
    int32_t *Iwork ;        /* alias into Numeric->Work */

    /* off-diagonal entries in a conventional compressed-column sparse matrix */
    int32_t *Offp ;         /* size n+1, column pointers */
    int32_t *Offi ;         /* size nzoff, row indices */
    void *Offx ;        /* size nzoff, numerical values */
    int32_t nzoff ;

} klu_numeric ;

*/
