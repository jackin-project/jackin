//! Tests for `editor` mounts tab render rendering.
use super::render_editor;
use crate::config::AppConfig;
use crate::console::tui::state::{EditorState, EditorTab, FieldFocus};
use crate::workspace::{MountConfig, WorkspaceConfig};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

#[test]
fn readonly_mount_renders_ro_mode() {
    let ws = WorkspaceConfig {
        mounts: vec![MountConfig {
            src: "/host/a".into(),
            dst: "/host/a".into(),
            readonly: true,
            isolation: crate::isolation::MountIsolation::Shared,
        }],
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Mounts;
    editor.tab_bar_focused = false;
    editor.active_field = FieldFocus::Row(0);

    let config = AppConfig::default();
    let backend = TestBackend::new(80, 10);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_editor(f, f.area(), &editor, &config, true);
    })
    .unwrap();

    let buf = term.backend().buffer();
    let found = (0..buf.area.height).any(|y| {
        let row = (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>();
        row.contains(" ro ") || row.trim_end().ends_with(" ro") || row.contains(" ro  ")
    });
    assert!(
        found,
        "readonly mount render must show `ro` in the mode column"
    );
}
