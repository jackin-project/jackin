// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
            serialized.contains(r#"SERVICE_TOKEN = { op = "op://abc/def/fld", breadcrumb = { version = 1, value = "Private/Claude/security/auth token" } }"#),
            "expected inline-table emit, got:\n{serialized}"
        );
}
