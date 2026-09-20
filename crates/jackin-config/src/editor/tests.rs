// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `editor`.
use super::*;
use crate::RoleSource;
use jackin_core::{Agent, WorkspaceName};
fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}
use tempfile::tempdir;

fn workspace_file_contents(paths: &JackinPaths, name: &str) -> String {
    std::fs::read_to_string(paths.workspaces_dir.join(format!("{name}.toml"))).unwrap()
}

fn workspace_tree_bytes(paths: &JackinPaths) -> Option<Vec<(String, Vec<u8>)>> {
    let entries = match std::fs::read_dir(&paths.workspaces_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => panic!("reading workspace tree: {error}"),
    };
    let mut files = entries
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read(entry.path()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Some(files)
}

fn assert_no_staged_writes(paths: &JackinPaths) {
    for directory in [&paths.config_dir, &paths.workspaces_dir] {
        let entries = std::fs::read_dir(directory).unwrap();
        for entry in entries {
            let entry = entry.unwrap();
            assert!(
                !entry.file_name().to_string_lossy().contains(".tmp."),
                "staged file leaked: {}",
                entry.path().display()
            );
        }
    }
}

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

#[test]
fn remove_env_var_agent_scope() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[roles.agent-smith]
git = "https://example.com/a.git"
"#,
    )
    .unwrap();

    let scope = EnvScope::Role("agent-smith".to_owned());
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&scope, "LOG_LEVEL", "debug".into())
        .unwrap();
    assert!(
        editor.remove_env_var(&scope, "LOG_LEVEL"),
        "first remove should return true"
    );
    assert!(
        !editor.remove_env_var(&scope, "LOG_LEVEL"),
        "second remove should return false"
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("LOG_LEVEL"), "key not purged: {out}");
}

#[test]
fn remove_env_var_workspace_scope() {
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

    let scope = EnvScope::Workspace("prod".to_owned());
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&scope, "DB_URL", "op://Work/Prod/db-url".into())
        .unwrap();
    assert!(
        editor.remove_env_var(&scope, "DB_URL"),
        "first remove should return true"
    );
    assert!(
        !editor.remove_env_var(&scope, "DB_URL"),
        "second remove should return false"
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("DB_URL"), "key not purged: {out}");
}

#[test]
fn remove_env_var_workspace_agent_scope() {
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

    let scope = EnvScope::WorkspaceRole {
        workspace: "prod".to_owned(),
        role: "agent-smith".to_owned(),
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&scope, "SERVICE_TOKEN", "op://Work/OpenAI/default".into())
        .unwrap();
    assert!(
        editor.remove_env_var(&scope, "SERVICE_TOKEN"),
        "first remove should return true"
    );
    assert!(
        !editor.remove_env_var(&scope, "SERVICE_TOKEN"),
        "second remove should return false"
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("SERVICE_TOKEN"), "key not purged: {out}");
}

#[test]
fn remove_env_var_leaves_sibling_keys_intact() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&EnvScope::Global, "KEY_A", "value-a".into())
        .unwrap();
    editor
        .set_env_var(&EnvScope::Global, "KEY_B", "value-b".into())
        .unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    assert!(editor.remove_env_var(&EnvScope::Global, "KEY_A"));
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("KEY_A"), "KEY_A still present: {out}");
    assert!(
        out.contains(r#"KEY_B = "value-b""#),
        "sibling KEY_B gone: {out}"
    );
}

#[test]
fn set_env_comment_adds_line_above_key() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[env]
API_TOKEN = "op://vault-id/item-id/field"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_env_comment(
        &EnvScope::Global,
        "API_TOKEN",
        Some("op://Personal/Google/password"),
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        out.contains("# op://Personal/Google/password\nAPI_TOKEN"),
        "expected comment directly above key: {out}"
    );
}

#[test]
fn set_env_comment_replaces_existing_comment() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        "[env]\n# old annotation\nAPI_TOKEN = \"x\"\n",
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_env_comment(&EnvScope::Global, "API_TOKEN", Some("new annotation"));
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("# new annotation"), "{out}");
    assert!(!out.contains("# old annotation"), "{out}");
}

#[test]
fn set_env_comment_none_removes_annotation() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        "[env]\n# some note\nAPI_TOKEN = \"x\"\n",
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_env_comment(&EnvScope::Global, "API_TOKEN", None);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("# some note"), "{out}");
    assert!(
        out.contains(r#"API_TOKEN = "x""#),
        "key still present: {out}"
    );
}

#[test]
fn mutating_sibling_preserves_comment_above_other_key() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let original = "[env]\n# rotate quarterly\nAPI_TOKEN = \"x\"\nOTHER = \"y\"\n";
    std::fs::write(&paths.config_file, original).unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&EnvScope::Global, "OTHER", "z".into())
        .unwrap();
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        out.contains("# rotate quarterly\nAPI_TOKEN = \"x\""),
        "sibling mutation wiped adjacent comment: {out}"
    );
    assert!(out.contains(r#"OTHER = "z""#), "{out}");
}

#[test]
fn mutating_one_workspace_preserves_comments_in_another() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(
        paths.workspaces_dir.join("a.toml"),
        r#"# workspace a — keep this comment
workdir = "/a"
"#,
    )
    .unwrap();
    std::fs::write(
        paths.workspaces_dir.join("b.toml"),
        r#"# workspace b — also keep
workdir = "/b"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&EnvScope::Workspace("a".to_owned()), "K", "v".into())
        .unwrap();
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "b");
    assert!(out.contains("# workspace b — also keep"), "{out}");
    let out_a = workspace_file_contents(&paths, "a");
    assert!(out_a.contains("K = \"v\""), "{out_a}");
    let global = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!global.contains("[workspaces."), "{global}");
}

#[test]
fn fixture_round_trip_is_byte_identical() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    let original = include_str!("../fixtures/config.round_trip.toml");
    std::fs::write(&paths.config_file, original).unwrap();

    let editor = ConfigEditor::open(&paths).unwrap();
    editor.save().unwrap();

    let round_tripped = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        !round_tripped.contains("[workspaces."),
        "global file should contain only global config after split:\n{round_tripped}"
    );
    assert!(paths.workspaces_dir.join("prod.toml").exists());
    assert!(paths.workspaces_dir.join("playground.toml").exists());
}

#[test]
fn idempotent_save_is_byte_identical() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    let original = r#"version = "v1alpha3"
# Top-of-file note about this config
[github]
auth_forward = "sync"

# Roles we trust
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

# My production workspace
[workspaces.prod]
workdir = "/workspace/prod"

[[workspaces.prod.mounts]]
src = "/workspace/prod"
dst = "/workspace/prod"

[workspaces.prod.env]
# Rotate quarterly (last: 2026-Q1)
API_TOKEN = "op://Personal/api/token"
"#;
    std::fs::write(&paths.config_file, original).unwrap();

    let editor = ConfigEditor::open(&paths).unwrap();
    editor.save().unwrap();

    let global = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!global.contains("[workspaces."), "{global}");
    let workspace = workspace_file_contents(&paths, "prod");
    assert!(
        workspace.contains(r#"workdir = "/workspace/prod""#),
        "{workspace}"
    );
    assert!(
        workspace.contains(r#"API_TOKEN = "op://Personal/api/token""#),
        "{workspace}"
    );
}

#[test]
#[cfg(unix)]
fn saved_file_is_0600_on_unix() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nK = \"v\"\n").unwrap();

    let editor = ConfigEditor::open(&paths).unwrap();
    editor.save().unwrap();

    let perms = std::fs::metadata(&paths.config_file).unwrap().permissions();
    assert_eq!(perms.mode() & 0o777, 0o600, "config file must be 0600");
}

#[test]
fn save_leaves_no_tmp_file_on_success() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nK = \"v\"\n").unwrap();

    let editor = ConfigEditor::open(&paths).unwrap();
    editor.save().unwrap();

    let tmp_path = paths.config_file.with_extension("tmp");
    assert!(!tmp_path.exists(), "expected .tmp to be renamed away");
}

/// `save()` must reject before rename so an invalid mutation
/// can't brick subsequent CLI commands.
#[test]
fn save_rejects_invalid_candidate_and_preserves_on_disk_config() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    std::fs::write(&paths.config_file, "[env]\nVALID_KEY = \"valid-value\"\n").unwrap();
    AppConfig::load_or_init(&paths).unwrap();
    let baseline = std::fs::read_to_string(&paths.config_file).unwrap();

    // Inject `[roles.ghost.env]` without the required
    // `[roles.ghost].git` — fails serde parsing.
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.insert_at_path(
        &["roles".to_owned(), "ghost".to_owned(), "env".to_owned()],
        "LOG_LEVEL",
        "debug",
    );

    let err = editor.save().unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("rejecting candidate config"),
        "expected rejection message; got: {msg}"
    );

    let after = std::fs::read_to_string(&paths.config_file).unwrap();
    assert_eq!(
        after, baseline,
        "rejected save must leave the on-disk config byte-identical"
    );

    // No leftover .tmp file.
    let tmp_path = paths.config_file.with_extension("tmp");
    assert!(
        !tmp_path.exists(),
        "rejected save must clean up its temp file at {}",
        tmp_path.display()
    );
}

