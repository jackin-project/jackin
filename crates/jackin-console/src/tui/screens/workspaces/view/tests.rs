//! Tests for `view`.
use super::*;
use crate::tui::screens::workspaces::model::ManagerListRow;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

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
fn workspace_list_display_helpers_own_visible_defaults() {
    let current = current_directory_display_row(true, true, true, false);
    assert_eq!(current.label, "Current directory");
    assert!(current.expanded);
    assert!(current.has_instances);
    assert!(current.selected);

    let new_workspace = new_workspace_display_row(false, true);
    assert_eq!(new_workspace.label, new_workspace_list_label());
    assert!(new_workspace.hovered);
    assert!(!new_workspace.expanded);
    assert_eq!(
        workspace_instance_list_label("abc123", "chainargos/agent-smith"),
        "abc123  chainargos/agent-smith"
    );
    let instance = workspace_instance_display_row("abc123", "chainargos/agent-smith", true, true);
    assert_eq!(instance.label, "abc123  chainargos/agent-smith");
    assert_eq!(instance.tone, WorkspaceListRowTone::Instance);
    assert!(instance.selected);
    assert!(instance.hovered);
    assert!(!instance.expanded);
    assert!(!instance.has_instances);
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
        Some(current_directory_display_row(true, true, true, false))
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
            expanded: true,
            has_instances: false,
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
            |idx| (idx == 3).then(|| ("i-cwd".to_owned(), "role".to_owned())),
            |_| None,
            |_, _| None,
        ),
        Some(workspace_instance_display_row("i-cwd", "role", true, true))
    );
    assert_eq!(
        workspace_list_display_row_for_row(
            display_row_facts(ManagerListRow::WorkspaceInstance(1, 4)),
            |_| None,
            |_| None,
            |ws, inst| (ws == 1 && inst == 4).then(|| ("i-ws".to_owned(), "smith".to_owned())),
        ),
        Some(workspace_instance_display_row(
            "i-ws", "smith", false, false
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
