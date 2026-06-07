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
