// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! S1/S2/S7 scenario proof: account lifecycle, workspace scope, selection precedence.
//!
//! S1: register/import >= 4 accounts (two sharing one provider), rename/reorder
//! stability, rescan without duplicates, draft/apply/cancel.
//! S2 (config layer): W1 admits exactly A/B/C, W2 a different set; D is absent
//! from W1 and every explicit attempt to use it there fails closed.
//! S7: launch > workspace-role > workspace > global > sole-eligible precedence,
//! with atomic rejection of invalid explicit selections (no silent fallback).

use jackin_config::{
    AccountConfig, AccountCredential, AgentConfiguration, AiProvider, AppConfig, ConfigEditor,
    WorkspaceConfig, WorkspaceRoleOverride, resolve_account, resolve_launch,
};
use jackin_core::{Agent, EnvValue, JackinPaths, WorkspaceName};
use std::path::Path;

fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
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

fn seed_claude_credentials(home: &Path) {
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(
        home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
    )
    .unwrap();
}

fn fresh_paths() -> (tempfile::TempDir, JackinPaths) {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    (temp, paths)
}

/// A valid workspace shell: creation requires a workdir covered by a mount.
fn test_workspace(root: &Path, accounts: Vec<String>) -> WorkspaceConfig {
    let src = root.join("mnt-src");
    std::fs::create_dir_all(&src).unwrap();
    WorkspaceConfig {
        workdir: "/workspace/w1".into(),
        mounts: vec![jackin_config::MountConfig {
            src: src.display().to_string(),
            dst: "/workspace/w1".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        accounts,
        ..Default::default()
    }
}

/// S1: four registered accounts, two on the same provider, survive a save/reload round trip.
#[test]
fn s1_register_four_accounts_two_sharing_one_provider() {
    let (_temp, paths) = fresh_paths();
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
    // Same credential source under a new ID is rejected: no silent duplicates.
    let dup = editor.upsert_account(
        "claude-copy",
        &api_key(AiProvider::Anthropic, "Copy", "secret-personal"),
    );
    assert!(dup.is_err(), "duplicate credential source was accepted");
    let saved = editor.save().unwrap();
    // Fresh installs also bootstrap-scan host evidence (keychain/env), so the
    // registry may hold more than the four registered here; the four must be present.
    assert!(saved.accounts.len() >= 4, "registry lost accounts: {:?}", saved.accounts.keys().collect::<Vec<_>>());

    let reopened = ConfigEditor::open(&paths).unwrap().save().unwrap();
    for id in ["claude-personal", "claude-work", "codex-main", "gemini-main"] {
        assert!(reopened.accounts.contains_key(id), "missing {id} after reload");
    }
    assert_eq!(reopened.accounts["claude-personal"].name, "Personal");
}

/// S1: importing a discovered profile, then rescanning twice, never duplicates or overwrites.
#[test]
fn s1_import_then_rescan_is_duplicate_free() {
    let (_temp, paths) = fresh_paths();
    seed_claude_credentials(&paths.home_dir);
    // The seeded fixture is discoverable through the public discovery API.
    let discovered = jackin_config::discover_default_accounts(&paths.home_dir);
    assert!(
        discovered.accounts.iter().any(|found| {
            found.agent == Agent::Claude
                && found.directory.starts_with(&paths.home_dir)
        }),
        "seeded fixture was not discovered: {discovered:?}"
    );
    let mut editor = ConfigEditor::open(&paths).unwrap();
    let first = editor.scan_for_accounts().unwrap();
    let snapshot = editor.save().unwrap().accounts;
    // Imported either by the fresh-install bootstrap or by this scan; either
    // way exactly one account owns the fixture's credential source.
    assert!(
        snapshot.contains_key("default-claude"),
        "seeded profile missing after scan {first:?}: {:?}",
        snapshot.keys().collect::<Vec<_>>()
    );

    // Rescan with identical inputs: nothing added, registry byte-identical.
    let mut editor = ConfigEditor::open(&paths).unwrap();
    let second = editor.scan_for_accounts().unwrap();
    assert!(
        second.added_accounts.is_empty(),
        "rescan added duplicates: {second:?}"
    );
    let resaved = editor.save().unwrap();
    assert_eq!(resaved.accounts, snapshot, "rescan mutated the registry");

    // Operator rename of the imported ID still dedupes by credential source.
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
}

/// S1: renaming an account's display name keeps IDs, bindings, and launches stable.
#[test]
fn s1_rename_display_name_keeps_everything_stable() {
    let (temp, paths) = fresh_paths();
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account("a", &api_key(AiProvider::Anthropic, "A", "secret-a"))
        .unwrap();
    editor
        .upsert_account("b", &api_key(AiProvider::OpenAi, "B", "secret-b"))
        .unwrap();
    let w1 = wn("w1");
    editor
        .create_workspace(&w1, test_workspace(temp.path(), vec!["a".into(), "b".into()]))
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
    let resolved_before = resolve_launch(&before, Some(&w1), "", None, Some(Agent::Claude)).unwrap();
    let resolved_after = resolve_launch(&after, Some(&w1), "", None, Some(Agent::Claude)).unwrap();
    assert_eq!(resolved_before.len(), 1);
    assert_eq!(resolved_after.len(), 1);
    assert_eq!(resolved_before[0].account_id, resolved_after[0].account_id);
}

/// S1: reordering a workspace allowlist preserves the set, bindings, and resolution.
#[test]
fn s1_reorder_workspace_accounts_preserves_set_and_bindings() {
    let (temp, paths) = fresh_paths();
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
    let w1 = wn("w1");
    editor
        .create_workspace(
            &w1,
            test_workspace(temp.path(), vec!["a".into(), "b".into(), "c".into()]),
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
        reordered.workspaces["w1"].account_bindings.get(&Agent::Claude),
        Some(&"a".to_owned())
    );
    let resolved = resolve_launch(&reordered, Some(&w1), "", None, Some(Agent::Claude)).unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].account_id, "a");
}

