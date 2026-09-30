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

/// Parse a netlist given as text.
pub(crate) fn parse_netlist(netlist: &str) -> crate::reader::instance_parser::Deck {
    let mut options =
        crate::reader::ParseOptions::new_with_source("inline.spicy", netlist.to_string());
    crate::reader::parse(&mut options).expect("parse netlist")
}
