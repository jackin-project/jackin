// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
            inline_account_picker_open: true,
            launch_account_picker_open: true,
            inline_new_session_picker_open: true,
            inline_agent_picker_open: true,
            inline_role_picker_open: true,
        }),
        WorkspaceSidebarPlan::InlineAccountPicker
    );
    assert_eq!(
        workspace_sidebar_plan(WorkspaceSidebarFacts {
            inline_account_picker_open: false,
            launch_account_picker_open: true,
            inline_new_session_picker_open: true,
            inline_agent_picker_open: true,
            inline_role_picker_open: true,
        }),
        WorkspaceSidebarPlan::LaunchAccountPicker
    );
    assert_eq!(
        workspace_sidebar_plan(WorkspaceSidebarFacts {
            inline_account_picker_open: false,
            launch_account_picker_open: false,
            inline_new_session_picker_open: true,
            inline_agent_picker_open: true,
            inline_role_picker_open: true,
        }),
        WorkspaceSidebarPlan::InlineNewSessionPicker
    );
    assert_eq!(
        workspace_sidebar_plan(WorkspaceSidebarFacts {
            inline_account_picker_open: false,
            launch_account_picker_open: false,
            inline_new_session_picker_open: false,
            inline_agent_picker_open: true,
            inline_role_picker_open: true,
        }),
        WorkspaceSidebarPlan::InlineAgentPicker
    );
    assert_eq!(
        workspace_sidebar_plan(WorkspaceSidebarFacts {
            inline_account_picker_open: false,
            launch_account_picker_open: false,
            inline_new_session_picker_open: false,
            inline_agent_picker_open: false,
            inline_role_picker_open: true,
        }),
        WorkspaceSidebarPlan::InlineRolePicker
    );
    assert_eq!(
        workspace_sidebar_plan(WorkspaceSidebarFacts {
            inline_account_picker_open: false,
            launch_account_picker_open: false,
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
fn workspace_list_names_window_matches_literal_visible_indices() {
    let plan_at = |selected_index: usize, scroll_y: u16| {
        workspace_list_names_render_plan(WorkspaceListNamesRenderFacts {
            // height 12 -> viewport_h 10 over 20 rows.
            area: Rect::new(0, 0, 30, 12),
            selected_index,
            row_count: 20,
            scroll_y,
        })
        .follow_scroll_y
    };
    let last = |first: u16| usize::from(first) + 10 - 1;

    // Cursor inside the stored window: offset kept (rows 4..=13).
    let first = plan_at(8, 4);
    assert_eq!((first, last(first)), (4, 13));
    // Cursor above the window: window jumps up to the cursor (rows 2..=11).
    let first = plan_at(2, 4);
    assert_eq!((first, last(first)), (2, 11));
    // Cursor below the window: window scrolls to pin it at the bottom
    // (rows 10..=19).
    let first = plan_at(19, 4);
    assert_eq!((first, last(first)), (10, 19));
    // Stored offset beyond the max clamps to 10, then the cursor above it
    // pulls the window up to the cursor (rows 8..=17).
    let first = plan_at(8, 60);
    assert_eq!((first, last(first)), (8, 17));
    // Zero-height viewport: the upstream guard keeps the offset at 0.
    let plan = workspace_list_names_render_plan(WorkspaceListNamesRenderFacts {
        area: Rect::new(0, 0, 30, 2),
        selected_index: 8,
        row_count: 20,
        scroll_y: 4,
    });
    assert_eq!(plan.follow_scroll_y, 0);
}

#[test]
fn launch_account_picker_uses_single_word_title() {
    assert_eq!(account_picker_title(None), " Account ");
}

#[test]
fn inline_account_picker_keeps_instance_context() {
    assert_eq!(account_picker_title(Some("abc123")), " abc123 — Account ");
}

#[test]
fn account_picker_sidebar_wraps_title_labels_and_selection() {
    let backend = TestBackend::new(32, 6);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            render_account_picker_sidebar(
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
    assert!(text.contains("Account"));
    assert!(text.contains("Anthropic"));
    assert!(text.contains("Kimi"));
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
