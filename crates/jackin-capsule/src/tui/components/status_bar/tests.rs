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
    let mut buf = Vec::new();
    bar.render(&mut buf, 80, &tabs, 0, &states, None, false);
    let (start, end) = bar.tab_regions[0];
    assert_eq!(end - start, 10);
    // Re-rendering with no state must keep the same width.
    let mut buf2 = Vec::new();
    bar.render(&mut buf2, 80, &tabs, 0, &[], None, false);
    let (s2, e2) = bar.tab_regions[0];
    assert_eq!(e2 - s2, 10);
    assert_eq!((s2, e2), (start, end));
}

#[test]
fn refresh_click_regions_matches_raw_render_regions() {
    // PR #495 migration invariant: the Ratatui StatusBarWidget computes its
    // click regions via refresh_click_regions, the raw StatusBar::render sets
    // them inline. Both must agree exactly, or a click after a Ratatui frame
    // lands on the wrong tab. Cover several widths incl. an overflow case.
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
        let mut raw = StatusBar::new();
        let mut buf = Vec::new();
        raw.render(&mut buf, cols, &tabs, 1, &states, None, false);

        let mut widget = StatusBar::new();
        widget.refresh_click_regions(cols, &tabs, 1, &states);

        assert_eq!(
            widget.tab_regions, raw.tab_regions,
            "tab regions diverged at cols={cols}"
        );
        assert_eq!(
            widget.hint_region, raw.hint_region,
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
fn pane_box_truncates_long_titles_instead_of_omitting_them() {
    let mut buf = Vec::new();
    draw_pane_box(&mut buf, 0, 0, 4, 16, "Shell title that is too long", false);
    let out = String::from_utf8_lossy(&buf);

    assert!(
        out.contains("Shell"),
        "long pane title should still render a truncated prefix: {out:?}"
    );
    assert!(
        !out.contains("Shell title that is too long"),
        "long pane title should not overflow the box: {out:?}"
    );
}

#[test]
fn idle_hint_is_rendered() {
    let mut bar = StatusBar::new();
    let mut buf = Vec::new();
    bar.render(&mut buf, 80, &[], 0, &[], None, false);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("☰Menu"), "menu hint missing: {s:?}");
    assert!(
        !s.contains("☰ Menu"),
        "menu hint should not pad between icon and label: {s:?}"
    );
    assert!(
        !s.contains("Ctrl+\\"),
        "menu hint should omit shortcut: {s:?}"
    );
    assert!(
        s.contains(BUTTON_BG_IDLE),
        "menu hint should use blue button chrome: {s:?}"
    );
    assert!(bar.hint_at(1, 75), "menu hint should be clickable");
}

#[test]
fn idle_hint_hover_uses_lifted_button_chrome() {
    let mut bar = StatusBar::new();
    let mut buf = Vec::new();
    bar.render(&mut buf, 80, &[], 0, &[], None, true);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains(" ☰Menu "), "menu hint should be padded: {s:?}");
    assert!(
        s.contains(BUTTON_BG_IDLE_HOVER),
        "hovered menu hint should use lifted blue chrome: {s:?}"
    );
}

#[test]
fn awaiting_prefix_hint_is_rendered() {
    let mut bar = StatusBar::new();
    bar.set_prefix_mode(PrefixMode::Awaiting);
    let mut buf = Vec::new();
    bar.render(&mut buf, 80, &[], 0, &[], None, false);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("prefix…"), "prefix hint missing: {s:?}");
    assert!(
        s.contains(BUTTON_BG_AWAITING),
        "awaiting prefix hint should use active blue chrome: {s:?}"
    );
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

#[test]
fn active_tab_emits_row1_underline() {
    let mut bar = StatusBar::new();
    let tabs = vec![Tab::new_single("Claude", 1, "test")];
    let mut buf = Vec::new();
    bar.render(&mut buf, 80, &tabs, 0, &[], None, false);
    let s = String::from_utf8_lossy(&buf);
    // Row 1 = ANSI row 2 (1-based). Underline uses `━`.
    assert!(s.contains("\x1b[2;"), "row 2 cursor move missing: {s:?}");
    assert!(s.contains("━"), "underline glyph missing: {s:?}");
}
