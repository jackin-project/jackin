//! Debug-log naming helpers for the root console TUI.

use crate::console::{ConsoleStage, ConsoleState};
use jackin_console::tui::debug::ConsoleLocationDebug;

pub(crate) fn console_location_debug(console_state: &ConsoleState) -> String {
    let ConsoleStage::Manager(ms) = &console_state.stage;
    jackin_console::tui::debug::console_location_debug_name(&ConsoleLocationDebug {
        quit_confirm: console_state.quit_confirm.is_some(),
        stage: ms.stage.debug_stage(),
        list_modal: ms
            .list_modal
            .as_ref()
            .map(jackin_console::tui::debug::ConsoleModalDebugKind::modal_debug_kind),
    })
}