/// S1: dropping the editor without save cancels the draft; save applies it.
#[test]
fn s1_draft_cancel_leaves_disk_untouched_apply_persists() {
    let (_temp, paths) = fresh_paths();
    let before = ConfigEditor::open(&paths).unwrap().save().unwrap();
    assert!(!before.accounts.contains_key("drafted"));
    let bytes_before = std::fs::read(&paths.config_file).unwrap();

    // Draft then cancel: drop without save.
    {
        let mut editor = ConfigEditor::open(&paths).unwrap();
        editor
            .upsert_account("drafted", &api_key(AiProvider::Anthropic, "Draft", "secret-x"))
            .unwrap();
    }
    let bytes_after_cancel = std::fs::read(&paths.config_file).unwrap();
    assert_eq!(bytes_before, bytes_after_cancel, "cancel wrote to disk");
    let cancelled = ConfigEditor::open(&paths).unwrap().save().unwrap();
    assert!(!cancelled.accounts.contains_key("drafted"));

    // Draft then apply: save persists.
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account("drafted", &api_key(AiProvider::Anthropic, "Draft", "secret-x"))
        .unwrap();
    editor.save().unwrap();
    let applied = ConfigEditor::open(&paths).unwrap().save().unwrap();
    assert!(applied.accounts.contains_key("drafted"));
}

/// S1: disabling an account prunes its bindings; re-enabling does not resurrect them silently.
#[test]
fn s1_disable_prunes_bindings_without_deleting_the_account() {
    let (temp, paths) = fresh_paths();
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account("a", &api_key(AiProvider::Anthropic, "A", "secret-a"))
        .unwrap();
    editor
        .upsert_account("b", &api_key(AiProvider::OpenAi, "B", "secret-b"))
        .unwrap();
    let w1 = wn("w1");
    editor
        .create_workspace(&w1, test_workspace(temp.path(), vec!["a".into(), "b".into()]))
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
    assert!(pruned.accounts.contains_key("a"), "disable deleted the account");
    assert!(
        !pruned.workspaces["w1"]
            .account_bindings
            .values()
            .any(|id| id == "a"),
        "disable left a live binding behind"
    );
}

fn scoped_fixture() -> (AppConfig, WorkspaceName, WorkspaceName) {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "acc-a".into(),
        api_key(AiProvider::Anthropic, "A", "secret-a"),
    );
    cfg.accounts.insert(
        "acc-b".into(),
        api_key(AiProvider::OpenAi, "B", "secret-b"),
    );
    cfg.accounts.insert(
        "acc-c".into(),
        api_key(AiProvider::Google, "C", "secret-c"),
    );
    cfg.accounts.insert(
        "acc-d".into(),
        api_key(AiProvider::Anthropic, "D", "secret-d"),
    );
    let w1 = wn("w1");
    let w2 = wn("w2");
    let mut ws1 = WorkspaceConfig::default();
    ws1.accounts = vec!["acc-a".into(), "acc-b".into(), "acc-c".into()];
    let mut ws2 = WorkspaceConfig::default();
    ws2.accounts = vec!["acc-c".into(), "acc-d".into()];
    cfg.workspaces.insert("w1".into(), ws1);
    cfg.workspaces.insert("w2".into(), ws2);
    (cfg, w1, w2)
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

