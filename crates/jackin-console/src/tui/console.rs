//! Concrete type aliases and constructors binding the generic console TUI to
//! the operator-facing application.

use std::cell::RefCell;
use std::rc::Rc;

use jackin_config::{AppConfig, LoadWorkspaceInput};
use jackin_core::RoleSelector;
use jackin_env::OpCache;

use crate::tui::model::{ConsoleApp, ConsoleAppStage};
use crate::tui::state::ManagerState;

pub type ConsoleStage = ConsoleAppStage<ManagerState<'static>>;
pub type ConsoleState =
    ConsoleApp<ManagerState<'static>, LoadWorkspaceInput, RoleSelector, Rc<RefCell<OpCache>>>;
pub type ConsoleOutcome = crate::tui::message::ConsoleOutcome<
    RoleSelector,
    jackin_config::ResolvedWorkspace,
    jackin_core::Agent,
    jackin_protocol::Provider,
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
mod tests;
