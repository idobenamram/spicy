use std::cmp::min;

/// Block Triangular Form (BTF), Strongly Connected Components (SCC)
/// the algorithm is described in the paper:
/// "An implementation of Tarjan's algorithm for the Block Triangularization of a Matrix"
/// by I. S. Duff and J. K. Reid
/// but the paper is not very helpful.
/// the easier thing is to read the implementation of Timothy A. Davis.
/// here: https://github.com/DrTimothyAldenDavis/SuiteSparse/blob/stable/BTF/Include/btf.h
/// the code is extensively documented and much easier to understand.
///
/// one thing that i kinda struggled with was how we take the definitions from Tarjan's paper
/// on graphs and apply them to the matrix here. In the paper the algorithm works
/// on a directed graph G(V, E) where V is the set of vertices and E is the set of edges.
/// in the setting of a sparse matrix, the vertices are the nodes (columns or rows) and the edges
/// are the non-zero entries in a column (or row).
/// you are using the adjacency list (fancy term for the list of edges for each vertex) of the columns
/// which are the non zero rows in the column. now the matrix is symmetric by definition of MNA
/// meaning that the non-zero row (j) is an edge from the column i -> column j (by row j).
/// it was a little confusing for me at first, so i wanted to write it down.
use crate::solver::matrix::csc::CscMatrix;

const UNVISITED: usize = usize::MAX; // visited[j] = UNVISITED means node j has not been visited yet
const UNASSIGNED: usize = usize::MAX - 1; // visited[j] = UNASSIGNED means node j has been visited 
// but not assigned to a strongly connected component yet (or block)
// if visted[j] = k, the node j is assigned to the k-th SCC block

fn dfs(
    // inputs
    m: &CscMatrix,
    // Q in davis's code
    // this is the column permutation we got from the max transversal algorithm
    column_permutations: &[isize],
    // the current column we are visiting
    current_column: usize,

    // the depth first search index of the tree
    node_graph_index: &mut usize,
    // the number of SCC blocks found so far
    number_of_scc_blocks: &mut usize,

    // see docs above on UNASSIGNED
    visited: &mut [usize],
    // graph_indices[j] is the index of the node j in the graph if it has been visited
    graph_indices: &mut [isize],
    // low[j] is the lowest graph_index of any node reachable from node j
    low: &mut [isize],

    // stacks
    component_stack: &mut [usize],
    column_stack: &mut [usize],
    position_stack: &mut [usize],
) {
    let mut component_head = 0;
    let mut column_head: i64 = 0;
    column_stack[column_head as usize] = current_column;
    assert!(visited[current_column] == UNVISITED);

    while column_head >= 0 {
        let col = column_stack[column_head as usize];
        // TODO: support BTF_UNFLIP
        let column_after_permutation = column_permutations[col] as usize;
        let end_of_column = m.col_start(column_after_permutation + 1);

        if visited[col] == UNVISITED {
            component_head += 1;
            component_stack[component_head] = col;
            // increment the node graph index
            *node_graph_index += 1;
            graph_indices[col] = *node_graph_index as isize;
            low[col] = *node_graph_index as isize;
            visited[col] = UNASSIGNED;

            position_stack[column_head as usize] = m.col_start(column_after_permutation);
        }

        let row_ptr = position_stack[column_head as usize];
        while row_ptr < end_of_column {
            // examine edge from node "col" to node "row"
            let row = m.row_index(row_ptr);
            if visited[row] == UNVISITED {
                position_stack[column_head as usize] = row_ptr + 1;
                column_head += 1;
                column_stack[column_head as usize] = row;
                assert!(graph_indices[row] == -1);
                assert!(low[row] == -1);
                break;
            } else if visited[row] == UNASSIGNED {
                // node "row" has been visited, but not assigned to a component block
                // update the low value of the current node
                assert!(graph_indices[row] > 0);
                assert!(low[row] > 0);
                low[col] = min(low[col], low[row]);
            }

            if row_ptr == end_of_column {
                // all edges from node "col" have been examined
                // pop from the column stack
                column_head -= 1;

                // found a SCC block
                if low[col] == graph_indices[col] {
                    loop {
                        assert!(column_head >= 0);

                        // pop from the component stack
                        let i = component_stack[column_head as usize];
                        column_head -= 1;
                        // assert!(i >= 0);
                        // we didn't somehow assign this already
                        assert!(visited[i] == UNASSIGNED);
                        // add to the SCC block
                        visited[i] = *number_of_scc_blocks;
                        // if we've popped the root of the SCC block, we're done
                        if i == col {
                            break;
                        }
                    }
                    *number_of_scc_blocks += 1;
                }

                // if parent exists update it
                if column_head >= 0 {
                    let parent = column_stack[column_head as usize];
                    low[parent] = min(low[parent], low[col]);
                }
            }
        }
    }
}

