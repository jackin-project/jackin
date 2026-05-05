//! Pure data types and state computation for the Auth panel.
//!
//! `AuthPanelState::compute_for` walks an [`AppConfig`] to materialize
//! one row per (scope, agent) and (role, agent) pair. Each row carries:
//!
//! - the effective [`AuthForwardMode`] after the 3-layer resolver runs,
//! - a [`ProvenanceTag`] saying *which* layer supplied that mode, and
//! - a [`CredentialBadge`] saying whether the mode's required env var
//!   resolves to a non-empty value in the merged 4-layer operator env.
//!
//! Rendering lives in `render.rs` (Task 16). This module deliberately
//! avoids any ratatui dependency so the per-row computation is unit-
//! testable in isolation.

use crate::agent::Agent;
use crate::config::{AppConfig, AuthForwardMode};

/// Which layer of the 3-layer resolver supplied this row's mode value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceTag {
    /// Mode came from `[<agent>]` (global default).
    Global,
    /// Mode came from `[workspaces.<ws>.<agent>]`.
    Workspace,
    /// Mode came from `[workspaces.<ws>.roles.<role>.<agent>]`.
    MostSpecific,
    /// No layer at this level; inherited from a broader layer above.
    /// Used for the `workspace_rows` section's display when only the
    /// global default applies.
    Inherited,
}

/// Status badge displayed on a row showing whether the credential resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialBadge {
    /// Required env var resolves to non-empty in the merged operator env.
    Resolves,
    /// Required env var is unset or empty.
    Unset,
    /// Mode does not require a credential (Sync, Ignore).
    NotApplicable,
}

/// A single row in the Auth panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRow {
    pub role: String,
    pub agent: Agent,
    pub mode: AuthForwardMode,
    pub provenance: ProvenanceTag,
    pub credential: CredentialBadge,
}

/// State for the Auth panel within a workspace context.
#[derive(Debug, Default, Clone)]
pub struct AuthPanelState {
    pub workspace: String,
    /// One row per agent (Claude, Codex, ...) showing the global default.
    pub global_rows: Vec<AuthRow>,
    /// One row per agent showing the effective workspace-level mode.
    pub workspace_rows: Vec<AuthRow>,
    /// One row per (role, agent) showing the most-specific mode.
    pub role_agent_rows: Vec<AuthRow>,
}

impl AuthPanelState {
    /// Compute the panel state for `workspace` from `cfg`.
    ///
    /// For each agent (Claude, Codex), produces:
    ///   - 1 global row (`role: ""`, provenance: `Global`)
    ///   - 1 workspace row (`role: ""`, provenance: `Workspace` if set,
    ///     `Inherited` otherwise)
    ///   - 1 role × agent row per role in the workspace's `allowed_roles`
    ///
    /// The credential badge is computed against the 4-layer operator env
    /// using the same precedence the launch path applies (workspace ×
    /// role > workspace > role > global). `Resolves` is reported
    /// optimistically for `OpRef` entries — the picker validated them at
    /// commit time, and re-running `op read` here would block the render
    /// loop. `Plain` resolves only when the persisted string is non-empty.
    pub fn compute_for(cfg: &AppConfig, workspace: &str) -> Self {
        let mut state = Self {
            workspace: workspace.to_string(),
            ..Self::default()
        };

        for agent in [Agent::Claude, Agent::Codex] {
            // Global row.
            let global_mode = match agent {
                Agent::Claude => cfg.claude.as_ref().map(|c| c.auth_forward),
                Agent::Codex => cfg.codex.as_ref().map(|c| c.auth_forward),
            }
            .unwrap_or_default();
            state.global_rows.push(AuthRow {
                role: String::new(),
                agent,
                mode: global_mode,
                provenance: ProvenanceTag::Global,
                credential: badge_for(cfg, workspace, "", agent, global_mode),
            });

            // Workspace row — explicit when set, Inherited from global otherwise.
            let ws_explicit = cfg.workspaces.get(workspace).and_then(|ws| match agent {
                Agent::Claude => ws.claude.as_ref().map(|c| c.auth_forward),
                Agent::Codex => ws.codex.as_ref().map(|c| c.auth_forward),
            });
            let (ws_mode, ws_prov) = ws_explicit
                .map_or((global_mode, ProvenanceTag::Inherited), |m| {
                    (m, ProvenanceTag::Workspace)
                });
            state.workspace_rows.push(AuthRow {
                role: String::new(),
                agent,
                mode: ws_mode,
                provenance: ws_prov,
                credential: badge_for(cfg, workspace, "", agent, ws_mode),
            });

            // Role × agent rows — one per allowed role.
            if let Some(ws) = cfg.workspaces.get(workspace) {
                for role in &ws.allowed_roles {
                    let mode = crate::config::resolve_mode(cfg, agent, workspace, role);
                    let role_explicit = ws.roles.get(role).and_then(|ro| match agent {
                        Agent::Claude => ro.claude.as_ref().map(|c| c.auth_forward),
                        Agent::Codex => ro.codex.as_ref().map(|c| c.auth_forward),
                    });
                    let provenance = if role_explicit.is_some() {
                        ProvenanceTag::MostSpecific
                    } else if ws_explicit.is_some() {
                        ProvenanceTag::Workspace
                    } else {
                        ProvenanceTag::Global
                    };
                    state.role_agent_rows.push(AuthRow {
                        role: role.clone(),
                        agent,
                        mode,
                        provenance,
                        credential: badge_for(cfg, workspace, role, agent, mode),
                    });
                }
            }
        }

        state
    }
}

