// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn validate_accounts_rejects_disabled_bindings_at_all_scopes() {
    let (mut cfg, ws) = config();
    cfg.accounts.get_mut("work").unwrap().enabled = false;
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .accounts
        .push("work".into());

    // Global binding to disabled account is rejected
    cfg.account_bindings.insert(Agent::Claude, "work".into());
    cfg.validate_accounts().unwrap_err();
    cfg.account_bindings.clear();
    cfg.validate_accounts().unwrap();

    // Workspace binding to disabled account is rejected
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "work".into());
    cfg.validate_accounts().unwrap_err();
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .clear();
    cfg.validate_accounts().unwrap();

    // Workspace-role binding to disabled account is rejected
    cfg.workspaces.get_mut(ws.as_str()).unwrap().roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::from([(Agent::Claude, "work".into())]),
            ..Default::default()
        },
    );
    cfg.validate_accounts().unwrap_err();
    cfg.workspaces.get_mut(ws.as_str()).unwrap().roles.clear();
    cfg.validate_accounts().unwrap();
}

#[test]
fn prune_account_bindings_clears_all_scopes_and_enables_fallback() {
    let (mut cfg, ws) = config();
    // Allow personal and work in workspace
    cfg.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["personal".into(), "work".into()];

    // Bind work at global, workspace, and role scopes
    cfg.account_bindings.insert(Agent::Claude, "work".into());
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "work".into());
    cfg.workspaces.get_mut(ws.as_str()).unwrap().roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::from([(Agent::Claude, "work".into())]),
            ..Default::default()
        },
    );

    // Disable work
    cfg.accounts.get_mut("work").unwrap().enabled = false;
    cfg.validate_accounts().unwrap_err();

    // Prune disabled bindings across all scopes
    cfg.prune_account_bindings("work");
    cfg.validate_accounts().unwrap();

    assert!(cfg.account_bindings.is_empty());
    assert!(cfg.workspaces[ws.as_str()].account_bindings.is_empty());
    assert!(
        cfg.workspaces[ws.as_str()].roles["smith"]
            .account_bindings
            .is_empty()
    );

    // Fallback to the sole enabled account in workspace (personal)
    let resolved = resolve_account(&cfg, Agent::Claude, Some(&ws), "smith")
        .unwrap()
        .unwrap();
    assert_eq!(resolved.name, "Personal");
}

#[test]
fn resolve_launch_one_launch_wins_and_validates_atomically() {
    let (cfg, ws) = launch_fixture();
    let instances = resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["codex-c".to_owned(), "claude-a".to_owned()]),
        None,
    )
    .unwrap();
    assert_eq!(instances.len(), 2);
    assert_eq!(instances[0].config_id, "codex-c");
    assert_eq!(instances[0].label, "Codex · Work");
    assert!(!instances[0].synthesized);
    assert_eq!(instances[1].config_id, "claude-a");
    assert_eq!(instances[1].label, "Claude · Work");

    // Unknown ID fails the whole selection (no partial launch).
    resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["codex-c".to_owned(), "nope".to_owned()]),
        None,
    )
    .unwrap_err();
    // Duplicate ID rejected.
    resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["codex-c".to_owned(), "codex-c".to_owned()]),
        None,
    )
    .unwrap_err();
    // Explicit empty list resolves to no instances (shell-only).
    let empty = resolve_launch(&cfg, Some(&ws), "smith", Some(&[]), None).unwrap();
    assert!(empty.is_empty());
}

