//! Host console widgets that are independent of root application state.

pub use jackin_tui::ModalOutcome;
pub use jackin_tui::theme::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

pub mod agent_choice;
pub mod github_picker;
pub mod mount_dst_choice;
pub mod scope_picker;
pub mod source_picker;
pub mod workdir_pick;

/// Wrap-around cursor move for any list-style picker. `delta` is `-1`
/// for Up, `+1` for Down. No-op when `count == 0`.
pub(crate) fn cycle_select(list_state: &mut tui_widget_list::ListState, count: usize, delta: i32) {
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
