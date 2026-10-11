// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn instance_purge_confirm_label_names_container_and_role_when_known() {
    assert_eq!(
        instance_purge_confirm_label("alpha-123", Some("the-architect")),
        "alpha-123 (the-architect)"
    );
    assert_eq!(instance_purge_confirm_label("alpha-123", None), "alpha-123");
}

#[test]
fn create_prelude_input_helpers_name_fields() {
    let dst = create_prelude_mount_destination_input_state("/workspace");
    let name = create_prelude_workspace_name_input_state("project");

    assert_eq!(dst.label, "Destination");
    assert_eq!(dst.value(), "/workspace");
    assert_eq!(name.label, "Name this workspace");
    assert_eq!(name.value(), "project");
}

#[test]
fn create_prelude_default_helpers_supply_visible_fallbacks() {
    assert_eq!(
        create_prelude_mount_destination_default(Some("/host/project")),
        "/host/project"
    );
    assert_eq!(create_prelude_mount_destination_default(None), "");
    assert_eq!(
        create_prelude_workspace_name_default(Some("/host/project")),
        "project"
    );
    assert_eq!(create_prelude_workspace_name_default(None), "");
}

#[test]
fn create_prelude_mount_dst_choice_uses_source() {
    let state = create_prelude_mount_dst_choice_state("/host/project");

    assert_eq!(state.src, "/host/project");
}

#[test]
fn instance_session_empty_message_reports_load_state() {
    assert_eq!(
        instance_sessions_empty_message(false),
        "No sessions recorded"
    );
    assert_eq!(
        instance_sessions_empty_message(true),
        "Sessions unavailable (manifest read error)"
    );
}

#[test]
fn workspace_instance_live_content_marks_active_focused_selected_and_shell_panes() {
    let content = workspace_instance_live_content(
        1,
        Some(22),
        vec![
            WorkspaceInstanceLiveTabFacts {
                label: "one".to_owned(),
                focused_pane: 11,
                panes: vec![WorkspaceInstanceLivePaneFacts {
                    session_id: 11,
                    label: "shell-pane".to_owned(),
                    account_id: None,
                    config_id: None,
                    state_label: "idle".to_owned(),
                }],
            },
            WorkspaceInstanceLiveTabFacts {
                label: "two".to_owned(),
                focused_pane: 21,
                panes: vec![
                    WorkspaceInstanceLivePaneFacts {
                        session_id: 21,
                        label: "claude-pane".to_owned(),
                        account_id: Some("acc-work".to_owned()),
                        config_id: Some("claude-work".to_owned()),
                        state_label: "running".to_owned(),
                    },
                    WorkspaceInstanceLivePaneFacts {
                        session_id: 22,
                        label: "codex-pane".to_owned(),
                        account_id: Some("acc-personal".to_owned()),
                        config_id: Some("codex-personal".to_owned()),
                        state_label: "paused".to_owned(),
                    },
                ],
            },
        ],
    );

    assert_eq!(
        content,
        WorkspaceInstancePaneContent::Live {
            tabs: vec![
                WorkspaceInstanceTab {
                    index: 0,
                    label: "one".to_owned(),
                    active: false,
                    panes: vec![WorkspaceInstanceTabPane {
                        label: "shell-pane".to_owned(),
                        account_id: None,
                        config_id: None,
                        state_label: "idle".to_owned(),
                        focused: true,
                        selected: false,
                    }],
                },
                WorkspaceInstanceTab {
                    index: 1,
                    label: "two".to_owned(),
                    active: true,
                    panes: vec![
                        WorkspaceInstanceTabPane {
                            label: "claude-pane".to_owned(),
                            account_id: Some("acc-work".to_owned()),
                            config_id: Some("claude-work".to_owned()),
                            state_label: "running".to_owned(),
                            focused: true,
                            selected: false,
                        },
                        WorkspaceInstanceTabPane {
                            label: "codex-pane".to_owned(),
                            account_id: Some("acc-personal".to_owned()),
                            config_id: Some("codex-personal".to_owned()),
                            state_label: "paused".to_owned(),
                            focused: false,
                            selected: true,
                        },
                    ],
                },
            ],
        }
    );
}

