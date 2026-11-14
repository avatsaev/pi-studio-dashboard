//! Range access to the `"<serverId>\u{1f}<id>"` keyed maps: all entries of one server are
//! contiguous, so per-daemon work is O(log n + k), never a whole-map scan.

use std::collections::BTreeMap;
use std::ops::Bound;

fn prefix(server_id: &str) -> String {
    format!("{server_id}\u{1f}")
}

pub(crate) fn server_iter<'a, V>(map: &'a BTreeMap<String, V>, server_id: &str) -> impl Iterator<Item = (&'a String, &'a V)> {
    let p = prefix(server_id);
    map.range((Bound::Included(p.clone()), Bound::Unbounded)).take_while(move |(k, _)| k.starts_with(&p))
}

pub(crate) fn server_keys<V>(map: &BTreeMap<String, V>, server_id: &str) -> Vec<String> {
    server_iter(map, server_id).map(|(k, _)| k.clone()).collect()
}

/// Remove and return every entry of `server_id` for which `belongs` holds.
pub(crate) fn take_server<V>(map: &mut BTreeMap<String, V>, server_id: &str, belongs: fn(&V, &str) -> bool) -> BTreeMap<String, V> {
    let mut out = BTreeMap::new();
    for key in server_keys(map, server_id) {
        if let Some(v) = map.remove(&key) {
            if belongs(&v, server_id) {
                out.insert(key, v);
            } else {
                map.insert(key, v);
            }
        }
    }
    out
}
