/// Status bar layout regressions: brand pill, tab click regions,
/// hidden menu hint, overflow indicator.
use jackin_capsule::layout::Tab;
use jackin_capsule::protocol::AgentState;
use jackin_capsule::statusbar::{PrefixMode, StatusBar};

fn render(
    bar: &mut StatusBar,
    cols: u16,
    tabs: &[Tab],
    active: usize,
    states: &[(u64, AgentState)],
) -> String {
    let mut buf = Vec::new();
    bar.render(&mut buf, cols, tabs, active, states, None);
    String::from_utf8_lossy(&buf).to_string()
}

fn render_with_hover(
    bar: &mut StatusBar,
    cols: u16,
    tabs: &[Tab],
    active: usize,
    states: &[(u64, AgentState)],
    hovered_tab: Option<usize>,
) -> String {
    let mut buf = Vec::new();
    bar.render(&mut buf, cols, tabs, active, states, hovered_tab);
    String::from_utf8_lossy(&buf).to_string()
}

#[test]
fn brand_pill_renders_without_menu_hint() {
    let mut bar = StatusBar::new();
    let s = render(&mut bar, 80, &[], 0, &[]);
    assert!(s.contains("jackin'"));
    assert!(!s.contains("Menu"));
    assert!(bar.hint_region.is_none());
}

#[test]
fn active_tab_background_differs_from_brand_pill() {
    let mut bar = StatusBar::new();
    let tab = Tab::new_single("Codex", 7);
    let s = render(&mut bar, 80, &[tab], 0, &[]);
    let brand_green_bg = "\x1b[48;2;0;255;65m";
    let active_tab_graphite_bg = "\x1b[48;2;42;42;42m";
    assert_eq!(
        s.matches(brand_green_bg).count(),
        1,
        "brand green should only paint the brand pill"
    );
    assert!(
        s.contains(active_tab_graphite_bg),
        "active tab should use distinct graphite bg: {s:?}"
    );
}

#[test]
fn hovered_tab_uses_lifted_background() {
    let mut bar = StatusBar::new();
    let tabs = vec![Tab::new_single("Codex", 7), Tab::new_single("Shell", 8)];
    let s = render_with_hover(&mut bar, 80, &tabs, 0, &[], Some(1));
    assert!(
        s.contains("\x1b[48;2;48;48;48m"),
        "hovered inactive tab should use lifted bg: {s:?}"
    );
}

#[test]
fn active_tab_hover_uses_lifted_graphite_background() {
    let mut bar = StatusBar::new();
    let tabs = vec![Tab::new_single("Codex", 7), Tab::new_single("Shell", 8)];
    let s = render_with_hover(&mut bar, 80, &tabs, 0, &[], Some(0));
    assert!(
        s.contains("\x1b[48;2;58;58;58m"),
        "hovered active tab should use lifted graphite bg: {s:?}"
    );
}

#[test]
fn tab_click_region_includes_state_glyph_width() {
    let mut bar = StatusBar::new();
    let tab = Tab::new_single("Codex", 7);
    let tabs = vec![tab];
    let states = vec![(7u64, AgentState::Done)];
    let _ = render(&mut bar, 80, &tabs, 0, &states);
    let (start, end) = bar.tab_regions[0];
    // Cell layout: 1 pad + name(5) + 1 sep + 1 glyph + 1 pad = 9 cols.
    assert_eq!(end - start, 9);
}

#[test]
fn prefix_mode_swap_does_not_render_hint() {
    let mut bar = StatusBar::new();
    let s = render(&mut bar, 80, &[], 0, &[]);
    assert!(!s.contains("Menu"));
    bar.set_prefix_enabled(true);
    bar.set_prefix_mode(PrefixMode::Awaiting);
    let s = render(&mut bar, 80, &[], 0, &[]);
    assert!(!s.contains("prefix"));
}

#[test]
fn overflow_indicator_appears_when_tabs_exceed_width() {
    let mut bar = StatusBar::new();
    // Tabs of label "AAAAA" plus separator. With cols=30 only a handful
    // fit, so we should see the overflow `›` rendered.
    let tabs: Vec<Tab> = (0..10)
        .map(|i| Tab::new_single(format!("Tab{i:02}"), i as u64))
        .collect();
    let s = render(&mut bar, 30, &tabs, 0, &[]);
    assert!(s.contains("›"), "expected overflow indicator: {s:?}");
}

#[test]
fn click_outside_tab_region_returns_none() {
    let mut bar = StatusBar::new();
    let tab = Tab::new_single("Foo", 1);
    let _ = render(&mut bar, 80, &[tab], 0, &[]);
    let (start, _) = bar.tab_regions[0];
    assert!(start > 0);
    assert!(bar.tab_at_col(0).is_none());
    assert!(bar.tab_at_col(start).is_some());
}
