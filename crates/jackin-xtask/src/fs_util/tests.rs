// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::fs;

#[test]
fn read_dir_sorted_is_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["c.md", "a.md", "b.md"] {
        fs::write(dir.path().join(name), "x").unwrap();
    }
    let names: Vec<_> = read_dir_sorted(dir.path())
        .unwrap()
        .into_iter()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["a.md", "b.md", "c.md"]);
}
