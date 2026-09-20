// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use serde_json::Value;

use super::*;

fn known_legacy_snapshot() -> RollingReleaseSnapshot {
    RollingReleaseSnapshot {
        source_repository: LEGACY_SOURCE_REPOSITORY.to_owned(),
        tag_name: LEGACY_TAG.to_owned(),
        release_id: LEGACY_RELEASE_ID,
        release_name: LEGACY_RELEASE_NAME.to_owned(),
        release_body: LEGACY_RELEASE_BODY.to_owned(),
        tag_target: LEGACY_TAG_TARGET.to_owned(),
        draft: false,
        prerelease: true,
        assets: known_legacy_assets(),
    }
}

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

#[test]
fn consumer_updater_commits_only_when_status_is_dirty() {
    assert!(consumer_update_requires_commit(
        " M Formula/jackin-preview.rb\n"
    ));
    assert!(consumer_update_requires_commit(
        "?? Formula/jackin-preview.rb\n"
    ));
    assert!(!consumer_update_requires_commit(""));
    assert!(!consumer_update_requires_commit("\n"));
}

#[test]
fn unknown_invalid_rolling_release_is_rejected() {
    let mut snapshot = known_legacy_snapshot();
    snapshot.tag_target = "0".repeat(40);
    let error = ensure_known_legacy_rolling_release(&snapshot)
        .expect_err("changed rolling release must not enter migration");
    assert!(error.to_string().contains("unknown or changed"));
}

#[test]
fn known_legacy_release_archives_bytes_as_unverified_evidence() {
    let snapshot = known_legacy_snapshot();
    let downloaded = tempfile::tempdir().unwrap();
    for name in snapshot.assets.keys() {
        fs::write(downloaded.path().join(name), format!("untrusted:{name}")).unwrap();
    }
    let transaction = tempfile::tempdir().unwrap();

    let archive =
        archive_known_legacy_rolling_release(&snapshot, downloaded.path(), transaction.path())
            .unwrap();
    let metadata: Value =
        serde_json::from_slice(&fs::read(archive.join("metadata.json")).unwrap()).unwrap();
    assert_eq!(
        metadata["schema"],
        Value::String(LEGACY_ARCHIVE_SCHEMA.to_owned())
    );
    assert_eq!(
        metadata["verification_status"],
        Value::String("unverified-legacy-bytes".to_owned())
    );
    assert_eq!(
        metadata["assets"].as_object().unwrap().len(),
        snapshot.assets.len()
    );
    assert!(
        metadata["assets"]
            .as_object()
            .unwrap()
            .values()
            .all(|asset| asset["matches_expected"] == Value::Bool(false))
    );
    for name in snapshot.assets.keys() {
        assert_eq!(
            fs::read(archive.join("assets").join(name)).unwrap(),
            fs::read(downloaded.path().join(name)).unwrap()
        );
    }
}

#[test]
fn legacy_archive_rejects_unexpected_files_before_publishing() {
    let snapshot = known_legacy_snapshot();
    let downloaded = tempfile::tempdir().unwrap();
    for name in snapshot.assets.keys() {
        fs::write(downloaded.path().join(name), b"untrusted").unwrap();
    }
    fs::write(downloaded.path().join("unexpected.bin"), b"untrusted").unwrap();
    let transaction = tempfile::tempdir().unwrap();

    let error =
        archive_known_legacy_rolling_release(&snapshot, downloaded.path(), transaction.path())
            .expect_err("unexpected legacy assets must fail closed");
    assert!(error.to_string().contains("unexpected asset"));
    assert!(!transaction.path().join("legacy-preview").exists());
}
