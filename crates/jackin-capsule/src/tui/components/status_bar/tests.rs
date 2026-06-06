//! Tests for `status_bar`.
use super::*;
use crate::tui::layout::Tab;

#[test]
fn tab_click_region_width_matches_layout() {
    // Tab cell layout: ` <name> <glyph> ` = 1 pad + name +
    // 1 sep + 1 glyph + 1 pad = name + 4. With name="Claude" the
    // cell is 10 cols wide; the region is stable regardless of
    // the agent state.
    let mut bar = StatusBar::new();
    let tab = Tab::new_single("Claude", 1, "test");
    let tabs = vec![tab];
    let states = vec![(1u64, VisibleAgentState::Blocked)];
    bar.refresh_click_regions(80, &tabs, 0, &states);
    let (start, end) = bar.tab_regions[0];
    assert_eq!(end - start, 10);
    // Recomputing with no state must keep the same width.
    bar.refresh_click_regions(80, &tabs, 0, &[]);
    let (s2, e2) = bar.tab_regions[0];
    assert_eq!(e2 - s2, 10);
    assert_eq!((s2, e2), (start, end));
}

#[test]
fn refresh_click_regions_matches_status_bar_plan() {
    // PR #495 migration invariant: the Ratatui StatusBarWidget and click
    // regions share status_bar_plan. A click after a Ratatui frame must land
    // on the tab/menu cell that was painted. Cover several widths incl. an
    // overflow case.
    let tabs = vec![
        Tab::new_single("Claude", 1, "test"),
        Tab::new_single("Codex", 2, "test"),
        Tab::new_single("OpenCode", 3, "test"),
    ];
    let states = vec![
        (1u64, VisibleAgentState::Blocked),
        (2u64, VisibleAgentState::Done),
    ];
    for cols in [80u16, 120, 40, 24] {
        let plan = status_bar_plan(cols, &tabs, 1, &states, PrefixMode::Idle);

        let mut widget = StatusBar::new();
        widget.refresh_click_regions(cols, &tabs, 1, &states);

        let expected_tabs: Vec<(u16, u16)> = plan
            .cells
            .iter()
            .map(|c| (c.start_col0 + 1, c.start_col0 + 1 + c.cell_cols))
            .collect();
        assert_eq!(
            widget.tab_regions, expected_tabs,
            "tab regions diverged at cols={cols}"
        );
        assert_eq!(
            widget.hint_region,
            plan.hint_start.map(|start| (start, start + plan.hint_cols)),
            "hint region diverged at cols={cols}"
        );
    }
}

#[test]
fn tab_display_label_has_no_name_centering_padding() {
    assert_eq!(tab_display_label("Kimi"), "Kimi X");
    assert_eq!(tab_display_label("OpenCode"), "OpenCode X");
    assert!(!tab_display_label("Kimi").starts_with(' '));
}

#[test]
fn status_bar_keeps_supplied_container_name_and_instance_id() {
    let bar = StatusBar::new_with_role_labels(
        "the-architect".to_owned(),
        "jk-spamcw91-jackin-thearchitect".to_owned(),
        "spamcw91".to_owned(),
    );

    assert_eq!(bar.container_name(), "jk-spamcw91-jackin-thearchitect");
    assert_eq!(bar.instance_id_label(), "spamcw91");
}

#[test]
fn idle_hint_is_planned() {
    let mut bar = StatusBar::new();
    bar.refresh_click_regions(80, &[], 0, &[]);
    let plan = status_bar_plan(80, &[], 0, &[], PrefixMode::Idle);
    assert_eq!(plan.hint_text, " ☰Menu ");
    assert!(bar.hint_at(1, 75), "menu hint should be clickable");
}

#[test]
fn awaiting_prefix_hint_is_planned() {
    let plan = status_bar_plan(80, &[], 0, &[], PrefixMode::Awaiting);
    assert_eq!(plan.hint_text, " prefix… ");
}

#[test]
fn prefix_mode_follows_visible_mux_mode() {
    assert_eq!(
        prefix_mode_for_mux_mode(MuxMode::PrefixAwait),
        PrefixMode::Awaiting
    );
    for mode in [
        MuxMode::Normal,
        MuxMode::Dialog,
        MuxMode::Drag,
        MuxMode::Select,
    ] {
        assert_eq!(prefix_mode_for_mux_mode(mode), PrefixMode::Idle);
    }
}
