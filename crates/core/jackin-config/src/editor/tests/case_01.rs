// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn config_lock_fresh_editor_bootstraps_without_recursive_acquisition() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let editor = ConfigEditor::open(&paths).unwrap();
    editor.save().unwrap();
    assert!(paths.config_file.exists());
    assert!(paths.config_file.with_file_name("config.lock").exists());
    assert!(!publication_journal_path(&paths.config_file).exists());
}

#[test]
fn config_lock_competing_editors_serialize() {
    use std::sync::mpsc;
    use std::time::Duration;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let first = ConfigEditor::open(&paths).unwrap();
    let (opened_tx, opened_rx) = mpsc::channel();
    let second_paths = paths.clone();
    let waiter = std::thread::spawn(move || {
        let second = ConfigEditor::open(&second_paths).unwrap();
        opened_tx.send(()).unwrap();
        second
    });
    assert!(opened_rx.recv_timeout(Duration::from_millis(20)).is_err());
    drop(first);
    opened_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(waiter.join().unwrap());
}

#[test]
fn open_leaves_versioned_config_unchanged_when_workspace_split_conflicts() {
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
        format!(
            "version = \"{}\"\nworkdir = \"/other\"\n",
            crate::CURRENT_WORKSPACE_VERSION
        ),
    )
    .unwrap();

    let err = ConfigEditor::open(&paths).unwrap_err();
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
fn open_leaves_semantically_invalid_migration_unchanged() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();

    let global_before = b"version = \"v1alpha10\"\n\n[account_bindings]\nclaude = \"missing\"\n";
    let workspace_before = b"version = \"v1alpha8\"\nworkdir = \"/workspace/prod\"\n";
    std::fs::write(&paths.config_file, global_before).unwrap();
    std::fs::write(paths.workspaces_dir.join("prod.toml"), workspace_before).unwrap();
    let workspace_tree_before = workspace_tree_bytes(&paths);

    let err = ConfigEditor::open(&paths).unwrap_err();

    assert!(err.to_string().contains("unknown account"), "{err:#}");
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), global_before);
    assert_eq!(workspace_tree_bytes(&paths), workspace_tree_before);
    assert_no_staged_writes(&paths);
}

#[test]
fn open_admits_dangling_launch_instance_account_for_repair() {
    // Multi-account × #1006 reconciliation: unlike dangling bindings
    // (above), a launch instance referencing a not-yet-registered account
    // must not brick the editor — the account is added through the editor
    // itself, and instance references are enforced at save/load instead.
    // The versionless fixture forces a pending schema migration so the
    // open-path gate actually runs.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        "default_launch = [\"amp-main\"]\n\n[agent_configurations.amp-main]\nagent = \"amp\"\naccount = \"amp-profile\"\n",
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account(
            "amp-profile",
            &crate::AccountConfig {
                enabled: true,
                name: "Amp profile".into(),
                provider: crate::AiProvider::Amp,
                credential: crate::AccountCredential::ApiKey {
                    value: EnvValue::from("test-amp-key"),
                    base_url: None,
                    model: None,
                },
            },
        )
        .unwrap();
    let config = editor.save().unwrap();
    assert!(config.accounts.contains_key("amp-profile"));
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    assert_eq!(config.accounts, reloaded.accounts);
}

#[test]
fn open_leaves_every_workspace_file_unchanged_on_later_split_conflict() {
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
    let existing_prod = format!(
        "version = \"{}\"\nworkdir = \"/other\"\n",
        crate::CURRENT_WORKSPACE_VERSION
    );
    std::fs::write(paths.workspaces_dir.join("prod.toml"), &existing_prod).unwrap();
    let before_tree = workspace_tree_bytes(&paths);

    let err = ConfigEditor::open(&paths).unwrap_err();
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
fn open_leaves_standalone_old_config_unchanged_when_split_syntax_fails() {
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

    let err = ConfigEditor::open(&paths).unwrap_err();

    assert!(err.to_string().contains("parsing"), "{err:#}");
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), global_before);
    assert_eq!(workspace_tree_bytes(&paths), workspace_tree_before);
}

