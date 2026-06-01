use std::cell::RefCell;
use std::rc::Rc;

use crate::config::AppConfig;
use crate::operator_env::OpCache;
use crate::selector::RoleSelector;
use crate::workspace::LoadWorkspaceInput;

/// Single-variant today; kept as `enum` so future stages (e.g.
/// running-sessions cluster) land without churning every match site.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ConsoleStage {
    Manager(crate::console::manager::ManagerState<'static>),
}

#[derive(Debug)]
pub struct ConsoleState {
    pub stage: ConsoleStage,
    /// `LoadWorkspaceInput` (not an index) so each dispatch rebuilds
    /// its `WorkspaceChoice` from current config — manager edits flow
    /// through immediately.
    pub pending_launch: Option<LoadWorkspaceInput>,
    pub pending_launch_role: Option<RoleSelector>,
    /// Process-lifetime `op` metadata cache. `Rc<RefCell<_>>` because
    /// the TUI event loop is single-threaded.
    pub op_cache: Rc<RefCell<OpCache>>,
    /// Probed once at startup; mid-session installs require restart.
    /// Re-probing on every modal open would add a perceptible hitch.
    pub op_available: bool,
    /// Lifted to `ConsoleState` (not `ManagerState`) so it overlays
    /// any sub-stage uniformly.
    pub quit_confirm: Option<jackin_tui::components::ConfirmState>,
}

impl ConsoleState {
    pub fn new(config: &AppConfig, cwd: &std::path::Path) -> anyhow::Result<Self> {
        Self::new_with_op_available(config, cwd, false)
    }

    pub fn new_with_op_available(
        config: &AppConfig,
        cwd: &std::path::Path,
        op_available: bool,
    ) -> anyhow::Result<Self> {
        let op_cache = Rc::new(RefCell::new(OpCache::default()));
        Ok(Self {
            stage: ConsoleStage::Manager(
                crate::console::manager::ManagerState::from_config_with_cache_and_op(
                    config,
                    cwd,
                    op_cache.clone(),
                    op_available,
                ),
            ),
            pending_launch: None,
            pending_launch_role: None,
            op_cache,
            op_available,
            quit_confirm: None,
        })
    }
}