#[test]
fn save_rejects_reserved_name_candidate_and_preserves_on_disk_config() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    std::fs::write(&paths.config_file, "[env]\nVALID_KEY = \"v\"\n").unwrap();
    AppConfig::load_or_init(&paths).unwrap();
    let baseline = std::fs::read_to_string(&paths.config_file).unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    // Bypass the CLI pre-flight via the unchecked setter.
    editor
        .set_env_var(&EnvScope::Global, "DOCKER_HOST", "tcp://bad".into())
        .unwrap();

    let err = editor.save().unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("DOCKER_HOST") && msg.contains("reserved"),
        "expected reserved-name rejection; got: {msg}"
    );

    let after = std::fs::read_to_string(&paths.config_file).unwrap();
    assert_eq!(
        after, baseline,
        "rejected save must not touch on-disk config"
    );
}

#[test]
fn editor_save_atomic_staging_failure_preserves_every_original_file() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nGLOBAL = \"before\"\n").unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    let workspace_path = paths.workspaces_dir.join("prod.toml");
    std::fs::write(
        &workspace_path,
        "workdir = \"/workspace/prod\"\n\n[[mounts]]\nsrc = \"/workspace/prod\"\ndst = \"/workspace/prod\"\n",
    )
    .unwrap();

    AppConfig::load_or_init(&paths).unwrap();
    let global_before = std::fs::read(&paths.config_file).unwrap();
    let workspace_before = std::fs::read(&workspace_path).unwrap();
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&EnvScope::Global, "GLOBAL", "after".into())
        .unwrap();
    editor
        .set_env_var(
            &EnvScope::Workspace("prod".to_owned()),
            "LOCAL",
            "after".into(),
        )
        .unwrap();

    let mut stage_number = 0;
    let err = editor
        .save_with_stager(|path, contents| {
            stage_number += 1;
            if stage_number == 2 {
                return Err(std::io::Error::other("injected second-stage failure").into());
            }
            stage_atomic_write(path, contents)
        })
        .unwrap_err();
    assert!(err.to_string().contains("injected second-stage failure"));
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), global_before);
    assert_eq!(std::fs::read(&workspace_path).unwrap(), workspace_before);
    let staged_leaks: Vec<_> = std::fs::read_dir(&paths.config_dir)
        .unwrap()
        .chain(std::fs::read_dir(&paths.workspaces_dir).unwrap())
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(
        staged_leaks.is_empty(),
        "leftover staged files: {staged_leaks:?}"
    );
}

#[test]
fn editor_save_second_commit_failure_rolls_back_every_original_file() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        "# global comment\n[env]\nGLOBAL = \"before\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    let workspace_path = paths.workspaces_dir.join("prod.toml");
    std::fs::write(
        &workspace_path,
        "# workspace comment\nworkdir = \"/workspace/prod\"\n\n[[mounts]]\nsrc = \"/workspace/prod\"\ndst = \"/workspace/prod\"\n",
    )
    .unwrap();

    AppConfig::load_or_init(&paths).unwrap();
    let global_before = std::fs::read(&paths.config_file).unwrap();
    let workspace_before = std::fs::read(&workspace_path).unwrap();
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&EnvScope::Global, "GLOBAL", "after".into())
        .unwrap();
    editor
        .set_env_var(
            &EnvScope::Workspace("prod".to_owned()),
            "LOCAL",
            "after".into(),
        )
        .unwrap();

    // Fail the workspace rename after the global rename already committed:
    // once the workspace file is staged (second stage call), swap its
    // target for a non-empty directory so the rename fails and the
    // transaction must restore the already-committed global file.
    // Rollback restores bytes, not modes (restore re-stages through a
    // fresh 0o600 temp file), so only contents are asserted below.
    let mut stage_number = 0;
    let err = editor
        .save_with_stager(|path, contents| {
            let staged = stage_atomic_write(path, contents)?;
            stage_number += 1;
            if stage_number == 2 {
                std::fs::remove_file(&workspace_path)?;
                std::fs::create_dir(&workspace_path)?;
                std::fs::write(workspace_path.join("blocker"), b"not a config")?;
            }
            Ok(staged)
        })
        .unwrap_err();
    assert_eq!(stage_number, 2);
    assert!(
        err.to_string().contains("renaming"),
        "expected the injected rename failure; got: {err:#}"
    );
    std::fs::remove_file(workspace_path.join("blocker")).unwrap();
    std::fs::remove_dir(&workspace_path).unwrap();
    std::fs::write(&workspace_path, &workspace_before).unwrap();
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), global_before);
    assert_eq!(std::fs::read(&workspace_path).unwrap(), workspace_before);

    let leftovers: Vec<_> = std::fs::read_dir(&paths.config_dir)
        .unwrap()
        .chain(std::fs::read_dir(&paths.workspaces_dir).unwrap())
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.contains(".tmp.")
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "leftover transaction files: {leftovers:?}"
    );

    // The failed editor released its lock only after rollback completed; a
    // fresh editor can immediately publish the same tree again.
    ConfigEditor::open(&paths).unwrap().save().unwrap();
}

#[test]
fn editor_save_keeps_exclusive_lock_during_publication() {
    use std::sync::mpsc;
    use std::time::Duration;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nGLOBAL = \"before\"\n").unwrap();
    AppConfig::load_or_init(&paths).unwrap();

    let (go_tx, go_rx) = mpsc::channel();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (opened_tx, opened_rx) = mpsc::channel();
    let probe_paths = paths.clone();
    let probe = std::thread::spawn(move || {
        go_rx.recv().unwrap();
        attempt_tx.send(()).unwrap();
        opened_tx
            .send(ConfigEditor::open(&probe_paths).is_ok())
            .unwrap();
    });

    let editor = ConfigEditor::open(&paths).unwrap();
    let mut stage_number = 0;
    editor
        .save_with_stager(|path, contents| {
            let staged = stage_atomic_write(path, contents)?;
            stage_number += 1;
            if stage_number == 1 {
                go_tx.send(()).unwrap();
                attempt_rx.recv().unwrap();
                assert!(
                    opened_rx.recv_timeout(Duration::from_millis(50)).is_err(),
                    "a second editor observed publication before the first released its lock"
                );
            }
            Ok(staged)
        })
        .unwrap();
    probe.join().unwrap();
    assert!(opened_rx.recv_timeout(Duration::from_secs(1)).unwrap());
}

#[test]
fn editor_save_repeated_is_byte_idempotent_for_global_and_workspace_files() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nGLOBAL = \"value\"\n").unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    let workspace_path = paths.workspaces_dir.join("prod.toml");
    std::fs::write(
        &workspace_path,
        "workdir = \"/workspace/prod\"\n\n[[mounts]]\nsrc = \"/workspace/prod\"\ndst = \"/workspace/prod\"\n",
    )
    .unwrap();

    AppConfig::load_or_init(&paths).unwrap();
    ConfigEditor::open(&paths).unwrap().save().unwrap();
    let global_after_first_save = std::fs::read(&paths.config_file).unwrap();
    let workspace_after_first_save = std::fs::read(&workspace_path).unwrap();

    ConfigEditor::open(&paths).unwrap().save().unwrap();
    assert_eq!(
        std::fs::read(&paths.config_file).unwrap(),
        global_after_first_save
    );
    assert_eq!(
        std::fs::read(&workspace_path).unwrap(),
        workspace_after_first_save
    );
}

#[test]
fn editor_save_rejects_invalid_workspace_stem_before_any_write() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nSAFE = \"before\"\n").unwrap();
    AppConfig::load_or_init(&paths).unwrap();
    let before = std::fs::read(&paths.config_file).unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(
            &EnvScope::Workspace("../escape".to_owned()),
            "VALUE",
            "after".into(),
        )
        .unwrap();
    editor.save().unwrap_err();
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), before);
    assert!(!paths.config_dir.join("escape.toml").exists());
}

// ---- mount tests ----

#[test]
fn add_mount_unscoped_creates_single_mount_entry() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.add_mount(
        "shared-home",
        MountConfig {
            src: "/home/user".to_owned(),
            dst: "/workspace/home".to_owned(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        },
        None,
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[docker.mounts.shared-home]"), "{out}");
    assert!(out.contains(r#"src = "/home/user""#), "{out}");
}

#[test]
fn add_mount_scoped_creates_nested_entry() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    // Behavioral equivalence with AppConfig::add_mount:
    // scope is the OUTER key; name is the INNER key.
    // So scope=agent-smith produces [docker.mounts.agent-smith] with creds = {...}
    editor.add_mount(
        "creds",
        MountConfig {
            src: "/run/secrets/x".to_owned(),
            dst: "/secrets/x".to_owned(),
            readonly: true,
            isolation: crate::MountIsolation::Shared,
        },
        Some("agent-smith"),
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    // The scoped shape: [docker.mounts.agent-smith] with creds sub-table
    assert!(out.contains("[docker.mounts.agent-smith]"), "{out}");
    assert!(out.contains(r#"src = "/run/secrets/x""#), "{out}");
    assert!(out.contains("readonly = true"), "{out}");
}

#[test]
fn remove_mount_unscoped_deletes_entry() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[docker.mounts.shared-home]
src = "/home/user"
dst = "/workspace/home"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_mount("shared-home", None);
    editor.save().unwrap();

    assert!(removed);
    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("shared-home"), "{out}");
}

#[test]
fn remove_mount_returns_false_for_missing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_mount("nope", None);
    editor.save().unwrap();
    assert!(!removed);
}

#[test]
fn remove_mount_scoped_last_entry_deletes_scope_table() {
    // Matches AppConfig::remove_mount cleanup: when the last named mount
    // in a scope is removed, the scope table itself is removed.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[docker.mounts.agent-smith]
creds = { src = "/run/secrets/x", dst = "/secrets/x" }
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_mount("creds", Some("agent-smith"));
    editor.save().unwrap();

    assert!(removed);
    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        !out.contains("agent-smith"),
        "empty scope table should be gone: {out}"
    );
}

#[test]
fn remove_mount_scoped_preserves_scope_when_siblings_remain() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[docker.mounts.agent-smith]
creds = { src = "/a", dst = "/a" }
logs = { src = "/b", dst = "/b" }
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_mount("creds", Some("agent-smith"));
    editor.save().unwrap();

    assert!(removed);
    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        out.contains("[docker.mounts.agent-smith]"),
        "scope table should still exist: {out}"
    );
    assert!(!out.contains("creds"), "{out}");
    assert!(out.contains("logs"), "{out}");
}

