// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn failed_split_syntax_leaves_global_and_workspace_files_unchanged() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();

    let global_before = b"version = \"v1alpha10\"\n";
    let alpha_before = b"version = \"v1alpha8\"\nworkdir = \"/workspace/alpha\"\n\n[[mounts]]\nsrc = \"/tmp/alpha\"\ndst = \"/workspace/alpha\"\n";
    let broken_before = b"version = \"v1alpha8\"\nworkdir = [\n";
    std::fs::write(&paths.config_file, global_before).unwrap();
    std::fs::write(paths.workspaces_dir.join("alpha.toml"), alpha_before).unwrap();
    std::fs::write(paths.workspaces_dir.join("broken.toml"), broken_before).unwrap();
    let workspace_tree_before = workspace_tree_bytes(&paths);

    let err = AppConfig::load_or_init(&paths).unwrap_err();

    assert!(err.to_string().contains("parsing"), "{err:#}");
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), global_before);
    assert_eq!(workspace_tree_bytes(&paths), workspace_tree_before);
}

#[test]
fn empty_legacy_workspaces_table_still_gets_version_stamp() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[workspaces]\n").unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();
    let out = std::fs::read_to_string(&paths.config_file).unwrap();

    assert_eq!(config.version, CURRENT_CONFIG_VERSION);
    assert!(
        out.contains(&format!(r#"version = "{CURRENT_CONFIG_VERSION}""#)),
        "{out}"
    );
}

#[test]
fn load_rejects_invalid_workspace_filename() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(paths.workspaces_dir.join("..toml"), "").unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("invalid workspace filename"), "{msg}");
}

#[test]
fn config_needs_split_migration_returns_false_for_legacy_without_workspaces() {
    let raw = "[roles.agent-smith]\ngit = \"https://example.test/role.git\"\n";
    assert!(!config_needs_split_migration(raw).unwrap());
}

#[test]
fn config_needs_split_migration_returns_true_for_versioned_with_workspaces() {
    // Versioned config with a leftover `[workspaces.X]` table still needs the
    // in-memory split path, so a later split conflict cannot leave the global
    // file partially migrated.
    let raw = "version = \"v1alpha1\"\n\n[workspaces.prod]\nworkdir = \"/workspace/prod\"\n";
    assert!(config_needs_split_migration(raw).unwrap());
}

#[test]
fn config_needs_split_migration_returns_true_for_legacy_with_workspaces() {
    let raw = "[workspaces.prod]\nworkdir = \"/workspace/prod\"\n";
    assert!(config_needs_split_migration(raw).unwrap());
}

#[test]
fn config_needs_split_migration_returns_false_for_empty_workspaces_table() {
    let raw = "[workspaces]\n";
    assert!(!config_needs_split_migration(raw).unwrap());
}

#[test]
fn atomic_write_creates_parent_directories() {
    let temp = tempdir().unwrap();
    let nested = temp.path().join("a/b/c/file.toml");
    atomic_write(&nested, "k = 1\n").unwrap();
    assert_eq!(std::fs::read_to_string(&nested).unwrap(), "k = 1\n");
}

#[test]
fn atomic_write_overwrites_existing_file() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("file.toml");
    atomic_write(&path, "k = 1\n").unwrap();
    atomic_write(&path, "k = 2\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "k = 2\n");
}

#[test]
fn atomic_write_cleans_staged_file_on_rename_failure() {
    // Force rename to fail by placing a directory at the destination.
    let temp = tempdir().unwrap();
    let target = temp.path().join("target.toml");
    std::fs::create_dir(&target).unwrap();

    let err = atomic_write(&target, "k = 1\n").unwrap_err();
    assert!(format!("{err:#}").contains("renaming"), "{err}");

    // No `.tmp.<pid>.<n>` leftovers in the parent directory.
    let leaks: Vec<_> = std::fs::read_dir(temp.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("target.toml.tmp.")
        })
        .collect();
    assert!(leaks.is_empty(), "leftover staged files: {leaks:?}");
}

