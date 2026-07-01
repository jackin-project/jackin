#[cfg(test)]
use super::*;

#[test]
fn hovered_list_row_extracts_list_row_target() {
    assert_eq!(
        hovered_list_row(Some(ManagerHoverTarget::ListRow(
            ManagerListRow::SavedWorkspace(2)
        ))),
        Some(ManagerListRow::SavedWorkspace(2))
    );
    assert_eq!(hovered_list_row(None), None);
}
}