#[test]
fn resolve_launch_multi_instance_admission_follows_folder_var_kind() {
    let (mut cfg, ws) = launch_fixture();
    // Two Claude instances share a `Dir`-kind folder var → admitted.
    let instances = resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["claude-a".to_owned(), "claude-b".to_owned()]),
        None,
    )
    .unwrap();
    assert_eq!(instances.len(), 2);

    let profile_for = |agent: Agent, name: &str| AccountConfig {
        enabled: true,
        name: name.into(),
        provider: AiProvider::for_agent(agent).unwrap(),
        credential: AccountCredential::Profile {
            agent,
            directory: PathBuf::from("/profiles").join(name),
            xdg_roots: None,
            source_selector: None,
        },
    };
    let add_pair = |cfg: &mut AppConfig, agent: Agent, prefix: &str| {
        for (id, account) in [
            (format!("{prefix}-a"), format!("{prefix}-work")),
            (format!("{prefix}-b"), format!("{prefix}-personal")),
        ] {
            cfg.accounts
                .insert(account.clone(), profile_for(agent, &account));
            cfg.agent_configurations.insert(
                id,
                AgentConfiguration {
                    agent,
                    account: account.clone(),
                    model: None,
                    base_url: None,
                    display_label: None,
                    invoked_via_wrapper: None,
                },
            );
            cfg.workspaces
                .get_mut(ws.as_str())
                .unwrap()
                .accounts
                .push(account);
        }
    };
    // Kimi has no folder var → two instances rejected.
    add_pair(&mut cfg, Agent::Kimi, "kimi");
    let error = resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["kimi-a".to_owned(), "kimi-b".to_owned()]),
        None,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("no config-folder env var"),
        "unexpected kimi rejection: {error}"
    );
    // Amp is `XdgRoot`-kind → two instances rejected with the XDG reason.
    add_pair(&mut cfg, Agent::Amp, "amp");
    let error = resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["amp-a".to_owned(), "amp-b".to_owned()]),
        None,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("XDG_DATA_HOME"),
        "unexpected amp rejection: {error}"
    );
    // OpenCode also exports an XDG root. Until each pane has a complete
    // process-wide XDG namespace, two provider-bound profiles in one
    // container are rejected rather than sharing unrelated OpenCode state.
    add_pair(&mut cfg, Agent::Opencode, "opencode");
    let error = resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["opencode-a".to_owned(), "opencode-b".to_owned()]),
        None,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("XDG_DATA_HOME"),
        "unexpected OpenCode rejection: {error}"
    );
    // A lone second-agent instance still resolves.
    let instances =
        resolve_launch(&cfg, Some(&ws), "smith", Some(&["kimi-a".to_owned()]), None).unwrap();
    assert_eq!(instances.len(), 1);
}

#[test]
fn resolve_launch_scope_precedence_replaces_without_union() {
    let (mut cfg, ws) = launch_fixture();
    cfg.default_launch = Some(vec!["codex-c".into()]);
    cfg.workspaces.get_mut(ws.as_str()).unwrap().default_launch =
        Some(vec!["claude-a".into(), "claude-b".into()]);
    cfg.workspaces.get_mut(ws.as_str()).unwrap().roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            default_launch: Some(vec!["claude-b".into()]),
            ..Default::default()
        },
    );
    // Role scope replaces workspace + global entirely.
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "claude-b");
    // Other roles fall through to the workspace scope.
    let instances = resolve_launch(&cfg, Some(&ws), "other", None, None).unwrap();
    assert_eq!(instances.len(), 2);
    assert_eq!(instances[0].config_id, "claude-a");
}

#[test]
fn resolve_launch_global_candidates_filter_by_authorization() {
    let (mut cfg, ws) = launch_fixture();
    cfg.agent_configurations.insert(
        "zai-codex".into(),
        AgentConfiguration {
            agent: Agent::Codex,
            account: "zai-key".into(),
            model: Some("glm-4".into()),
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );
    cfg.default_launch = Some(vec!["zai-codex".into(), "codex-c".into()]);
    // zai-key is outside the workspace allowlist: filtered, not an error.
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "codex-c");
    // Workspace-scoped defaults validate atomically instead.
    cfg.workspaces.get_mut(ws.as_str()).unwrap().default_launch = Some(vec!["zai-codex".into()]);
    resolve_launch(&cfg, Some(&ws), "smith", None, None).unwrap_err();
}

