//! Editor screen view helpers.

use super::model::EditorTab;

#[must_use]
pub fn tab_labels(active: EditorTab) -> Vec<(&'static str, bool)> {
    EditorTab::ALL
        .iter()
        .map(|tab| (tab.label(), *tab == active))
        .collect()
}
