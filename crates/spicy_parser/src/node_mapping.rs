use crate::netlist_types::{CurrentBranchIndex, NodeIndex, NodeName};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct NodeMapping {
    node_mapping: HashMap<NodeName, NodeIndex>,
    node_counter: usize,
    branch_mapping: HashMap<String, CurrentBranchIndex>,
    branch_counter: usize,
}

impl NodeMapping {
    pub fn new() -> Self {
        let mut node_mapping = HashMap::new();
        let branch_mapping = HashMap::new();
        // always insert ground node at index 0
        node_mapping.insert(NodeName("0".to_string()), NodeIndex(0));
        Self {
            node_mapping,
            node_counter: 1,
            branch_mapping,
            branch_counter: 1,
        }
    }

    pub fn insert_node(&mut self, node_name: NodeName) -> NodeIndex {
        let node_counter = &mut self.node_counter;
        *self.node_mapping.entry(node_name).or_insert_with(|| {
            let node = NodeIndex(*node_counter);
            *node_counter += 1;
            node
        })
    }
    pub fn insert_branch(&mut self, branch_name: String) -> CurrentBranchIndex {
        let branch_counter = &mut self.branch_counter;
        *self.branch_mapping.entry(branch_name).or_insert_with(|| {
            let branch = CurrentBranchIndex(*branch_counter);
            *branch_counter += 1;
            branch
        })
    }

    pub fn nodes_len(&self) -> usize {
        self.node_counter - 1 // -1 for the ground node
    }

    pub fn branches_len(&self) -> usize {
        self.branch_counter - 1
    }

    /// Convert a node index to an MNA node index.
    /// Returns None for the ground node.
    pub fn mna_node_index(&self, node_index: NodeIndex) -> Option<usize> {
        if node_index.0 == 0 {
            None
        } else {
            Some(node_index.0 - 1)
        }
    }

    /// Convert a branch index to an MNA branch index.
    pub fn mna_branch_index(&self, branch_index: CurrentBranchIndex) -> usize {
        assert!(
            branch_index.0 != 0,
            "branch index 0 is reserved for devices without an MNA current unknown"
        );
        self.nodes_len() + (branch_index.0 - 1)
    }

    pub fn mna_matrix_dim(&self) -> usize {
        // Unknowns are non-ground node voltages and branch currents.
        self.nodes_len() + self.branches_len()
    }

    /// Node names in MNA order (ground excluded).
    ///
    /// Index `i` in the returned vec corresponds to the MNA node voltage unknown at row/col `i`.
    pub fn node_names_mna_order(&self) -> Vec<String> {
        let mut names = vec![String::new(); self.nodes_len()];
        for (name, node_index) in &self.node_mapping {
            if let Some(i) = self.mna_node_index(*node_index) {
                names[i] = name.0.clone();
            }
        }
        names
    }

    /// Branch names in MNA order (only branches that allocate a current unknown).
    ///
    /// Index `i` in the returned vec corresponds to the MNA branch-current unknown at
    /// row/col `self.nodes_len() + i`.
    pub fn branch_names_mna_order(&self) -> Vec<String> {
        let mut names = vec![String::new(); self.branches_len()];
        for (name, branch_index) in &self.branch_mapping {
            // Branch indices start at 1 (0 is reserved for devices without a current unknown).
            let i = branch_index
                .0
                .checked_sub(1)
                .expect("branch index 0 should not appear in branch_mapping");
            if i < names.len() {
                names[i] = name.clone();
            }
        }
        names
    }
}

impl Default for NodeMapping {
    fn default() -> Self {
        Self::new()
    }
}

