//! Tests for `editor` eligible agents for override rendering.
//! Roles already carrying an override are NOT filtered — the
//! picker can add more keys to an existing override.
use super::eligible_agents_for_override;
use crate::config::{AppConfig, RoleSource};
use crate::console::tui::state::{EditorState, EditorTab, FieldFocus};
use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};

fn config_with_agents(names: &[&str]) -> AppConfig {
    let mut config = AppConfig::default();
    for name in names {
        config.roles.insert((*name).into(), RoleSource::default());
    }
    config
}

fn ws_with_overrides(allowed: &[&str], override_agents: &[&str]) -> WorkspaceConfig {
    let mut roles = std::collections::BTreeMap::new();
    for a in override_agents {
        let mut env = std::collections::BTreeMap::new();
        env.insert("LOG_LEVEL".into(), "debug".into());
        roles.insert(
            (*a).into(),
            WorkspaceRoleOverride {
                env,
                claude: None,
                codex: None,
                amp: None,
                kimi: None,
                opencode: None,
                github: None,
            },
        );
    }
    WorkspaceConfig {
        allowed_roles: allowed.iter().map(|s| (*s).into()).collect(),
        roles,
        ..WorkspaceConfig::default()
    }
}

fn editor_for(ws: WorkspaceConfig) -> EditorState<'static> {
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);
    editor
}

#[test]
fn eligible_agents_returns_allowed_when_list_non_empty() {
    // Non-empty `allowed_roles` is taken at face value — the
    // result matches the workspace's allowed list verbatim.
    let cfg = config_with_agents(&["agent-smith", "agent-brown", "agent-architect"]);
    let editor = editor_for(ws_with_overrides(&["agent-smith"], &[]));
    let eligible = eligible_agents_for_override(&editor, &cfg);
    assert_eq!(eligible, vec!["agent-smith".to_string()]);
}

#[test]
fn eligible_agents_returns_all_registered_when_allowed_empty() {
    // Empty `allowed_roles` is the "all roles allowed" shorthand —
    // every globally-registered role is eligible.
    let cfg = config_with_agents(&["agent-smith", "agent-brown"]);
    let editor = editor_for(ws_with_overrides(&[], &[]));
    let mut eligible = eligible_agents_for_override(&editor, &cfg);
    eligible.sort();
    assert_eq!(
        eligible,
        vec!["agent-brown".to_string(), "agent-smith".to_string()]
    );
}

#[test]
fn eligible_agents_does_not_filter_by_existing_overrides() {
    // Operators may want to add additional keys to an role that
    // already carries some — the picker must therefore include
    // every allowed role regardless of whether `pending.roles`
    // already lists them.
    let cfg = config_with_agents(&["agent-smith", "agent-brown"]);
    let editor = editor_for(ws_with_overrides(
        &["agent-smith", "agent-brown"],
        &["agent-smith"],
    ));
    let mut eligible = eligible_agents_for_override(&editor, &cfg);
    eligible.sort();
    assert_eq!(
        eligible,
        vec!["agent-brown".to_string(), "agent-smith".to_string()],
        "agent-smith already has overrides but must still appear so the operator can add another key to it"
    );
}

#[test]
fn eligible_agents_returns_empty_when_no_allowed_and_no_registered() {
    // Empty `allowed_roles` shorthand AND no registered roles:
    // the picker would be empty, so the caller is expected to
    // short-circuit and not open the modal.
    let cfg = config_with_agents(&[]);
    let editor = editor_for(ws_with_overrides(&[], &[]));
    let eligible = eligible_agents_for_override(&editor, &cfg);
    assert!(eligible.is_empty());
}