#[test]
fn set_agent_trust_toggles_trusted_field() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[roles.my-role]
git = "https://example.com/a.git"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_agent_trust("my-role", true);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("trusted = true"), "{out}");
}

#[test]
fn set_agent_trust_false_removes_field() {
    // Canonical TOML representation of trusted=false is absent (serde
    // skip_serializing_if on RoleSource::trusted).
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[roles.my-role]
git = "x"
trusted = true
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_agent_trust("my-role", false);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("trusted"), "{out}");
}

#[test]
fn upsert_builtin_agent_creates_entry_when_missing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_builtin_agent(
        "agent-smith",
        "https://github.com/jackin-project/jackin-agent-smith.git",
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[roles.agent-smith]"), "{out}");
    assert!(out.contains("trusted = true"), "{out}");
}

#[test]
fn create_workspace_adds_table() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let mount_src = temp.path().join("src");
    std::fs::create_dir_all(&mount_src).unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let ws = WorkspaceConfig {
        workdir: "/workspace/new".to_owned(),
        mounts: vec![MountConfig {
            src: mount_src.display().to_string(),
            dst: "/workspace/new".to_owned(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        }],
        ..Default::default()
    };

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .create_workspace(&WorkspaceName::parse("new-ws").unwrap(), ws)
        .unwrap();
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "new-ws");
    assert!(
        !std::fs::read_to_string(&paths.config_file)
            .unwrap()
            .contains("[workspaces.")
    );
    assert!(out.contains(r#"workdir = "/workspace/new""#), "{out}");
}

#[test]
fn create_workspace_rejects_invalid_workdir_mount_combo() {
    // Editor delegates to AppConfig::create_workspace, which validates
    // that the workdir is equal-to / inside / parent-of some mount dst.
    // A workdir that doesn't line up with any mount dst must be rejected.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let mount_src = temp.path().join("src");
    std::fs::create_dir_all(&mount_src).unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let ws = WorkspaceConfig {
        workdir: "/elsewhere".to_owned(),
        mounts: vec![MountConfig {
            src: mount_src.display().to_string(),
            dst: "/workspace/unrelated".to_owned(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        }],
        ..Default::default()
    };

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let err = editor
        .create_workspace(&WorkspaceName::parse("bad-ws").unwrap(), ws)
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("workspace") || msg.contains("mount") || msg.contains("workdir"),
        "expected validation error mentioning workspace/mount/workdir: {msg}"
    );
}

#[test]
fn set_last_agent_preserves_other_fields() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let original = r#"[workspaces.prod]
workdir = "/workspace/prod"
default_role = "agent-smith"
"#;
    std::fs::write(&paths.config_file, original).unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_last_agent(&WorkspaceName::parse("prod").unwrap(), "agent-smith");
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "prod");
    assert!(out.contains(r#"last_role = "agent-smith""#), "{out}");
    assert!(out.contains(r#"default_role = "agent-smith""#), "{out}");
}

#[test]
fn upsert_agent_source_preserves_existing_env() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[roles.foo]
git = "OLD"

[roles.foo.env]
MY_VAR = "preserved"
"#,
    )
    .unwrap();

    let source = RoleSource {
        git: "NEW".to_owned(),
        trusted: true,
        env: BTreeMap::new(),
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_agent_source("foo", &source);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains(r#"git = "NEW""#), "{out}");
    assert!(out.contains(r#"MY_VAR = "preserved""#), "{out}");
}

#[test]
fn remove_workspace_deletes_table() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.a]
workdir = "/a"

[workspaces.b]
workdir = "/b"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_workspace(&wn("a")).unwrap();
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("[workspaces.a]"), "{out}");
    assert!(!paths.workspaces_dir.join("a.toml").exists());
    assert!(paths.workspaces_dir.join("b.toml").exists());
}

#[test]
fn rename_workspace_preserves_nested_fields() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.old-name]
workdir = "/a"

[[workspaces.old-name.mounts]]
src = "/s"
dst = "/a"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .rename_workspace(
            &WorkspaceName::parse("old-name").unwrap(),
            &WorkspaceName::parse("new-name").unwrap(),
        )
        .unwrap();
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "new-name");
    assert!(!paths.workspaces_dir.join("old-name.toml").exists());
    assert!(
        out.contains(r#"workdir = "/a""#),
        "nested field preserved: {out}"
    );
    assert!(out.contains("[[mounts]]"), "array table preserved: {out}");
    assert!(!out.contains("old-name"), "{out}");
}

#[test]
fn rename_workspace_write_failure_preserves_old_file() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(&paths.config_file, "").unwrap();
    std::fs::write(
        paths.workspaces_dir.join("old-name.toml"),
        r#"workdir = "/a"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .rename_workspace(
            &WorkspaceName::parse("old-name").unwrap(),
            &WorkspaceName::parse("new-name").unwrap(),
        )
        .unwrap();
    std::fs::create_dir(paths.workspaces_dir.join("new-name.toml")).unwrap();

    let err = editor.save().unwrap_err();

    let chain = format!("{err:#}");
    assert!(
        chain.contains("Is a directory") || chain.contains("is a directory"),
        "{chain}"
    );
    assert!(
        paths.workspaces_dir.join("old-name.toml").exists(),
        "failed rename save must leave the original workspace file in place"
    );
}

#[test]
fn rename_workspace_rejects_collision() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.a]
workdir = "/a"

[workspaces.b]
workdir = "/b"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let err = editor
        .rename_workspace(
            &WorkspaceName::parse("a").unwrap(),
            &WorkspaceName::parse("b").unwrap(),
        )
        .unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
}

#[test]
fn rename_workspace_rejects_empty_new_name() {
    let err = WorkspaceName::parse("").unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[test]
fn set_env_var_writes_inline_table_for_op_ref() {
    use jackin_core::{EnvValue, OpRef};

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\n").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(
            &EnvScope::Global,
            "SERVICE_TOKEN",
            EnvValue::OpRef(OpRef {
                op: "op://abc/def/fld".into(),
                path: "Private/Claude/security/auth token".into(),
                account: None,
                on_demand: false,
            }),
        )
        .unwrap();
    editor.save().unwrap();

    let serialized = std::fs::read_to_string(&paths.config_file).unwrap();
    // Inline-table form, not a scalar string with quoted JSON.
    assert!(
            serialized.contains(r#"SERVICE_TOKEN = { op = "op://abc/def/fld", path = "Private/Claude/security/auth token" }"#),
            "expected inline-table emit, got:\n{serialized}"
        );
}

#[test]
fn set_env_var_persists_op_ref_account() {
    use jackin_core::{EnvValue, OpRef};

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\n").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(
            &EnvScope::Global,
            "SERVICE_TOKEN",
            EnvValue::OpRef(OpRef {
                op: "op://abc/def/fld".into(),
                path: "Work/Claude/auth token".into(),
                account: Some("WORKACCT".into()),
                on_demand: false,
            }),
        )
        .unwrap();
    editor.save().unwrap();

    // The account must land on the inline table; without it a
    // non-default-account ref resolves against op's default account.
    let saved = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
            saved.contains(
                r#"SERVICE_TOKEN = { op = "op://abc/def/fld", path = "Work/Claude/auth token", account = "WORKACCT" }"#
            ),
            "expected account key in inline table, got:\n{saved}"
        );
}

#[test]
fn set_env_var_rejects_account_owned_credentials_without_persisting_value() {
    use jackin_core::EnvValue;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nSAFE = \"keep\"\n").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let error = editor
        .set_env_var(
            &EnvScope::Global,
            "ANTHROPIC_API_KEY",
            EnvValue::Plain("account-owned-sentinel".into()),
        )
        .unwrap_err();

    assert!(error.to_string().contains("account credentials"));
    editor.save().unwrap();
    let serialized = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!serialized.contains("account-owned-sentinel"));
    assert!(serialized.contains("SAFE = \"keep\""));
}

#[test]
fn set_env_var_writes_scalar_string_for_plain() {
    use jackin_core::EnvValue;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\n").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(
            &EnvScope::Global,
            "DB_URL",
            EnvValue::Plain("postgres://localhost".into()),
        )
        .unwrap();
    editor.save().unwrap();

    let serialized = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        serialized.contains(r#"DB_URL = "postgres://localhost""#),
        "expected scalar-string emit, got:\n{serialized}"
    );
}

