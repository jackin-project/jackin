// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::{
    AccountConfig, AccountCredential, AgentConfiguration, AiProvider, AppConfig, ConfigEditor,
    WorkspaceConfig, WorkspaceRoleOverride, resolve_account, resolve_launch,
};
use jackin_core::{Agent, EnvValue, JackinPaths, WorkspaceName};
use std::path::Path;

fn wn(name: &str) -> anyhow::Result<WorkspaceName> {
    Ok(WorkspaceName::parse(name)?)
}

fn api_key(provider: AiProvider, name: &str, value: &str) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: name.into(),
        provider,
        credential: AccountCredential::ApiKey {
            value: EnvValue::Plain(value.into()),
            base_url: None,
            model: None,
        },
    }
}

fn seed_claude_credentials(home: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(home.join(".claude"))?;
    std::fs::write(
        home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
    )?;
    Ok(())
}

fn fresh_paths() -> anyhow::Result<(tempfile::TempDir, JackinPaths)> {
    let temp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    Ok((temp, paths))
}

fn test_workspace(root: &Path, accounts: Vec<String>) -> anyhow::Result<WorkspaceConfig> {
    let src = root.join("mnt-src");
    std::fs::create_dir_all(&src)?;
    Ok(WorkspaceConfig {
        workdir: "/workspace/w1".into(),
        mounts: vec![jackin_config::MountConfig {
            src: src.display().to_string(),
            dst: "/workspace/w1".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        accounts,
        ..Default::default()
    })
}

#[test]
fn s1_register_four_accounts_two_sharing_one_provider() -> anyhow::Result<()> {
    let (_temp, paths) = fresh_paths()?;
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account(
            "claude-personal",
            &api_key(AiProvider::Anthropic, "Personal", "secret-personal"),
        )
        .unwrap();
    editor
        .upsert_account(
            "claude-work",
            &api_key(AiProvider::Anthropic, "Work", "secret-work"),
        )
        .unwrap();
    editor
        .upsert_account(
            "codex-main",
            &api_key(AiProvider::OpenAi, "Codex", "secret-codex"),
        )
        .unwrap();
    editor
        .upsert_account(
            "gemini-main",
            &api_key(AiProvider::Google, "Gemini", "secret-gemini"),
        )
        .unwrap();
    let dup = editor.upsert_account(
        "claude-copy",
        &api_key(AiProvider::Anthropic, "Copy", "secret-personal"),
    );
    assert!(dup.is_err(), "duplicate credential source was accepted");
    let saved = editor.save().unwrap();
    assert!(
        saved.accounts.len() >= 4,
        "registry lost accounts: {:?}",
        saved.accounts.keys().collect::<Vec<_>>()
    );

    let reopened = ConfigEditor::open(&paths).unwrap().save().unwrap();
    for id in [
        "claude-personal",
        "claude-work",
        "codex-main",
        "gemini-main",
    ] {
        assert!(
            reopened.accounts.contains_key(id),
            "missing {id} after reload"
        );
    }
    assert_eq!(reopened.accounts["claude-personal"].name, "Personal");
    Ok(())
}

#[test]
fn s1_import_then_rescan_is_duplicate_free() -> anyhow::Result<()> {
    let (_temp, paths) = fresh_paths()?;
    seed_claude_credentials(&paths.home_dir)?;
    let discovered = jackin_config::discover_default_accounts(&paths.home_dir);
    assert!(
        discovered.accounts.iter().any(|found| {
            found.agent == Agent::Claude && found.directory.starts_with(&paths.home_dir)
        }),
        "seeded fixture was not discovered: {discovered:?}"
    );
    let mut editor = ConfigEditor::open(&paths).unwrap();
    let first = editor.scan_for_accounts().unwrap();
    let snapshot = editor.save().unwrap().accounts;
    assert!(
        snapshot.contains_key("default-claude"),
        "seeded profile missing after scan {first:?}: {:?}",
        snapshot.keys().collect::<Vec<_>>()
    );

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let second = editor.scan_for_accounts().unwrap();
    assert!(
        second.added_accounts.is_empty(),
        "rescan added duplicates: {second:?}"
    );
    let resaved = editor.save().unwrap();
    assert_eq!(resaved.accounts, snapshot, "rescan mutated the registry");

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let imported = resaved.accounts["default-claude"].clone();
    editor.remove_account("default-claude").unwrap();
    editor.upsert_account("renamed-import", &imported).unwrap();
    let third = editor.scan_for_accounts().unwrap();
    assert!(
        third.added_accounts.is_empty(),
        "rescan duplicated a renamed import: {third:?}"
    );
    let final_cfg = editor.save().unwrap();
    assert!(final_cfg.accounts.contains_key("renamed-import"));
    assert!(!final_cfg.accounts.contains_key("default-claude"));
    Ok(())
}

#[test]
fn s1_rename_display_name_keeps_everything_stable() -> anyhow::Result<()> {
    let (temp, paths) = fresh_paths()?;
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account("a", &api_key(AiProvider::Anthropic, "A", "secret-a"))
        .unwrap();
    editor
        .upsert_account("b", &api_key(AiProvider::OpenAi, "B", "secret-b"))
        .unwrap();
    let w1 = wn("w1")?;
    editor
        .create_workspace(
            &w1,
            test_workspace(temp.path(), vec!["a".into(), "b".into()])?,
        )
        .unwrap();
    editor
        .set_account_binding(Some(&w1), None, Agent::Claude, Some("a"))
        .unwrap();
    let before = editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let mut renamed = before.accounts["a"].clone();
    renamed.name = "A renamed".into();
    editor.upsert_account("a", &renamed).unwrap();
    let after = editor.save().unwrap();

    assert_eq!(after.accounts["a"].name, "A renamed");
    assert_eq!(
        after.workspaces["w1"].accounts,
        before.workspaces["w1"].accounts
    );
    assert_eq!(
        after.workspaces["w1"].account_bindings,
        before.workspaces["w1"].account_bindings
    );
    let resolved_before =
        resolve_launch(&before, Some(&w1), "", None, Some(Agent::Claude)).unwrap();
    let resolved_after = resolve_launch(&after, Some(&w1), "", None, Some(Agent::Claude)).unwrap();
    assert_eq!(resolved_before.len(), 1);
    assert_eq!(resolved_after.len(), 1);
    assert_eq!(resolved_before[0].account_id, resolved_after[0].account_id);
    Ok(())
}

#[test]
fn s1_reorder_workspace_accounts_preserves_set_and_bindings() -> anyhow::Result<()> {
    let (temp, paths) = fresh_paths()?;
    let mut editor = ConfigEditor::open(&paths).unwrap();
    for (id, provider, secret) in [
        ("a", AiProvider::Anthropic, "secret-a"),
        ("b", AiProvider::OpenAi, "secret-b"),
        ("c", AiProvider::Google, "secret-c"),
    ] {
        editor
            .upsert_account(id, &api_key(provider, id, secret))
            .unwrap();
    }
    let w1 = wn("w1")?;
    editor
        .create_workspace(
            &w1,
            test_workspace(temp.path(), vec!["a".into(), "b".into(), "c".into()])?,
        )
        .unwrap();
    editor
        .set_account_binding(Some(&w1), None, Agent::Claude, Some("a"))
        .unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_workspace_accounts(&w1, &["c".into(), "a".into(), "b".into()])
        .unwrap();
    let reordered = editor.save().unwrap();
    assert_eq!(
        reordered.workspaces["w1"].accounts,
        vec!["c".to_owned(), "a".to_owned(), "b".to_owned()]
    );
    assert_eq!(
        reordered.workspaces["w1"]
            .account_bindings
            .get(&Agent::Claude),
        Some(&"a".to_owned())
    );
    let resolved = resolve_launch(&reordered, Some(&w1), "", None, Some(Agent::Claude)).unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].account_id, "a");
    Ok(())
}

