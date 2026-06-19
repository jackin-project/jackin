//! Concrete type aliases and constructors binding the generic console TUI to
//! the operator-facing application.

use std::cell::RefCell;
use std::rc::Rc;

use jackin_config::{AppConfig, LoadWorkspaceInput};
use jackin_core::RoleSelector;
use jackin_env::OpCache;

use crate::tui::app::{ConsoleApp, ConsoleAppStage};
use crate::tui::state::ManagerState;

pub type ConsoleStage = ConsoleAppStage<ManagerState<'static>>;
pub type ConsoleState = ConsoleApp<
    ManagerState<'static>,
    LoadWorkspaceInput,
    RoleSelector,
    Rc<RefCell<OpCache>>,
>;

pub fn new_console_state(
    config: &AppConfig,
    cwd: &std::path::Path,
) -> anyhow::Result<ConsoleState> {
    new_console_state_with_startup_error(config, cwd, false, None)
}

pub fn new_console_state_with_op_available(
    config: &AppConfig,
    cwd: &std::path::Path,
    op_available: bool,
) -> anyhow::Result<ConsoleState> {
    new_console_state_with_startup_error(config, cwd, op_available, None)
}

pub fn new_console_state_with_startup_error(
    config: &AppConfig,
    cwd: &std::path::Path,
    op_available: bool,
    startup_error: Option<(String, String)>,
) -> anyhow::Result<ConsoleState> {
    let op_cache = Rc::new(RefCell::new(OpCache::default()));
    let mut manager = ManagerState::from_config_with_cache_and_op(
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
    use jackin_config::AppConfig;

    use crate::tui::state::Modal;

    use super::*;

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

        let ConsoleStage::Manager(manager) = state.stage;
        let Some(Modal::ErrorPopup { state: popup }) = manager.list_modal else {
            panic!("startup Docker failure should open ErrorDialog");
        };
        assert_eq!(popup.title, "Docker daemon not reachable");
        assert_eq!(popup.message, "failed to connect to Docker daemon");
    }
}
