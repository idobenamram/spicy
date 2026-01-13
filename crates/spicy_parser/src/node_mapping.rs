use crate::netlist_types::{CurrentBranchIndex, NodeIndex, NodeName};
use std::collections::HashMap;
use std::fmt;

#[derive(Clone)]
pub struct NodeMapping {
    node_mapping: HashMap<NodeName, NodeIndex>,
    node_counter: usize,
    branch_mapping: HashMap<String, CurrentBranchIndex>,
    branch_counter: usize,
}

// NOTE: We use `assert_debug_snapshot!` on parsed decks. `HashMap`'s iteration order is not
// deterministic, so we provide a stable `Debug` implementation for snapshot (and log) sanity.
impl fmt::Debug for NodeMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut node_entries: Vec<_> = self.node_mapping.iter().collect();
        node_entries.sort_by_key(|(_name, node_index)| node_index.0);

        let mut branch_entries: Vec<_> = self.branch_mapping.iter().collect();
        branch_entries.sort_by_key(|(_name, branch_index)| branch_index.0);

        let mut ds = f.debug_struct("NodeMapping");
        ds.field("node_mapping", &SortedDebugMap(&node_entries));
        ds.field("node_counter", &self.node_counter);
        ds.field("branch_mapping", &SortedDebugMap(&branch_entries));
        ds.field("branch_counter", &self.branch_counter);
        ds.finish()
    }
}

struct SortedDebugMap<'a, K: 'a, V: 'a>(&'a [(&'a K, &'a V)]);

impl<'a, K: fmt::Debug, V: fmt::Debug> fmt::Debug for SortedDebugMap<'a, K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut m = f.debug_map();
        for (k, v) in self.0 {
            m.entry(k, v);
        }
        m.finish()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ParseOptions, SourceMap, parse};
    use std::path::PathBuf;

    #[test]
    fn insert_node_ground_does_not_allocate() {
        let mut m = NodeMapping::new();

        let g = m.insert_node(NodeName("0".to_string()));
        assert_eq!(g, NodeIndex(0));
        assert_eq!(m.nodes_len(), 0);
        assert_eq!(m.node_names_mna_order(), Vec::<String>::new());

        // Next non-ground node should still get NodeIndex(1).
        let n1 = m.insert_node(NodeName("n1".to_string()));
        assert_eq!(n1, NodeIndex(1));
        assert_eq!(m.nodes_len(), 1);
        assert_eq!(m.node_names_mna_order(), vec!["n1".to_string()]);
    }

    #[test]
    #[should_panic(expected = "branch index 0 is reserved")]
    fn mna_branch_index_panics_on_zero() {
        let m = NodeMapping::new();
        let _ = m.mna_branch_index(CurrentBranchIndex(0));
    }

    #[test]
    fn node_mapping_nodes_and_branches_behave_as_expected() {
        let mut m = NodeMapping::new();

        // Ground always exists and maps to NodeIndex(0), but does not appear in MNA unknowns.
        assert_eq!(m.mna_node_index(NodeIndex(0)), None);
        assert_eq!(m.nodes_len(), 0);
        assert_eq!(m.branches_len(), 0);
        assert_eq!(m.mna_matrix_dim(), 0);
        assert_eq!(m.node_names_mna_order(), Vec::<String>::new());
        assert_eq!(m.branch_names_mna_order(), Vec::<String>::new());

        // Nodes.
        let n1 = m.insert_node(NodeName("n1".to_string()));
        let n2 = m.insert_node(NodeName("n2".to_string()));
        assert_eq!(n1, NodeIndex(1));
        assert_eq!(n2, NodeIndex(2));
        assert_eq!(m.nodes_len(), 2);
        assert_eq!(m.mna_node_index(n1), Some(0));
        assert_eq!(m.mna_node_index(n2), Some(1));
        assert_eq!(
            m.node_names_mna_order(),
            vec!["n1".to_string(), "n2".to_string()]
        );

        // Inserting the same node again must not allocate a new index.
        let n1_again = m.insert_node(NodeName("n1".to_string()));
        assert_eq!(n1_again, n1);
        assert_eq!(m.nodes_len(), 2);

        // Branches (current unknowns).
        let v1 = m.insert_branch("V1".to_string());
        let l1 = m.insert_branch("L1".to_string());
        assert_eq!(v1, CurrentBranchIndex(1));
        assert_eq!(l1, CurrentBranchIndex(2));
        assert_eq!(m.branches_len(), 2);

        // Inserting the same branch again must not allocate a new index.
        let v1_again = m.insert_branch("V1".to_string());
        assert_eq!(v1_again, v1);
        assert_eq!(m.branches_len(), 2);

        // Branch indices are appended after node-voltage unknowns in MNA.
        // Here: nodes_len=2, so branch 1 -> 2, branch 2 -> 3.
        assert_eq!(m.mna_branch_index(v1), 2);
        assert_eq!(m.mna_branch_index(l1), 3);
        assert_eq!(m.mna_matrix_dim(), 4);
        assert_eq!(
            m.branch_names_mna_order(),
            vec!["V1".to_string(), "L1".to_string()]
        );
    }

    #[test]
    fn parse_populates_node_and_branch_mapping_consistently() {
        let netlist = r#"mapping test
V1 in 0 1
R1 in out 1k
.op
.end
"#;

        let source_map = SourceMap::new(PathBuf::from("inline.spicy"), netlist.to_string());
        let mut options = ParseOptions {
            work_dir: PathBuf::from("."),
            source_path: PathBuf::from("."),
            source_map,
            max_include_depth: 10,
        };

        let deck = parse(&mut options).expect("parse");

        // Node names are ordered by allocated NodeIndex (ground excluded).
        assert_eq!(
            deck.node_mapping.node_names_mna_order(),
            vec!["in".to_string(), "out".to_string()]
        );
        // Branch names are ordered by allocated CurrentBranchIndex (reserved 0 excluded).
        assert_eq!(
            deck.node_mapping.branch_names_mna_order(),
            vec!["V1".to_string()]
        );

        assert_eq!(deck.devices.voltage_sources.len(), 1);
        assert_eq!(deck.devices.resistors.len(), 1);

        let v1 = &deck.devices.voltage_sources[0];
        assert_eq!(v1.name, "V1");
        assert_eq!(v1.positive, NodeIndex(1));
        assert_eq!(v1.negative, NodeIndex(0));
        assert_eq!(v1.current_branch, CurrentBranchIndex(1));

        let r1 = &deck.devices.resistors[0];
        assert_eq!(r1.name, "R1");
        assert_eq!(r1.positive, NodeIndex(1));
        assert_eq!(r1.negative, NodeIndex(2));
    }
}