#[test]
fn s1_draft_cancel_leaves_disk_untouched_apply_persists() -> anyhow::Result<()> {
    let (_temp, paths) = fresh_paths()?;
    let before = ConfigEditor::open(&paths).unwrap().save().unwrap();
    assert!(!before.accounts.contains_key("drafted"));
    let bytes_before = std::fs::read(&paths.config_file).unwrap();

    {
        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .upsert_account(
                "drafted",
                &api_key(AiProvider::Anthropic, "Draft", "secret-x"),
            )
            .unwrap();
    }
    let bytes_after_cancel = std::fs::read(&paths.config_file).unwrap();
    assert_eq!(bytes_before, bytes_after_cancel, "cancel wrote to disk");
    let cancelled = ConfigEditor::open(&paths).unwrap().save().unwrap();
    assert!(!cancelled.accounts.contains_key("drafted"));

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account(
            "drafted",
            &api_key(AiProvider::Anthropic, "Draft", "secret-x"),
        )
        .unwrap();
    editor.save().unwrap();
    let applied = ConfigEditor::open(&paths).unwrap().save().unwrap();
    assert!(applied.accounts.contains_key("drafted"));
    Ok(())
}

#[test]
fn s1_disable_prunes_bindings_without_deleting_the_account() -> anyhow::Result<()> {
    let (temp, paths) = fresh_paths()?;
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account("a", &api_key(AiProvider::Anthropic, "A", "secret-a"))
        .unwrap();
    editor
        .upsert_account("b", &api_key(AiProvider::OpenAi, "B", "secret-b"))
        .unwrap();
    let w1 = wn("w1")?;
    editor
        .create_workspace(
            &w1,
            test_workspace(temp.path(), vec!["a".into(), "b".into()])?,
        )
        .unwrap();
    editor
        .set_account_binding(Some(&w1), None, Agent::Claude, Some("a"))
        .unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let mut disabled = api_key(AiProvider::Anthropic, "A", "secret-a");
    disabled.enabled = false;
    editor.upsert_account("a", &disabled).unwrap();
    let pruned = editor.save().unwrap();
    assert!(
        pruned.accounts.contains_key("a"),
        "disable deleted the account"
    );
    assert!(
        !pruned.workspaces["w1"]
            .account_bindings
            .values()
            .any(|id| id == "a"),
        "disable left a live binding behind"
    );
    Ok(())
}

