// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::sort_json_keys;

#[test]
fn keys_sort_recursively_regardless_of_insertion_order() {
    let mut root = serde_json::Map::new();
    root.insert("zebra".to_owned(), serde_json::json!(1));
    let mut inner = serde_json::Map::new();
    inner.insert("yankee".to_owned(), serde_json::json!(true));
    inner.insert("alpha".to_owned(), serde_json::json!([3, 2, 1]));
    root.insert("apple".to_owned(), serde_json::Value::Object(inner));
    let sorted = sort_json_keys(serde_json::Value::Object(root));
    let bytes = serde_json::to_vec(&sorted).expect("test value serializes");
    assert_eq!(
        bytes,
        br#"{"apple":{"alpha":[3,2,1],"yankee":true},"zebra":1}"#.as_slice()
    );
}