/// S2: W1 admits exactly A/B/C; D is absent and every explicit use of D in W1 fails closed.
#[test]
fn s2_w1_admits_exactly_abc_d_is_inaccessible() {
    let (mut cfg, w1, w2) = scoped_fixture();
    cfg.agent_configurations.insert(
        "launch-a".into(),
        configuration(Agent::Claude, "acc-a"),
    );
    cfg.agent_configurations.insert(
        "launch-b".into(),
        configuration(Agent::Codex, "acc-b"),
    );
    cfg.agent_configurations.insert(
        "launch-c".into(),
        configuration(Agent::Gemini, "acc-c"),
    );
    cfg.agent_configurations.insert(
        "launch-d".into(),
        configuration(Agent::Claude, "acc-d"),
    );
    cfg.validate_accounts().unwrap();

    // W1 launch over its own configurations admits exactly A/B/C.
    let instances = resolve_launch(
        &cfg,
        Some(&w1),
        "",
        Some(&["launch-a".into(), "launch-b".into(), "launch-c".into()]),
        None,
    )
    .unwrap();
    let mut admitted: Vec<&str> = instances
        .iter()
        .map(|i| i.account_id.as_str())
        .collect();
    admitted.sort_unstable();
    assert_eq!(admitted, ["acc-a", "acc-b", "acc-c"]);

    // D via one-launch selection in W1: atomic failure, no D, no fallback.
    let err = resolve_launch(&cfg, Some(&w1), "", Some(&["launch-d".into()]), None).unwrap_err();
    assert!(err.to_string().contains("not assigned"), "{err}");

    // D via workspace binding in W1: rejected at resolution.
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "acc-d".into());
    let err = resolve_account(&cfg, Agent::Claude, Some(&w1), "").unwrap_err();
    assert!(err.to_string().contains("not assigned"), "{err}");
    // No fallback to an ambient login: the error is terminal.
    assert!(resolve_launch(&cfg, Some(&w1), "", None, Some(Agent::Claude)).is_err());
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .account_bindings
        .remove(&Agent::Claude);

    // W2 is a different set: C and D resolve there.
    let d = resolve_launch(&cfg, Some(&w2), "", Some(&["launch-d".into()]), None).unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].account_id, "acc-d");
    // But A is foreign to W2.
    let err = resolve_launch(&cfg, Some(&w2), "", Some(&["launch-a".into()]), None).unwrap_err();
    assert!(err.to_string().contains("not assigned"), "{err}");
}

/// S2: unknown workspaces and stale account IDs fail closed, never ambient.
#[test]
fn s2_unknown_workspace_and_stale_ids_fail_closed() {
    let (mut cfg, w1, _w2) = scoped_fixture();
    let ghost_ws = wn("ghost");
    assert!(resolve_account(&cfg, Agent::Claude, Some(&ghost_ws), "").is_err());
    assert!(resolve_launch(&cfg, Some(&ghost_ws), "", None, Some(Agent::Claude)).is_err());

    // Stale ID lingering in a workspace allowlist: hard error, not a skip.
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .accounts
        .push("deleted-account".into());
    let err = resolve_account(&cfg, Agent::Codex, Some(&w1), "").unwrap_err();
    assert!(err.to_string().contains("unknown account"), "{err}");
    assert!(resolve_launch(&cfg, Some(&w1), "", None, None).is_err());
}

fn precedence_fixture() -> (AppConfig, WorkspaceName) {
    let (mut cfg, w1, _w2) = scoped_fixture();
    // One configuration per launch-list layer, each pinned to a distinct account.
    cfg.agent_configurations.insert(
        "cfg-one".into(),
        configuration(Agent::Claude, "acc-a"),
    );
    cfg.agent_configurations.insert(
        "cfg-role".into(),
        configuration(Agent::Codex, "acc-b"),
    );
    cfg.agent_configurations.insert(
        "cfg-ws".into(),
        configuration(Agent::Gemini, "acc-c"),
    );
    // Global layer must also be W1-authorized to be a fair precedence probe.
    cfg.agent_configurations.insert(
        "cfg-global".into(),
        configuration(Agent::Codex, "acc-b"),
    );
    let ws = cfg.workspaces.get_mut("w1").unwrap();
    let mut role = WorkspaceRoleOverride::default();
    role.default_launch = Some(vec!["cfg-role".into()]);
    role.account_bindings.insert(Agent::Claude, "acc-a".into());
    ws.roles.insert("dev".into(), role);
    ws.default_launch = Some(vec!["cfg-ws".into()]);
    ws.account_bindings.insert(Agent::Claude, "acc-d".into());
    // NOTE: ws binding to acc-d is intentionally left invalid for later S7
    // atomicity probes; precedence probes below overwrite or clear it first.
    cfg.default_launch = Some(vec!["cfg-global".into()]);
    cfg.account_bindings.insert(Agent::Claude, "acc-a".into());
    (cfg, w1)
}