/// Pin the cleanup path for the github kind: clearing both the
/// `auth_forward` field and the `[github.env]` keys at workspace
/// scope must leave NO empty `[workspaces.<ws>.github]` or
/// `[workspaces.<ws>.github.env]` tables on disk. Regression guard
/// for the orphan-table I1 finding.
#[test]
fn clearing_workspace_github_prunes_empty_tables() {
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

    // Seed: `[workspaces.prod.github]` with auth_forward + a
    // GH_TOKEN env entry.
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_github_auth_forward(&wn("prod"), Some(GithubAuthMode::Token));
    let env_scope = EnvScope::WorkspaceGithub("prod".to_owned());
    editor
        .set_env_var(&env_scope, "GH_TOKEN", "op://Work/gh/pat".into())
        .unwrap();
    editor.save().unwrap();

    // Sanity: both the kind block and its env subtable land on disk.
    let after_save = workspace_file_contents(&paths, "prod");
    assert!(after_save.contains("[github]"));
    assert!(after_save.contains("auth_forward"));
    assert!(after_save.contains("GH_TOKEN"));

    // Operator presses `D` on github WorkspaceMode (mode → None)
    // and the env diff drops GH_TOKEN.
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_github_auth_forward(&wn("prod"), None);
    assert!(editor.remove_env_var(&env_scope, "GH_TOKEN"));
    editor.save().unwrap();

    let cleaned = workspace_file_contents(&paths, "prod");
    assert!(
        !cleaned.contains("github"),
        "stale [github] / [github.env] table left on disk:\n{cleaned}"
    );
    assert!(
        cleaned.contains("workdir"),
        "workspace block was wrongly removed by the cascade:\n{cleaned}"
    );
    assert!(
        cleaned.contains("workdir"),
        "sibling workdir field was wrongly stripped:\n{cleaned}"
    );
}

/// Same cascade contract for the per-(workspace × role) layer.
#[test]
fn clearing_workspace_role_github_prunes_empty_tables() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.prod]
workdir = "/workspace/prod"

[workspaces.prod.roles.scratch]
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_role_github_auth_forward(
        &wn("prod"),
        "scratch",
        Some(GithubAuthMode::Token),
    );
    let env_scope = EnvScope::WorkspaceRoleGithub {
        workspace: "prod".to_owned(),
        role: "scratch".to_owned(),
    };
    editor
        .set_env_var(&env_scope, "GH_TOKEN", "op://Work/gh/pat".into())
        .unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_role_github_auth_forward(&wn("prod"), "scratch", None);
    assert!(editor.remove_env_var(&env_scope, "GH_TOKEN"));
    editor.save().unwrap();

    let cleaned = workspace_file_contents(&paths, "prod");
    assert!(
        !cleaned.contains("github"),
        "stale [github] / [github.env] table left on disk:\n{cleaned}"
    );
}

/// Clearing GitHub policy preserves unrelated workspace tables.
#[test]
fn clearing_one_kind_preserves_sibling_kinds() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.prod]
workdir = "/workspace/prod"

[workspaces.prod.env]
PRESERVED = "yes"

[workspaces.prod.roles.smith.env]
ALSO_PRESERVED = "yes"

[workspaces.prod.github]
auth_forward = "ignore"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_github_auth_forward(&wn("prod"), None);
    editor.save().unwrap();

    let cleaned = workspace_file_contents(&paths, "prod");
    assert!(
        !cleaned.contains("[github]"),
        "github block should be removed:\n{cleaned}"
    );
    assert!(
        cleaned.contains("PRESERVED"),
        "workspace env must survive:\n{cleaned}"
    );
    assert!(
        cleaned.contains("ALSO_PRESERVED"),
        "role env must survive:\n{cleaned}"
    );
}

/// Removing the last `[…github.env]` key while `[…github]` still
/// has `auth_forward` set must prune ONLY `[…env]`. The kind block
/// stays.
#[test]
fn pruning_empty_env_preserves_kind_block_with_auth_forward() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.prod]
workdir = "/workspace/prod"

[workspaces.prod.github]
auth_forward = "token"

[workspaces.prod.github.env]
GH_TOKEN = "ghp_real"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let env_scope = EnvScope::WorkspaceGithub("prod".to_owned());
    assert!(editor.remove_env_var(&env_scope, "GH_TOKEN"));
    editor.save().unwrap();

    let cleaned = workspace_file_contents(&paths, "prod");
    assert!(
        !cleaned.contains("[github.env]"),
        "empty env subtable must be pruned:\n{cleaned}"
    );
    assert!(
        cleaned.contains("[github]"),
        "kind block must survive (still has auth_forward):\n{cleaned}"
    );
    assert!(
        cleaned.contains("auth_forward = \"token\""),
        "auth_forward value must survive:\n{cleaned}"
    );
}

/// Workspace with sibling content (`allowed_roles`, mounts) must
/// survive a github clear. Position-based prune bound prevents
/// the walker from reaching the workspace identifier slot.
#[test]
fn clearing_github_preserves_workspace_sibling_content() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.prod]
workdir = "/workspace/prod"
allowed_roles = ["agent-smith", "the-architect"]

[workspaces.prod.github]
auth_forward = "token"

[workspaces.prod.github.env]
GH_TOKEN = "ghp_real"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_github_auth_forward(&wn("prod"), None);
    let env_scope = EnvScope::WorkspaceGithub("prod".to_owned());
    assert!(editor.remove_env_var(&env_scope, "GH_TOKEN"));
    editor.save().unwrap();

    let cleaned = workspace_file_contents(&paths, "prod");
    assert!(
        !cleaned.contains("[github"),
        "github / github.env tables should be pruned:\n{cleaned}"
    );
    assert!(
        cleaned.contains("workdir"),
        "workspace block must survive:\n{cleaned}"
    );
    assert!(
        cleaned.contains("workdir"),
        "workdir field must survive:\n{cleaned}"
    );
    assert!(
        cleaned.contains("allowed_roles"),
        "allowed_roles must survive:\n{cleaned}"
    );
}

/// Position-based prune protects against an operator workspace
/// literally named "github" / "claude" / "codex" / "env" — the
/// walk depth is bounded so the workspace identifier slot at
/// path[1] is never reached.
#[test]
fn workspace_named_github_survives_github_clear() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.github]
workdir = "/workspace/edge-case"

[workspaces.github.github]
auth_forward = "ignore"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_github_auth_forward(&wn("github"), None);
    editor.save().unwrap();

    let cleaned = workspace_file_contents(&paths, "github");
    // Inner [github] gone (kind block); workspace file preserved.
    assert!(
        cleaned.contains("workdir"),
        "workspace named 'github' must survive:\n{cleaned}"
    );
    assert!(
        cleaned.contains("workdir"),
        "workdir on workspace 'github' must survive:\n{cleaned}"
    );
}

#[test]
fn set_git_coauthor_trailer_enable_writes_git_table() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_git_coauthor_trailer(true);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("coauthor_trailer = true"), "{out}");
    assert!(out.contains("[git]"), "{out}");
}

#[test]
fn set_git_coauthor_trailer_disable_prunes_git_table() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[git]\ncoauthor_trailer = true\n").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_git_coauthor_trailer(false);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        !out.contains("[git]"),
        "empty [git] table should be pruned: {out}"
    );
    assert!(!out.contains("coauthor_trailer"), "{out}");
}

#[test]
fn set_git_coauthor_trailer_disable_when_absent_is_noop() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_git_coauthor_trailer(false);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("[git]"), "{out}");
    assert!(!out.contains("coauthor_trailer"), "{out}");
}

#[test]
fn set_git_dco_enable_writes_git_table() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_git_dco(true);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("dco = true"), "{out}");
    assert!(out.contains("[git]"), "{out}");
}

#[test]
fn set_git_dco_disable_prunes_git_table() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[git]\ndco = true\n").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_git_dco(false);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        !out.contains("[git]"),
        "empty [git] table should be pruned: {out}"
    );
    assert!(!out.contains("dco"), "{out}");
}

#[test]
fn set_git_dco_disable_when_absent_is_noop() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_git_dco(false);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("[git]"), "{out}");
    assert!(!out.contains("dco"), "{out}");
}

#[test]
fn disabling_one_git_field_preserves_the_other() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        "[git]\ncoauthor_trailer = true\ndco = true\n",
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_git_coauthor_trailer(false);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        out.contains("[git]"),
        "[git] table must not be pruned when dco is still set: {out}"
    );
    assert!(!out.contains("coauthor_trailer"), "{out}");
    assert!(out.contains("dco = true"), "{out}");
}

fn profile_account() -> crate::AccountConfig {
    crate::AccountConfig {
        enabled: true,
        name: "Work".into(),
        provider: crate::AiProvider::Anthropic,
        credential: crate::AccountCredential::Profile {
            agent: Agent::Claude,
            directory: "/home/operator/.claude-work".into(),
            xdg_roots: None,
            source_selector: None,
        },
    }
}

