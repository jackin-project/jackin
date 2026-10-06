// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
                r#"SERVICE_TOKEN = { op = "op://abc/def/fld", breadcrumb = { version = 1, value = "Work/Claude/auth token" }, account = "WORKACCT" }"#
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
