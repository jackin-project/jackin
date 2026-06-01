use crate::config::AppConfig;
use crate::console::tui::render::list_geometry::{
    clamp_list_scroll_for_area, selected_sidebar_scroll_areas,
};
use crate::console::manager::state::ManagerState;
use crate::isolation::MountIsolation;
use crate::workspace::{MountConfig, WorkspaceConfig};
use jackin_tui::components::scrollable_panel::{
    max_offset as max_scroll_offset, viewport_height as scroll_viewport_height,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

fn split_mount(idx: usize) -> MountConfig {
    MountConfig {
        src: format!("/host/long/source/path/{idx}"),
        dst: format!("/container/long/destination/path/{idx}"),
        readonly: false,
        isolation: MountIsolation::Shared,
    }
}

#[test]
fn list_vertical_clamp_uses_rendered_sidebar_height() {
    let mut config = AppConfig::default();
    config.workspaces.insert(
        "demo".into(),
        WorkspaceConfig {
            workdir: "/workspace/demo".into(),
            mounts: (0..10).map(split_mount).collect(),
            ..Default::default()
        },
    );
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.selected = 1;

    let body = Rect::new(0, 0, 100, 10);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(state.list_split_pct),
            Constraint::Percentage(100u16.saturating_sub(state.list_split_pct)),
        ])
        .split(body);
    let areas = selected_sidebar_scroll_areas(columns[1], &state, &config, tmp.path()).unwrap();
    let rendered_viewport = scroll_viewport_height(areas.workspace.area);
    let desired_viewport = scroll_viewport_height(Rect::new(0, 0, 0, 12));
    assert!(rendered_viewport < desired_viewport);

    let expected = max_scroll_offset(areas.workspace.content_height, rendered_viewport);
    assert!(expected > max_scroll_offset(areas.workspace.content_height, desired_viewport));

    state.list_mounts_scroll_y = u16::MAX;
    clamp_list_scroll_for_area(body, &mut state, &config, tmp.path());

    assert_eq!(state.list_mounts_scroll_y, expected);
}

#[test]
fn tui_header_uses_lowercase_jackin_with_apostrophe() {
    let backend = ratatui::backend::TestBackend::new(40, 1);
    let mut term = ratatui::Terminal::new(backend).unwrap();
    term.draw(|f| {
        jackin_console::tui::view::render_header(f, Rect::new(0, 0, 40, 1), "workspaces");
    })
    .unwrap();

    let buf = term.backend().buffer();
    let dump: String = buf
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();

    assert!(
        dump.contains("jackin'"),
        "header must render 'jackin'' (lowercase + trailing apostrophe); got {dump:?}"
    );
    assert!(
        !dump.contains("JACKIN"),
        "header must not render 'JACKIN' (uppercase); got {dump:?}"
    );
}
