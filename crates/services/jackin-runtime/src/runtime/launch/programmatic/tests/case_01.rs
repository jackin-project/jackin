// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn a_fully_supplied_programmatic_launch_validates() {
    let mut options = opts();
    options.model = Some("claude-opus-5".to_owned());
    options.effort = Some(ReasoningEffort::Medium);
    options
        .env
        .insert("LINEAR_TEAM".to_owned(), "core".to_owned());
    options.on_demand_bindings.push(ExecBinding {
        name: "OP_SERVICE_ACCOUNT_TOKEN".to_owned(),
        kind: ExecKind::Op,
        source: "op://tailrocks/jackin-operator/credential".to_owned(),
    });
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Ok(())
    );
}

#[test]
fn an_interactive_launch_skips_every_programmatic_check() {
    // The interactive path can still answer a prompt, so an unresolved agent
    // and a missing trust grant are not validation failures there.
    let options = LoadOptions::default();
    assert_eq!(
        options.validate_programmatic(&untrusted_config(), &selector()),
        Ok(())
    );
}

#[test]
fn an_unresolved_agent_is_a_validation_failure() {
    let mut options = opts();
    options.agent = None;
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::AgentNotResolved {
            role: selector().key()
        })
    );
}

#[test]
fn a_missing_trust_grant_is_a_validation_failure_naming_the_grant_command() {
    let error = opts()
        .validate_programmatic(&untrusted_config(), &selector())
        .expect_err("an untrusted role must not launch non-interactively");
    assert_eq!(
        error,
        LoadOptionsError::TrustNotGranted {
            role: selector().key()
        }
    );
    assert!(
        error.to_string().contains("jackin config trust grant"),
        "the error must name the command that fixes it, got {error}"
    );
}

#[test]
fn an_unregistered_role_is_treated_as_untrusted() {
    assert_eq!(
        opts().validate_programmatic(&AppConfig::default(), &selector()),
        Err(LoadOptionsError::TrustNotGranted {
            role: selector().key()
        })
    );
}

#[test]
fn a_builtin_role_needs_no_explicit_grant() {
    let (builtin, _) = jackin_config::BUILTIN_ROLES
        .first()
        .copied()
        .expect("at least one built-in role ships with jackin");
    let builtin_selector = RoleSelector::parse(builtin).expect("built-in selector must parse");
    assert_eq!(
        opts().validate_programmatic(&AppConfig::default(), &builtin_selector),
        Ok(())
    );
}

#[test]
fn a_role_branch_cannot_be_loaded_without_a_tty() {
    let mut options = opts();
    options.role_branch = Some("feat/my-pr".to_owned());
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::RoleBranchNotAllowed {
            branch: "feat/my-pr".to_owned()
        })
    );
}

#[test]
fn a_missing_registered_account_is_a_validation_failure() {
    let mut options = opts();
    options.selection = Some(jackin_core::LaunchSelection::Account("missing".to_owned()));
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::AccountMissing {
            account: "missing".to_owned()
        })
    );
}