#[test]
fn save_commit_failure_does_not_leave_earlier_files_committed() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    AppConfig::load_or_init(&paths).unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    let alpha_path = paths.workspaces_dir.join("alpha.toml");
    let prod_path = paths.workspaces_dir.join("prod.toml");
    let workspace = |name: &str| {
        format!(
            "version = \"{}\"\nworkdir = \"/workspace/{name}\"\n",
            crate::CURRENT_WORKSPACE_VERSION
        )
    };
    std::fs::write(&alpha_path, workspace("alpha")).unwrap();
    std::fs::write(&prod_path, workspace("prod")).unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&EnvScope::Global, "GLOBAL", "after".into())
        .unwrap();
    editor
        .set_env_var(
            &EnvScope::Workspace("alpha".to_owned()),
            "ALPHA",
            "after".into(),
        )
        .unwrap();
    editor
        .set_env_var(
            &EnvScope::Workspace("prod".to_owned()),
            "PROD",
            "after".into(),
        )
        .unwrap();

    let global_before = std::fs::read(&paths.config_file).unwrap();
    let alpha_before = std::fs::read(&alpha_path).unwrap();
    std::fs::remove_file(&prod_path).unwrap();
    std::fs::create_dir(&prod_path).unwrap();

    let err = editor.save().unwrap_err();
    assert!(err.to_string().contains("renaming"), "{err:#}");
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), global_before);
    assert_eq!(std::fs::read(&alpha_path).unwrap(), alpha_before);
    let staged_leaks: Vec<_> = std::fs::read_dir(&paths.config_dir)
        .unwrap()
        .chain(std::fs::read_dir(&paths.workspaces_dir).unwrap())
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(
        staged_leaks.is_empty(),
        "rollback left staged files: {staged_leaks:?}"
    );
    assert!(
        !publication_journal_path(&paths.config_file).exists(),
        "failed save left a publication journal behind"
    );
}

#[test]
fn set_env_var_creates_global_env_table() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(
            &EnvScope::Global,
            "API_TOKEN",
            "op://Personal/api/token".into(),
        )
        .unwrap();
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[env]"), "missing [env] table: {out}");
    assert!(
        out.contains(r#"API_TOKEN = "op://Personal/api/token""#),
        "missing entry: {out}"
    );
}

#[test]
fn set_env_var_upserts_workspace_agent_scope() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.prod]
workdir = "/workspace/prod"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(
            &EnvScope::WorkspaceRole {
                workspace: "prod".to_owned(),
                role: "agent-smith".to_owned(),
            },
            "SERVICE_TOKEN",
            "op://Work/OpenAI/default".into(),
        )
        .unwrap();
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "prod");
    assert!(
        out.contains("[roles.agent-smith.env]"),
        "missing nested table: {out}"
    );
    assert!(
        out.contains(r#"SERVICE_TOKEN = "op://Work/OpenAI/default""#),
        "missing entry: {out}"
    );
}

#[test]
fn set_env_var_overwrites_existing_value() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[env]
API_TOKEN = "old-value"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&EnvScope::Global, "API_TOKEN", "new-value".into())
        .unwrap();
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains(r#"API_TOKEN = "new-value""#), "{out}");
    assert!(!out.contains("old-value"), "{out}");
}

#[test]
fn remove_env_var_returns_true_when_present() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[env]
API_TOKEN = "x"
OTHER = "y"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_env_var(&EnvScope::Global, "API_TOKEN");
    editor.save().unwrap();

    assert!(removed);
    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("API_TOKEN"), "{out}");
    assert!(out.contains(r#"OTHER = "y""#), "sibling gone: {out}");
}

#[test]
fn remove_env_var_returns_false_when_absent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_env_var(&EnvScope::Global, "API_TOKEN");
    editor.save().unwrap();

    assert!(!removed);
}
