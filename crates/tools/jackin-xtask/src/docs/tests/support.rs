// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

pub(super) fn write_meta_mk(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    write_meta(path, value).unwrap();
}

pub(super) fn repo_link_fixture(page_body: &str) -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    write(
        &root.join("crates/services/jackin-host/src/host_desktop.rs"),
        "pub fn open() {}\n",
    );
    write(&root.join("Cargo.toml"), "[workspace]\n");
    write(&root.join("docs/content/guide.mdx"), page_body);
    repo
}

pub(super) fn roadmap_fixture(extra: &[(&str, &str)]) -> tempfile::TempDir {
    let docs = tempfile::tempdir().unwrap();
    let d = docs.path();
    write_meta_mk(
        &d.join("roadmap/(grp)/meta.json"),
        &json!({ "pages": ["shipme"] }),
    );
    write(
        &d.join("roadmap/(grp)/shipme.mdx"),
        "---\ntitle: Ship Me\n---\n\n**Status**: Open\n\n## Problem\n\nbody\n",
    );
    for (rel, body) in extra {
        write(&d.join(rel), body);
    }
    docs
}