#[test]
fn load_or_init_dual_migrates_legacy_config_with_legacy_workspaces() {
    // Pin the dual-migration contract: a legacy `config.toml` (no
    // `version`) carrying `[workspaces.X]` tables ends up with
    // the current version on the global file AND on each split
    // workspace file after one load. The bootstrap migration is
    // content-changing, so this test guards the ordering that the
    // version migration runs alongside the split rather than getting
    // silently skipped.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"# operator comment
[env]
GLOBAL = "yes"

[workspaces.prod]
workdir = "/workspace/prod"

[[workspaces.prod.mounts]]
src = "/tmp/prod"
dst = "/workspace/prod"
"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();
    assert!(config.workspaces.contains_key("prod"));
    assert_eq!(
        config.bootstrap,
        Some(crate::BootstrapState {
            version: crate::BOOTSTRAP_VERSION,
            fresh_install: false,
        })
    );

    let global_on_disk = std::fs::read_to_string(&paths.config_file).unwrap();
    let global_parsed: toml::Value = toml::from_str(&global_on_disk).unwrap();
    assert_eq!(
        global_parsed["version"].as_str().unwrap(),
        CURRENT_CONFIG_VERSION
    );
    assert_eq!(
        global_parsed["bootstrap"]["version"].as_integer(),
        Some(i64::from(crate::BOOTSTRAP_VERSION))
    );
    assert_eq!(
        global_parsed["bootstrap"]["fresh_install"].as_bool(),
        Some(false)
    );
    assert!(!global_on_disk.contains("[workspaces."), "{global_on_disk}");

    let prod_on_disk = std::fs::read_to_string(paths.workspaces_dir.join("prod.toml")).unwrap();
    let prod_parsed: toml::Value = toml::from_str(&prod_on_disk).unwrap();
    assert_eq!(
        prod_parsed["version"].as_str().unwrap(),
        CURRENT_WORKSPACE_VERSION
    );

    // Re-running is a no-op: file content stays byte-identical.
    let global_before = std::fs::read(&paths.config_file).unwrap();
    let prod_before = std::fs::read(paths.workspaces_dir.join("prod.toml")).unwrap();
    AppConfig::load_or_init(&paths).unwrap();
    let global_after = std::fs::read(&paths.config_file).unwrap();
    let prod_after = std::fs::read(paths.workspaces_dir.join("prod.toml")).unwrap();
    assert_eq!(global_before, global_after);
    assert_eq!(prod_before, prod_after);
}

#[test]
fn load_workspace_files_migrates_legacy_split_file_in_place() {
    // Pin the contract that legacy `workspaces/<name>.toml` files (no
    // `version` key) get rewritten on first load. Without this test the
    // migrate-on-scan call in `load_workspace_files` is unreachable in
    // tests — every other workspace fixture uses the current version.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(
        paths.workspaces_dir.join("prod.toml"),
        "# keep me\nworkdir = \"/workspace/prod\"\n",
    )
    .unwrap();

    let map = load_workspace_files(&paths.workspaces_dir).unwrap();
    assert!(map.contains_key("prod"));

    let on_disk = std::fs::read_to_string(paths.workspaces_dir.join("prod.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&on_disk).unwrap();
    assert_eq!(
        parsed["version"].as_str().unwrap(),
        CURRENT_WORKSPACE_VERSION
    );
    assert!(on_disk.contains("# keep me"), "{on_disk}");
}

#[test]
fn load_workspace_files_ignores_leftover_staged_files() {
    // A `.tmp.<pid>.<n>` file in workspaces/ must not be treated as a
    // workspace file (extension filter is `.toml`).
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(
        paths.workspaces_dir.join("real.toml"),
        "version = \"v1alpha2\"\nworkdir = \"/w\"\n",
    )
    .unwrap();
    std::fs::write(
        paths.workspaces_dir.join("real.toml.tmp.99999.0"),
        "garbage",
    )
    .unwrap();

    let map = load_workspace_files(&paths.workspaces_dir).unwrap();
    assert!(map.contains_key("real"));
    assert_eq!(map.len(), 1);
}

#[test]
fn disc_read_only_current_and_older_workspace_migrate_in_memory() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(
        &paths.config_file,
        format!("version = \"{CURRENT_CONFIG_VERSION}\"\n"),
    )
    .unwrap();
    std::fs::write(
        paths.workspaces_dir.join("current.toml"),
        disc_workspace_toml(CURRENT_WORKSPACE_VERSION, "/workspace/current"),
    )
    .unwrap();
    std::fs::write(
        paths.workspaces_dir.join("older.toml"),
        disc_workspace_toml("v1alpha7", "/workspace/older"),
    )
    .unwrap();

    let snapshot = load_read_only_config_snapshot(&paths).unwrap();

    assert!(
        snapshot.diagnostics.is_empty(),
        "{:?}",
        snapshot.diagnostics
    );
    assert_eq!(snapshot.config.version, CURRENT_CONFIG_VERSION);
    assert_eq!(snapshot.config.workspaces.len(), 2);
    assert_eq!(
        snapshot.config.workspaces["older"].version,
        CURRENT_WORKSPACE_VERSION
    );
}