#[test]
fn account_editor_persists_explicit_workspace_and_role_selection() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("work", &profile_account()).unwrap();
    editor
        .create_workspace(&wn("project"), account_workspace(temp.path()))
        .unwrap();
    assert!(
        editor
            .set_account_binding(Some(&wn("project")), None, Agent::Claude, Some("work"))
            .is_err()
    );
    editor
        .set_workspace_accounts(&wn("project"), &["work".into()])
        .unwrap();
    editor
        .set_account_binding(
            Some(&wn("project")),
            Some("smith"),
            Agent::Claude,
            Some("work"),
        )
        .unwrap();
    let config = editor.save().unwrap();
    assert_eq!(config.workspaces["project"].accounts, ["work"]);
    assert_eq!(
        config.workspaces["project"].roles["smith"].account_bindings[&Agent::Claude],
        "work"
    );
    let persisted = workspace_file_contents(&paths, "project");
    assert!(persisted.contains("[roles.smith.account_bindings]"));
}

#[test]
fn removing_account_prunes_all_assignments_and_bindings() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("work", &profile_account()).unwrap();
    editor
        .create_workspace(&wn("project"), account_workspace(temp.path()))
        .unwrap();
    editor
        .set_workspace_accounts(&wn("project"), &["work".into()])
        .unwrap();
    editor
        .set_account_binding(None, None, Agent::Claude, Some("work"))
        .unwrap();
    editor
        .set_account_binding(Some(&wn("project")), None, Agent::Claude, Some("work"))
        .unwrap();
    editor
        .set_account_binding(
            Some(&wn("project")),
            Some("smith"),
            Agent::Claude,
            Some("work"),
        )
        .unwrap();
    editor.save().unwrap();
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("work").unwrap();
    let config = editor.save().unwrap();
    assert!(!config.accounts.contains_key("work"));
    assert!(config.account_bindings.is_empty());
    let workspace = &config.workspaces["project"];
    assert!(workspace.accounts.is_empty());
    assert!(workspace.account_bindings.is_empty());
    assert!(workspace.roles["smith"].account_bindings.is_empty());
}

#[test]
fn disabling_and_removing_accounts_prune_all_launch_scopes_atomically() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    let work = profile_account();
    let mut other = profile_account();
    other.name = "Other".into();
    other.credential = crate::AccountCredential::Profile {
        agent: Agent::Claude,
        directory: "/home/operator/.claude-other".into(),
        xdg_roots: None,
        source_selector: None,
    };
    let mut config = AppConfig::default();
    config.accounts.insert("work".into(), work);
    config.accounts.insert("other".into(), other);
    for (id, account) in [("work-config", "work"), ("other-config", "other")] {
        config.agent_configurations.insert(
            id.into(),
            crate::AgentConfiguration {
                agent: Agent::Claude,
                account: account.into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
    }
    config.default_launch = Some(vec!["work-config".into(), "other-config".into()]);

    let mut workspace = WorkspaceConfig {
        workdir: "/workspace/project".into(),
        accounts: vec!["work".into(), "other".into()],
        default_launch: Some(vec!["work-config".into(), "other-config".into()]),
        ..Default::default()
    };
    workspace.roles.insert(
        "smith".into(),
        crate::WorkspaceRoleOverride {
            default_launch: Some(vec!["work-config".into(), "other-config".into()]),
            ..Default::default()
        },
    );
    std::fs::write(&paths.config_file, toml::to_string_pretty(&config).unwrap()).unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(
        paths.workspaces_dir.join("project.toml"),
        toml::to_string_pretty(&workspace).unwrap(),
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let mut disabled = config.accounts["work"].clone();
    disabled.enabled = false;
    editor.upsert_account("work", &disabled).unwrap();
    let config = editor.save().unwrap();
    assert!(!config.accounts["work"].enabled);
    assert!(!config.agent_configurations.contains_key("work-config"));
    assert_eq!(
        config.default_launch.as_deref(),
        Some(["other-config".into()].as_slice())
    );
    let workspace = &config.workspaces["project"];
    assert_eq!(
        workspace.default_launch.as_deref(),
        Some(["other-config".into()].as_slice())
    );
    assert_eq!(
        workspace.roles["smith"].default_launch.as_deref(),
        Some(["other-config".into()].as_slice())
    );

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("other").unwrap();
    let config = editor.save().unwrap();
    assert!(!config.accounts.contains_key("other"));
    assert!(config.agent_configurations.is_empty());
    assert_eq!(config.default_launch, Some(Vec::new()));
    let workspace = &config.workspaces["project"];
    assert_eq!(workspace.default_launch, Some(Vec::new()));
    assert_eq!(workspace.roles["smith"].default_launch, Some(Vec::new()));
}

fn account_workspace(source: &Path) -> WorkspaceConfig {
    WorkspaceConfig {
        workdir: "/workspace/project".into(),
        mounts: vec![MountConfig {
            src: source.display().to_string(),
            dst: "/workspace/project".into(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        }],
        ..Default::default()
    }
}

#[test]
fn account_mutations_reject_duplicate_sources_and_disabled_defaults() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut editor = ConfigEditor::open(&paths).unwrap();
    let mut account = profile_account();
    editor.upsert_account("work", &account).unwrap();
    account.name = "Same login, new label".into();
    assert!(editor.upsert_account("duplicate", &account).is_err());
    editor.upsert_account("work", &account).unwrap();
    account.enabled = false;
    editor.upsert_account("work", &account).unwrap();
    assert!(
        editor
            .set_account_binding(None, None, Agent::Claude, Some("work"))
            .is_err()
    );
    let cfg = editor.save().unwrap();
    assert!(!cfg.accounts["work"].enabled);
    assert!(!cfg.accounts.contains_key("duplicate"));
}

#[test]
fn clearing_final_role_binding_prunes_empty_override() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("work", &profile_account()).unwrap();
    editor
        .create_workspace(&wn("project"), account_workspace(temp.path()))
        .unwrap();
    editor
        .set_workspace_accounts(&wn("project"), &["work".into()])
        .unwrap();
    editor
        .set_account_binding(
            Some(&wn("project")),
            Some("smith"),
            Agent::Claude,
            Some("work"),
        )
        .unwrap();
    editor
        .set_account_binding(Some(&wn("project")), Some("smith"), Agent::Claude, None)
        .unwrap();
    let cfg = editor.save().unwrap();
    assert!(cfg.workspaces["project"].roles.is_empty());
}

#[test]
fn clearing_role_binding_preserves_nested_environment() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("work", &profile_account()).unwrap();
    let mut workspace = account_workspace(temp.path());
    workspace.roles.insert(
        "smith".into(),
        crate::WorkspaceRoleOverride {
            env: [("PROJECT_KEY".into(), EnvValue::Plain("fixture".into()))]
                .into_iter()
                .collect(),
            ..Default::default()
        },
    );
    editor.create_workspace(&wn("project"), workspace).unwrap();
    editor
        .set_workspace_accounts(&wn("project"), &["work".into()])
        .unwrap();
    editor
        .set_account_binding(
            Some(&wn("project")),
            Some("smith"),
            Agent::Claude,
            Some("work"),
        )
        .unwrap();
    editor
        .set_account_binding(Some(&wn("project")), Some("smith"), Agent::Claude, None)
        .unwrap();
    let cfg = editor.save().unwrap();
    assert_eq!(
        cfg.workspaces["project"].roles["smith"].env["PROJECT_KEY"],
        EnvValue::Plain("fixture".into())
    );
}

#[test]
fn prune_account_bindings_removes_bindings_across_all_scopes() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("work", &profile_account()).unwrap();
    editor
        .create_workspace(&wn("project"), account_workspace(temp.path()))
        .unwrap();
    editor
        .set_workspace_accounts(&wn("project"), &["work".into()])
        .unwrap();
    editor
        .set_account_binding(None, None, Agent::Claude, Some("work"))
        .unwrap();
    editor
        .set_account_binding(Some(&wn("project")), None, Agent::Claude, Some("work"))
        .unwrap();
    editor
        .set_account_binding(
            Some(&wn("project")),
            Some("smith"),
            Agent::Claude,
            Some("work"),
        )
        .unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.prune_account_bindings("work").unwrap();
    let config = editor.save().unwrap();

    assert!(config.account_bindings.is_empty());
    assert!(config.workspaces["project"].account_bindings.is_empty());
    assert!(
        config.workspaces["project"].roles["smith"]
            .account_bindings
            .is_empty()
    );
    assert!(config.accounts["work"].enabled);
}

#[test]
fn disabling_account_via_upsert_prunes_bindings_across_all_scopes() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("work", &profile_account()).unwrap();
    editor
        .create_workspace(&wn("project"), account_workspace(temp.path()))
        .unwrap();
    editor
        .set_workspace_accounts(&wn("project"), &["work".into()])
        .unwrap();
    editor
        .set_account_binding(None, None, Agent::Claude, Some("work"))
        .unwrap();
    editor
        .set_account_binding(Some(&wn("project")), None, Agent::Claude, Some("work"))
        .unwrap();
    editor
        .set_account_binding(
            Some(&wn("project")),
            Some("smith"),
            Agent::Claude,
            Some("work"),
        )
        .unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let mut disabled = profile_account();
    disabled.enabled = false;
    editor.upsert_account("work", &disabled).unwrap();
    let config = editor.save().unwrap();

    assert!(!config.accounts["work"].enabled);
    assert!(config.account_bindings.is_empty());
    assert!(config.workspaces["project"].account_bindings.is_empty());
    assert!(
        config.workspaces["project"].roles["smith"]
            .account_bindings
            .is_empty()
    );
}