#[test]
fn launch_selection_rejects_accounts_outside_workspace_allowlist() {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider, WorkspaceConfig};
    let mut config = trusted_config();
    config.accounts.insert(
        "private".to_owned(),
        AccountConfig {
            enabled: true,
            name: "Private".to_owned(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: Agent::Codex,
                directory: "/private/codex".into(),
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    config
        .workspaces
        .insert("work".to_owned(), WorkspaceConfig::default());
    let workspace = jackin_core::WorkspaceName::parse("work").unwrap();
    with_account_selection(&config, Agent::Codex, Some(&workspace), "codex", "private")
        .unwrap_err();
    config
        .workspaces
        .get_mut("work")
        .unwrap()
        .accounts
        .push("private".to_owned());
    let selected =
        with_account_selection(&config, Agent::Codex, Some(&workspace), "codex", "private")
            .unwrap();
    assert!(
        jackin_config::resolve_account(&selected, Agent::Codex, Some(&workspace), "codex")
            .unwrap()
            .is_some()
    );
    assert!(
        config.workspaces["work"].roles.is_empty(),
        "per-launch binding must not mutate persistent config"
    );
}

#[test]
fn launch_selection_without_defaults_synthesizes_ephemeral_default() {
    let (config, workspace) = two_account_config();
    // Two eligible accounts and no defaults: `resolve_launch` alone is
    // ambiguous. The explicit pick must resolve it, not fail with it.
    jackin_config::resolve_launch(&config, Some(&workspace), "codex", None, None).unwrap_err();

    let selected =
        with_account_selection(&config, Agent::Codex, Some(&workspace), "codex", "private")
            .unwrap();
    let instances =
        jackin_config::resolve_launch(&selected, Some(&workspace), "codex", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].agent, Agent::Codex);
    assert_eq!(instances[0].account_id, "private");
    assert_eq!(instances[0].config_id, "private@codex");
    // The ephemeral default lives on the clone only.
    assert_eq!(config.agent_configurations.len(), 2);
    assert!(config.default_launch.is_none());
    assert!(config.workspaces["work"].roles.is_empty());
    assert!(
        jackin_config::resolve_account(&selected, Agent::Codex, Some(&workspace), "codex")
            .unwrap()
            .is_some(),
        "the legacy binding stays in place for the auth-mode path"
    );
}

#[test]
fn launch_selection_reuses_existing_synthesized_template_without_replacing_it() {
    let (mut config, _) = two_account_config();
    let template_id = "private@codex";
    let template = AgentConfiguration {
        agent: Agent::Codex,
        account: "private".to_owned(),
        model: Some("template-model".to_owned()),
        base_url: Some("https://template.example/v1".to_owned()),
        display_label: Some("Private template".to_owned()),
        invoked_via_wrapper: None,
    };
    config
        .agent_configurations
        .insert(template_id.to_owned(), template.clone());

    let selected = with_account_selection(&config, Agent::Codex, None, "codex", "private").unwrap();

    assert_eq!(selected.default_launch, Some(vec![template_id.to_owned()]));
    assert_eq!(selected.agent_configurations[template_id], template);
    let instance = jackin_config::resolve_launch(&selected, None, "codex", None, None)
        .unwrap()
        .pop()
        .expect("the preserved template must remain launchable");
    assert_eq!(instance.model.as_deref(), Some("template-model"));
    assert_eq!(
        instance.base_url.as_deref(),
        Some("https://template.example/v1")
    );
    assert_eq!(instance.label, "Private template");
}

#[test]
fn launch_selection_rejects_wrapper_template_before_any_selection_is_admitted() {
    let (mut config, _) = two_account_config();
    let template_id = "private@codex";
    let template = AgentConfiguration {
        agent: Agent::Codex,
        account: "private".to_owned(),
        model: Some("template-model".to_owned()),
        base_url: Some("https://template.example/v1".to_owned()),
        display_label: Some("Private template".to_owned()),
        invoked_via_wrapper: Some(jackin_config::WrapperSpec {
            identity: "codex-wrapper".to_owned(),
            args: vec!["--template".to_owned()],
        }),
    };
    config
        .agent_configurations
        .insert(template_id.to_owned(), template.clone());

    let error = with_account_selection(&config, Agent::Codex, None, "codex", "private")
        .expect_err("an unsupported wrapper must fail before launch selection succeeds");
    assert!(
        error
            .to_string()
            .contains("declares an unsupported shell wrapper"),
        "unexpected wrapper rejection: {error:#}"
    );
    assert!(
        config.default_launch.is_none(),
        "failed selection must not mutate the caller's config"
    );
    assert_eq!(
        config.agent_configurations[template_id], template,
        "failed selection must not replace the template or discard its settings"
    );
}

#[test]
fn launch_selection_with_admitting_defaults_narrows_to_selected_account() {
    let (mut config, workspace) = two_account_config();
    config.workspaces.get_mut("work").unwrap().default_launch =
        Some(vec!["codex-main".into(), "codex-alt".into()]);

    let selected =
        with_account_selection(&config, Agent::Codex, Some(&workspace), "codex", "shared").unwrap();
    // The pick is admitted, but the sibling account is not part of this
    // launch. The inherited workspace list stays intact for other callers;
    // this launch gets a role-local exact override.
    assert_eq!(
        selected.workspaces["work"].roles["codex"].default_launch,
        Some(vec!["codex-alt".to_owned()])
    );
    assert_eq!(
        selected.workspaces["work"].default_launch,
        Some(vec!["codex-main".to_owned(), "codex-alt".to_owned()])
    );
    let instances =
        jackin_config::resolve_launch(&selected, Some(&workspace), "codex", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "codex-alt");
    assert_eq!(instances[0].account_id, "shared");
}

#[test]
fn configuration_selection_narrows_to_exact_configuration() {
    let (mut config, workspace) = two_account_config();
    config.workspaces.get_mut("work").unwrap().default_launch =
        Some(vec!["codex-main".into(), "codex-alt".into()]);

    let selected = with_configuration_selection(
        &config,
        Agent::Codex,
        Some(&workspace),
        "codex",
        "codex-main",
    )
    .unwrap();
    assert_eq!(
        selected.workspaces["work"].roles["codex"].default_launch,
        Some(vec!["codex-main".to_owned()])
    );
    let instances =
        jackin_config::resolve_launch(&selected, Some(&workspace), "codex", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "codex-main");
    assert_eq!(instances[0].account_id, "private");
}

#[test]
fn launch_selection_outside_admitted_set_errors() {
    let (mut config, workspace) = two_account_config();
    config.workspaces.get_mut("work").unwrap().default_launch = Some(vec!["codex-main".into()]);

    // `shared` is registered, compatible, and allowlisted — but the
    // configured default admits only `private`.
    let error = with_account_selection(&config, Agent::Codex, Some(&workspace), "codex", "shared")
        .unwrap_err();
    assert!(
        error.to_string().contains("not admitted"),
        "a pick outside the admitted set must fail, never substitute; got {error:?}"
    );
}

#[test]
fn launch_selection_with_invalid_defaults_errors() {
    let (mut config, workspace) = two_account_config();
    config.workspaces.get_mut("work").unwrap().default_launch = Some(vec!["ghost".into()]);

    let error = with_account_selection(&config, Agent::Codex, Some(&workspace), "codex", "private")
        .unwrap_err();
    assert!(
        error.to_string().contains("unknown agent configuration"),
        "got {error:?}"
    );
}

#[test]
fn ad_hoc_launch_selection_without_defaults_sets_global_ephemeral_default() {
    let (config, _) = two_account_config();
    let selected = with_account_selection(&config, Agent::Codex, None, "codex", "shared").unwrap();
    assert_eq!(
        selected.default_launch,
        Some(vec!["shared@codex".to_owned()])
    );
    assert!(config.default_launch.is_none());
    let instances = jackin_config::resolve_launch(&selected, None, "codex", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].account_id, "shared");
}

