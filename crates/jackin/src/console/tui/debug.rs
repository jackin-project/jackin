//! Debug-log naming helpers for the root console TUI.

use crate::console::{ConsoleStage, ConsoleState};
use jackin_console::tui::debug::{
    ConsoleLocationDebug, ConsoleStageDebug, console_location_debug_name,
};

pub(crate) fn console_location_debug(console_state: &ConsoleState) -> String {
    let ConsoleStage::Manager(ms) = &console_state.stage;
    let stage = match &ms.stage {
        crate::console::tui::state::ManagerStage::List => ConsoleStageDebug::List,
        crate::console::tui::state::ManagerStage::Editor(editor) => ConsoleStageDebug::Editor {
            mode: format!("{:?}", editor.mode),
            tab: format!("{:?}", editor.active_tab),
            field: format!("{:?}", editor.active_field),
            modal: editor
                .modal
                .as_ref()
                .map(jackin_console::tui::app::ConsoleModal::debug_kind),
        },
        crate::console::tui::state::ManagerStage::CreatePrelude(prelude) => {
            ConsoleStageDebug::CreatePrelude {
                step: format!("{:?}", prelude.step),
                modal: prelude
                    .modal
                    .as_ref()
                    .map(jackin_console::tui::app::ConsoleModal::debug_kind),
            }
        }
        crate::console::tui::state::ManagerStage::ConfirmDelete { .. } => {
            ConsoleStageDebug::ConfirmDelete
        }
        crate::console::tui::state::ManagerStage::ConfirmInstancePurge { .. } => {
            ConsoleStageDebug::ConfirmInstancePurge
        }
        crate::console::tui::state::ManagerStage::Settings(settings) => {
            ConsoleStageDebug::Settings {
                tab: format!("{:?}", settings.active_tab),
                selected: settings.mounts.selected,
                modal: settings.mounts.modal.as_ref().map(
                    jackin_console::tui::screens::settings::model::GlobalMountModal::debug_kind,
                ),
            }
        }
    };
    console_location_debug_name(&ConsoleLocationDebug {
        quit_confirm: console_state.quit_confirm.is_some(),
        stage,
        list_modal: ms
            .list_modal
            .as_ref()
            .map(jackin_console::tui::app::ConsoleModal::debug_kind),
    })
}