#[test]
fn open_detailed_fresh_install_scans_and_stamps_sentinel() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let (editor, report) = ConfigEditor::open_detailed(&paths).unwrap();
    assert!(report.fresh_install);
    let config = editor.save().unwrap();
    assert_eq!(config.bootstrap, Some(crate::BootstrapState::initialized()));
    // Every reported ID exists in the registry (no phantom additions).
    for id in &report.added_accounts {
        assert!(config.accounts.contains_key(id), "missing {id}");
    }
    // Reopening is not a fresh install and rescans nothing.
    let (_, second) = ConfigEditor::open_detailed(&paths).unwrap();
    assert!(!second.fresh_install);
    assert!(second.added_accounts.is_empty());
}

#[test]
fn open_detailed_consumes_installer_marker_exactly_once() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        format!(
            "version = \"{}\"\n\n[bootstrap]\nversion = 1\nfresh_install = true\n",
            crate::CURRENT_CONFIG_VERSION
        ),
    )
    .unwrap();
    let (editor, report) = ConfigEditor::open_detailed(&paths).unwrap();
    assert!(report.fresh_install);
    let config = editor.save().unwrap();
    assert_eq!(config.bootstrap, Some(crate::BootstrapState::initialized()));
    let raw = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!raw.contains("fresh_install = true"), "{raw}");
}

#[test]
fn failed_fresh_install_bootstrap_keeps_marker_for_retry() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        format!(
            "version = \"{}\"\n\n[bootstrap]\nversion = 1\nfresh_install = true\n\n[accounts.bad]\nname = \"\"\nprovider = \"anthropic\"\n\n[accounts.bad.credential]\ntype = \"api_key\"\nvalue = \"$BAD\"\n",
            crate::CURRENT_CONFIG_VERSION
        ),
    )
    .unwrap();

    let bootstrap_failure = ConfigEditor::open_detailed(&paths).err();
    assert!(bootstrap_failure.is_some());
    let raw = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(raw.contains("fresh_install = true"), "{raw}");

    std::fs::write(
        &paths.config_file,
        format!(
            "version = \"{}\"\n\n[bootstrap]\nversion = 1\nfresh_install = true\n",
            crate::CURRENT_CONFIG_VERSION
        ),
    )
    .unwrap();
    let (editor, report) = ConfigEditor::open_detailed(&paths).unwrap();
    assert!(report.fresh_install);
    editor.save().unwrap();
    let raw = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!raw.contains("fresh_install = true"), "{raw}");
}

#[test]
fn open_detailed_upgrade_never_resurrects_or_rescans() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    // Pre-sentinel config with one deliberate account and no marker.
    std::fs::write(
        &paths.config_file,
        "version = \"v1alpha10\"\n\n[accounts.kept]\nenabled = true\nname = \"Kept\"\nprovider = \"anthropic\"\n\n[accounts.kept.credential]\ntype = \"api_key\"\nvalue = \"${ANTHROPIC_API_KEY}\"\n",
    )
    .unwrap();
    let (editor, report) = ConfigEditor::open_detailed(&paths).unwrap();
    assert!(!report.fresh_install);
    assert!(report.added_accounts.is_empty());
    let config = editor.save().unwrap();
    // Exactly the deliberate account survives: nothing resurrected, nothing added.
    assert_eq!(
        config.accounts.keys().collect::<Vec<_>>(),
        vec![&"kept".to_owned()]
    );
    assert_eq!(config.bootstrap, Some(crate::BootstrapState::initialized()));
}

fn minimal_config_file(paths: &JackinPaths) {
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        format!("version = \"{}\"\n", crate::CURRENT_CONFIG_VERSION),
    )
    .unwrap();
}

fn claude_credentials_fixture(home: &Path) {
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(
        home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
    )
    .unwrap();
}

#[test]
fn scan_for_accounts_imports_profiles_with_bootstrap_naming_and_dedupes() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(report.added_accounts.contains(&"default-claude".to_owned()));
    assert_eq!(report.added_accounts.len(), report.added.len());
    let (_, account) = report
        .added
        .iter()
        .find(|(id, _)| id == "default-claude")
        .unwrap();
    assert_eq!(account.name, "Claude default");
    assert_eq!(account.provider, crate::AiProvider::Anthropic);
    let config = editor.save().unwrap();
    assert!(config.accounts.contains_key("default-claude"));

    // Re-scan dedupes to a no-op: same IDs, same sources, nothing added.
    let mut editor = ConfigEditor::open(&paths).unwrap();
    let second = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(second.added_accounts.is_empty(), "{second:?}");
}

#[test]
fn removed_account_stays_excluded_from_scan_after_reload() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let first = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(first.added_accounts.contains(&"default-claude".to_owned()));
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("default-claude").unwrap();
    let removed = editor.save().unwrap();
    assert!(!removed.accounts.contains_key("default-claude"));
    assert_eq!(removed.account_scan_exclusions.len(), 1);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(report.added_accounts.is_empty(), "{report:?}");
    let reloaded = editor.save().unwrap();
    assert!(!reloaded.accounts.contains_key("default-claude"));
    assert!(
        !AppConfig::load_or_init(&paths)
            .unwrap()
            .accounts
            .contains_key("default-claude")
    );
}

#[cfg(unix)]
#[test]
fn removed_amp_xdg_account_stays_excluded_after_symlinked_shell_scan() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let root = temp.path().join("xdg");
    let alias = temp.path().join("xdg-alias");
    let data = root.join("data");
    let config = root.join("config");
    let cache = root.join("cache");
    std::fs::create_dir_all(data.join("amp")).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        data.join("amp/secrets.json"),
        r#"{"apiKey@https://ampcode.com/":"fixture-key"}"#,
    )
    .unwrap();
    symlink(&root, &alias).unwrap();

    let account = crate::AccountConfig {
        enabled: true,
        name: "Amp removed".into(),
        provider: crate::AiProvider::Amp,
        credential: crate::AccountCredential::Profile {
            agent: Agent::Amp,
            directory: data.join("amp"),
            xdg_roots: Some(crate::XdgRoots {
                data: data.clone(),
                config: config.clone(),
                cache: cache.clone(),
            }),
            source_selector: None,
        },
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("custom-amp", &account).unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("custom-amp").unwrap();
    editor.save().unwrap();

    let plan = crate::import_plan(&crate::parse_zshrc_source(&format!(
        "XDG_DATA_HOME={}/./data\nXDG_CONFIG_HOME={}/config/..//config\nXDG_CACHE_HOME={}/cache\n",
        alias.display(),
        alias.display(),
        alias.display()
    )));
    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(report.added_accounts.is_empty(), "{report:?}");
    assert!(report.unapplied_zshrc_xdg_roots.is_empty(), "{report:?}");
    assert!(!editor.save().unwrap().accounts.contains_key("custom-amp"));
}

#[cfg(unix)]
#[test]
fn removed_opencode_account_stays_excluded_after_symlinked_home_scan() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let real_home = paths.home_dir.clone();
    let alias_home = temp.path().join("home-alias");
    let directory = real_home.join(".local/share/opencode");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("auth.json"),
        r#"{"opencode-go":{"type":"api","key":"fixture-key"}}"#,
    )
    .unwrap();
    symlink(&real_home, &alias_home).unwrap();

    let account = crate::AccountConfig {
        enabled: true,
        name: "OpenCode removed".into(),
        provider: crate::AiProvider::Opencode,
        credential: crate::AccountCredential::Profile {
            agent: Agent::Opencode,
            directory: real_home.join(".local/share/./opencode"),
            xdg_roots: None,
            source_selector: None,
        },
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("removed-opencode", &account).unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("removed-opencode").unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(&alias_home.join("."), &BTreeMap::new())
        .unwrap();
    assert!(
        !report
            .added_accounts
            .contains(&"default-opencode-opencode".to_owned()),
        "{report:?}"
    );
    assert!(
        !editor
            .save()
            .unwrap()
            .accounts
            .contains_key("default-opencode-opencode")
    );
}

#[test]
fn removed_amp_xdg_profile_stays_excluded_from_shell_scan() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let data = temp.path().join("xdg-data");
    let config = temp.path().join("xdg-config");
    let cache = temp.path().join("xdg-cache");
    std::fs::create_dir_all(data.join("amp")).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        data.join("amp/secrets.json"),
        r#"{"apiKey@https://ampcode.com/":"fixture-key"}"#,
    )
    .unwrap();
    let plan = crate::import_plan(&crate::parse_zshrc_source(&format!(
        "XDG_DATA_HOME={}\nXDG_CONFIG_HOME={}\nXDG_CACHE_HOME={}\n",
        data.display(),
        config.display(),
        cache.display()
    )));

    let mut editor = ConfigEditor::open(&paths).unwrap();
    assert!(
        editor
            .apply_zshrc_plan(&plan)
            .unwrap()
            .added_accounts
            .contains(&"custom-amp".to_owned())
    );
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("custom-amp").unwrap();
    let removed = editor.save().unwrap();
    assert_eq!(removed.account_scan_exclusions.len(), 1);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(report.added_accounts.is_empty(), "{report:?}");
    let reloaded = editor.save().unwrap();
    assert!(!reloaded.accounts.contains_key("custom-amp"));
}

