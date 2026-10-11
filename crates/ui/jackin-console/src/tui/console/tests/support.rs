// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn fresh_state() -> ConsoleState {
    let cwd = std::env::temp_dir();
    let config = AppConfig::default();
    new_console_state(&config, &cwd).unwrap()
}

pub(super) fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent {
        code,
        modifiers: crossterm::event::KeyModifiers::NONE,
        kind: crossterm::event::KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }
}

pub(super) fn unresolved_workspace() -> ResolvedWorkspace {
    ResolvedWorkspace {
        name: String::new(),
        label: "scratch".to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: Vec::new(),
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    }
}

pub(super) fn run_prompt_for_unknown_role(
    on_failure: OnPromptFailure,
) -> (ConsoleState, PromptOutcome) {
    let cwd = std::env::temp_dir();
    let config = AppConfig::default();
    let mut state = new_console_state(&config, &cwd).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let workspace = unresolved_workspace();
    let input = LoadWorkspaceInput::CurrentDir;
    let outcome = prompt_agent_for_launch(
        &mut state,
        &selector,
        &workspace,
        input,
        on_failure,
        AgentPickerChoices::Failed(anyhow::anyhow!("unknown role")),
    );
    (state, outcome)
}

pub(super) fn unresolvable_mount_config() -> (tempfile::TempDir, AppConfig) {
    let temp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        RoleSource {
            git: "https://example.invalid/org/repo.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "ws".to_owned(),
        WorkspaceConfig {
            workdir: "/workspace/project".to_owned(),
            mounts: vec![MountConfig {
                src: temp.path().join("gone").display().to_string(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );
    (temp, config)
}
