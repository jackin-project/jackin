// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn config_with_workspace_default(agent: Option<Agent>) -> AppConfig {
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        jackin_config::RoleSource {
            git: "https://example.invalid/agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "jackin".to_owned(),
        jackin_config::WorkspaceConfig {
            workdir: "/workspace".to_owned(),
            default_role: Some("agent-smith".to_owned()),
            default_agent: agent,
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    config
}

fn prewarm_args(flags: PrewarmFlags) -> PrewarmArgs {
    PrewarmArgs {
        agents: Vec::new(),
        flags,
        role: None,
        workspace: None,
        role_git: None,
        role_branch: None,
    }
}

#[test]
fn image_workspace_default_agent_narrows_binary_prewarm() {
    let config = config_with_workspace_default(Some(Agent::Codex));
    let args = PrewarmArgs {
        flags: PrewarmFlags {
            image: true,
            ..PrewarmFlags::default()
        },
        workspace: Some("jackin".to_owned()),
        ..prewarm_args(PrewarmFlags::default())
    };
    let target = PrewarmImageTarget::resolve(&args, &config)
        .unwrap()
        .pop()
        .unwrap();

    assert_eq!(target.agents, vec![Agent::Codex]);
    assert_eq!(binary_prewarm_agents(&args, &[target]), vec![Agent::Codex]);
}

#[test]
fn image_role_without_agent_keeps_all_binary_prewarm() {
    let config = config_with_workspace_default(Some(Agent::Codex));
    let args = PrewarmArgs {
        flags: PrewarmFlags {
            image: true,
            ..PrewarmFlags::default()
        },
        role: Some("agent-smith".to_owned()),
        ..prewarm_args(PrewarmFlags::default())
    };
    let target = PrewarmImageTarget::resolve(&args, &config)
        .unwrap()
        .pop()
        .unwrap();

    assert!(target.agents.is_empty());
    assert_eq!(binary_prewarm_agents(&args, &[target]), Agent::ALL.to_vec());
}

#[test]
fn image_all_workspaces_unions_default_agents_for_binary_prewarm() {
    let mut config = config_with_workspace_default(Some(Agent::Codex));
    config.roles.insert(
        "the-architect".to_owned(),
        jackin_config::RoleSource {
            git: "https://example.invalid/the-architect.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "docs".to_owned(),
        jackin_config::WorkspaceConfig {
            workdir: "/docs".to_owned(),
            default_role: Some("the-architect".to_owned()),
            default_agent: Some(Agent::Claude),
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    let args = prewarm_args(PrewarmFlags {
        image: true,
        all_workspaces: true,
        ..PrewarmFlags::default()
    });
    let targets = PrewarmImageTarget::resolve(&args, &config).unwrap();

    assert_eq!(targets.len(), 2);
    assert_eq!(
        binary_prewarm_agents(&args, &targets),
        vec![Agent::Claude, Agent::Codex]
    );
}

#[test]
fn image_all_roles_expands_configured_roles_without_agent_narrowing() {
    let mut config = config_with_workspace_default(Some(Agent::Codex));
    config.roles.insert(
        "the-architect".to_owned(),
        jackin_config::RoleSource {
            git: "https://example.invalid/the-architect.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    let args = prewarm_args(PrewarmFlags {
        image: true,
        all_roles: true,
        ..PrewarmFlags::default()
    });

    let targets = PrewarmImageTarget::resolve(&args, &config).unwrap();

    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].selector.key(), "agent-smith");
    assert_eq!(targets[1].selector.key(), "the-architect");
    assert!(targets.iter().all(|target| target.agents.is_empty()));
    assert_eq!(binary_prewarm_agents(&args, &targets), Agent::ALL.to_vec());
}

#[test]
fn image_all_roles_respects_explicit_agent_filter() {
    let config = config_with_workspace_default(Some(Agent::Codex));
    let args = PrewarmArgs {
        agents: vec![Agent::Kimi],
        flags: PrewarmFlags {
            image: true,
            all_roles: true,
            ..PrewarmFlags::default()
        },
        ..prewarm_args(PrewarmFlags::default())
    };

    let targets = PrewarmImageTarget::resolve(&args, &config).unwrap();

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].agents, vec![Agent::Kimi]);
    assert_eq!(binary_prewarm_agents(&args, &targets), vec![Agent::Kimi]);
}

#[test]
fn roles_prewarm_does_not_require_image_targets() {
    let config = config_with_workspace_default(Some(Agent::Codex));
    let args = prewarm_args(PrewarmFlags {
        roles: true,
        ..PrewarmFlags::default()
    });

    assert!(PrewarmImageTarget::resolve(&args, &config).is_err());
    assert_eq!(binary_prewarm_agents(&args, &[]), Agent::ALL.to_vec());
    assert!(!should_prewarm_sidecar_image(&args));
}