fn scoped_fixture() -> anyhow::Result<(AppConfig, WorkspaceName, WorkspaceName)> {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "acc-a".into(),
        api_key(AiProvider::Anthropic, "A", "secret-a"),
    );
    cfg.accounts
        .insert("acc-b".into(), api_key(AiProvider::OpenAi, "B", "secret-b"));
    cfg.accounts
        .insert("acc-c".into(), api_key(AiProvider::Google, "C", "secret-c"));
    cfg.accounts.insert(
        "acc-d".into(),
        api_key(AiProvider::Anthropic, "D", "secret-d"),
    );
    let w1 = wn("w1")?;
    let w2 = wn("w2")?;
    let ws1 = WorkspaceConfig {
        accounts: vec!["acc-a".into(), "acc-b".into(), "acc-c".into()],
        ..Default::default()
    };
    let ws2 = WorkspaceConfig {
        accounts: vec!["acc-c".into(), "acc-d".into()],
        ..Default::default()
    };
    cfg.workspaces.insert("w1".into(), ws1);
    cfg.workspaces.insert("w2".into(), ws2);
    Ok((cfg, w1, w2))
}

fn configuration(agent: Agent, account: &str) -> AgentConfiguration {
    AgentConfiguration {
        agent,
        account: account.into(),
        model: None,
        base_url: None,
        display_label: None,
        invoked_via_wrapper: None,
    }
}