#[test]
fn workspace_instance_live_content_keeps_mixed_pane_identity_in_rendered_rows() {
    let content = workspace_instance_live_content(
        0,
        None,
        vec![WorkspaceInstanceLiveTabFacts {
            label: "mixed".to_owned(),
            focused_pane: 1,
            panes: vec![
                WorkspaceInstanceLivePaneFacts {
                    session_id: 1,
                    label: "worker".to_owned(),
                    account_id: Some("acc-work".to_owned()),
                    config_id: Some("claude-work".to_owned()),
                    state_label: "running".to_owned(),
                },
                WorkspaceInstanceLivePaneFacts {
                    session_id: 2,
                    label: "worker".to_owned(),
                    account_id: Some("acc-personal".to_owned()),
                    config_id: Some("claude-personal".to_owned()),
                    state_label: "idle".to_owned(),
                },
            ],
        }],
    );

    let WorkspaceInstancePaneContent::Live { tabs } = &content else {
        panic!("expected live pane content");
    };
    assert_eq!(tabs[0].panes[0].label, tabs[0].panes[1].label);
    assert_eq!(tabs[0].panes[0].account_id.as_deref(), Some("acc-work"));
    assert_eq!(tabs[0].panes[1].account_id.as_deref(), Some("acc-personal"));
    assert_eq!(tabs[0].panes[0].config_id.as_deref(), Some("claude-work"));
    assert_eq!(
        tabs[0].panes[1].config_id.as_deref(),
        Some("claude-personal")
    );

    let rendered = live_instance_lines(tabs)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("(acc-work · claude-work)"), "{rendered}");
    assert!(
        rendered.contains("(acc-personal · claude-personal)"),
        "{rendered}"
    );
}

#[test]
fn workspace_instance_session_content_routes_rows_and_empty_states() {
    assert_eq!(
        workspace_instance_session_content(false, Vec::new()),
        WorkspaceInstancePaneContent::Empty {
            message: "No sessions recorded".to_owned(),
        }
    );
    assert_eq!(
        workspace_instance_session_content(true, Vec::new()),
        WorkspaceInstancePaneContent::Empty {
            message: "Sessions unavailable (manifest read error)".to_owned(),
        }
    );
    assert_eq!(
        workspace_instance_session_content(
            false,
            vec![WorkspaceInstanceSessionRow {
                name: "tmux-a".to_owned(),
                agent_runtime: "claude".to_owned(),
                account_id: Some("acc-work".to_owned()),
                config_id: Some("claude-work".to_owned()),
            }],
        ),
        WorkspaceInstancePaneContent::Sessions {
            rows: vec![WorkspaceInstanceSessionRow {
                name: "tmux-a".to_owned(),
                agent_runtime: "claude".to_owned(),
                account_id: Some("acc-work".to_owned()),
                config_id: Some("claude-work".to_owned()),
            }],
        }
    );
}

#[test]
fn workspace_instance_pane_wraps_content_and_focus() {
    let pane = workspace_instance_pane(
        "abc123".to_owned(),
        true,
        WorkspaceInstancePaneContent::Empty {
            message: "No sessions recorded".to_owned(),
        },
    );

    assert_eq!(pane.instance_id, "abc123");
    assert!(pane.focused);
}

