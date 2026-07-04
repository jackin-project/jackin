// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Walk every migration fixture in `tests/fixtures/migrations/` and prove the
//! current binary still upgrades each historical input to a file that parses
//! successfully against the current serde schema. The `after.toml` in each
//! fixture is a realistic example of the current schema — it documents what a
//! canonical file looks like, including new fields added since the predecessor
//! version.
//!
//! Operators delayed by months — landing on the current jackin version after
//! several `CURRENT_*_VERSION` bumps — exercise exactly the same chain this
//! test covers, so a parse failure is the regression that would break their
//! upgrade.

#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "migration fixture tests include fixture names in fail-fast panic messages"
)]

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(dead_code, reason = "fixture metadata is validated by deserialization")]
struct FixtureMeta {
    from_version: String,
    target_version: String,
    target_version_shipped: String,
    summary: String,
}

type MigrateFn = fn(&Path) -> anyhow::Result<()>;

#[test]
fn config_fixtures_round_trip_to_current() {
    walk_fixtures("config", |p| {
        jackin_config::migrate_config_file_if_needed(p).map(|_| ())
    });
}

#[test]
fn workspace_fixtures_round_trip_to_current() {
    walk_fixtures("workspace", |p| {
        jackin_config::migrate_workspace_file_if_needed(p).map(|_| ())
    });
}

#[test]
fn manifest_fixtures_round_trip_to_current() {
    walk_fixtures("manifest", |p| {
        jackin_manifest::migrations::migrate_manifest_file(p).map(|_| ())
    });
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("migrations")
}

fn walk_fixtures(file_kind: &str, migrate: MigrateFn) {
    let file_kind_dir = fixture_root().join(file_kind);
    let entries: Vec<_> = fs::read_dir(&file_kind_dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", file_kind_dir.display()))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .collect();
    assert!(
        !entries.is_empty(),
        "no fixtures under {} — every supported `from_version` needs a directory",
        file_kind_dir.display()
    );

    for entry in entries {
        let dir = entry.path();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let before = fs::read_to_string(dir.join("before.toml"))
            .unwrap_or_else(|e| panic!("reading {name}/before.toml: {e}"));
        let expected_after = fs::read_to_string(dir.join("after.toml"))
            .unwrap_or_else(|e| panic!("reading {name}/after.toml: {e}"));
        let meta_raw = fs::read_to_string(dir.join("meta.toml"))
            .unwrap_or_else(|e| panic!("reading {name}/meta.toml: {e}"));
        let meta: FixtureMeta =
            toml::from_str(&meta_raw).unwrap_or_else(|e| panic!("parsing {name}/meta.toml: {e}"));

        // Per-fixture tempdir so leftover state from a prior iteration cannot
        // mask a migration that fails to fully overwrite the target.
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join(filename_for(file_kind));
        fs::write(&target, &before).unwrap();
        migrate(&target).unwrap_or_else(|e| panic!("migrating {name}: {e:#}"));
        let actual_after = fs::read_to_string(&target).unwrap();

        // The migrated file must parse against the current schema.
        parse_fixture(file_kind, &actual_after, &name, "actual");

        // The hand-written after.toml must also parse — it is the canonical
        // example of what the current schema looks like.
        parse_fixture(file_kind, &expected_after, &name, "expected");

        // The version in the migrated file must match the declared target.
        let actual_doc: toml::Value = toml::from_str(&actual_after).unwrap();
        let actual_version = actual_doc["version"].as_str().unwrap_or("");
        assert_eq!(
            actual_version,
            meta.target_version,
            "fixture {name}: migrated file has version {actual_version}, expected {target}",
            target = meta.target_version
        );

        // The expected after.toml must also declare the current version.
        let expected_doc: toml::Value = toml::from_str(&expected_after).unwrap();
        let expected_version = expected_doc["version"].as_str().unwrap_or("");
        assert_eq!(
            expected_version,
            meta.target_version,
            "fixture {name}: after.toml has version {expected_version}, expected {target}",
            target = meta.target_version
        );
    }
}

fn parse_fixture(file_kind: &str, contents: &str, name: &str, side: &str) {
    match file_kind {
        "config" => {
            let _parsed: jackin_config::AppConfig = toml::from_str(contents)
                .unwrap_or_else(|e| panic!("{side} {name} failed to parse as AppConfig: {e}"));
        }
        "workspace" => {
            let _parsed: jackin::workspace::WorkspaceConfig = toml::from_str(contents)
                .unwrap_or_else(|e| {
                    panic!("{side} {name} failed to parse as WorkspaceConfig: {e}")
                });
        }
        "manifest" => {
            let _parsed: jackin_manifest::RoleManifest = toml::from_str(contents)
                .unwrap_or_else(|e| panic!("{side} {name} failed to parse as RoleManifest: {e}"));
        }
        other => panic!("unknown file_kind {other:?}"),
    }
}

fn filename_for(file_kind: &str) -> &'static str {
    match file_kind {
        "config" | "workspace" => "test.toml",
        "manifest" => "jackin.role.toml",
        other => panic!("unknown file_kind {other:?}; add an arm to filename_for"),
    }
}
