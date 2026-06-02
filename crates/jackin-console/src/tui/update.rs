//! Top-level console TUI update helpers.

use jackin_tui::runtime::UpdateResult;

pub type ConsoleUpdate<E> = UpdateResult<E>;

#[derive(Debug, Clone)]
pub enum StatusOverlayPlan {
    Open(jackin_tui::components::StatusPopupState),
    Dismiss,
}

#[derive(Debug)]
pub enum ListModalPlan {
    ContainerInfo(jackin_tui::components::ContainerInfoState),
    GithubPicker(crate::tui::components::github_picker::GithubPickerState),
    Dismiss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlinePickerDismissal {
    NewSession,
    Role,
    Agent,
    Provider,
    LaunchProvider,
}

#[must_use]
pub fn selection_move_plan(selected: usize, row_count: usize, delta: isize) -> usize {
    crate::focus::moved_selection(selected, row_count, delta)
}

#[must_use]
pub fn selected_index_plan(selected: usize, row_count: usize) -> usize {
    crate::focus::selected_index(selected, row_count)
}

#[must_use]
pub const fn unclamped_scroll_plan(current_scroll: u16, delta: i16) -> u16 {
    let mut scroll = current_scroll;
    jackin_tui::components::apply_scroll_delta_unclamped(&mut scroll, delta);
    scroll
}

#[must_use]
pub fn term_width_scroll_plan(
    current_scroll_x: u16,
    delta: i16,
    term_width: u16,
    content_width: usize,
) -> u16 {
    let mut scroll_x = current_scroll_x;
    jackin_tui::components::apply_term_width_scroll_delta(
        &mut scroll_x,
        delta,
        term_width,
        content_width,
    );
    scroll_x
}

#[must_use]
pub fn open_status_overlay_plan(
    title: impl Into<String>,
    message: impl Into<String>,
) -> StatusOverlayPlan {
    StatusOverlayPlan::Open(crate::tui::components::status_popup::status_popup_state(
        title, message,
    ))
}

#[must_use]
pub const fn dismiss_status_overlay_plan() -> StatusOverlayPlan {
    StatusOverlayPlan::Dismiss
}

#[must_use]
pub fn open_container_info_modal_plan(
    state: jackin_tui::components::ContainerInfoState,
) -> ListModalPlan {
    ListModalPlan::ContainerInfo(state)
}

#[must_use]
pub fn open_github_picker_modal_plan(
    state: crate::tui::components::github_picker::GithubPickerState,
) -> ListModalPlan {
    ListModalPlan::GithubPicker(state)
}

#[must_use]
pub const fn dismiss_list_modal_plan() -> ListModalPlan {
    ListModalPlan::Dismiss
}

#[must_use]
pub const fn inline_picker_dismissal_plan(kind: InlinePickerDismissal) -> InlinePickerDismissal {
    kind
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_width_scroll_plan_updates_and_clamps_offset() {
        assert_eq!(term_width_scroll_plan(0, 8, 10, 40), 8);
        assert_eq!(term_width_scroll_plan(8, -99, 10, 40), 0);
    }

    #[test]
    fn selection_move_plan_clamps_to_rows() {
        assert_eq!(selection_move_plan(0, 3, 99), 2);
        assert_eq!(selection_move_plan(2, 3, -99), 0);
    }

    #[test]
    fn selected_index_plan_clamps_to_rows() {
        assert_eq!(selected_index_plan(99, 3), 2);
        assert_eq!(selected_index_plan(0, 0), 0);
    }

    #[test]
    fn unclamped_scroll_plan_updates_without_upper_clamp() {
        assert_eq!(unclamped_scroll_plan(4, 3), 7);
        assert_eq!(unclamped_scroll_plan(4, -99), 0);
    }

    #[test]
    fn status_overlay_plans_construct_open_and_dismiss() {
        let StatusOverlayPlan::Open(state) = open_status_overlay_plan("Title", "Body") else {
            panic!("expected open plan");
        };
        let debug = format!("{state:?}");
        assert!(debug.contains("Title"));
        assert!(debug.contains("Body"));
        assert!(matches!(
            dismiss_status_overlay_plan(),
            StatusOverlayPlan::Dismiss
        ));
    }

    #[test]
    fn inline_picker_dismissal_plan_returns_requested_kind() {
        assert_eq!(
            inline_picker_dismissal_plan(InlinePickerDismissal::Agent),
            InlinePickerDismissal::Agent
        );
    }
}
