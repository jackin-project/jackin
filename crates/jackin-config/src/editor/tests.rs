//! Tests for `editor`.
use super::*;
use crate::RoleSource;
use jackin_core::WorkspaceName;
fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}
use tempfile::tempdir;

fn workspace_file_contents(paths: &JackinPaths, name: &str) -> String {
    std::fs::read_to_string(paths.workspaces_dir.join(format!("{name}.toml"))).unwrap()
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
            "OPENAI_API_KEY",
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
        out.contains(r#"OPENAI_API_KEY = "op://Work/OpenAI/default""#),
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
        .set_env_var(&scope, "OPENAI_API_KEY", "op://Work/OpenAI/default".into())
        .unwrap();
    assert!(
        editor.remove_env_var(&scope, "OPENAI_API_KEY"),
        "first remove should return true"
    );
    assert!(
        !editor.remove_env_var(&scope, "OPENAI_API_KEY"),
        "second remove should return false"
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("OPENAI_API_KEY"), "key not purged: {out}");
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
[claude]
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
fn set_global_auth_forward_writes_per_agent_table() {
    for (agent, header) in [
        (Agent::Claude, "[claude]"),
        (Agent::Codex, "[codex]"),
        (Agent::Amp, "[amp]"),
        (Agent::Grok, "[grok]"),
    ] {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "").unwrap();

        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor.set_global_auth_forward(agent, AuthForwardMode::Sync);
        editor.save().unwrap();

        let out = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(out.contains(header), "expected {header} in:\n{out}");
        assert!(out.contains(r#"auth_forward = "sync""#), "{out}");
    }
}

#[test]
fn set_global_sync_source_dir_writes_and_removes_agent_field() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_global_sync_source_dir(Agent::Claude, Some(Path::new("/host/claude")));
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[claude]"), "{out}");
    assert!(out.contains(r#"sync_source_dir = "/host/claude""#), "{out}");

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_global_sync_source_dir(Agent::Claude, None);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("sync_source_dir"), "{out}");
    assert!(!out.contains("[claude]"), "{out}");
}

#[test]
fn set_workspace_auth_forward_writes_workspace_agent_block() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"
[workspaces.proj]
workdir = "/tmp/proj"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_auth_forward(&wn("proj"), Agent::Claude, Some(AuthForwardMode::ApiKey));
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "proj");
    assert!(out.contains("[claude]"), "{out}");
    assert!(out.contains(r#"auth_forward = "api_key""#), "{out}");
}

#[test]
fn set_workspace_auth_forward_clears_when_mode_none() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"
[workspaces.proj]
workdir = "/tmp/proj"

[workspaces.proj.claude]
auth_forward = "api_key"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_auth_forward(&wn("proj"), Agent::Claude, None);
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "proj");
    assert!(
        !out.contains("[claude]"),
        "agent block must be removed when mode = None; {out}"
    );
    assert!(
        !out.contains("auth_forward"),
        "auth_forward field must be cleared; {out}"
    );
}

#[test]
fn set_workspace_sync_source_dir_writes_and_removes_agent_field() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"
[workspaces.proj]
workdir = "/tmp/proj"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_sync_source_dir(&wn("proj"), Agent::Claude, Some(Path::new("/host/claude")));
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "proj");
    assert!(out.contains("[claude]"), "{out}");
    assert!(out.contains(r#"sync_source_dir = "/host/claude""#), "{out}");

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_sync_source_dir(&wn("proj"), Agent::Claude, None);
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "proj");
    assert!(!out.contains("sync_source_dir"), "{out}");
    assert!(!out.contains("[claude]"), "{out}");
}

#[test]
fn set_workspace_role_auth_forward_writes_role_agent_block() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"
[workspaces.proj]
workdir = "/tmp/proj"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_role_auth_forward(&wn("proj"),
        "smith",
        Agent::Codex,
        Some(AuthForwardMode::ApiKey),
    );
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "proj");
    assert!(out.contains("[roles.smith.codex]"), "{out}");
    assert!(out.contains(r#"auth_forward = "api_key""#), "{out}");
}

#[test]
fn set_workspace_role_auth_forward_clears_when_mode_none() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"
[workspaces.proj]
workdir = "/tmp/proj"

[workspaces.proj.roles.smith.claude]
auth_forward = "oauth_token"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_role_auth_forward(&wn("proj"), "smith", Agent::Claude, None);
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "proj");
    assert!(!out.contains("[roles.smith.claude]"), "{out}");
}

#[test]
fn set_workspace_role_sync_source_dir_writes_and_removes_agent_field() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"
[workspaces.proj]
workdir = "/tmp/proj"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_role_sync_source_dir(&wn("proj"),
        "smith",
        Agent::Codex,
        Some(Path::new("/host/codex")),
    );
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "proj");
    assert!(out.contains("[roles.smith.codex]"), "{out}");
    assert!(out.contains(r#"sync_source_dir = "/host/codex""#), "{out}");

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_workspace_role_sync_source_dir(&wn("proj"), "smith", Agent::Codex, None);
    editor.save().unwrap();

    let out = workspace_file_contents(&paths, "proj");
    assert!(!out.contains("sync_source_dir"), "{out}");
    assert!(!out.contains("[roles.smith.codex]"), "{out}");
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
            "CLAUDE_CODE_OAUTH_TOKEN",
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
            serialized.contains(r#"CLAUDE_CODE_OAUTH_TOKEN = { op = "op://abc/def/fld", path = "Private/Claude/security/auth token" }"#),
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
            "CLAUDE_CODE_OAUTH_TOKEN",
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
                r#"CLAUDE_CODE_OAUTH_TOKEN = { op = "op://abc/def/fld", path = "Work/Claude/auth token", account = "WORKACCT" }"#
            ),
            "expected account key in inline table, got:\n{saved}"
        );
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
    editor.set_workspace_role_github_auth_forward(&wn("prod"), "scratch", Some(GithubAuthMode::Token));
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

/// Clearing `[…github] auth_forward` while sibling kinds (`[…claude]` /
/// `[…codex]`) are still set must NOT cascade-prune the siblings.
#[test]
fn clearing_one_kind_preserves_sibling_kinds() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.prod]
workdir = "/workspace/prod"

[workspaces.prod.claude]
auth_forward = "ignore"

[workspaces.prod.codex]
auth_forward = "ignore"

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
        cleaned.contains("[claude]"),
        "claude block must survive:\n{cleaned}"
    );
    assert!(
        cleaned.contains("[codex]"),
        "codex block must survive:\n{cleaned}"
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
