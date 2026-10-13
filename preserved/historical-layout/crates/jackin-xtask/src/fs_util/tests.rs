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

#[test]
fn enforcement_reports_bare_read_dir_outside_helper() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("crates/jackin-xtask/src");
    fs::create_dir_all(&src).unwrap();
    let source = ["fn gate() { std::fs::", "read_dir(\".\"); }\n"].concat();
    fs::write(src.join("gate.rs"), source).unwrap();
    let err = enforce_sorted_iteration(root.path())
        .expect_err("bare read_dir must fail")
        .to_string();
    assert!(err.contains("gate.rs:1"), "{err}");
}
