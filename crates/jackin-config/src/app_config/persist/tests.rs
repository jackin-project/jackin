// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `persist`.
use super::*;
use crate::{CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION};
use jackin_core::JackinPaths;
use tempfile::tempdir;

fn wait_for_mtime_tick() {
    #[expect(
        clippy::disallowed_methods,
        reason = "mtime idempotency test needs a wall-clock boundary before checking no rewrite"
    )]
    std::thread::sleep(std::time::Duration::from_millis(50));
}

#[test]
fn sync_does_not_rewrite_config_when_already_current() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    // First load creates the file
    AppConfig::load_or_init(&paths).unwrap();
    let mtime_before = std::fs::metadata(&paths.config_file)
        .unwrap()
        .modified()
        .unwrap();

    // Small delay so mtime would differ if rewritten
    wait_for_mtime_tick();

    // Second load should not rewrite
    AppConfig::load_or_init(&paths).unwrap();
    let mtime_after = std::fs::metadata(&paths.config_file)
        .unwrap()
        .modified()
        .unwrap();

    assert_eq!(mtime_before, mtime_after);
}

#[test]
fn load_rejects_invalid_auth_forward_value() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[claude]
auth_forward = "bogus"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#,
    )
    .unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unknown variant `bogus`") || msg.contains("invalid auth_forward mode"),
        "expected parse error rejecting `bogus`, got: {msg}"
    );
}

#[test]
fn load_or_init_migrates_legacy_config_version() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"# keep me

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();
    let out = std::fs::read_to_string(&paths.config_file).unwrap();

    assert_eq!(config.version, CURRENT_CONFIG_VERSION);
    assert!(
        out.contains(&format!(r#"version = "{CURRENT_CONFIG_VERSION}""#)),
        "{out}"
    );
    assert!(out.contains("# keep me"), "{out}");
}

#[test]
fn load_or_init_rejects_newer_config_version() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, r#"version = "v2alpha1""#).unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();

    assert!(
        err.to_string()
            .contains(&format!("only understands up to {CURRENT_CONFIG_VERSION}"))
    );
}

#[test]
fn load_or_init_rejects_reserved_env_name_in_global_layer() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[env]
DOCKER_HOST = "override-attempt"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#,
    )
    .unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("DOCKER_HOST"), "{msg}");
    assert!(msg.contains("reserved"), "{msg}");
    assert!(msg.contains("global"), "{msg}");
}

#[test]
fn load_is_idempotent_when_builtins_already_synced() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    // Bootstrap once so builtins are synced and file stabilizes.
    AppConfig::load_or_init(&paths).unwrap();
    let mtime_before = std::fs::metadata(&paths.config_file)
        .unwrap()
        .modified()
        .unwrap();

    wait_for_mtime_tick();

    // Second load on a stable file must not rewrite.
    AppConfig::load_or_init(&paths).unwrap();
    let mtime_after = std::fs::metadata(&paths.config_file)
        .unwrap()
        .modified()
        .unwrap();

    assert_eq!(mtime_before, mtime_after);
}

#[test]
fn load_migrates_legacy_workspaces_into_split_files() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[env]
GLOBAL = "yes"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.prod]
workdir = "/workspace/prod"

[[workspaces.prod.mounts]]
src = "/tmp/prod"
dst = "/workspace/prod"

