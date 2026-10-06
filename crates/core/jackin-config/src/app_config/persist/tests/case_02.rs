// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn embedded_workspace_runs_supported_migrations_before_deserialization() {
    let raw = include_str!("../../../fixtures/config.embedded_workspace_legacy.toml");
    let (config, embedded) = parse_global_config(raw.as_bytes()).unwrap();

    assert_eq!(config.version, CURRENT_CONFIG_VERSION);
    assert_eq!(
        config.account_scan_exclusions,
        std::collections::BTreeSet::from(["removed-account-fingerprint".to_owned()])
    );
    let workspace = embedded.get("legacy").unwrap();
    assert_eq!(workspace.version, CURRENT_WORKSPACE_VERSION);
    assert!(workspace.roles.is_empty());
}

#[test]
fn split_embedded_workspace_migration_preserves_global_fields_and_is_idempotent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let raw = include_str!("../../../fixtures/config.embedded_workspace_legacy.toml");

    let config = load_split_config(&paths, Some(raw.to_owned())).unwrap();
    assert_eq!(config.version, CURRENT_CONFIG_VERSION);
    assert!(
        config
            .account_scan_exclusions
            .contains("removed-account-fingerprint")
    );

    let global = std::fs::read_to_string(&paths.config_file).unwrap();
    let global_value: toml::Value = toml::from_str(&global).unwrap();
    assert_eq!(
        global_value["version"].as_str(),
        Some(CURRENT_CONFIG_VERSION)
    );
    assert_eq!(
        global_value["account_scan_exclusions"][0].as_str(),
        Some("removed-account-fingerprint")
    );

    let workspace_path = paths.workspaces_dir.join("legacy.toml");
    let workspace = std::fs::read_to_string(&workspace_path).unwrap();
    let workspace_value: toml::Value = toml::from_str(&workspace).unwrap();
    assert_eq!(
        workspace_value["version"].as_str(),
        Some(CURRENT_WORKSPACE_VERSION)
    );
    assert!(
        !workspace.contains("codex"),
        "legacy agent table survived: {workspace}"
    );

    let global_before = std::fs::read(&paths.config_file).unwrap();
    let workspace_before = std::fs::read(&workspace_path).unwrap();
    load_split_config(&paths, Some(raw.to_owned())).unwrap();
    assert_eq!(global_before, std::fs::read(&paths.config_file).unwrap());
    assert_eq!(workspace_before, std::fs::read(&workspace_path).unwrap());
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
fn legacy_split_migration_accepts_equivalent_mixed_version_workspace() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();

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
    std::fs::write(
        paths.workspaces_dir.join("prod.toml"),
        r#"version = "v1alpha4"
workdir = "/workspace/prod"
op_account = "WORKACCT"

[[mounts]]
src = "/tmp/prod"
dst = "/workspace/prod"

[env]
TOKEN = { op = "op://v/i/f", path = "Work/Claude/token" }
"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();
    assert!(config.workspaces.contains_key("prod"));

    let split = std::fs::read_to_string(paths.workspaces_dir.join("prod.toml")).unwrap();
    assert!(
        split.contains(&format!(r#"version = "{CURRENT_WORKSPACE_VERSION}""#)),
        "{split}"
    );
    assert!(split.contains(r#"account = "WORKACCT""#), "{split}");
    assert!(!split.contains("op_account"), "{split}");
}

#[test]
fn split_workspace_rejects_legacy_op_account_after_migration_edge() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(
        paths.workspaces_dir.join("current.toml"),
        format!(
            r#"version = "{CURRENT_WORKSPACE_VERSION}"
workdir = "/workspace/current"
op_account = "MISLABELED"

[[mounts]]
src = "/tmp/current"
dst = "/workspace/current"
"#
        ),
    )
    .unwrap();

    let snapshot = load_read_only_config_snapshot(&paths).unwrap();

    assert_eq!(
        snapshot.diagnostics,
        vec![ConfigSourceDiagnostic {
            scope: ConfigSourceScope::Workspace("current".to_owned()),
            issue: ConfigSourceIssue::Malformed,
        }]
    );
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
fn failed_split_migration_leaves_versioned_config_unchanged() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    let versioned = r#"version = "v1alpha10"

[workspaces.prod]
workdir = "/workspace/prod"
"#;
    std::fs::write(&paths.config_file, versioned).unwrap();
    std::fs::write(
        paths.workspaces_dir.join("prod.toml"),
        format!("version = \"{CURRENT_WORKSPACE_VERSION}\"\nworkdir = \"/other\"\n"),
    )
    .unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();
    assert!(
        err.to_string()
            .contains("already exists with different contents")
    );
    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert_eq!(out, versioned);
    assert!(out.contains("version = \"v1alpha10\""));
    assert!(!out.contains("[bootstrap]"));
}

#[test]
fn failed_split_migration_leaves_every_workspace_file_unchanged_on_later_conflict() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    let versioned = r#"version = "v1alpha10"

[workspaces.alpha]
workdir = "/workspace/alpha"

[workspaces.prod]
workdir = "/workspace/prod"
"#;
    std::fs::write(&paths.config_file, versioned).unwrap();
    let existing_prod =
        format!("version = \"{CURRENT_WORKSPACE_VERSION}\"\nworkdir = \"/other\"\n");
    std::fs::write(paths.workspaces_dir.join("prod.toml"), &existing_prod).unwrap();
    let before_tree = workspace_tree_bytes(&paths);

    let err = AppConfig::load_or_init(&paths).unwrap_err();
    assert!(
        err.to_string()
            .contains("already exists with different contents")
    );
    assert_eq!(
        std::fs::read(&paths.config_file).unwrap(),
        versioned.as_bytes()
    );
    assert_eq!(workspace_tree_bytes(&paths), before_tree);
    assert!(!paths.workspaces_dir.join("alpha.toml").exists());
}

#[test]
fn failed_split_validation_leaves_global_and_workspace_files_unchanged() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();

    let global_before = b"version = \"v1alpha10\"\n";
    let alpha_before = b"version = \"v1alpha8\"\nworkdir = \"/workspace/alpha\"\n\n[[mounts]]\nsrc = \"/tmp/alpha\"\ndst = \"/workspace/alpha\"\n";
    let broken_before = b"version = \"v1alpha8\"\nworkdir = \"/workspace/broken\"\n";
    std::fs::write(&paths.config_file, global_before).unwrap();
    std::fs::write(paths.workspaces_dir.join("alpha.toml"), alpha_before).unwrap();
    std::fs::write(paths.workspaces_dir.join("broken.toml"), broken_before).unwrap();
    let workspace_tree_before = workspace_tree_bytes(&paths);

    let err = AppConfig::load_or_init(&paths).unwrap_err();

    assert!(err.to_string().contains("mount"), "{err:#}");
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), global_before);
    assert_eq!(workspace_tree_bytes(&paths), workspace_tree_before);
}

#[test]
fn load_split_config_leaves_semantically_invalid_migration_unchanged() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();

    let global_before = b"version = \"v1alpha10\"\n\n[account_bindings]\nclaude = \"missing\"\n";
    let workspace_before = b"version = \"v1alpha8\"\nworkdir = \"/workspace/prod\"\n";
    std::fs::write(&paths.config_file, global_before).unwrap();
    std::fs::write(paths.workspaces_dir.join("prod.toml"), workspace_before).unwrap();
    let workspace_tree_before = workspace_tree_bytes(&paths);

    let err = load_split_config(
        &paths,
        Some(String::from_utf8_lossy(global_before).into_owned()),
    )
    .unwrap_err();

    assert!(err.to_string().contains("unknown account"), "{err:#}");
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), global_before);
    assert_eq!(workspace_tree_bytes(&paths), workspace_tree_before);
    assert_no_staged_writes(&paths);
}
