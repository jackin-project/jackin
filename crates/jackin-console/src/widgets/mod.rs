//! Host console widgets that are independent of root application state.

pub use jackin_tui::ModalOutcome;
pub use jackin_tui::theme::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

pub mod agent_choice;
pub mod confirm_save;
pub mod file_browser;
pub mod github_picker;
pub mod mount_dst_choice;
pub mod op_picker;
pub mod role_picker;
pub mod scope_picker;
pub mod source_picker;
pub mod workdir_pick;

/// Braille spinner animation shared across modal loading panels.
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Wrap-around cursor move for any list-style picker. `delta` is `-1`
/// for Up, `+1` for Down. No-op when `count == 0`.
pub fn cycle_select(list_state: &mut tui_widget_list::ListState, count: usize, delta: i32) {
    if count == 0 {
        return;
    }
    let cur = list_state.selected.unwrap_or(0);
    let next = if delta < 0 {
        if cur == 0 { count - 1 } else { cur - 1 }
    } else if cur + 1 >= count {
        0
    } else {
        cur + 1
    };
    list_state.select(Some(next));
}

#[must_use]
pub const fn first_selection(count: usize) -> Option<usize> {
    if count == 0 { None } else { Some(0) }
}

#[must_use]
pub const fn clamp_selection(selected: Option<usize>, count: usize) -> Option<usize> {
    if count == 0 {
        None
    } else if let Some(selected) = selected {
        if selected >= count {
            Some(count - 1)
        } else {
            Some(selected)
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{clamp_selection, first_selection};

    #[test]
    fn first_selection_is_zero_only_when_nonempty() {
        assert_eq!(first_selection(0), None);
        assert_eq!(first_selection(3), Some(0));
    }

    #[test]
    fn clamp_selection_handles_empty_missing_and_past_end() {
        assert_eq!(clamp_selection(Some(2), 0), None);
        assert_eq!(clamp_selection(None, 3), None);
        assert_eq!(clamp_selection(Some(4), 3), Some(2));
        assert_eq!(clamp_selection(Some(1), 3), Some(1));
    }
}
