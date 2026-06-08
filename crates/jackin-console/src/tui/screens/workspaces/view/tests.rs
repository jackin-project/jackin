//! Tests for `view`.
use super::*;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

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
fn new_workspace_row_uses_action_row_style() {
    let rows = vec![
        Some(new_workspace_display_row(false, false)),
        Some(new_workspace_display_row(true, false)),
    ];
    let (lines, _) = list_name_lines(&rows, 24, true);

    assert_eq!(lines[0].spans[0].content.as_ref(), "  ");
    assert_eq!(lines[0].spans[1].content.as_ref(), "+ New workspace");
    assert_eq!(lines[0].spans[1].style, action_row_style(false));

    assert_eq!(lines[1].spans[0].content.as_ref(), "\u{25b8} ");
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
