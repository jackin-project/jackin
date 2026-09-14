// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{
    classify_preview_source, mise_release_tools_changed, path_affects_preview,
    preview_commit_from_body,
};

#[test]
fn runtime_change_requires_preview() {
    assert!(path_affects_preview("crates/jackin-runtime/src/lib.rs"));
    assert!(path_affects_preview("docker/runtime/entrypoint.sh"));
    assert!(path_affects_preview("Cargo.lock"));
}

#[test]
fn docs_only_change_does_not_require_preview() {
    assert!(!path_affects_preview("docs/content/index.mdx"));
    assert!(!path_affects_preview("README.md"));
    assert!(!path_affects_preview("tests/manager_flow.rs"));
}

#[test]
fn mise_release_tool_pin_change_requires_preview() {
    let base = "[tools]\nnode = \"24\"\n";
    let head = "[tools]\nnode = \"24\"\n[tools]\nzig = \"0.16.0\"\n";
    assert!(mise_release_tools_changed(base, head));
}

#[test]
fn unrelated_mise_tool_change_does_not_require_preview() {
    let base = "[tools]\nbun = \"1.3.14\"\n";
    let head = "[tools]\nbun = \"1.3.15\"\n";
    assert!(!mise_release_tools_changed(base, head));
}

#[test]
fn classify_preview_source_respects_mise_gate() {
    assert!(!classify_preview_source(&["mise.toml"], false));
    assert!(classify_preview_source(&["mise.toml"], true));
    assert!(!classify_preview_source(&["docs/readme.md"], false));
    assert!(classify_preview_source(
        &["crates/jackin-core/src/lib.rs"],
        false
    ));
}

#[test]
fn preview_commit_from_body_reads_commit_link() {
    let sha = "a506eee0123456789012345678901234567890ab";
    assert_eq!(sha.len(), 40);
    let body = format!(
        "Preview build from [{short}](https://github.com/jackin-project/jackin/commit/{sha}).",
        short = &sha[..7]
    );
    assert_eq!(preview_commit_from_body(&body), Some(sha.to_owned()));
}
