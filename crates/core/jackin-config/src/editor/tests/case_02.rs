// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

    let original = include_str!("../../fixtures/config.round_trip.toml");
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