#[test]
fn resolve_launch_fallback_needs_a_single_eligible_instance() {
    let (cfg, ws) = launch_fixture();
    // Several eligible instances: picker needed, never a silent pick.
    let err = resolve_launch(&cfg, Some(&ws), "smith", None, None).unwrap_err();
    assert!(err.to_string().contains("multiple accounts"), "{err}");
    // Sole eligible instance fast-starts with a synthesized ID.
    let mut solo = AppConfig::default();
    solo.accounts.insert("only".into(), profile("Only"));
    let instances = resolve_launch(&solo, None, "smith", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "only@claude");
    assert!(instances[0].synthesized);
    assert_eq!(instances[0].label, "Claude · Only");
    // Zero eligible accounts is an actionable error.
    let empty = AppConfig::default();
    resolve_launch(&empty, None, "smith", None, None).unwrap_err();
}

#[test]
fn resolve_launch_committed_agent_honors_global_binding() {
    // E2E shape: many accounts, no launch lists, one per-agent default.
    // Fast start must honor the binding, never prompt.
    let (mut cfg, ws) = launch_fixture();
    cfg.account_bindings
        .insert(Agent::Claude, "claude-personal".into());
    for workspace in [None, Some(&ws)] {
        let instances =
            resolve_launch(&cfg, workspace, "smith", None, Some(Agent::Claude)).unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].account_id, "claude-personal");
        assert_eq!(instances[0].agent, Agent::Claude);
        assert_eq!(instances[0].config_id, "claude-personal@claude");
        assert_eq!(instances[0].label, "Claude · Personal");
        assert!(instances[0].synthesized);
    }
}

#[test]
fn resolve_launch_role_binding_beats_global_binding() {
    let (mut cfg, ws) = launch_fixture();
    cfg.account_bindings
        .insert(Agent::Claude, "claude-personal".into());
    cfg.workspaces.get_mut(ws.as_str()).unwrap().roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::from([(Agent::Claude, "claude-work".into())]),
            ..Default::default()
        },
    );
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None, Some(Agent::Claude)).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].account_id, "claude-work");
}

#[test]
fn resolve_launch_default_launch_beats_binding() {
    // A full launch list names the composition explicitly; a bare
    // per-agent account preference loses at any scope.
    let (mut cfg, ws) = launch_fixture();
    cfg.default_launch = Some(vec!["claude-a".into()]);
    cfg.account_bindings
        .insert(Agent::Claude, "claude-personal".into());
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None, Some(Agent::Claude)).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "claude-a");
    assert!(!instances[0].synthesized);
}

#[test]
fn resolve_launch_agent_scopes_sole_eligible_fallback() {
    // No bindings: only codex-work supports Codex, so the committed
    // agent resolves it alone; without an agent the same registry is
    // ambiguous and still needs a picker.
    let (cfg, ws) = launch_fixture();
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None, Some(Agent::Codex)).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "codex-work@codex");
    let err = resolve_launch(&cfg, Some(&ws), "smith", None, None).unwrap_err();
    assert!(err.to_string().contains("multiple accounts"), "{err}");
}

#[test]
fn resolve_launch_invalid_binding_fails_without_silent_fallback() {
    let (mut cfg, _) = launch_fixture();
    cfg.account_bindings.insert(Agent::Claude, "nope".into());
    let err = resolve_launch(&cfg, None, "smith", None, Some(Agent::Claude)).unwrap_err();
    assert!(err.to_string().contains("unknown account"), "{err}");
    // codex-work is a Codex-owned profile: incompatible with Claude.
    cfg.account_bindings
        .insert(Agent::Claude, "codex-work".into());
    let err = resolve_launch(&cfg, None, "smith", None, Some(Agent::Claude)).unwrap_err();
    assert!(err.to_string().contains("does not support"), "{err}");
}