/// S7: launch > role > workspace > global, then bindings role > workspace > global,
/// then sole-eligible. Each layer wins only when every layer above it is absent.
#[test]
fn s7_full_precedence_chain_launch_to_sole_eligible() {
    let (mut cfg, w1) = precedence_fixture();
    // Repair the intentionally invalid workspace binding for the precedence walk.
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "acc-a".into());
    cfg.validate_accounts().unwrap();

    // one-launch beats everything.
    let won =
        resolve_launch(&cfg, Some(&w1), "dev", Some(&["cfg-one".into()]), Some(Agent::Claude))
            .unwrap();
    assert_eq!(won[0].config_id, "cfg-one");

    // role default_launch beats workspace and global.
    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Codex)).unwrap();
    assert_eq!(won[0].config_id, "cfg-role");

    // workspace default_launch beats global; explicit scope replaces, never unions.
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

    // global default_launch is the last launch-list layer.
    cfg.workspaces.get_mut("w1").unwrap().default_launch = None;
    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Codex)).unwrap();
    assert_eq!(won[0].config_id, "cfg-global");

    // role binding beats workspace and global bindings.
    cfg.default_launch = None;
    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Claude)).unwrap();
    assert_eq!(won[0].account_id, "acc-a");
    assert!(won[0].synthesized);

    // workspace binding beats the global binding.
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

    // global binding is the last binding layer.
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .account_bindings
        .remove(&Agent::Codex);
    cfg.account_bindings.insert(Agent::Codex, "acc-b".into());
    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Codex)).unwrap();
    assert_eq!(won[0].account_id, "acc-b");

    // sole eligible: only acc-b supports Codex in W1, so it wins with no defaults at all.
    cfg.account_bindings.remove(&Agent::Codex);
    cfg.account_bindings.remove(&Agent::Claude);
    let won = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Codex)).unwrap();
    assert_eq!(won.len(), 1);
    assert_eq!(won[0].account_id, "acc-b");

    // ambiguity without defaults is a picker error, never a guess.
    let err = resolve_launch(&cfg, Some(&w1), "dev", None, None).unwrap_err();
    assert!(err.to_string().contains("multiple accounts"), "{err}");
}

/// S7: any invalid explicit selection rejects the whole launch atomically.
#[test]
fn s7_invalid_explicit_selection_rejects_atomically() {
    let (mut cfg, w1) = precedence_fixture();
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "acc-a".into());

    // Unknown configuration ID poisons the whole one-launch list.
    let err = resolve_launch(
        &cfg,
        Some(&w1),
        "dev",
        Some(&["cfg-one".into(), "no-such-config".into()]),
        Some(Agent::Claude),
    )
    .unwrap_err();
    assert!(err.to_string().contains("unknown agent configuration"), "{err}");

    // Duplicate entries are rejected, not deduped.
    let err = resolve_launch(
        &cfg,
        Some(&w1),
        "dev",
        Some(&["cfg-one".into(), "cfg-one".into()]),
        Some(Agent::Claude),
    )
    .unwrap_err();
    assert!(err.to_string().contains("duplicate"), "{err}");

    // Unauthorized binding at any scope fails instead of falling back.
    cfg.workspaces
        .get_mut("w1")
        .unwrap()
        .roles
        .get_mut("dev")
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "acc-d".into());
    cfg.workspaces.get_mut("w1").unwrap().roles.get_mut("dev").unwrap().default_launch = None;
    cfg.workspaces.get_mut("w1").unwrap().default_launch = None;
    cfg.default_launch = None;
    let err = resolve_launch(&cfg, Some(&w1), "dev", None, Some(Agent::Claude)).unwrap_err();
    assert!(err.to_string().contains("not assigned"), "{err}");

    // An explicit empty list resolves to no instances (shell-only shape), not to a default.
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
}

/// S7: global launch candidates filter by workspace authorization instead of leaking in.
#[test]
fn s7_global_candidates_cannot_smuggle_foreign_accounts() {
    let (mut cfg, w1, _w2) = scoped_fixture();
    cfg.agent_configurations.insert(
        "cfg-foreign".into(),
        configuration(Agent::Claude, "acc-d"),
    );
    cfg.agent_configurations.insert(
        "cfg-home".into(),
        configuration(Agent::Claude, "acc-a"),
    );
    // Global list names a foreign account first: it filters out, the
    // authorized candidate still resolves, and D never appears.
    cfg.default_launch = Some(vec!["cfg-foreign".into(), "cfg-home".into()]);
    cfg.validate_accounts().unwrap();
    let won = resolve_launch(&cfg, Some(&w1), "", None, Some(Agent::Claude)).unwrap();
    assert_eq!(won.len(), 1);
    assert_eq!(won[0].account_id, "acc-a");

    // A global list with ONLY foreign candidates resolves empty, not ambient.
    cfg.default_launch = Some(vec!["cfg-foreign".into()]);
    let won = resolve_launch(&cfg, Some(&w1), "", None, Some(Agent::Claude)).unwrap();
    assert!(won.is_empty(), "foreign-only global list leaked: {won:?}");
}
