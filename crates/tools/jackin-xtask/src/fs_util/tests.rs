// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::{cell::Cell, ffi::OsString, fs, path::Path};

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
fn read_dir_sorted_with_maps_skips_and_sorts_admitted_entries() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["keep-c.txt", "skip.md", "keep-a.txt", "keep-b.txt"] {
        fs::write(dir.path().join(name), "x").unwrap();
    }

    let names = read_dir_sorted_with(
        dir.path(),
        |entry| {
            let name = entry.file_name();
            Ok(name
                .to_str()
                .is_some_and(|name| {
                    Path::new(name)
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
                })
                .then_some(name))
        },
        OsString::cmp,
    )
    .unwrap();

    assert_eq!(
        names,
        vec![
            OsString::from("keep-a.txt"),
            OsString::from("keep-b.txt"),
            OsString::from("keep-c.txt")
        ]
    );
}

#[test]
fn read_dir_sorted_with_rejection_stops_immediately_and_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..16 {
        fs::write(dir.path().join(format!("entry-{index:02}")), "x").unwrap();
    }
    let calls = Cell::new(0);

    let error = read_dir_sorted_with(
        dir.path(),
        |entry| {
            let call = calls.get() + 1;
            calls.set(call);
            if call == 3 {
                anyhow::bail!("admission rejected the third entry");
            }
            Ok(Some(entry.file_name()))
        },
        OsString::cmp,
    )
    .expect_err("callback rejection must fail the directory read");

    assert!(error.to_string().contains("third entry"));
    assert_eq!(calls.get(), 3, "later entries must not reach the callback");
}

#[test]
fn read_dir_sorted_with_rejects_before_retaining_an_over_budget_entry() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..64 {
        fs::write(dir.path().join(format!("entry-{index:02}")), "x").unwrap();
    }
    const ENTRY_LIMIT: usize = 4;
    let visited = Cell::new(0);
    let admitted = Cell::new(0);

    let error = read_dir_sorted_with(
        dir.path(),
        |entry| {
            visited.set(visited.get() + 1);
            if admitted.get() == ENTRY_LIMIT {
                anyhow::bail!("source inventory limit reached");
            }
            admitted.set(admitted.get() + 1);
            Ok(Some(entry.file_name()))
        },
        OsString::cmp,
    )
    .expect_err("over-budget input must stop before collecting the rest");

    assert!(error.to_string().contains("inventory limit"));
    assert_eq!(admitted.get(), ENTRY_LIMIT);
    assert_eq!(visited.get(), ENTRY_LIMIT + 1);
}

#[test]
fn enforcement_reports_bare_read_dir_outside_helper() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("crates/tools/jackin-xtask/src");
    fs::create_dir_all(&src).unwrap();
    let source = ["fn gate() { std::fs::", "read_dir(\".\"); }\n"].concat();
    fs::write(src.join("gate.rs"), source).unwrap();
    let err = enforce_sorted_iteration(root.path())
        .expect_err("bare read_dir must fail")
        .to_string();
    assert!(err.contains("gate.rs:1"), "{err}");
}
