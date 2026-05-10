//! Walk every migration fixture in `tests/fixtures/migrations/` and prove the
//! current binary still upgrades each historical input to the byte-equal
//! `after.toml`. Operators delayed by months — landing on the current jackin
//! version after several `CURRENT_*_VERSION` bumps — exercise exactly the
//! same chain this test covers, so a fixture mismatch is the regression that
//! would break their upgrade.
//!
//! Per fixture directory: `meta.toml` carries `from_version`,
//! `target_version`, `target_version_shipped`, and `summary`; `before.toml`
//! is the input; `after.toml` is the byte-equal expected output. Re-bake
//! `after.toml` whenever a new step is appended to the relevant registry.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
struct FixtureMeta {
    #[allow(dead_code)]
    from_version: String,
    #[allow(dead_code)]
    target_version: String,
    #[allow(dead_code)]
    target_version_shipped: String,
    #[allow(dead_code)]
    summary: String,
}

type MigrateFn = fn(&Path) -> anyhow::Result<()>;

#[test]
fn fixtures_round_trip_to_current() {
    let cases: &[(&str, MigrateFn)] = &[
        ("config", |p| {
            jackin::config::migrate_config_file_if_needed(p).map(|_| ())
        }),
        ("workspace", |p| {
            jackin::config::migrate_workspace_file_if_needed(p).map(|_| ())
        }),
        ("manifest", |p| {
            jackin::manifest::migrations::migrate_manifest_file(p).map(|_| ())
        }),
    ];
    for (kind, migrate) in cases {
        walk_fixtures(kind, *migrate);
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

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join(filename_for(file_kind));

    for entry in entries {
        let dir = entry.path();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let before = fs::read_to_string(dir.join("before.toml"))
            .unwrap_or_else(|e| panic!("reading {name}/before.toml: {e}"));
        let expected_after = fs::read_to_string(dir.join("after.toml"))
            .unwrap_or_else(|e| panic!("reading {name}/after.toml: {e}"));
        let meta_raw = fs::read_to_string(dir.join("meta.toml"))
            .unwrap_or_else(|e| panic!("reading {name}/meta.toml: {e}"));
        let _meta: FixtureMeta =
            toml::from_str(&meta_raw).unwrap_or_else(|e| panic!("parsing {name}/meta.toml: {e}"));

        fs::write(&target, &before).unwrap();
        migrate(&target).unwrap_or_else(|e| panic!("migrating {name}: {e:#}"));
        let actual_after = fs::read_to_string(&target).unwrap();
        assert_eq!(
            actual_after, expected_after,
            "fixture {name} drifted from after.toml — re-bake the fixture if the change is intentional"
        );
    }
}

fn filename_for(file_kind: &str) -> &'static str {
    match file_kind {
        "config" | "workspace" => "test.toml",
        "manifest" => "jackin.role.toml",
        other => panic!("unknown file_kind {other:?}; add an arm to filename_for"),
    }
}
