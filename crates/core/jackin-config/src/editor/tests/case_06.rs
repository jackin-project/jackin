// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