#[test]
fn removed_api_key_endpoint_account_stays_excluded_from_environment_scan() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let account = crate::AccountConfig {
        enabled: true,
        name: "OpenAI endpoint".into(),
        provider: crate::AiProvider::OpenAi,
        credential: crate::AccountCredential::ApiKey {
            value: EnvValue::from("$OPENAI_API_KEY"),
            base_url: Some("https://proxy.example/v1".into()),
            model: Some("gpt-endpoint".into()),
        },
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("openai-api-key", &account).unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("openai-api-key").unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(
            &paths.home_dir,
            &BTreeMap::from([
                ("OPENAI_API_KEY".to_owned(), "fixture".to_owned()),
                (
                    "OPENAI_BASE_URL".to_owned(),
                    "https://proxy.example/v1".to_owned(),
                ),
            ]),
        )
        .unwrap();
    assert!(
        !report.added_accounts.contains(&"openai-api-key".to_owned()),
        "{report:?}"
    );
    let reloaded = editor.save().unwrap();
    assert!(!reloaded.accounts.contains_key("openai-api-key"));
}

#[test]
fn environment_accounts_with_distinct_endpoints_keep_distinct_sources() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let existing = crate::AccountConfig {
        enabled: true,
        name: "OpenAI proxy A".into(),
        provider: crate::AiProvider::OpenAi,
        credential: crate::AccountCredential::ApiKey {
            value: EnvValue::from("$OPENAI_API_KEY"),
            base_url: Some("https://proxy-a.example/v1".into()),
            model: Some("model-a".into()),
        },
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("openai-proxy-a", &existing).unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(
            &paths.home_dir,
            &BTreeMap::from([
                ("OPENAI_API_KEY".to_owned(), "fixture-key".to_owned()),
                (
                    "OPENAI_BASE_URL".to_owned(),
                    "https://proxy-b.example/v1".to_owned(),
                ),
            ]),
        )
        .unwrap();
    assert!(report.added_accounts.contains(&"openai-api-key".to_owned()));
}

#[test]
fn removed_environment_account_with_endpoint_stays_excluded() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let environment = BTreeMap::from([
        ("OPENAI_API_KEY".to_owned(), "fixture-key".to_owned()),
        (
            "OPENAI_BASE_URL".to_owned(),
            "https://proxy.example/v1".to_owned(),
        ),
    ]);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let first = editor
        .scan_for_accounts_with(&paths.home_dir, &environment)
        .unwrap();
    assert!(first.added_accounts.contains(&"openai-api-key".to_owned()));
    let (_, account) = first
        .added
        .iter()
        .find(|(id, _)| id == "openai-api-key")
        .unwrap();
    assert!(matches!(
        &account.credential,
        crate::AccountCredential::ApiKey {
            base_url: Some(url),
            ..
        } if url == "https://proxy.example/v1"
    ));
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("openai-api-key").unwrap();
    let removed = editor.save().unwrap();
    assert_eq!(removed.account_scan_exclusions.len(), 1);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let second = editor
        .scan_for_accounts_with(&paths.home_dir, &environment)
        .unwrap();
    assert!(second.added_accounts.is_empty(), "{second:?}");
    assert!(!second.changed);
    assert!(
        !editor
            .save()
            .unwrap()
            .accounts
            .contains_key("openai-api-key")
    );
}

#[test]
fn scan_for_accounts_reads_live_home_and_environment() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);
    let mut editor = ConfigEditor::open(&paths).unwrap();
    // Ambient process environment may add further accounts; the fixture
    // profile must always be among them.
    let report = editor.scan_for_accounts().unwrap();
    assert!(report.added_accounts.contains(&"default-claude".to_owned()));
}

#[test]
fn scan_for_accounts_never_overwrites_operator_id_registrations() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let mut operator = profile_account();
    operator.name = "Operator".into();
    operator.credential = crate::AccountCredential::Profile {
        agent: Agent::Claude,
        directory: temp.path().join("elsewhere"),
        xdg_roots: None,
        source_selector: None,
    };
    editor.upsert_account("default-claude", &operator).unwrap();
    let report = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(!report.added_accounts.contains(&"default-claude".to_owned()));
    let config = editor.save().unwrap();
    assert_eq!(config.accounts["default-claude"].name, "Operator");
}

#[test]
fn scan_for_accounts_skips_sources_registered_under_other_ids() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    // Same credential source as the discovered default, registered under
    // an operator-chosen ID: the scan must skip, not error.
    let mut renamed = profile_account();
    renamed.credential = crate::AccountCredential::Profile {
        agent: Agent::Claude,
        directory: paths.home_dir.join(".claude"),
        xdg_roots: None,
        source_selector: None,
    };
    editor.upsert_account("mine", &renamed).unwrap();
    let report = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(!report.added_accounts.contains(&"default-claude".to_owned()));
}

#[test]
fn scan_for_accounts_imports_environment_references_without_values() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);

    let oauth_var = jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME;
    let environment = BTreeMap::from([
        ("ANTHROPIC_API_KEY".to_owned(), "live-secret".to_owned()),
        (oauth_var.to_owned(), "live-token".to_owned()),
    ]);
    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(&paths.home_dir, &environment)
        .unwrap();
    for (id, expected) in [
        ("anthropic-api-key", "$ANTHROPIC_API_KEY".to_owned()),
        ("claude-oauth-token", format!("${oauth_var}")),
    ] {
        let (_, account) = report.added.iter().find(|(found, _)| found == id).unwrap();
        let persisted = match &account.credential {
            crate::AccountCredential::ApiKey { value, .. }
            | crate::AccountCredential::OAuthToken { value, .. } => value.as_persisted_str(),
            other @ crate::AccountCredential::Profile { .. } => {
                panic!("unexpected credential for {id}: {other:?}")
            }
        };
        assert_eq!(persisted, expected);
    }
    // Values never enter the report, even under Debug.
    let dumped = format!("{report:?}");
    assert!(!dumped.contains("live-secret"), "{dumped}");
    assert!(!dumped.contains("live-token"), "{dumped}");
    let config = editor.save().unwrap();
    assert!(config.accounts.contains_key("anthropic-api-key"));
}

#[test]
fn profile_scan_candidate_skips_agents_without_native_billing() {
    for agent in [Agent::Omp, Agent::Hermes] {
        let discovered = crate::DiscoveredAccount {
            agent,
            provider: crate::AiProvider::for_agent(agent),
            directory: "/tmp/store".into(),
            source_selector: None,
            evidence: crate::CredentialEvidence::File("/tmp/store/auth.json".into()),
        };
        assert!(profile_scan_candidate(&discovered).is_none());
    }
    let discovered = crate::DiscoveredAccount {
        agent: Agent::Claude,
        provider: Some(crate::AiProvider::Anthropic),
        directory: "/tmp/claude".into(),
        source_selector: None,
        evidence: crate::CredentialEvidence::File("/tmp/claude/.credentials.json".into()),
    };
    let (id, account) = profile_scan_candidate(&discovered).unwrap();
    assert_eq!(id, "default-claude");
    assert_eq!(account.name, "Claude default");
}

#[test]
fn profile_scan_candidate_preserves_opencode_store_provider_identity() {
    let discovered = crate::DiscoveredAccount {
        agent: Agent::Opencode,
        provider: Some(crate::AiProvider::Zai),
        directory: "/tmp/opencode".into(),
        source_selector: None,
        evidence: crate::CredentialEvidence::File("/tmp/opencode/auth.json".into()),
    };
    let (id, account) = profile_scan_candidate(&discovered).unwrap();
    assert_eq!(id, "default-opencode-zai");
    assert_eq!(account.provider, crate::AiProvider::Zai);
    assert_eq!(account.name, "OpenCode zai default");
}

#[test]
fn zshrc_provider_accepts_only_canonical_catalog_slugs() {
    for provider in crate::AiProvider::ALL {
        assert_eq!(zshrc_provider(provider.slug()), Some(*provider));
    }
    assert_eq!(zshrc_provider("kimi"), None);
    assert_eq!(zshrc_provider("gemini"), None);
}

#[test]
fn apply_zshrc_plan_seeds_verified_directories_and_op_refs() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let override_dir = temp.path().join("claude-override");
    std::fs::create_dir_all(&override_dir).unwrap();
    std::fs::write(
        override_dir.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
    )
    .unwrap();
    let source = format!(
        "CLAUDE_CONFIG_DIR={}\nANTHROPIC_API_KEY=$(op read op://vault/item/field)\n",
        override_dir.display()
    );
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));
    assert_eq!(plan.directories.len(), 1);
    assert_eq!(plan.op_refs.len(), 1);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(report.added_accounts.contains(&"custom-claude".to_owned()));
    assert!(
        report
            .added_accounts
            .contains(&"anthropic-api-key".to_owned())
    );
    let (_, key) = report
        .added
        .iter()
        .find(|(id, _)| id == "anthropic-api-key")
        .unwrap();
    assert!(matches!(
        key.credential,
        crate::AccountCredential::ApiKey {
            value: EnvValue::OpRef(_),
            ..
        }
    ));
    let config = editor.save().unwrap();
    assert!(config.accounts.contains_key("custom-claude"));
    assert!(config.accounts.contains_key("anthropic-api-key"));
}

