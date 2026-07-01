//! Tests for `view`.
use super::*;
use crate::tui::screens::workspaces::model::ManagerListRow;
use jackin_core::instance::InstanceStatus;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

fn instance_row_label(instance_id: &str, role_key: &str) -> InstanceRowLabel {
    InstanceRowLabel {
        instance_id: instance_id.to_owned(),
        role_key: role_key.to_owned(),
        status: InstanceStatus::Running,
    }
}

fn display_row_facts(row: ManagerListRow) -> WorkspaceListDisplayRowFacts {
    WorkspaceListDisplayRowFacts {
        row,
        selected: false,
        hovered: false,
        current_dir_expanded: false,
        current_dir_has_instances: false,
    }
}

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
                    agent: None,
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
                        agent: Some("claude".to_owned()),
                        state_label: "running".to_owned(),
                    },
                    WorkspaceInstanceLivePaneFacts {
                        session_id: 22,
                        label: "codex-pane".to_owned(),
                        agent: Some("codex".to_owned()),
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
                        agent_label: "shell".to_owned(),
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
                            agent_label: "claude".to_owned(),
                            state_label: "running".to_owned(),
                            focused: true,
                            selected: false,
                        },
                        WorkspaceInstanceTabPane {
                            label: "codex-pane".to_owned(),
                            agent_label: "codex".to_owned(),
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
            }],
        ),
        WorkspaceInstancePaneContent::Sessions {
            rows: vec![WorkspaceInstanceSessionRow {
                name: "tmux-a".to_owned(),
                agent_runtime: "claude".to_owned(),
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
    assert_eq!(workspace_instance_pane_agent_label(None), "shell");
    assert_eq!(
        workspace_instance_pane_agent_label(Some("claude")),
        "claude"
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

#[test]
fn workspace_list_display_rows_assembles_visual_rows() {
    let visual_rows = vec![
        Some(ManagerListRow::CurrentDirectory),
        None,
        Some(ManagerListRow::SavedWorkspace(1)),
        Some(ManagerListRow::WorkspaceInstance(1, 0)),
    ];

    let rows = workspace_list_display_rows(
        WorkspaceListDisplayRowsFacts {
            visual_rows: &visual_rows,
            visual_selected: 2,
            hovered_row: Some(ManagerListRow::WorkspaceInstance(1, 0)),
            current_dir_expanded: true,
            current_dir_has_instances: true,
        },
        |_| None,
        |idx| (idx == 1).then(|| ("ws-one".to_owned(), false, true)),
        |ws_idx, inst_idx| {
            (ws_idx == 1 && inst_idx == 0).then(|| instance_row_label("abc123", "role"))
        },
    );

    assert_eq!(
        rows[0],
        Some(current_directory_display_row(
            Disclosure::for_instances(true, true),
            false,
            false,
        ))
    );
    assert_eq!(rows[1], None);
    assert_eq!(
        rows[2],
        Some(WorkspaceListDisplayRow {
            label: "ws-one".to_owned(),
            tone: WorkspaceListRowTone::Workspace,
            disclosure: Disclosure::Collapsed,
            selected: true,
            hovered: false,
        })
    );
    assert_eq!(
        rows[3],
        Some(workspace_instance_display_row(
            "abc123",
            "role",
            InstanceStatus::Running,
            false,
            true
        ))
    );
}

#[test]
fn workspace_preview_pane_plan_routes_all_row_kinds() {
    assert_eq!(
        workspace_preview_pane_plan(ManagerListRow::CurrentDirectory),
        WorkspacePreviewPanePlan::CurrentDirectory
    );
    assert_eq!(
        workspace_preview_pane_plan(ManagerListRow::NewWorkspace),
        WorkspacePreviewPanePlan::NewWorkspace
    );
    assert_eq!(
        workspace_preview_pane_plan(ManagerListRow::SavedWorkspace(2)),
        WorkspacePreviewPanePlan::SavedWorkspace(2)
    );
    assert_eq!(
        workspace_preview_pane_plan(ManagerListRow::CurrentDirectoryInstance(3)),
        WorkspacePreviewPanePlan::Instance {
            workspace_idx: None,
            instance_idx: 3,
        }
    );
    assert_eq!(
        workspace_preview_pane_plan(ManagerListRow::WorkspaceInstance(4, 5)),
        WorkspacePreviewPanePlan::Instance {
            workspace_idx: Some(4),
            instance_idx: 5,
        }
    );
}

#[test]
fn workspace_sidebar_plan_routes_picker_precedence() {
    assert_eq!(
        workspace_sidebar_plan(WorkspaceSidebarFacts {
            inline_provider_picker_open: true,
            launch_provider_picker_open: true,
            inline_new_session_picker_open: true,
            inline_agent_picker_open: true,
            inline_role_picker_open: true,
        }),
        WorkspaceSidebarPlan::InlineProviderPicker
    );
    assert_eq!(
        workspace_sidebar_plan(WorkspaceSidebarFacts {
            inline_provider_picker_open: false,
            launch_provider_picker_open: true,
            inline_new_session_picker_open: true,
            inline_agent_picker_open: true,
            inline_role_picker_open: true,
        }),
        WorkspaceSidebarPlan::LaunchProviderPicker
    );
    assert_eq!(
        workspace_sidebar_plan(WorkspaceSidebarFacts {
            inline_provider_picker_open: false,
            launch_provider_picker_open: false,
            inline_new_session_picker_open: true,
            inline_agent_picker_open: true,
            inline_role_picker_open: true,
        }),
        WorkspaceSidebarPlan::InlineNewSessionPicker
    );
    assert_eq!(
        workspace_sidebar_plan(WorkspaceSidebarFacts {
            inline_provider_picker_open: false,
            launch_provider_picker_open: false,
            inline_new_session_picker_open: false,
            inline_agent_picker_open: true,
            inline_role_picker_open: true,
        }),
        WorkspaceSidebarPlan::InlineAgentPicker
    );
    assert_eq!(
        workspace_sidebar_plan(WorkspaceSidebarFacts {
            inline_provider_picker_open: false,
            launch_provider_picker_open: false,
            inline_new_session_picker_open: false,
            inline_agent_picker_open: false,
            inline_role_picker_open: true,
        }),
        WorkspaceSidebarPlan::InlineRolePicker
    );
    assert_eq!(
        workspace_sidebar_plan(WorkspaceSidebarFacts {
            inline_provider_picker_open: false,
            launch_provider_picker_open: false,
            inline_new_session_picker_open: false,
            inline_agent_picker_open: false,
            inline_role_picker_open: false,
        }),
        WorkspaceSidebarPlan::ListNames
    );
}

#[test]
fn workspace_sidebar_focus_requires_list_focus_without_modal() {
    assert!(workspace_sidebar_owns_focus(true, false));
    assert!(!workspace_sidebar_owns_focus(true, true));
    assert!(!workspace_sidebar_owns_focus(false, false));
}

#[test]
fn workspace_list_display_row_for_row_returns_none_for_missing_backing_data() {
    assert_eq!(
        workspace_list_display_row_for_row(
            display_row_facts(ManagerListRow::SavedWorkspace(9)),
            |_| None,
            |_| None,
            |_, _| None,
        ),
        None
    );
}

#[test]
fn new_workspace_row_uses_action_row_style() {
    let rows = vec![
        Some(new_workspace_display_row(false, false)),
        Some(new_workspace_display_row(true, false)),
    ];
    let (lines, _) = list_name_lines(&rows, 24, true);

    assert_eq!(lines[0].spans[0].content.as_ref(), "  ");
    assert_eq!(lines[0].spans[0].style, action_row_style(false));
    assert_eq!(lines[0].spans[1].content.as_ref(), "+ New workspace");
    assert_eq!(lines[0].spans[1].style, action_row_style(false));

    assert_eq!(lines[1].spans[0].content.as_ref(), "\u{25b8} ");
    assert_eq!(lines[1].spans[0].style, action_row_style(true));
    assert_eq!(lines[1].spans[1].content.as_ref(), "+ New workspace");
    assert_eq!(lines[1].spans[1].style, action_row_style(true));
}

#[test]
fn workspace_list_names_render_plan_derives_viewport_and_follow_scroll() {
    let plan = workspace_list_names_render_plan(WorkspaceListNamesRenderFacts {
        area: Rect::new(0, 0, 30, 6),
        selected_index: 8,
        row_count: 12,
        scroll_y: 0,
    });

    assert_eq!(plan.viewport_width, 28);
    assert_eq!(plan.follow_scroll_y, 5);
}

#[test]
fn launch_provider_picker_uses_single_word_title() {
    assert_eq!(provider_picker_title(None), " Provider ");
}

#[test]
fn inline_provider_picker_keeps_instance_context() {
    assert_eq!(provider_picker_title(Some("abc123")), " abc123 — Provider ");
}

#[test]
fn picker_sidebar_cursor_is_focus_gated() {
    let render = |focused| {
        let backend = TestBackend::new(20, 6);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                render_picker_sidebar(
                    frame,
                    Rect::new(0, 0, 20, 6),
                    " Provider ",
                    vec!["Anthropic".to_owned(), "Kimi".to_owned()],
                    Some(0),
                    focused,
                );
            })
            .expect("draw");
        terminal.backend().buffer().clone()
    };

    let focused = render(true);
    let unfocused = render(false);

    assert_eq!(focused[(1, 1)].symbol(), "▸");
    assert_eq!(unfocused[(1, 1)].symbol(), " ");
}

#[test]
fn provider_picker_sidebar_wraps_title_labels_and_selection() {
    let backend = TestBackend::new(32, 6);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            render_provider_picker_sidebar(
                frame,
                Rect::new(0, 0, 32, 6),
                Some("abc123"),
                vec!["Anthropic".to_owned(), "Kimi".to_owned()],
                1,
                true,
            );
        })
        .expect("draw");
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();

    assert!(text.contains("abc123"));
    assert!(text.contains("Provider"));
    assert!(text.contains("Anthropic"));
    assert!(text.contains("Kimi"));
    assert_eq!(terminal.backend().buffer()[(1, 2)].symbol(), "▸");
}

#[test]
fn typed_picker_sidebars_render_labels() {
    let role_picker = crate::tui::components::role_picker::RolePickerState::new(vec![
        jackin_core::RoleSelector::parse("agent-smith").unwrap(),
    ]);
    let agent_picker = crate::tui::components::agent_choice::AgentChoiceState::with_choices(vec![
        jackin_core::Agent::Codex,
    ]);
    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).expect("terminal");

    terminal
        .draw(|frame| {
            render_role_picker_sidebar(
                frame,
                Rect::new(0, 0, 20, 10),
                "workspace",
                &role_picker,
                true,
            );
            render_agent_picker_sidebar(
                frame,
                Rect::new(20, 0, 20, 10),
                "agent-smith",
                &agent_picker,
                true,
            );
        })
        .expect("draw");
    let buf = terminal.backend().buffer();
    let text: String = (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("agent-smith"));
    assert!(text.contains("Codex"));
}
