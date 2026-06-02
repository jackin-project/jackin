//! Tests for `editor` general tab render rendering.
use super::render_general_tab;
use crate::config::AppConfig;
use crate::console::tui::layout::editor::prepare_editor_tab_for_area;
use crate::console::tui::state::{EditorState, FieldFocus};
use crate::workspace::WorkspaceConfig;
use jackin_tui::components::scrollable_panel::viewport_width as scroll_viewport_width;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

#[test]
fn general_tab_clamps_horizontal_scroll_with_shared_scrollable_block() {
    let ws = WorkspaceConfig {
        workdir: "/workspace/path/that/is/long/enough/to/require/horizontal/scrolling".into(),
        ..Default::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_field = FieldFocus::Row(1);
    editor.tab_content_scroll_focused = true;
    editor.tab_scroll_x = u16::MAX;
    let area = Rect::new(0, 0, 42, 8);
    prepare_editor_tab_for_area(area, &mut editor, &AppConfig::default());

    let backend = TestBackend::new(42, 8);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_general_tab(f, area, &editor);
    })
    .unwrap();

    let viewport = scroll_viewport_width(area);
    assert_eq!(
        editor.tab_scroll_x,
        jackin_tui::components::scrollable_panel::max_offset(editor.tab_content_width, viewport)
    );
    assert!(editor.tab_scroll_x > 0);
}