#[test]
fn workspace_list_display_helpers_own_visible_defaults() {
    let current = current_directory_display_row(Disclosure::for_instances(true, true), true, false);
    assert_eq!(current.label, "Current directory");
    assert_eq!(current.disclosure, Disclosure::Expanded);
    assert!(current.selected);

    let new_workspace = new_workspace_display_row(false, true);
    assert_eq!(new_workspace.label, new_workspace_list_label());
    assert!(new_workspace.hovered);
    assert_eq!(new_workspace.disclosure, Disclosure::None);
    assert_eq!(
        workspace_instance_list_label("abc123", "chainargos/agent-smith", InstanceStatus::Running),
        "abc123  chainargos/agent-smith"
    );
    assert_eq!(
        workspace_instance_list_label("abc123", "role", InstanceStatus::Crashed),
        "abc123  role  [crashed]"
    );
    let instance = workspace_instance_display_row(
        "abc123",
        "chainargos/agent-smith",
        InstanceStatus::Running,
        true,
        true,
    );
    assert_eq!(instance.label, "abc123  chainargos/agent-smith");
    assert_eq!(instance.tone, WorkspaceListRowTone::Instance);
    assert!(instance.selected);
    assert!(instance.hovered);
    assert_eq!(instance.disclosure, Disclosure::None);
    assert_eq!(workspace_instance_pane_identity_label(None, None), "shell");
    assert_eq!(
        workspace_instance_pane_identity_label(Some("acc-work"), Some("claude-work")),
        "acc-work · claude-work"
    );
    assert_eq!(current_directory_workspace_title(), "Current directory");
    assert_eq!(picker_sidebar_title("alpha"), " alpha ");
    assert_eq!(
        role_global_mounts_title("agent-smith"),
        " Role global mounts · agent-smith "
    );
    assert_eq!(global_mounts_title(), " Global mounts ");
}

#[test]
fn workspace_list_display_row_for_row_routes_all_row_kinds() {
    assert_eq!(
        workspace_list_display_row_for_row(
            WorkspaceListDisplayRowFacts {
                row: ManagerListRow::CurrentDirectory,
                selected: true,
                hovered: false,
                current_dir_expanded: true,
                current_dir_has_instances: true,
            },
            |_| None,
            |_| None,
            |_, _| None,
        ),
        Some(current_directory_display_row(
            Disclosure::for_instances(true, true),
            true,
            false,
        ))
    );
    assert_eq!(
        workspace_list_display_row_for_row(
            WorkspaceListDisplayRowFacts {
                row: ManagerListRow::SavedWorkspace(2),
                selected: false,
                hovered: true,
                current_dir_expanded: false,
                current_dir_has_instances: false,
            },
            |_| None,
            |idx| (idx == 2).then(|| ("ws".to_owned(), true, false)),
            |_, _| None,
        ),
        Some(WorkspaceListDisplayRow {
            label: "ws".to_owned(),
            tone: WorkspaceListRowTone::Workspace,
            disclosure: Disclosure::None,
            selected: false,
            hovered: true,
        })
    );
    assert_eq!(
        workspace_list_display_row_for_row(
            WorkspaceListDisplayRowFacts {
                row: ManagerListRow::CurrentDirectoryInstance(3),
                selected: true,
                hovered: true,
                current_dir_expanded: false,
                current_dir_has_instances: false,
            },
            |idx| (idx == 3).then(|| instance_row_label("i-cwd", "role")),
            |_| None,
            |_, _| None,
        ),
        Some(workspace_instance_display_row(
            "i-cwd",
            "role",
            InstanceStatus::Running,
            true,
            true
        ))
    );
    assert_eq!(
        workspace_list_display_row_for_row(
            display_row_facts(ManagerListRow::WorkspaceInstance(1, 4)),
            |_| None,
            |_| None,
            |ws, inst| (ws == 1 && inst == 4).then(|| instance_row_label("i-ws", "smith")),
        ),
        Some(workspace_instance_display_row(
            "i-ws",
            "smith",
            InstanceStatus::Running,
            false,
            false
        ))
    );
    assert_eq!(
        workspace_list_display_row_for_row(
            display_row_facts(ManagerListRow::NewWorkspace),
            |_| None,
            |_| None,
            |_, _| None,
        ),
        Some(new_workspace_display_row(false, false))
    );
}