pub(crate) fn btf_scc(m: &CscMatrix, column_permutations: &mut [isize]) -> (usize, Vec<isize>) {
    let n = m.dim.ncols;
    let out_of_bounds = n + 1;

    let mut graph_indices: Vec<isize> = vec![-1; n];
    let mut low: Vec<isize> = vec![-1; n];
    // called flag in davis's code
    let mut visited: Vec<usize> = vec![UNVISITED; n];

    let mut component_stack: Vec<usize> = vec![out_of_bounds; n];
    let mut column_stack: Vec<usize> = vec![out_of_bounds; n];
    let mut position_stack: Vec<usize> = vec![out_of_bounds; n];

    // each node in the graph is given a monotinic index
    let mut node_graph_index = 0;
    let mut number_of_scc_blocks = 0;

    for col in 0..n {
        assert!(visited[col] == UNVISITED || (visited[col] < number_of_scc_blocks));
        if visited[col] == UNVISITED {
            dfs(
                m,
                &column_permutations,
                col,
                &mut node_graph_index,
                &mut number_of_scc_blocks,
                &mut visited,
                &mut graph_indices,
                &mut low,
                // stacks outside of function for allocation efficiency
                &mut component_stack,
                &mut column_stack,
                &mut position_stack,
            );
        }
    }

    assert!(node_graph_index == n);

    // block info is stored in the visted array, visited[j] = k means node j is in the k-th SCC block
    // from here we want to create a symmetric permutation to move to block triangular form

    // first we will computre the "boundary array" which will tell us the start and end of each SCC block

    let mut boundary_array = vec![0; number_of_scc_blocks];

    for col in 0..n {
        // sanity checks that the blocks were generated correctly
        assert!(graph_indices[col] > 0 && graph_indices[col] <= n as isize);
        assert!(low[col] > 0 && low[col] <= n as isize);
        assert!(visited[col] < number_of_scc_blocks);
        // visited[col] is the SCC block index of the current column
        // increment the boundary array to get the number of nodes in the current block
        boundary_array[visited[col]] += 1;
    }

    // boundary_array[b] is now the number of nodes in SCC block b
    // compute cumulative sum of boundary_array, using graph_indices[0..nblocks-1] as workspace

    // we can merge the two loops,
    // but we are going to continue using the graph_indices array as a workspace
    for b in 1..number_of_scc_blocks {
        graph_indices[b] = graph_indices[b - 1] + boundary_array[b - 1];
    }
    for b in 0..number_of_scc_blocks {
        boundary_array[b] = graph_indices[b] as isize;
    }
    boundary_array[number_of_scc_blocks] = n as isize;

    // construct the permutation, perserving the natural order

    let mut btf_permutation = vec![-1; n];

    for col in 0..n {
        // visited[col] is the SCC block index of col
        let block = visited[col];
        // graph_indices[block] is the index to the new node(column) in the current SCC block for col
        btf_permutation[graph_indices[block] as usize] = col as isize;
        // increment the index in graph_indices for the block
        graph_indices[block] += 1;
    }

    for col in 0..n {
        // sanity check that the permutation was constructed correctly
        assert!(btf_permutation[col] >= 0);
    }

    // lets call orignal matrix = A
    // column permutation = Q
    // btf_permutation = P
    // the btf_permutation was done on A*Q
    // so the full permutation then is P*(A*Q)*P^T, instead we will return
    // Q = Q*P^T to make it simpler to apply the permutation to the matrix
    for k in 0..n {
        // btf_permutation[k] is the new column index for the k-th column after the column permutation
        graph_indices[k] = column_permutations[btf_permutation[k] as usize] as isize;
    }
    // overwrite the column permutation with the (Q*P^T) permutation
    for col in 0..n {
        column_permutations[col] = graph_indices[col];
    }

    todo!()
}