#[test]
fn s2_w1_admits_exactly_abc_d_is_inaccessible() -> anyhow::Result<()> {
    let (mut cfg, w1, w2) = scoped_fixture()?;
    cfg.agent_configurations
        .insert("launch-a".into(), configuration(Agent::Claude, "acc-a"));
    cfg.agent_configurations
        .insert("launch-b".into(), configuration(Agent::Codex, "acc-b"));
    cfg.agent_configurations
        .insert("launch-c".into(), configuration(Agent::Gemini, "acc-c"));
    cfg.agent_configurations
        .insert("launch-d".into(), configuration(Agent::Claude, "acc-d"));
    cfg.validate_accounts().unwrap();

    let instances = resolve_launch(
        &cfg,
        Some(&w1),
        "",
        Some(&["launch-a".into(), "launch-b".into(), "launch-c".into()]),
        None,
    )
    .unwrap();
    let mut admitted: Vec<&str> = instances.iter().map(|i| i.account_id.as_str()).collect();
    admitted.sort_unstable();
    assert_eq!(admitted, ["acc-a", "acc-b", "acc-c"]);

    let err = resolve_launch(&cfg, Some(&w1), "", Some(&["launch-d".into()]), None).unwrap_err();
    assert!(err.to_string().contains("not assigned"), "{err}");

    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "acc-d".into());
    let err = resolve_account(&cfg, Agent::Claude, Some(&w1), "").unwrap_err();
    assert!(err.to_string().contains("not assigned"), "{err}");
    let _err = resolve_launch(&cfg, Some(&w1), "", None, Some(Agent::Claude)).unwrap_err();
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .account_bindings
        .remove(&Agent::Claude);

    let d = resolve_launch(&cfg, Some(&w2), "", Some(&["launch-d".into()]), None).unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].account_id, "acc-d");
    let err = resolve_launch(&cfg, Some(&w2), "", Some(&["launch-a".into()]), None).unwrap_err();
    assert!(err.to_string().contains("not assigned"), "{err}");
    Ok(())
}

#[test]
fn s2_unknown_workspace_and_stale_ids_fail_closed() -> anyhow::Result<()> {
    let (mut cfg, w1, _w2) = scoped_fixture()?;
    let ghost_ws = wn("ghost")?;
    let _err = resolve_account(&cfg, Agent::Claude, Some(&ghost_ws), "").unwrap_err();
    let _err = resolve_launch(&cfg, Some(&ghost_ws), "", None, Some(Agent::Claude)).unwrap_err();

    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .accounts
        .push("deleted-account".into());
    let err = resolve_account(&cfg, Agent::Codex, Some(&w1), "").unwrap_err();
    assert!(err.to_string().contains("unknown account"), "{err}");
    let _err = resolve_launch(&cfg, Some(&w1), "", None, None).unwrap_err();
    Ok(())
}

fn precedence_fixture() -> anyhow::Result<(AppConfig, WorkspaceName)> {
    let (mut cfg, w1, _w2) = scoped_fixture()?;
    cfg.agent_configurations
        .insert("cfg-one".into(), configuration(Agent::Claude, "acc-a"));
    cfg.agent_configurations
        .insert("cfg-role".into(), configuration(Agent::Codex, "acc-b"));
    cfg.agent_configurations
        .insert("cfg-ws".into(), configuration(Agent::Gemini, "acc-c"));
    cfg.agent_configurations
        .insert("cfg-global".into(), configuration(Agent::Codex, "acc-b"));
    let ws = cfg
        .workspaces
        .get_mut("w1")
        .ok_or_else(|| anyhow::anyhow!("w1 missing from scoped fixture"))?;
    let mut role = WorkspaceRoleOverride {
        default_launch: Some(vec!["cfg-role".into()]),
        ..Default::default()
    };
    role.account_bindings.insert(Agent::Claude, "acc-a".into());
    ws.roles.insert("dev".into(), role);
    ws.default_launch = Some(vec!["cfg-ws".into()]);
    ws.account_bindings.insert(Agent::Claude, "acc-d".into());
    cfg.default_launch = Some(vec!["cfg-global".into()]);
    cfg.account_bindings.insert(Agent::Claude, "acc-a".into());
    Ok((cfg, w1))
}