#[test]
fn image_prewarm_also_prewarms_sidecar_image() {
    let args = PrewarmArgs {
        flags: PrewarmFlags {
            image: true,
            ..PrewarmFlags::default()
        },
        role: Some("agent-smith".to_owned()),
        ..prewarm_args(PrewarmFlags::default())
    };

    assert!(should_prewarm_sidecar_image(&args));
}

#[test]
fn sidecar_prewarm_can_run_without_image_targets() {
    let args = prewarm_args(PrewarmFlags {
        sidecar: true,
        ..PrewarmFlags::default()
    });

    assert!(should_prewarm_sidecar_image(&args));
}

#[test]
fn sidecar_container_prewarm_uses_container_path_image_lookup() {
    let args = prewarm_args(PrewarmFlags {
        sidecar_container: true,
        ..PrewarmFlags::default()
    });

    assert!(!should_prewarm_sidecar_image(&args));
    assert!(should_prewarm_sidecar_container(&args));
    assert!(!should_keep_sidecar_container(&args));
}

#[test]
fn daemon_prewarm_keeps_sidecar_without_duplicate_image_lookup() {
    let args = prewarm_args(PrewarmFlags {
        daemon: true,
        ..PrewarmFlags::default()
    });

    assert!(!should_prewarm_sidecar_image(&args));
    assert!(should_prewarm_sidecar_container(&args));
    assert!(should_keep_sidecar_container(&args));
}

#[test]
fn daemon_prewarm_records_plan_and_skipped_work() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "prewarm").unwrap();
    let _guard = run.activate();

    emit_daemon_prewarm_plan();

    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"launch_plan\""),
        "{diagnostics}"
    );
    assert!(diagnostics.contains("PrewarmOnly"), "{diagnostics}");
    assert!(
        diagnostics.contains("daemon_prewarm:kept_sidecar"),
        "{diagnostics}"
    );
    assert!(
        diagnostics.contains("standalone_sidecar_image_prewarm"),
        "{diagnostics}"
    );
}

#[test]
fn roles_prewarm_can_target_one_role_without_image() {
    let config = config_with_workspace_default(Some(Agent::Codex));
    let args = PrewarmArgs {
        flags: PrewarmFlags {
            roles: true,
            ..PrewarmFlags::default()
        },
        role: Some("agent-smith".to_owned()),
        ..prewarm_args(PrewarmFlags::default())
    };
    let targets = PrewarmRoleTarget::resolve(&args, &config).unwrap();

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].selector.key(), "agent-smith");
    assert_eq!(
        targets[0].role_git,
        "https://example.invalid/agent-smith.git"
    );
}

#[test]
fn roles_prewarm_can_target_workspace_default_role_without_image() {
    let config = config_with_workspace_default(Some(Agent::Codex));
    let args = PrewarmArgs {
        flags: PrewarmFlags {
            roles: true,
            ..PrewarmFlags::default()
        },
        workspace: Some("jackin".to_owned()),
        ..prewarm_args(PrewarmFlags::default())
    };
    let targets = PrewarmRoleTarget::resolve(&args, &config).unwrap();

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].selector.key(), "agent-smith");
    assert_eq!(
        targets[0].role_git,
        "https://example.invalid/agent-smith.git"
    );
}

#[test]
fn roles_prewarm_all_workspaces_deduplicates_default_roles_without_image() {
    let mut config = config_with_workspace_default(Some(Agent::Codex));
    config.workspaces.insert(
        "docs".to_owned(),
        jackin_config::WorkspaceConfig {
            workdir: "/docs".to_owned(),
            default_role: Some("agent-smith".to_owned()),
            default_agent: Some(Agent::Claude),
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    let args = prewarm_args(PrewarmFlags {
        roles: true,
        all_workspaces: true,
        ..PrewarmFlags::default()
    });
    let targets = PrewarmRoleTarget::resolve(&args, &config).unwrap();

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].selector.key(), "agent-smith");
}

#[test]
fn roles_prewarm_can_use_role_git_override_without_image() {
    let config = AppConfig::default();
    let args = PrewarmArgs {
        flags: PrewarmFlags {
            roles: true,
            ..PrewarmFlags::default()
        },
        role: Some("agent-smith".to_owned()),
        role_git: Some("https://example.invalid/custom.git".to_owned()),
        ..prewarm_args(PrewarmFlags::default())
    };
    let targets = PrewarmRoleTarget::resolve(&args, &config).unwrap();

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].role_git, "https://example.invalid/custom.git");
}