#[test]
fn apply_zshrc_custom_profile_does_not_collide_with_default_profile() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);
    let override_dir = temp.path().join("claude-override");
    std::fs::create_dir_all(&override_dir).unwrap();
    std::fs::write(
        override_dir.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"custom-fixture"}}"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let defaults = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(
        defaults
            .added_accounts
            .contains(&"default-claude".to_owned())
    );
    let source = format!("CLAUDE_CONFIG_DIR={}\n", override_dir.display());
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));
    let custom = editor.apply_zshrc_plan(&plan).unwrap();

    assert!(custom.added_accounts.contains(&"custom-claude".to_owned()));
    let config = editor.save().unwrap();
    assert!(config.accounts.contains_key("default-claude"));
    assert!(config.accounts.contains_key("custom-claude"));
    assert_ne!(
        config.accounts["default-claude"].source_directory(),
        config.accounts["custom-claude"].source_directory()
    );
}

#[test]
fn apply_zshrc_plan_skips_unverified_directories_and_unknown_vars() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let empty_dir = temp.path().join("empty-override");
    std::fs::create_dir_all(&empty_dir).unwrap();
    let source = format!(
        "CLAUDE_CONFIG_DIR={}\nWIDGET_API_KEY=$(op read op://vault/item/field)\n",
        empty_dir.display()
    );
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));
    assert_eq!(plan.directories.len(), 1);
    assert_eq!(plan.op_refs.len(), 1);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(report.added_accounts.is_empty(), "{report:?}");
    assert!(report.issues.is_empty(), "{report:?}");
}

#[test]
fn apply_zshrc_plan_persists_canonical_model_and_reports_unsupported_wrapper() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account(
            "moonshot-api-key",
            &crate::AccountConfig {
                enabled: true,
                name: "Kimi API".into(),
                provider: crate::AiProvider::Moonshot,
                credential: crate::AccountCredential::ApiKey {
                    value: EnvValue::Plain("$KIMI_API_KEY".into()),
                    base_url: None,
                    model: None,
                },
            },
        )
        .unwrap();
    let plan = crate::import_plan(&crate::parse_zshrc_source(
        "kimi_key() { echo fixture; }\nMOONSHOT_MODEL=kimi-k2\nMOONSHOT_BASE_URL=https://api.kimi.example/v1\nKIMI_API_KEY=$(kimi_key)\n",
    ));

    let report = editor.apply_zshrc_plan(&plan).unwrap();

    assert!(report.unapplied_zshrc_models.is_empty(), "{report:?}");
    assert!(report.changed);
    assert_eq!(report.unapplied_zshrc_wrappers.len(), 1);
    assert_eq!(report.unapplied_zshrc_wrappers[0].var, "KIMI_API_KEY");
    let config = editor.save().unwrap();
    let account = &config.accounts["moonshot-api-key"];
    assert_eq!(
        account.credential,
        crate::AccountCredential::ApiKey {
            value: EnvValue::Plain("$KIMI_API_KEY".into()),
            base_url: Some("https://api.kimi.example/v1".into()),
            model: Some("kimi-k2".into()),
        }
    );
}

#[test]
fn removed_op_ref_account_keeps_model_endpoint_tombstone_identity() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let plan = crate::import_plan(&crate::parse_zshrc_source(
        "MOONSHOT_MODEL=kimi-k2\nMOONSHOT_BASE_URL=https://proxy.example/v1\nKIMI_API_KEY=$(op read op://vault/item/field)\n",
    ));

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let first = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(
        first
            .added_accounts
            .contains(&"moonshot-api-key".to_owned())
    );
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("moonshot-api-key").unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let second = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(second.added_accounts.is_empty(), "{second:?}");
    assert!(
        second
            .unapplied_zshrc_models
            .iter()
            .any(|model| model.name == "moonshot")
    );
    assert!(
        !editor
            .save()
            .unwrap()
            .accounts
            .contains_key("moonshot-api-key")
    );
}

#[test]
fn apply_zshrc_plan_leaves_provider_alias_models_unapplied() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let mut editor = ConfigEditor::open(&paths).unwrap();
    for (id, name, provider, variable) in [
        (
            "moonshot-api-key",
            "Kimi API",
            crate::AiProvider::Moonshot,
            "$KIMI_API_KEY",
        ),
        (
            "google-api-key",
            "Gemini API",
            crate::AiProvider::Google,
            "$GEMINI_API_KEY",
        ),
    ] {
        editor
            .upsert_account(
                id,
                &crate::AccountConfig {
                    enabled: true,
                    name: name.into(),
                    provider,
                    credential: crate::AccountCredential::ApiKey {
                        value: EnvValue::Plain(variable.into()),
                        base_url: None,
                        model: None,
                    },
                },
            )
            .unwrap();
    }

    let plan = crate::import_plan(&crate::parse_zshrc_source(
        "KIMI_MODEL=kimi-k2\nKIMI_BASE_URL=https://api.kimi.example/v1\nGEMINI_MODEL=gemini-2.5-pro\nGEMINI_BASE_URL=https://generativelanguage.example/v1\n",
    ));
    let report = editor.apply_zshrc_plan(&plan).unwrap();
    let names: Vec<_> = report
        .unapplied_zshrc_models
        .iter()
        .map(|model| model.name.as_str())
        .collect();
    assert_eq!(names, ["gemini", "kimi"]);

    let config = editor.save().unwrap();
    for id in ["moonshot-api-key", "google-api-key"] {
        let crate::AccountCredential::ApiKey {
            model, base_url, ..
        } = &config.accounts[id].credential
        else {
            panic!("expected API-key account for {id}");
        };
        assert!(model.is_none(), "alias model applied to {id}");
        assert!(base_url.is_none(), "alias endpoint applied to {id}");
    }
}

#[test]
fn apply_zshrc_plan_persists_amp_xdg_roots_with_discovered_credentials() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let data = temp.path().join("xdg-data");
    let config = temp.path().join("xdg-config");
    let cache = temp.path().join("xdg-cache");
    std::fs::create_dir_all(data.join("amp")).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        data.join("amp/secrets.json"),
        r#"{"apiKey@https://ampcode.com/":"fixture-key"}"#,
    )
    .unwrap();
    let source = format!(
        "XDG_DATA_HOME={}\nXDG_CONFIG_HOME={}\nXDG_CACHE_HOME={}\n",
        data.display(),
        config.display(),
        cache.display()
    );
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();

    assert!(report.unapplied_zshrc_xdg_roots.is_empty(), "{report:?}");
    assert!(report.added_accounts.contains(&"custom-amp".to_owned()));
    let account = &report
        .added
        .iter()
        .find(|(id, _)| id == "custom-amp")
        .unwrap()
        .1;
    assert!(matches!(
        &account.credential,
        crate::AccountCredential::Profile {
            agent: Agent::Amp,
            directory,
            xdg_roots: Some(_),
            source_selector: None,
        } if directory == &data.join("amp")
    ));
    let config = editor.save().unwrap();
    assert!(config.accounts.contains_key("custom-amp"));
}

#[test]
fn removed_amp_xdg_account_stays_excluded_from_zshrc_scan() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let data = temp.path().join("xdg-data");
    let config = temp.path().join("xdg-config");
    let cache = temp.path().join("xdg-cache");
    std::fs::create_dir_all(data.join("amp")).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        data.join("amp/secrets.json"),
        r#"{"apiKey@https://ampcode.com/":"fixture-key"}"#,
    )
    .unwrap();
    let source = format!(
        "XDG_DATA_HOME={}\nXDG_CONFIG_HOME={}\nXDG_CACHE_HOME={}\n",
        data.display(),
        config.display(),
        cache.display()
    );
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let first = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(first.added_accounts.contains(&"custom-amp".to_owned()));
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("custom-amp").unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let second = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(second.added_accounts.is_empty(), "{second:?}");
    assert!(second.unapplied_zshrc_xdg_roots.is_empty(), "{second:?}");
    assert!(!editor.save().unwrap().accounts.contains_key("custom-amp"));
}

#[test]
fn apply_zshrc_plan_rejects_opencode_xdg_root_before_amp_persistence() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let data = temp.path().join("xdg-data");
    let config = temp.path().join("xdg-config");
    let cache = temp.path().join("xdg-cache");
    std::fs::create_dir_all(data.join("amp")).unwrap();
    std::fs::create_dir_all(data.join("opencode")).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        data.join("amp/secrets.json"),
        r#"{"apiKey@https://ampcode.com/":"fixture-amp"}"#,
    )
    .unwrap();
    std::fs::write(
        data.join("opencode/auth.json"),
        r#"{"opencode-go":{"type":"api","key":"fixture-opencode"}}"#,
    )
    .unwrap();
    let source = format!(
        "XDG_DATA_HOME={}\nXDG_CONFIG_HOME={}\nXDG_CACHE_HOME={}\n",
        data.display(),
        config.display(),
        cache.display()
    );
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();

    assert!(report.added_accounts.is_empty(), "{report:?}");
    assert_eq!(
        report.unapplied_zshrc_xdg_roots,
        vec![crate::XdgRoots {
            data: data.clone(),
            config: config.clone(),
            cache: cache.clone(),
        }]
    );
    assert!(report.issues.iter().any(|issue| {
        issue.agent == Agent::Opencode
            && issue.error
                == crate::DiscoveryError::Unsupported(
                    "OpenCode XDG roots from shell imports require an explicit profile directory",
                )
    }));
    let config = editor.save().unwrap();
    assert!(!config.accounts.contains_key("custom-amp"));
    assert!(
        !config
            .accounts
            .values()
            .any(|account| account.provider == crate::AiProvider::Opencode)
    );
}