[workspaces.prod.env]
LOCAL = "only-prod"
"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();
    assert!(config.workspaces.contains_key("prod"));

    let global = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        global.contains(&format!(r#"version = "{CURRENT_CONFIG_VERSION}""#)),
        "{global}"
    );
    assert!(global.contains("[env]"), "{global}");
    assert!(!global.contains("[workspaces."), "{global}");

    let workspace = std::fs::read_to_string(paths.workspaces_dir.join("prod.toml")).unwrap();
    assert!(
        workspace.contains(&format!(r#"version = "{CURRENT_WORKSPACE_VERSION}""#)),
        "{workspace}"
    );
    assert!(
        workspace.contains(r#"workdir = "/workspace/prod""#),
        "{workspace}"
    );
    assert!(workspace.contains("[env]"), "{workspace}");
    assert!(workspace.contains(r#"LOCAL = "only-prod""#), "{workspace}");
}

#[test]
fn load_preserves_legacy_workspace_op_account_onto_refs() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    // A pre-split config.toml with an embedded workspace carrying the
    // old root-level `op_account` and an op ref that relied on it.
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.prod]
workdir = "/workspace/prod"
op_account = "WORKACCT"

[[workspaces.prod.mounts]]
src = "/tmp/prod"
dst = "/workspace/prod"

[workspaces.prod.env]
TOKEN = { op = "op://v/i/f", path = "Work/Claude/token" }
"#,
    )
    .unwrap();

    AppConfig::load_or_init(&paths).unwrap();

    let workspace = std::fs::read_to_string(paths.workspaces_dir.join("prod.toml")).unwrap();
    // The account must land on the op ref, and the root key must be gone
    // (v1alpha7 shape) — not silently dropped during the typed split.
    assert!(
        workspace.contains(r#"account = "WORKACCT""#),
        "legacy op_account must be stamped onto the ref:\n{workspace}"
    );
    assert!(
        !workspace.contains("op_account"),
        "root op_account must be removed after the move:\n{workspace}"
    );
}

#[test]
fn legacy_non_string_op_account_bails_loudly() {
    // A present-but-non-string op_account is operator data; it must
    // surface, not be silently dropped (mirrors the v1alpha7 migration).
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.prod]
workdir = "/workspace/prod"
op_account = 123

[[workspaces.prod.mounts]]
src = "/tmp/prod"
dst = "/workspace/prod"
"#,
    )
    .unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();
    let chain = format!("{err:#}");
    assert!(
        chain.contains("op_account") && chain.contains("must be a string"),
        "non-string op_account must bail loudly: {chain}"
    );
}

#[test]
fn legacy_op_account_split_is_idempotent_on_reentry() {
    // Simulates crash recovery: the per-workspace split file was written
    // (account stamped onto the ref) but the global rewrite that removes
    // [workspaces.*] did not commit, so the legacy tables reappear on the
    // next startup and migrate_legacy_workspaces re-runs. It must treat
    // the already-stamped file as identical and continue, not bail.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let legacy = r#"[workspaces.prod]
workdir = "/workspace/prod"
op_account = "WORKACCT"

[[workspaces.prod.mounts]]
src = "/tmp/prod"
dst = "/workspace/prod"

[workspaces.prod.env]
TOKEN = { op = "op://v/i/f", path = "Work/Claude/token" }
"#;
    std::fs::write(&paths.config_file, legacy).unwrap();

    // First migration writes prod.toml (stamped) and rewrites the global.
    AppConfig::load_or_init(&paths).unwrap();
    let stamped = std::fs::read_to_string(paths.workspaces_dir.join("prod.toml")).unwrap();
    assert!(stamped.contains(r#"account = "WORKACCT""#), "{stamped}");

    // Re-introduce the legacy tables (the rewrite "didn't commit") and
    // re-run: must succeed idempotently against the stamped split file.
    std::fs::write(&paths.config_file, legacy).unwrap();
    AppConfig::load_or_init(&paths)
        .expect("re-entry with an already-stamped split file must be idempotent");

    // The split file is unchanged by the second pass.
    let after = std::fs::read_to_string(paths.workspaces_dir.join("prod.toml")).unwrap();
    assert_eq!(stamped, after);
}

#[test]
fn failed_split_migration_leaves_legacy_config_unchanged() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(
        paths.workspaces_dir.join("prod.toml"),
        r#"version = "v1alpha3"
workdir = "/other"
"#,
    )
    .unwrap();
    let legacy = r#"[workspaces.prod]
workdir = "/workspace/prod"
"#;
    std::fs::write(&paths.config_file, legacy).unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();
    let out = std::fs::read_to_string(&paths.config_file).unwrap();

    assert!(
        err.to_string()
            .contains("already exists with different contents")
    );
    assert_eq!(out, legacy);
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
fn config_needs_split_migration_returns_false_for_versioned_with_workspaces() {
    // Versioned config with a leftover `[workspaces.X]` table: split
    // migration is skipped here because `load_split_config` will
    // `std::mem::take` and split-migrate the workspaces.
    let raw = "version = \"v1alpha1\"\n\n[workspaces.prod]\nworkdir = \"/workspace/prod\"\n";
    assert!(!config_needs_split_migration(raw).unwrap());
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
    // workspace file after one load. The current registries are
    // no-ops; once a real content-changing config migration lands,
    // this test guards the ordering that the version migration runs
    // alongside the split rather than getting silently skipped.
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

    let global_on_disk = std::fs::read_to_string(&paths.config_file).unwrap();
    let global_parsed: toml::Value = toml::from_str(&global_on_disk).unwrap();
    assert_eq!(
        global_parsed["version"].as_str().unwrap(),
        CURRENT_CONFIG_VERSION
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
