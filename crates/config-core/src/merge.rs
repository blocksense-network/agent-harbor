//! JSON merging functionality

use serde_json::Value as J;

/// Merge two JSON values with deep object merging and array replacement
///
/// Objects are merged recursively, scalars/arrays replace the left value.
pub fn merge_two_json(base: &mut J, layer: J) {
    match (base, layer) {
        (J::Object(a), J::Object(b)) => {
            for (k, v) in b {
                merge_two_json(a.entry(k).or_insert(J::Null), v);
            }
        }
        // Policy: arrays are replaced wholesale
        (a @ J::Array(_), J::Array(b)) => *a = J::Array(b),
        (_, J::Null) => { /* keep left if right is null */ }
        (a, b) => *a = b,
    }
}

/// Insert a value at a dotted path in JSON
pub fn insert_dotted(root: &mut J, dotted: &str, v: J) {
    let parts: Vec<&str> = dotted.split('.').collect();

    // Navigate to the parent of the final key
    let mut cur = root;
    for p in &parts[..parts.len() - 1] {
        if !cur.is_object() {
            *cur = J::Object(Default::default());
        }
        let map = cur.as_object_mut().unwrap();
        if !map.contains_key(*p) {
            map.insert((*p).into(), J::Object(Default::default()));
        }
        cur = map.get_mut(*p).unwrap();
    }

    // Insert the value at the final key
    let final_key = parts.last().unwrap();
    if let J::Object(map) = cur {
        map.insert((*final_key).into(), v);
    } else {
        *cur = serde_json::json!({ *final_key: v });
    }
}
