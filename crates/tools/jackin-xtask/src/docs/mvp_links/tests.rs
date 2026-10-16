// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;

use super::{MVP_PACK_REL, check_mvp_links};

fn pack_root(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join(MVP_PACK_REL)
}

fn write_pack(dir: &tempfile::TempDir, name: &str, content: &str) -> PathBuf {
    let path = pack_root(dir).join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
    path
}

#[test]
fn valid_links_pass() {
    let dir = tempfile::tempdir().unwrap();
    write_pack(&dir, "SPEC.md", "# Spec\n");
    write_pack(
        &dir,
        "PLAN.md",
        "See [the spec](SPEC.md) and [terms](TERMS.md#technical-terms).\n",
    );
    write_pack(&dir, "TERMS.md", "# Terms\n");
    write_pack(
        &dir,
        "components/01.md",
        "Back to [plan](../PLAN.md). External [site](https://example.com/x). Same page [here](#top).\n",
    );
    write_pack(
        &dir,
        "EXAMPLE.md",
        "```pkl\namends \"Workflow.pkl\"\n[not a link](missing-in-fence.pkl)\n```\n",
    );
    check_mvp_links(dir.path()).unwrap();
}

#[test]
fn broken_link_fails_with_location() {
    let dir = tempfile::tempdir().unwrap();
    write_pack(&dir, "SPEC.md", "See [gone](missing.md) for details.\n");
    let err = check_mvp_links(dir.path()).unwrap_err();
    let message = format!("{err:#}");
    assert!(
        message.contains("plans/workflows/mvp/SPEC.md:1: missing.md"),
        "unexpected message: {message}"
    );
}

#[test]
fn nested_broken_link_reports_nested_path() {
    let dir = tempfile::tempdir().unwrap();
    write_pack(&dir, "PLAN.md", "# Plan\n");
    write_pack(
        &dir,
        "components/02.md",
        "See [plan](../PLAN.md) and [gone](../gone.md).\n",
    );
    let err = check_mvp_links(dir.path()).unwrap_err();
    let message = format!("{err:#}");
    assert!(
        message.contains("plans/workflows/mvp/components/02.md:1: ../gone.md"),
        "unexpected message: {message}"
    );
    assert!(!message.contains("PLAN.md)"), "valid link must not fail");
}

#[test]
fn missing_pack_directory_fails() {
    let dir = tempfile::tempdir().unwrap();
    let err = check_mvp_links(dir.path()).unwrap_err();
    assert!(
        format!("{err:#}").contains("MVP pack directory not found"),
        "unexpected message: {err:#}"
    );
}
