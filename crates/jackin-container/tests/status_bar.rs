/// Status bar layout regressions: brand pill, tab click regions,
/// prefix-mode hint, overflow indicator.
use jackin_container::layout::Tab;
use jackin_container::protocol::AgentState;
use jackin_container::statusbar::{PrefixMode, StatusBar};

fn render(
    bar: &mut StatusBar,
    cols: u16,
    tabs: &[Tab],
    active: usize,
    states: &[(u64, AgentState)],
) -> String {
    let mut buf = Vec::new();
    bar.render(&mut buf, cols, tabs, active, states);
    String::from_utf8_lossy(&buf).to_string()
}

#[test]
fn brand_pill_renders_first() {
    let mut bar = StatusBar::new();
    let s = render(&mut bar, 80, &[], 0, &[]);
    let brand = s.find("jackin'").expect("brand text missing");
    let menu = s.find("menu:").expect("idle hint missing");
    assert!(brand < menu);
}

#[test]
fn tab_click_region_includes_state_glyph_width() {
    let mut bar = StatusBar::new();
    let tab = Tab::new_single("Codex", 7);
    let tabs = vec![tab];
    let states = vec![(7u64, AgentState::Done)]; // appends " ○"
    let _ = render(&mut bar, 80, &tabs, 0, &states);
    let (start, end) = bar.tab_regions[0];
    // " Codex ○ " — 5 label + " ○" glyph = 7 chars + 2 padding = 9 cols.
    assert_eq!(end - start, 9);
}

#[test]
fn prefix_mode_swap_changes_hint() {
    let mut bar = StatusBar::new();
    let s = render(&mut bar, 80, &[], 0, &[]);
    assert!(s.contains("menu: Ctrl+J"));
    bar.set_prefix_enabled(true);
    bar.set_prefix_mode(PrefixMode::Awaiting);
    let s = render(&mut bar, 80, &[], 0, &[]);
    assert!(s.contains("prefix…"));
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
