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

#[test]
fn config_unknown_field_policy_is_preserve() {
    // AppConfig deliberately does NOT use deny_unknown_fields (forward-compat).
    assert_unknown_field_policy("config", UnknownFieldPolicy::Preserve, |p| {
        jackin_config::migrate_config_file_if_needed(p).map(|_| ())
    });
}

#[test]
fn workspace_unknown_field_policy_is_preserve() {
    // WorkspaceConfig deliberately does NOT use deny_unknown_fields (forward-compat).
    assert_unknown_field_policy("workspace", UnknownFieldPolicy::Preserve, |p| {
        jackin_config::migrate_workspace_file_if_needed(p).map(|_| ())
    });
}

#[test]
fn manifest_unknown_field_policy_is_deny() {
    assert_unknown_field_policy("manifest", UnknownFieldPolicy::Deny, |p| {
        jackin_manifest::migrations::migrate_manifest_file(p).map(|_| ())
    });
}

#[derive(Clone, Copy)]
enum UnknownFieldPolicy {
    /// Unknown keys may survive migration and still parse (AppConfig).
    Preserve,
    /// Unknown keys must be rejected by parse if they survive migration.
    Deny,
}

/// Per-kind unknown-field policy asserted against a real migration fixture.
fn assert_unknown_field_policy(file_kind: &str, policy: UnknownFieldPolicy, migrate: MigrateFn) {
    let file_kind_dir = fixture_root().join(file_kind);
    let entry = fs::read_dir(&file_kind_dir)
        .unwrap()
        .filter_map(Result::ok)
        .find(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .unwrap_or_else(|| panic!("no fixtures under {}", file_kind_dir.display()));
    let dir = entry.path();
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    let before = fs::read_to_string(dir.join("before.toml")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join(filename_for(file_kind));
    let mutated = format!("{before}\nx_unknown_probe = \"1\"\n");
    fs::write(&target, &mutated).unwrap();
    match migrate(&target) {
        Ok(()) => {
            let after = fs::read_to_string(&target).unwrap();
            let parse_err = match file_kind {
                "config" => toml::from_str::<jackin_config::AppConfig>(&after)
                    .err()
                    .map(|e| e.to_string()),
                "workspace" => toml::from_str::<jackin::workspace::WorkspaceConfig>(&after)
                    .err()
                    .map(|e| e.to_string()),
                "manifest" => toml::from_str::<jackin_manifest::RoleManifest>(&after)
                    .err()
                    .map(|e| e.to_string()),
                other => panic!("unknown file_kind {other:?}"),
            };
            match policy {
                UnknownFieldPolicy::Preserve => {
                    assert!(
                        parse_err.is_none(),
                        "fixture {name}: AppConfig preserve policy expects parse success, got {parse_err:?}"
                    );
                }
                UnknownFieldPolicy::Deny => {
                    if after.contains("x_unknown_probe") {
                        assert!(
                            parse_err.is_some(),
                            "fixture {name}: unknown field survived migration and was accepted by schema"
                        );
                        let msg = parse_err.unwrap();
                        assert!(
                            msg.contains("unknown field") || msg.contains("x_unknown_probe"),
                            "fixture {name}: expected unknown-field error, got {msg}"
                        );
                    }
                }
            }
        }
        Err(e) => {
            let msg = format!("{e:#}");
            assert!(
                msg.contains("unknown") || msg.contains("x_unknown_probe") || msg.contains("parse"),
                "fixture {name}: migrate failed for reason other than unknown-field policy: {msg}"
            );
        }
    }
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

        // Golden: the migration must produce exactly the committed after.toml.
        assert_eq!(
            actual_after, expected_after,
            "fixture {name}: migrated output differs from after.toml golden"
        );

        // Idempotence: migrating an already-current file must be a no-op.
        migrate(&target).unwrap_or_else(|e| panic!("re-migrating {name}: {e:#}"));
        let after_second = fs::read_to_string(&target).unwrap();
        assert_eq!(
            after_second, actual_after,
            "fixture {name}: migration is not idempotent (second run changed the file)"
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
