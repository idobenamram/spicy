use crate::netlist_types::{NodeIndex, NodeName};
use std::collections::HashMap;
use std::fmt;

#[derive(Clone)]
pub struct NodeMapping {
    node_mapping: HashMap<NodeName, NodeIndex>,
    node_counter: usize,
}

// NOTE: We use `assert_debug_snapshot!` on parsed decks. `HashMap`'s iteration order is not
// deterministic, so we provide a stable `Debug` implementation for snapshot (and log) sanity.
impl fmt::Debug for NodeMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut node_entries: Vec<_> = self.node_mapping.iter().collect();
        node_entries.sort_by_key(|(_name, node_index)| node_index.0);

        let mut ds = f.debug_struct("NodeMapping");
        ds.field("node_mapping", &SortedDebugMap(&node_entries));
        ds.field("node_counter", &self.node_counter);
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
        // always insert ground node at index 0
        node_mapping.insert(NodeName(NodeName::GROUND.to_string()), NodeIndex(0));
        Self {
            node_mapping,
            node_counter: 1,
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

    /// Number of nodes, ground excluded.
    pub fn nodes_len(&self) -> usize {
        self.node_counter - 1 // -1 for the ground node
    }

    /// Names of the nodes other than ground, in index order: entry `i` is node `i + 1`.
    pub fn node_names(&self) -> Vec<String> {
        let mut names = vec![String::new(); self.nodes_len()];
        for (name, node_index) in &self.node_mapping {
            if let Some(i) = node_index.0.checked_sub(1) {
                names[i] = name.0.clone();
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
        assert_eq!(m.node_names(), Vec::<String>::new());

        // Next non-ground node should still get NodeIndex(1).
        let n1 = m.insert_node(NodeName("n1".to_string()));
        assert_eq!(n1, NodeIndex(1));
        assert_eq!(m.nodes_len(), 1);
        assert_eq!(m.node_names(), vec!["n1".to_string()]);
    }

    #[test]
    fn nodes_are_numbered_in_order_of_first_appearance() {
        let mut m = NodeMapping::new();
        let n1 = m.insert_node(NodeName("n1".to_string()));
        let n2 = m.insert_node(NodeName("n2".to_string()));
        assert_eq!((n1, n2), (NodeIndex(1), NodeIndex(2)));
        assert_eq!(m.nodes_len(), 2);
        assert_eq!(m.node_names(), vec!["n1".to_string(), "n2".to_string()]);

        // Inserting the same node again must not allocate a new index.
        let n1_again = m.insert_node(NodeName("n1".to_string()));
        assert_eq!(n1_again, n1);
        assert_eq!(m.nodes_len(), 2);
    }

    #[test]
    fn parse_populates_node_mapping() {
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
            deck.node_mapping.node_names(),
            vec!["in".to_string(), "out".to_string()]
        );

        assert_eq!(deck.devices.voltage_sources.len(), 1);
        assert_eq!(deck.devices.resistors.len(), 1);

        let v1 = &deck.devices.voltage_sources[0];
        assert_eq!(v1.name, "V1");
        assert_eq!(v1.positive, NodeIndex(1));
        assert_eq!(v1.negative, NodeIndex(0));

        let r1 = &deck.devices.resistors[0];
        assert_eq!(r1.name, "R1");
        assert_eq!(r1.positive, NodeIndex(1));
        assert_eq!(r1.negative, NodeIndex(2));
    }
}
