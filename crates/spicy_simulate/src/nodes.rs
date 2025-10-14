use std::collections::HashMap;
use spicy_parser::netlist_types::Device;

#[derive(Debug)]
pub(crate) struct Nodes {
    pub(crate) nodes: HashMap<String, usize>,
    pub(crate) voltage_sources: HashMap<String, usize>,
}

impl Nodes {
    pub(crate) fn new(devices: &Vec<Device>) -> Self {
        let mut nodes = HashMap::new();
        let mut voltage_sources = HashMap::new();
        let mut src_index = 0;

        // assume already validated that ground exists
        nodes.insert("0".to_string(), 0);
        let mut node_index = 1;
        for device in devices {
            match device {
                Device::Inductor(_) | Device::VoltageSource(_) => {
                    voltage_sources.insert(device.name().to_string(), src_index);
                    src_index += 1;
                }
                _ => {}
            }
            for node in device.nodes() {
                if !nodes.contains_key(&node.name) {
                    nodes.insert(node.name.clone(), node_index);
                    node_index += 1;
                }
            }
        }

        Self {
            nodes,
            voltage_sources,
        }
    }

    pub(crate) fn get_node_names(&self) -> Vec<String> {
        let mut names = vec![String::new(); self.nodes.len()];
        for name in self.nodes.keys() {
            if let Some(index) = self.get_node_index(name) {
                names[index] = name.clone();
            }
        }
        names
    }

    pub(crate) fn get_source_names(&self) -> Vec<String> {
        let mut names = vec![String::new(); self.source_len()];
        for name in self.voltage_sources.keys() {
            if let Some(index) = self.voltage_sources.get(name).copied() {
                names[index] = name.clone();
            }
        }
        names
    }

    pub(crate) fn get_node_index(&self, name: &str) -> Option<usize> {
        if name != "0" {
            let x = self.nodes.get(name).copied().expect("node not found");
            if x != 0 { Some(x - 1) } else { None }
        } else {
            None
        }
    }

    pub(crate) fn get_voltage_source_index(&self, name: &str) -> Option<usize> {
        self.voltage_sources.get(name).copied().map(|index| self.node_len() + index)
    }

    // TODO: save this?
    pub(crate) fn node_len(&self) -> usize {
        self.nodes.values().copied()
            .max()
            .expect("no nodes found")
    }

    pub(crate) fn source_len(&self) -> usize {
        self.voltage_sources.len()
    }
}