// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn normalize_resolves_dot_segments_lexically() {
    assert_eq!(
        normalize(Path::new("crates/p/src/../asset.bin")),
        PathBuf::from("crates/p/asset.bin")
    );
    assert_eq!(
        normalize(Path::new("crates/p/../../docker/x.sh")),
        PathBuf::from("docker/x.sh")
    );
    assert_eq!(
        normalize(Path::new("../../etc/x")),
        PathBuf::from("../../etc/x")
    );
}

#[test]
fn path_dependencies_read_all_sections() {
    let manifest = r#"
[dependencies]
parent = { path = ".." }
serde = "1"

[dev-dependencies]
helper = { path = "../helper" }

[build-dependencies]
gen = { path = "gen" }
"#;
    let value = toml::from_str::<toml::Value>(manifest).expect("parse");
    let mut deps = path_dependencies(&value);
    deps.sort();
    assert_eq!(
        deps,
        vec![
            PathBuf::from(".."),
            PathBuf::from("../helper"),
            PathBuf::from("gen"),
        ]
    );
}

#[test]
fn scanner_finds_real_cross_crate_includes() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .expect("workspace root above crates/tools/jackin-xtask");
    let source =
        std::fs::read_to_string(root.join("crates/adapters/jackin-image/src/derived_image.rs"))
            .expect("read derived_image.rs");
    let mut inputs = InputOwners::default();
    scan_source_inputs(
        &source,
        Path::new("crates/adapters/jackin-image/src/derived_image.rs"),
        Path::new("crates/adapters/jackin-image"),
        Some("jackin-image"),
        None,
        &mut inputs,
    );
    assert!(!inputs.unknown);
    let owners = inputs.members;
    assert!(owners.contains_key(Path::new("docker/runtime/entrypoint.sh")));
    assert!(owners.contains_key(Path::new(
        "crates/services/jackin-agent-status/packs/claude.toml"
    )));
}

#[test]
fn repo_scan_stays_within_resolvable_inputs() {
    let root = workspace_root();
    let mut inputs = InputOwners::default();
    for path in rust_sources(&root) {
        let source = std::fs::read_to_string(&path).expect("read source");
        let relative = path
            .strip_prefix(&root)
            .expect("source under root")
            .to_path_buf();
        // Empty scope: every literal resolves in-scope; only genuinely
        // unresolvable inputs set `unknown`.
        scan_source_inputs(&source, &relative, Path::new(""), None, None, &mut inputs);
    }
    assert!(
        !inputs.unknown,
        "unresolvable includes in: {:?}",
        inputs.unknown_sources
    );
}

#[test]
fn every_include_invocation_carries_a_literal() {
    let root = workspace_root();
    for path in rust_sources(&root) {
        let source = std::fs::read_to_string(&path).expect("read source");
        for hit in find_include_macros(&source) {
            let after = skip_ascii_trivia(&source[hit..]);
            let opener = after.as_bytes().first().copied();
            if !matches!(opener, Some(b'(' | b'[' | b'{')) {
                continue;
            }
            let candidate = skip_ascii_trivia(&after[1..]);
            assert_eq!(
                candidate.as_bytes().first(),
                Some(&b'"'),
                "{}:{}: include invocation without a string literal",
                path.display(),
                source[..hit].matches('\n').count() + 1,
            );
        }
    }
}

#[test]
fn manifest_union_comment_only_change_stays_empty() {
    let head = b"[workspace]\nmembers = []\n[workspace.dependencies]\nserde = \"1\"\n";
    let commented =
        b"# staged note\n[workspace]\nmembers = []\n[workspace.dependencies]\nserde = \"1\"\n";
    assert_eq!(
        union_workspace_dependency_changes(head, commented, commented),
        Some(BTreeSet::new())
    );
    assert_eq!(
        union_workspace_dependency_changes(head, head, commented),
        Some(BTreeSet::new())
    );
}

#[test]
fn lock_union_comment_only_change_stays_empty() {
    let head = b"[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n";
    let commented = b"# staged note\n[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n";
    assert_eq!(
        union_lock_changes(head, commented, commented),
        Some(BTreeSet::new())
    );
    assert_eq!(
        union_lock_changes(head, head, commented),
        Some(BTreeSet::new())
    );
}

#[test]
fn manifest_union_staged_only_breaking_edit_widens() {
    // Staged-only structural edit, worktree reverted to HEAD: HEAD↔worktree
    // alone is empty, but the staged pair is unprovable so the union widens.
    let head = b"[workspace]\nmembers = []\n[workspace.dependencies]\nserde = \"1\"\n";
    let staged =
        b"[workspace]\nmembers = [\"crates/a\"]\n[workspace.dependencies]\nserde = \"1\"\n";
    assert_eq!(union_workspace_dependency_changes(head, staged, head), None);
}

#[test]
fn lock_union_staged_only_version_bump_selects() {
    let head = b"[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n";
    let staged = b"[[package]]\nname = \"serde\"\nversion = \"1.1.0\"\n";
    assert_eq!(
        union_lock_changes(head, staged, head),
        Some(BTreeSet::from(["serde".to_owned()]))
    );
}

#[test]
fn unions_merge_head_index_and_index_worktree_pairs() {
    let base =
        b"[workspace]\nmembers = []\n[workspace.dependencies]\nserde = \"1\"\ntokio = \"1\"\n";
    let bumped_serde =
        b"[workspace]\nmembers = []\n[workspace.dependencies]\nserde = \"2\"\ntokio = \"1\"\n";
    let bumped_both =
        b"[workspace]\nmembers = []\n[workspace.dependencies]\nserde = \"2\"\ntokio = \"2\"\n";
    assert_eq!(
        union_workspace_dependency_changes(base, bumped_serde, bumped_both),
        Some(BTreeSet::from(["serde".to_owned(), "tokio".to_owned()]))
    );

    let base_lock = b"[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n[[package]]\nname = \"tokio\"\nversion = \"1.0.0\"\n";
    let bumped_serde_lock = b"[[package]]\nname = \"serde\"\nversion = \"1.1.0\"\n[[package]]\nname = \"tokio\"\nversion = \"1.0.0\"\n";
    let bumped_both_lock = b"[[package]]\nname = \"serde\"\nversion = \"1.1.0\"\n[[package]]\nname = \"tokio\"\nversion = \"1.2.0\"\n";
    assert_eq!(
        union_lock_changes(base_lock, bumped_serde_lock, bumped_both_lock),
        Some(BTreeSet::from(["serde".to_owned(), "tokio".to_owned()]))
    );
}

#[test]
fn unions_widen_when_either_pair_is_unparseable() {
    let head = b"[workspace]\nmembers = []\n[workspace.dependencies]\nserde = \"1\"\n";
    assert_eq!(
        union_workspace_dependency_changes(b"[[[broken", head, head),
        None
    );
    assert_eq!(
        union_workspace_dependency_changes(head, head, b"[[[broken"),
        None
    );
    let lock = b"[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n";
    assert_eq!(union_lock_changes(b"[[[broken", lock, lock), None);
    assert_eq!(union_lock_changes(lock, lock, b"[[[broken"), None);
}