#[test]
fn an_empty_model_override_is_a_validation_failure() {
    let mut options = opts();
    options.model = Some("   ".to_owned());
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::EmptyModel)
    );
}

#[test]
fn a_reserved_env_name_is_a_validation_failure() {
    let (reserved, _) = jackin_core::RESERVED_RUNTIME_ENV_VARS
        .first()
        .copied()
        .expect("the runtime reserves at least one env name");
    let mut options = opts();
    options.env.insert(reserved.to_owned(), "x".to_owned());
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::ReservedEnvName {
            name: reserved.to_owned()
        })
    );
}

#[test]
fn an_empty_env_name_is_a_validation_failure() {
    let mut options = opts();
    options.env.insert(String::new(), "x".to_owned());
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::EmptyEnvName)
    );
}

#[test]
fn a_duplicate_pre_approved_on_demand_binding_is_a_validation_failure() {
    let binding = ExecBinding {
        name: "OP_TOKEN".to_owned(),
        kind: ExecKind::Op,
        source: "op://vault/item/credential".to_owned(),
    };
    let mut options = opts();
    options.on_demand_bindings = vec![binding.clone(), binding];
    assert_eq!(
        options.validate_programmatic(&trusted_config(), &selector()),
        Err(LoadOptionsError::DuplicateOnDemandBinding {
            name: "OP_TOKEN".to_owned()
        })
    );
}
