//! Pure data types for the Auth panel.
//!
//! Provides [`ProvenanceTag`], [`CredentialBadge`], [`badge_for`], and
//! [`classify_env_value`]. These are consumed by the flat-row renderer in
//! `src/console/manager/render/editor.rs`.

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

/// Compute the credential badge for a given (workspace, role, agent, mode).
///
/// Walks the 4 env layers in launch-time precedence order (workspace × role
/// → workspace → role → global) and returns the badge for the first hit.
/// `OpRef` entries report `Resolves` optimistically — the picker validated
/// them at commit time, and verifying again here would require running
/// `op read`, which the panel deliberately avoids.
pub fn badge_for(
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

pub const fn classify_env_value(value: &crate::operator_env::EnvValue) -> CredentialBadge {
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
