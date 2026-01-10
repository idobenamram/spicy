use crate::netlist_types::NodeName;
use serde::Serialize;
use serde::ser::SerializeMap;
use std::collections::HashMap;

#[cfg(test)]
pub(crate) fn serialize_sorted_map<S, K, V>(
    m: &HashMap<K, V>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    K: Ord + Serialize,
    V: Serialize,
{
    let mut items: Vec<_> = m.iter().collect();
    items.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

    let mut map = serializer.serialize_map(Some(items.len()))?;
    for (k, v) in items {
        map.serialize_entry(k, v)?;
    }
    map.end()
}

// TODO: could probably be generalized
#[cfg(test)]
pub(crate) fn serialize_node_map<S>(
    m: &HashMap<NodeName, NodeName>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;

    let mut items: Vec<_> = m.iter().collect();
    items.sort_by(|(k1, _), (k2, _)| k1.0.cmp(&k2.0));

    let mut map = serializer.serialize_map(Some(items.len()))?;
    for (k, v) in items {
        map.serialize_entry(&k.0, v)?;
    }
    map.end()
}
