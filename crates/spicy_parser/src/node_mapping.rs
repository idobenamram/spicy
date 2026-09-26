use crate::netlist_types::{NameKey, NodeIndex, NodeName};
use std::collections::HashMap;
use std::fmt;

/// Numbers the circuit's nodes in order of first appearance; ground is 0.
/// Names ignore case (`OUT` and `out` are one node) and are shown as first
/// written.
#[derive(Clone)]
pub struct NodeMapping {
    indices: HashMap<NameKey, NodeIndex>,
    /// Each node's name as first written, by index.
    names: Vec<NodeName>,
}

impl fmt::Debug for NodeMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeMapping")
            .field("nodes", &self.names)
            .finish()
    }
}

impl NodeMapping {
    pub fn new() -> Self {
        let ground = NodeName(NodeName::GROUND.to_string());
        Self {
            indices: HashMap::from([(NameKey::new(&ground.0), NodeIndex(0))]),
            names: vec![ground],
        }
    }

    pub fn insert_node(&mut self, node_name: NodeName) -> NodeIndex {
        let key = NameKey::new(&node_name.0);
        if let Some(&index) = self.indices.get(&key) {
            return index;
        }
        let index = NodeIndex(self.names.len());
        self.indices.insert(key, index);
        self.names.push(node_name);
        index
    }

    /// Number of nodes, ground excluded.
    pub fn nodes_len(&self) -> usize {
        self.names.len() - 1
    }

    /// Names of the nodes other than ground, in index order: entry `i` is node `i + 1`.
    pub fn node_names(&self) -> Vec<String> {
        self.names[1..].iter().map(|name| name.0.clone()).collect()
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
    fn node_names_ignore_case() {
        let mut m = NodeMapping::new();
        let first = m.insert_node(NodeName("OUT".to_string()));
        let again = m.insert_node(NodeName("out".to_string()));
        assert_eq!(first, again);
        assert_eq!(
            m.node_names(),
            vec!["OUT".to_string()],
            "shown as first written"
        );
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
