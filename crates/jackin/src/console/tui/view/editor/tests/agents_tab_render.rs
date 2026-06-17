//! Tests for `editor` agents tab render rendering.
//! Pins `[x]`/`[ ]` to the *effectively allowed* state — empty
//! `allowed_roles` is the "all allowed" shorthand.
use super::super::render_roles_tab;
use crate::config::{AppConfig, RoleSource};
use crate::console::tui::state::{EditorState, EditorTab, FieldFocus};
use crate::workspace::WorkspaceConfig;
use jackin_console::tui::screens::editor::view::prepare_editor_tab_for_area;
use jackin_tui::components::scrollable_panel::viewport_width as scroll_viewport_width;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

fn ws_with_allowed(names: &[&str]) -> WorkspaceConfig {
    WorkspaceConfig {
        allowed_roles: names.iter().map(|s| (*s).into()).collect(),
        ..WorkspaceConfig::default()
    }
}

fn config_with_agents(names: &[&str]) -> AppConfig {
    let mut config = AppConfig::default();
    for name in names {
        config.roles.insert((*name).into(), RoleSource::default());
    }
    config
}

fn render_to_dump(ws: WorkspaceConfig, config: &AppConfig) -> String {
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Roles;
    editor.active_field = FieldFocus::Row(0);
    let backend = TestBackend::new(60, 10);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_roles_tab(f, Rect::new(0, 0, 60, 10), &editor, config);
    })
    .unwrap();
    let buf = term.backend().buffer();
    // Collapse the buffer to newline-delimited rows so the test
    // assertion can match per-row semantics ("row N contains `[x]`").
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn in_all_mode_all_rows_render_as_checked() {
    // Empty `allowed_roles` ⇒ "all" mode ⇒ every row is `[x]`.
    let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
    let ws = ws_with_allowed(&[]);
    let dump = render_to_dump(ws, &cfg);

    // Every role name should appear on a line that also carries `[x]`.
    for name in ["alpha", "beta", "gamma"] {
        let line = dump
            .lines()
            .find(|l| l.contains(name))
            .unwrap_or_else(|| panic!("role `{name}` not rendered in:\n{dump}"));
        assert!(
            line.contains("[x]"),
            "in 'all' mode role `{name}` row must render `[x]`; got `{line}`"
        );
        assert!(
            !line.contains("[ ]"),
            "in 'all' mode role `{name}` must not render `[ ]`; got `{line}`"
        );
    }
}

#[test]
fn roles_tab_clamps_horizontal_scroll_with_shared_state() {
    let cfg = config_with_agents(&["chainargos/agent-brown-with-extra-long-role-name-for-scroll"]);
    let ws = ws_with_allowed(&[]);
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Roles;
    editor.active_field = FieldFocus::Row(0);
    editor.set_tab_content_scroll_focused(true);
    editor.tab_scroll_x = u16::MAX;
    let area = Rect::new(0, 0, 42, 8);
    prepare_editor_tab_for_area(area, &mut editor, &cfg);
    let backend = TestBackend::new(42, 8);
    let mut term = Terminal::new(backend).unwrap();

    term.draw(|f| {
        render_roles_tab(f, area, &editor, &cfg);
    })
    .unwrap();

    let viewport = scroll_viewport_width(area);
    assert_eq!(
        editor.tab_scroll_x,
        jackin_tui::components::scrollable_panel::max_offset(editor.tab_content_width, viewport)
    );
    assert!(editor.tab_scroll_x > 0);
}

/// The default-role row carries the `★` marker; non-default rows
/// render a plain space in the marker column. Pins the glyph that
/// the `*` keybinding produces in the rendered list.
#[test]
fn default_agent_row_carries_star_marker() {
    let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut ws = ws_with_allowed(&[]);
    ws.default_role = Some("beta".into());
    let dump = render_to_dump(ws, &cfg);

    let beta_line = dump
        .lines()
        .find(|l| l.contains("beta"))
        .expect("beta must render");
    assert!(
        beta_line.contains('\u{2605}'),
        "default role row must carry the `★` marker; got `{beta_line}`"
    );

    let alpha_line = dump
        .lines()
        .find(|l| l.contains("alpha"))
        .expect("alpha must render");
    assert!(
        !alpha_line.contains('\u{2605}'),
        "non-default rows must not carry `★`; got `{alpha_line}`"
    );
}

#[test]
fn in_custom_mode_only_listed_agents_show_checked() {
    // Non-empty list ⇒ "custom" mode ⇒ only listed rows are `[x]`.
    let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
    let ws = ws_with_allowed(&["beta"]);
    let dump = render_to_dump(ws, &cfg);

    let beta_line = dump
        .lines()
        .find(|l| l.contains("beta"))
        .expect("beta must render");
    assert!(
        beta_line.contains("[x]"),
        "listed role `beta` must render `[x]`; got `{beta_line}`"
    );

    for name in ["alpha", "gamma"] {
        let line = dump
            .lines()
            .find(|l| l.contains(name))
            .unwrap_or_else(|| panic!("role `{name}` not rendered in:\n{dump}"));
        assert!(
            line.contains("[ ]"),
            "unlisted role `{name}` must render `[ ]` in 'custom' mode; got `{line}`"
        );
    }
}
