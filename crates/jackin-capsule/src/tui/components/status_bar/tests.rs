//! Tests for `status_bar`.
use super::*;
use crate::tui::layout::{PaneTree, Tab};

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
    bar.set_click_regions_from_plan(&status_bar_plan(80, &tabs, 0, &states, PrefixMode::Idle));
    let (start, end) = bar.tab_regions[0];
    assert_eq!(end - start, 10);
    // Recomputing with no state must keep the same width.
    bar.set_click_regions_from_plan(&status_bar_plan(80, &tabs, 0, &[], PrefixMode::Idle));
    let (s2, e2) = bar.tab_regions[0];
    assert_eq!(e2 - s2, 10);
    assert_eq!((s2, e2), (start, end));
}

#[test]
fn tab_display_label_has_no_name_centering_padding() {
    assert_eq!(tab_display_label("Kimi"), "Kimi X");
    assert_eq!(tab_display_label("OpenCode"), "OpenCode X");
    assert!(!tab_display_label("Kimi").starts_with(' '));
}

#[test]
fn each_visible_agent_state_maps_to_a_distinct_tab_glyph() {
    assert_eq!(
        TabGlyph::from(VisibleAgentState::Blocked),
        TabGlyph::Blocked
    );
    assert_eq!(TabGlyph::from(VisibleAgentState::Done), TabGlyph::Done);
    assert_eq!(
        TabGlyph::from(VisibleAgentState::Working),
        TabGlyph::Working
    );
    assert_eq!(TabGlyph::from(VisibleAgentState::Idle), TabGlyph::Idle);
    assert_eq!(
        TabGlyph::from(VisibleAgentState::Unknown),
        TabGlyph::Unknown
    );
}

#[test]
fn tab_label_shows_working_and_idle_instead_of_blank_none() {
    let tab = Tab::new_single("Claude", 1, "test");

    let (_, working) = tab_label(&tab, &[(1, VisibleAgentState::Working)]);
    let (_, idle) = tab_label(&tab, &[(1, VisibleAgentState::Idle)]);
    let (_, unknown) = tab_label(&tab, &[(1, VisibleAgentState::Unknown)]);

    assert_eq!(working, TabGlyph::Working);
    assert_eq!(idle, TabGlyph::Idle);
    assert_eq!(unknown, TabGlyph::Unknown);
}

#[test]
fn tab_label_rolls_up_attention_priority() {
    let mut tab = Tab::new_single("Mix", 1, "test");
    tab.tree = PaneTree::HSplit {
        left: Box::new(PaneTree::Leaf(1)),
        right: Box::new(PaneTree::Leaf(2)),
        ratio: 0.5,
    };

    let (_, glyph) = tab_label(
        &tab,
        &[
            (1, VisibleAgentState::Working),
            (2, VisibleAgentState::Blocked),
        ],
    );

    assert_eq!(glyph, TabGlyph::Blocked);
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
    let plan = status_bar_plan(80, &[], 0, &[], PrefixMode::Idle);
    bar.set_click_regions_from_plan(&plan);
    assert_eq!(plan.hint_text, " ☰Menu ");
    assert!(bar.hint_at(1, 75), "menu hint should be clickable");
}

#[test]
fn awaiting_prefix_hint_is_planned() {
    let plan = status_bar_plan(80, &[], 0, &[], PrefixMode::Awaiting);
    assert_eq!(plan.hint_text, " prefix… ");
}

#[test]
fn idle_menu_hint_is_icon_only() {
    let plan = status_bar_plan(80, &[], 0, &[], PrefixMode::Idle);
    assert_eq!(plan.hint_text, " ☰Menu ");
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
