//! Walk every migration fixture in `tests/fixtures/migrations/` and prove the
//! current binary still upgrades each historical input to the byte-equal
//! `after.toml`. Operators delayed by months — landing on the current jackin
//! version after several `CURRENT_*_VERSION` bumps — exercise exactly the
//! same chain this test covers, so a fixture mismatch is the regression that
//! would break their upgrade.

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn config_fixtures_round_trip_to_current() {
    walk_fixtures(&fixture_root().join("config"), &|path| {
        jackin::config::migrate_config_file_if_needed(path).map(|_| ())
    });
}

#[test]
fn workspace_fixtures_round_trip_to_current() {
    walk_fixtures(&fixture_root().join("workspace"), &|path| {
        jackin::config::migrate_workspace_file_if_needed(path).map(|_| ())
    });
}

#[test]
fn manifest_fixtures_round_trip_to_current() {
    walk_fixtures(&fixture_root().join("manifest"), &|path| {
        jackin::manifest::migrations::migrate_manifest_file(path).map(|_| ())
    });
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("migrations")
}

fn walk_fixtures(file_kind_dir: &Path, migrate: &dyn Fn(&Path) -> anyhow::Result<()>) {
    let entries: Vec<_> = fs::read_dir(file_kind_dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", file_kind_dir.display()))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
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
        let meta: toml::Value =
            toml::from_str(&meta_raw).unwrap_or_else(|e| panic!("parsing {name}/meta.toml: {e}"));
        assert!(
            meta.get("from_version").and_then(|v| v.as_str()).is_some(),
            "{name}/meta.toml missing `from_version` string"
        );
        assert!(
            meta.get("target_version")
                .and_then(|v| v.as_str())
                .is_some(),
            "{name}/meta.toml missing `target_version` string"
        );
        assert!(
            meta.get("summary").and_then(|v| v.as_str()).is_some(),
            "{name}/meta.toml missing `summary` string"
        );

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join(filename_for(file_kind_dir));
        fs::write(&target, &before).unwrap();

        migrate(&target).unwrap_or_else(|e| panic!("migrating {name}: {e:#}"));

        let actual_after = fs::read_to_string(&target).unwrap();
        assert_eq!(
            actual_after, expected_after,
            "fixture {name} drifted from after.toml — re-bake the fixture if the change is intentional"
        );
    }
}

fn filename_for(file_kind_dir: &Path) -> &'static str {
    match file_kind_dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .as_ref()
    {
        // Manifest migrations operate on the role-repo file by name.
        "manifest" => "jackin.role.toml",
        // Config and workspace migrations work on any `.toml` path.
        _ => "test.toml",
    }
}