#[test]
fn s7_full_precedence_chain_launch_to_sole_eligible() -> anyhow::Result<()> {
    let (mut cfg, w1) = precedence_fixture()?;
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "acc-a".into());
    cfg.validate_accounts().unwrap();

    let won = resolve_launch(
        &cfg,
        Some(&w1),
        "dev",
        Some(&["cfg-one".into()]),
        Some(Agent::Claude),
    )
    .unwrap();
    assert_eq!(won[0].config_id, "cfg-one");

    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Codex)).unwrap();
    assert_eq!(won[0].config_id, "cfg-role");

    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .roles
        .get_mut("dev")
        .unwrap()
        .default_launch = None;
    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Gemini)).unwrap();
    assert_eq!(won.len(), 1);
    assert_eq!(won[0].config_id, "cfg-ws");

    cfg.workspaces.get_mut("w1").unwrap().default_launch = None;
    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Codex)).unwrap();
    assert_eq!(won[0].config_id, "cfg-global");

    cfg.default_launch = None;
    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Claude)).unwrap();
    assert_eq!(won[0].account_id, "acc-a");
    assert!(won[0].synthesized);

    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .roles
        .get_mut("dev")
        .unwrap()
        .account_bindings
        .remove(&Agent::Claude);
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .account_bindings
        .insert(Agent::Codex, "acc-b".into());
    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Codex)).unwrap();
    assert_eq!(won[0].account_id, "acc-b");

    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .account_bindings
        .remove(&Agent::Codex);
    cfg.account_bindings.insert(Agent::Codex, "acc-b".into());
    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Codex)).unwrap();
    assert_eq!(won[0].account_id, "acc-b");

    cfg.account_bindings.remove(&Agent::Codex);
    cfg.account_bindings.remove(&Agent::Claude);
    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Codex)).unwrap();
    assert_eq!(won.len(), 1);
    assert_eq!(won[0].account_id, "acc-b");

    let err = resolve_launch(&cfg, Some(&w1), "dev", None, None).unwrap_err();
    assert!(err.to_string().contains("multiple accounts"), "{err}");
    Ok(())
}

#[test]
fn s7_invalid_explicit_selection_rejects_atomically() -> anyhow::Result<()> {
    let (mut cfg, w1) = precedence_fixture()?;
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "acc-a".into());

    let err = resolve_launch(
        &cfg,
        Some(&w1),
        "dev",
        Some(&["cfg-one".into(), "no-such-config".into()]),
        Some(Agent::Claude),
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("unknown agent configuration"),
        "{err}"
    );

    let err = resolve_launch(
        &cfg,
        Some(&w1),
        "dev",
        Some(&["cfg-one".into(), "cfg-one".into()]),
        Some(Agent::Claude),
    )
    .unwrap_err();
    assert!(err.to_string().contains("duplicate"), "{err}");

    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .roles
        .get_mut("dev")
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "acc-d".into());
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .roles
        .get_mut("dev")
        .unwrap()
        .default_launch = None;
    cfg.workspaces.get_mut("w1").unwrap().default_launch = None;
    cfg.default_launch = None;
    let err = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Claude)).unwrap_err();
    assert!(err.to_string().contains("not assigned"), "{err}");

    cfg.workspaces.get_mut("w1").unwrap().default_launch = Some(vec![]);
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .roles
        .get_mut("dev")
        .unwrap()
        .account_bindings
        .remove(&Agent::Claude);
    let empty: Vec<String> = vec![];
    let won = resolve_launch(&cfg, Some(&w1), "dev", Some(&empty), Some(Agent::Claude)).unwrap();
    assert!(won.is_empty());
    Ok(())
}

#[test]
fn s7_global_candidates_cannot_smuggle_foreign_accounts() -> anyhow::Result<()> {
    let (mut cfg, w1, _w2) = scoped_fixture()?;
    cfg.agent_configurations
        .insert("cfg-foreign".into(), configuration(Agent::Claude, "acc-d"));
    cfg.agent_configurations
        .insert("cfg-home".into(), configuration(Agent::Claude, "acc-a"));
    cfg.default_launch = Some(vec!["cfg-foreign".into(), "cfg-home".into()]);
    cfg.validate_accounts().unwrap();
    let won = resolve_launch(&cfg, Some(&w1), "", None, Some(Agent::Claude)).unwrap();
    assert_eq!(won.len(), 1);
    assert_eq!(won[0].account_id, "acc-a");

    cfg.default_launch = Some(vec!["cfg-foreign".into()]);
    let won = resolve_launch(&cfg, Some(&w1), "", None, Some(Agent::Claude)).unwrap();
    assert!(won.is_empty(), "foreign-only global list leaked: {won:?}");
    Ok(())
}
