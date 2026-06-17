//! Top-level console TUI application: event loop wiring and effect dispatch.
//!
//! Owns `ConsoleState` and `ConsoleStage` type aliases that bind the generic
//! `jackin-console` app to host-side concrete types. Not responsible for
//! terminal setup/teardown (`run.rs`) or individual prompt rendering
//! (`prompts.rs`).

use std::cell::RefCell;
use std::rc::Rc;

use crate::config::AppConfig;
use crate::operator_env::OpCache;
use crate::selector::RoleSelector;
use crate::workspace::LoadWorkspaceInput;

pub type ConsoleStage =
    jackin_console::tui::app::ConsoleAppStage<crate::console::tui::ManagerState<'static>>;

pub type ConsoleState = jackin_console::tui::app::ConsoleApp<
    crate::console::tui::ManagerState<'static>,
    LoadWorkspaceInput,
    RoleSelector,
    Rc<RefCell<OpCache>>,
>;

pub fn new_console_state(
    config: &AppConfig,
    cwd: &std::path::Path,
) -> anyhow::Result<ConsoleState> {
    new_console_state_with_op_available(config, cwd, false)
}

pub fn new_console_state_with_op_available(
    config: &AppConfig,
    cwd: &std::path::Path,
    op_available: bool,
) -> anyhow::Result<ConsoleState> {
    new_console_state_with_startup_error(config, cwd, op_available, None)
}

pub(crate) fn new_console_state_with_startup_error(
    config: &AppConfig,
    cwd: &std::path::Path,
    op_available: bool,
    startup_error: Option<(String, String)>,
) -> anyhow::Result<ConsoleState> {
    let op_cache = Rc::new(RefCell::new(OpCache::default()));
    let mut manager = crate::console::tui::ManagerState::from_config_with_cache_and_op(
        config,
        cwd,
        Rc::clone(&op_cache),
        op_available,
    );
    if let Some((title, message)) = startup_error {
        manager.open_list_error_popup(title, message);
    }
    Ok(ConsoleState::new(
        ConsoleStage::Manager(manager),
        op_cache,
        op_available,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::tui::state::Modal;

    #[test]
    fn startup_error_opens_list_error_dialog() {
        let config = AppConfig::default();
        let cwd = std::path::Path::new("/");
        let state = new_console_state_with_startup_error(
            &config,
            cwd,
            false,
            Some((
                "Docker daemon not reachable".into(),
                "failed to connect to Docker daemon".into(),
            )),
        )
        .expect("console state");

        let manager = match state.stage {
            ConsoleStage::Manager(manager) => manager,
        };
        let Some(Modal::ErrorPopup { state: popup }) = manager.list_modal else {
            panic!("startup Docker failure should open ErrorDialog");
        };
        assert_eq!(popup.title, "Docker daemon not reachable");
        assert_eq!(popup.message, "failed to connect to Docker daemon");
    }
}
