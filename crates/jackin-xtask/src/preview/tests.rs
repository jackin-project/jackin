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
fn mise_release_tool_aliases_include_cargo_prefixed_tools() {
    let base = "[tools]\n\"cargo:cargo-zigbuild\" = \"0.22.0\"\n\"cargo:sccache\" = \"0.15.0\"\n";
    let head = "[tools]\n\"cargo:cargo-zigbuild\" = \"0.23.0\"\n\"cargo:sccache\" = \"0.16.0\"\n";
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
fn migration_phase_advances_legacy_tag_after_release_delete() {
    assert_eq!(
        plan_legacy_migration(LegacyReleaseState::Absent, LegacyTagState::Legacy, true).unwrap(),
        LegacyMigrationPhase::AdvanceTag
    );
}

#[test]
fn migration_phase_accepts_candidate_tag_without_release() {
    assert_eq!(
        plan_legacy_migration(LegacyReleaseState::Absent, LegacyTagState::Candidate, false)
            .unwrap(),
        LegacyMigrationPhase::Noop
    );
}

#[test]
fn migration_phase_archives_then_deletes_known_legacy_release() {
    assert_eq!(
        plan_legacy_migration(
            LegacyReleaseState::KnownLegacy,
            LegacyTagState::Legacy,
            false
        )
        .unwrap(),
        LegacyMigrationPhase::Archive
    );
    assert_eq!(
        plan_legacy_migration(
            LegacyReleaseState::KnownLegacy,
            LegacyTagState::Legacy,
            true
        )
        .unwrap(),
        LegacyMigrationPhase::DeleteRelease
    );
}

#[test]
fn candidate_tag_acceptance_is_exact_and_rejects_other_targets() {
    let candidate = "abcdef0123456789abcdef0123456789abcdef01";
    assert_eq!(
        classify_tag_target(Some(candidate), candidate).unwrap(),
        LegacyTagState::Candidate
    );
    assert_eq!(
        classify_tag_target(Some(LEGACY_TAG_TARGET), candidate).unwrap(),
        LegacyTagState::Legacy
    );
    classify_tag_target(Some(&"0".repeat(40)), candidate).unwrap_err();
}

#[test]
fn migration_phase_completes_without_release_or_tag() {
    assert_eq!(
        plan_legacy_migration(LegacyReleaseState::Absent, LegacyTagState::Absent, false).unwrap(),
        LegacyMigrationPhase::Noop
    );
}

#[test]
fn migration_phase_fails_closed_for_release_without_tag() {
    let error = plan_legacy_migration(
        LegacyReleaseState::KnownLegacy,
        LegacyTagState::Absent,
        true,
    )
    .expect_err("release-present/tag-absent must not mutate");
    assert!(error.to_string().contains("without its tag"));
}

#[test]
fn migration_phase_fails_closed_for_unknown_release() {
    let error = plan_legacy_migration(LegacyReleaseState::Unknown, LegacyTagState::Legacy, true)
        .expect_err("unknown release must not mutate");
    assert!(error.to_string().contains("unknown or changed"));
}

#[test]
fn migration_phase_requires_durable_archive_before_advancing_tag() {
    let error = plan_legacy_migration(LegacyReleaseState::Absent, LegacyTagState::Legacy, false)
        .expect_err("tag advancement without archive must fail closed");
    assert!(error.to_string().contains("durable legacy archive"));
}

#[test]
fn preview_tag_patch_uses_non_force_fast_forward_api() {
    let args = preview_tag_patch_api_args(
        LEGACY_SOURCE_REPOSITORY,
        "abcdef0123456789abcdef0123456789abcdef01",
    );
    assert_eq!(
        args,
        vec![
            "api",
            "--method",
            "PATCH",
            "--repo",
            LEGACY_SOURCE_REPOSITORY,
            "repos/jackin-project/jackin/git/refs/tags/preview",
            "--raw-field",
            "sha=abcdef0123456789abcdef0123456789abcdef01",
            "--field",
            "force=false",
        ]
    );
    assert!(!args.iter().any(|arg| arg == "DELETE"));
    assert!(!args.iter().any(|arg| arg == "force=true"));
}

#[test]
fn github_environment_markers_append_without_replacing_existing_state() {
    let directory = tempfile::tempdir().unwrap();
    let env_file = directory.path().join("github-env");
    fs::write(&env_file, "EXISTING=1\n").unwrap();

    append_env_marker(&env_file, RETAIN_MARKER, "1").unwrap();
    append_env_marker(&env_file, PREPUBLISH_MARKER, "1").unwrap();

    assert_eq!(
        fs::read_to_string(env_file).unwrap(),
        "EXISTING=1\nVELNOR_PUBLICATION_LOCK_RETAIN=1\nVELNOR_PREPUBLISH_COMPLETED=1\n"
    );
}

#[test]
fn source_remote_parser_accepts_only_expected_github_shapes() {
    assert_eq!(
        github_repository_from_remote("https://github.com/jackin-project/jackin.git"),
        Some(LEGACY_SOURCE_REPOSITORY.to_owned())
    );
    assert_eq!(
        github_repository_from_remote("git@github.com:jackin-project/jackin.git"),
        Some(LEGACY_SOURCE_REPOSITORY.to_owned())
    );
    assert_eq!(
        github_repository_from_remote("https://github.com/other/repo.git"),
        Some("other/repo".to_owned())
    );
    assert_eq!(
        github_repository_from_remote("https://evil.example/jackin-project/jackin"),
        None
    );
    assert_eq!(
        github_repository_from_remote("http://github.com/jackin-project/jackin.git"),
        None
    );
}

#[test]
fn current_contract_requires_content_verification_after_asset_set_check() {
    let release = GithubRelease {
        id: 1,
        tag_name: LEGACY_TAG.to_owned(),
        name: "Current preview".to_owned(),
        body: Some("current".to_owned()),
        draft: false,
        prerelease: true,
        assets: expected_package_file_names()
            .into_iter()
            .map(|name| GithubReleaseAsset {
                name,
                digest: Some(format!("sha256:{}", "a".repeat(64))),
            })
            .collect(),
    };
    let candidate = "abcdef0123456789abcdef0123456789abcdef01";
    let rejected = classify_release_with(
        Some(&release),
        LEGACY_SOURCE_REPOSITORY,
        Some(candidate),
        |_, _, _| Err(anyhow::anyhow!("malformed package contents")),
    )
    .unwrap();
    assert_eq!(rejected, LegacyReleaseState::Unknown);

    let accepted = classify_release_with(
        Some(&release),
        LEGACY_SOURCE_REPOSITORY,
        Some(candidate),
        |_, _, _| Ok(()),
    )
    .unwrap();
    assert_eq!(accepted, LegacyReleaseState::CurrentContract);
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
        metadata["phase"],
        Value::String(LEGACY_ARCHIVE_PHASE.to_owned())
    );
    assert_eq!(
        metadata["archive_tag"],
        Value::String(LEGACY_ARCHIVE_TAG.to_owned())
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
    assert!(
        ensure_verified_legacy_archive(&snapshot, &archive).is_err(),
        "untrusted bytes must not qualify for the durable archive"
    );
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