/// Compute the credential badge for a given (workspace, role, agent, mode).
///
/// Walks the 4 env layers in launch-time precedence order (workspace × role
/// → workspace → role → global) and returns the badge for the first hit.
/// `OpRef` entries report `Resolves` optimistically — the picker validated
/// them at commit time, and verifying again here would require running
/// `op read`, which the panel deliberately avoids.
fn badge_for(
    cfg: &AppConfig,
    workspace: &str,
    role: &str,
    agent: Agent,
    mode: AuthForwardMode,
) -> CredentialBadge {
    let Some(env_var) = agent.required_env_var(mode) else {
        return CredentialBadge::NotApplicable;
    };

    if let Some(value) = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.roles.get(role))
        .and_then(|ro| ro.env.get(env_var))
    {
        return classify_env_value(value);
    }
    if let Some(value) = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.env.get(env_var))
    {
        return classify_env_value(value);
    }
    if let Some(value) = cfg.roles.get(role).and_then(|r| r.env.get(env_var)) {
        return classify_env_value(value);
    }
    if let Some(value) = cfg.env.get(env_var) {
        return classify_env_value(value);
    }
    CredentialBadge::Unset
}

const fn classify_env_value(value: &crate::operator_env::EnvValue) -> CredentialBadge {
    use crate::operator_env::EnvValue;
    match value {
        // OpRef: assume the picker validated at commit time. The panel
        // does not run `op read` (would block render).
        EnvValue::OpRef(_) => CredentialBadge::Resolves,
        // Plain: resolves iff non-empty. The launch-time validator catches
        // `$VAR`/`${VAR}` expansion that ends up empty; for panel display
        // any non-empty persisted string is treated as `Resolves`.
        EnvValue::Plain(s) if !s.is_empty() => CredentialBadge::Resolves,
        EnvValue::Plain(_) => CredentialBadge::Unset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentAuthConfig;
    use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};

    fn build_cfg() -> AppConfig {
        let mut cfg = AppConfig {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::Sync,
            }),
            ..AppConfig::default()
        };
        let ws = WorkspaceConfig {
            workdir: "/tmp/proj".to_string(),
            allowed_roles: vec!["smith".to_string()],
            ..Default::default()
        };
        cfg.workspaces.insert("proj".into(), ws);
        cfg
    }

    #[test]
    fn provenance_global_when_only_global_set() {
        let cfg = build_cfg();
        let state = AuthPanelState::compute_for(&cfg, "proj");
        let row = state
            .role_agent_rows
            .iter()
            .find(|r| r.role == "smith" && r.agent == Agent::Claude)
            .expect("smith × Claude row");
        assert_eq!(row.provenance, ProvenanceTag::Global);
        assert_eq!(row.mode, AuthForwardMode::Sync);
    }

    #[test]
    fn provenance_workspace_when_workspace_overrides_global() {
        let mut cfg = build_cfg();
        let ws = cfg.workspaces.get_mut("proj").unwrap();
        ws.claude = Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
        });
        let state = AuthPanelState::compute_for(&cfg, "proj");
        let row = state
            .role_agent_rows
            .iter()
            .find(|r| r.role == "smith" && r.agent == Agent::Claude)
            .unwrap();
        assert_eq!(row.provenance, ProvenanceTag::Workspace);
        assert_eq!(row.mode, AuthForwardMode::ApiKey);
    }

    #[test]
    fn provenance_most_specific_when_role_override_present() {
        let mut cfg = build_cfg();
        let ws = cfg.workspaces.get_mut("proj").unwrap();
        ws.roles.insert(
            "smith".into(),
            WorkspaceRoleOverride {
                claude: Some(AgentAuthConfig {
                    auth_forward: AuthForwardMode::OAuthToken,
                }),
                ..Default::default()
            },
        );
        let state = AuthPanelState::compute_for(&cfg, "proj");
        let row = state
            .role_agent_rows
            .iter()
            .find(|r| r.role == "smith" && r.agent == Agent::Claude)
            .unwrap();
        assert_eq!(row.provenance, ProvenanceTag::MostSpecific);
        assert_eq!(row.mode, AuthForwardMode::OAuthToken);
    }

    #[test]
    fn credential_badge_not_applicable_for_sync() {
        let cfg = build_cfg();
        let state = AuthPanelState::compute_for(&cfg, "proj");
        let row = state
            .role_agent_rows
            .iter()
            .find(|r| r.role == "smith" && r.agent == Agent::Claude)
            .unwrap();
        assert_eq!(row.credential, CredentialBadge::NotApplicable);
    }

    #[test]
    fn credential_badge_unset_when_required_var_empty() {
        let mut cfg = build_cfg();
        let ws = cfg.workspaces.get_mut("proj").unwrap();
        ws.claude = Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
        });
        // ANTHROPIC_API_KEY is not set anywhere in cfg.
        let state = AuthPanelState::compute_for(&cfg, "proj");
        let row = state
            .role_agent_rows
            .iter()
            .find(|r| r.role == "smith" && r.agent == Agent::Claude)
            .unwrap();
        assert_eq!(row.credential, CredentialBadge::Unset);
    }

    #[test]
    fn credential_badge_resolves_when_env_var_present() {
        use crate::operator_env::EnvValue;
        let mut cfg = build_cfg();
        let ws = cfg.workspaces.get_mut("proj").unwrap();
        ws.claude = Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
        });
        ws.env.insert(
            "ANTHROPIC_API_KEY".into(),
            EnvValue::Plain("sk-ant-test".into()),
        );
        let state = AuthPanelState::compute_for(&cfg, "proj");
        let row = state
            .role_agent_rows
            .iter()
            .find(|r| r.role == "smith" && r.agent == Agent::Claude)
            .unwrap();
        assert_eq!(row.credential, CredentialBadge::Resolves);
    }

    #[test]
    fn global_rows_contain_one_per_agent() {
        let cfg = build_cfg();
        let state = AuthPanelState::compute_for(&cfg, "proj");
        assert_eq!(state.global_rows.len(), 2);
        assert!(state.global_rows.iter().any(|r| r.agent == Agent::Claude));
        assert!(state.global_rows.iter().any(|r| r.agent == Agent::Codex));
    }

    #[test]
    fn role_agent_rows_contain_one_per_role_per_agent() {
        let cfg = build_cfg();
        let state = AuthPanelState::compute_for(&cfg, "proj");
        assert_eq!(state.role_agent_rows.len(), 2); // 1 role × 2 agents
    }
}