#[test]
fn disc_read_only_legacy_embedded_workspace_stays_in_memory() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.legacy]
workdir = "/workspace/legacy"

[[workspaces.legacy.mounts]]
src = "/host/source"
dst = "/workspace/legacy"
"#,
    )
    .unwrap();
    let before = disc_dir_entries(&paths.config_dir);

    let snapshot = load_read_only_config_snapshot(&paths).unwrap();

    assert!(
        snapshot.diagnostics.is_empty(),
        "{:?}",
        snapshot.diagnostics
    );
    assert!(snapshot.config.workspaces.contains_key("legacy"));
    assert_eq!(disc_dir_entries(&paths.config_dir), before);
    assert!(!paths.workspaces_dir.exists());
}

#[test]
fn disc_read_only_missing_config_creates_nothing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    let snapshot = load_read_only_config_snapshot(&paths).unwrap();

    assert!(snapshot.diagnostics.is_empty());
    assert!(snapshot.config.workspaces.is_empty());
    assert!(!paths.config_dir.exists());
    assert!(!paths.config_file.with_file_name("config.lock").exists());
}

#[test]
fn disc_read_only_malformed_workspace_preserves_unrelated_sources() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    disc_write_tree(&paths);
    std::fs::write(
        paths.workspaces_dir.join("broken.toml"),
        "version = [not-toml",
    )
    .unwrap();

    let snapshot = load_read_only_config_snapshot(&paths).unwrap();

    assert!(snapshot.config.workspaces.contains_key("alpha"));
    assert!(!snapshot.config.workspaces.contains_key("broken"));
    assert!(snapshot.diagnostics.contains(&ConfigSourceDiagnostic {
        scope: ConfigSourceScope::Workspace("broken".to_owned()),
        issue: ConfigSourceIssue::Malformed,
    }));
}

#[test]
fn disc_read_only_newer_version_is_typed_and_sanitized() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(&paths.config_file, "version = \"v99alpha1\"\n").unwrap();

    let snapshot = load_read_only_config_snapshot(&paths).unwrap();

    assert_eq!(
        snapshot.diagnostics,
        vec![ConfigSourceDiagnostic {
            scope: ConfigSourceScope::Global,
            issue: ConfigSourceIssue::UnsupportedVersion,
        }]
    );
    assert!(
        format!("{:?}", snapshot.diagnostics)
            .find(temp.path().to_str().unwrap())
            .is_none()
    );
}

#[test]
fn disc_read_only_newer_workspace_version_preserves_diagnostic() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(
        paths.workspaces_dir.join("future.toml"),
        r#"version = "v99alpha1"
workdir = "/workspace/future"
"#,
    )
    .unwrap();

    let snapshot = load_read_only_config_snapshot(&paths).unwrap();

    assert_eq!(
        snapshot.diagnostics,
        vec![ConfigSourceDiagnostic {
            scope: ConfigSourceScope::Workspace("future".to_owned()),
            issue: ConfigSourceIssue::UnsupportedVersion,
        }]
    );
}
