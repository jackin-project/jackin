//! Tests for `stories`.
use super::*;
use std::collections::BTreeSet;

#[test]
fn every_exported_component_has_a_story() {
    let expected = BTreeSet::from([
        "BrandHeader",
        "ButtonStrip",
        "ConfirmDialog",
        "ContainerInfoState",
        "DiffView",
        "ErrorDialog",
        "FilterInput",
        "HintBar",
        "Panel",
        "SaveDiscardDialog",
        "ScrollablePanel",
        "SelectList",
        "StatusFooter",
        "StatusPopup",
        "TabStrip",
        "TextInput",
        "Toast",
    ]);
    let actual: BTreeSet<&str> = stories().into_iter().map(|story| story.component).collect();

    assert_eq!(actual, expected);
}

#[test]
fn confirm_stories_match_dialog_height_contract() {
    let stories = stories();
    let height = |id: &str| {
        stories
            .iter()
            .find(|story| story.id == id)
            .map(|story| story.height)
            .expect("story exists")
    };

    assert_eq!(
        height("confirm/default"),
        jackin_tui::components::confirm_required_height(&ConfirmState::new(
            "Delete workspace \"jackin-core\"?\nThis removes the saved workspace entry.",
        ))
    );
    assert_eq!(
        height("confirm/focus-yes"),
        jackin_tui::components::confirm_required_height(
            &ConfirmState::new("Exit without saving?").with_focus_yes()
        )
    );
}
