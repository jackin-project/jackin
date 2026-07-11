use jackin_capsule::tui::components::chrome::StatusBarWidget;
use jackin_capsule::tui::components::status_bar::{PrefixMode, StatusBar, status_bar_plan};
/// Status bar layout regressions: brand pill, tab click regions,
/// menu hint, overflow indicator.
use jackin_capsule::tui::layout::Tab;
use jackin_capsule::tui::model::VisibleAgentState;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn draw(
    cols: u16,
    tabs: &[Tab],
    active: usize,
    states: &[(u64, VisibleAgentState)],
    hovered_tab: Option<usize>,
    prefix_mode: PrefixMode,
) -> (StatusBar, Buffer) {
    let backend = TestBackend::new(cols, 2);
    let mut terminal = Terminal::new(backend).unwrap();
    let plan = status_bar_plan(cols, tabs, active, states, prefix_mode);
    terminal
        .draw(|frame| {
            frame.render_widget(
                StatusBarWidget {
                    plan: &plan,
                    prefix_mode,
                    hovered_tab,
                    menu_hovered: false,
                    focused: false,
                },
                frame.area(),
            );
        })
        .unwrap();

    let mut bar = StatusBar::new();
    bar.set_prefix_mode(prefix_mode);
    bar.set_click_regions_from_plan(&plan);
    (bar, terminal.backend().buffer().clone())
}

fn row_text(buf: &Buffer, row: u16, cols: u16) -> String {
    (0..cols)
        .map(|col| buf[(col, row)].symbol().to_owned())
        .collect()
}

#[test]
fn brand_pill_renders_before_menu_hint() {
    let (_, buf) = draw(80, &[], 0, &[], None, PrefixMode::Idle);
    let row = row_text(&buf, 0, 80);
    let brand = row.find("jackin❯").expect("brand text missing");
    let menu = row.find("Menu").expect("menu button missing");
    assert!(brand < menu);
}

#[test]
fn active_tab_background_differs_from_brand_pill() {
    let tab = Tab::new_single("Codex", 7, "test");
    let (_, buf) = draw(80, &[tab], 0, &[], None, PrefixMode::Idle);
    let brand_cells = (0..80)
        .filter(|x| buf[(*x, 0)].bg == jackin_tui::theme::BRAND_BLOCK)
        .count();
    let active_tab_cells = (0..80)
        .filter(|x| buf[(*x, 0)].bg == jackin_tui::theme::TAB_BG_ACTIVE)
        .count();
    assert_eq!(brand_cells, jackin_tui::display_cols(" jackin❯ "));
    assert!(active_tab_cells > 0, "active tab should use graphite bg");
}

#[test]
fn hovered_tab_uses_lifted_background() {
    let tabs = vec![
        Tab::new_single("Codex", 7, "test"),
        Tab::new_single("Shell", 8, "test"),
    ];
    let (_, buf) = draw(80, &tabs, 0, &[], Some(1), PrefixMode::Idle);
    assert!(
        (0..80).any(|x| buf[(x, 0)].bg == jackin_tui::theme::TAB_BG_INACTIVE_HOVER),
        "hovered inactive tab should use lifted bg"
    );
}

#[test]
fn active_tab_hover_uses_lifted_graphite_background() {
    let tabs = vec![
        Tab::new_single("Codex", 7, "test"),
        Tab::new_single("Shell", 8, "test"),
    ];
    let (_, buf) = draw(80, &tabs, 0, &[], Some(0), PrefixMode::Idle);
    assert!(
        (0..80).any(|x| buf[(x, 0)].bg == jackin_tui::theme::TAB_BG_ACTIVE_HOVER),
        "hovered active tab should use lifted graphite bg"
    );
}

#[test]
fn tab_click_region_includes_state_glyph_width() {
    let tab = Tab::new_single("Codex", 7, "test");
    let tabs = vec![tab];
    let states = vec![(7u64, VisibleAgentState::Done)];
    let (bar, _) = draw(80, &tabs, 0, &states, None, PrefixMode::Idle);
    let (start, end) = bar.tab_regions[0];
    // Cell layout: 1 pad + name(5) + 1 sep + 1 glyph + 1 pad = 9 cols.
    assert_eq!(end - start, 9);
}

#[test]
fn prefix_mode_swap_changes_menu_hint() {
    let (_, buf) = draw(80, &[], 0, &[], None, PrefixMode::Idle);
    let row = row_text(&buf, 0, 80);
    assert!(row.contains("Menu"));
    assert!(!row.contains("Ctrl+\\"));
    let (_, buf) = draw(80, &[], 0, &[], None, PrefixMode::Awaiting);
    let row = row_text(&buf, 0, 80);
    assert!(row.contains("prefix…"));
}

#[test]
fn overflow_indicator_appears_when_tabs_exceed_width() {
    // Tabs of label "AAAAA" plus separator. With cols=30 only a handful
    // fit, so we should see the overflow `›` rendered.
    let tabs: Vec<Tab> = (0..10)
        .map(|i| Tab::new_single(format!("Tab{i:02}"), u64::try_from(i).unwrap_or(0), "test"))
        .collect();
    let (_, buf) = draw(30, &tabs, 0, &[], None, PrefixMode::Idle);
    let row = row_text(&buf, 0, 30);
    assert!(row.contains("›"), "expected overflow indicator: {row:?}");
}

#[test]
fn click_outside_tab_region_returns_none() {
    let tab = Tab::new_single("Foo", 1, "test");
    let tabs = vec![tab];
    let (bar, _) = draw(80, &tabs, 0, &[], None, PrefixMode::Idle);
    let (start, _) = bar.tab_regions[0];
    assert!(start > 0);
    assert!(bar.tab_at_col(0).is_none());
    assert!(bar.tab_at_col(start).is_some());
}
