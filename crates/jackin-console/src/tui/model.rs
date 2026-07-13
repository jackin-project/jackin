//! Top-level console TUI app model.

mod create_prelude;
mod launch_prompt;
mod modal;
mod stage;

pub use self::create_prelude::*;
pub use self::launch_prompt::*;
pub use self::modal::*;
pub use self::stage::*;

#[cfg(test)]
mod tests;

/// Single-variant today; kept as `enum` so future stages can land without
/// churning every match site.
#[derive(Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub enum ConsoleAppStage<Manager> {
    Manager(Manager),
}

#[derive(Debug)]
pub struct ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache> {
    pub stage: ConsoleAppStage<Manager>,
    /// Launch input is stored as a value, not as a selected row index, so each
    /// dispatch can rebuild its current workspace choice from current config.
    pub pending_launch: Option<LaunchInput>,
    pub pending_launch_role: Option<RoleSelector>,
    /// Process-lifetime op metadata cache shared by picker instances.
    pub op_cache: OpCache,
    /// Probed once at startup; mid-session installs require restart.
    pub op_available: bool,
    /// Overlay above any sub-stage.
    pub quit_confirm: Option<jackin_tui::components::ConfirmState>,
}

impl<Manager, LaunchInput, RoleSelector, OpCache>
    ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>
{
    pub fn new(stage: ConsoleAppStage<Manager>, op_cache: OpCache, op_available: bool) -> Self {
        Self {
            stage,
            pending_launch: None,
            pending_launch_role: None,
            op_cache,
            op_available,
            quit_confirm: None,
        }
    }

    #[must_use]
    pub fn quit_confirm_state(&self) -> Option<&jackin_tui::components::ConfirmState> {
        self.quit_confirm.as_ref()
    }

    #[must_use]
    pub fn quit_confirm_open(&self) -> bool {
        self.quit_confirm.is_some()
    }

    pub fn open_quit_confirm(&mut self) {
        self.quit_confirm = Some(crate::tui::run::quit_confirm_state());
    }

    pub fn dismiss_quit_confirm(&mut self) {
        self.quit_confirm = None;
    }

    pub fn handle_quit_confirm_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<crate::tui::run::QuitConfirmPlan> {
        let confirm = self.quit_confirm.as_mut()?;
        let plan = crate::tui::run::quit_confirm_plan(confirm.handle_key(key));
        if matches!(plan, crate::tui::run::QuitConfirmPlan::Dismiss) {
            self.dismiss_quit_confirm();
        }
        Some(plan)
    }
}

impl<Manager, LaunchInput, RoleSelector, OpCache>
    ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>
where
    Manager: ConsoleManagerModalBlockPresence,
{
    #[must_use]
    pub fn base_surface_unblocked(&self) -> bool {
        match &self.stage {
            ConsoleAppStage::Manager(manager) => {
                crate::tui::run::no_modal_blocks_base_surface(crate::tui::run::ModalBlockState {
                    quit_confirm: self.quit_confirm.is_some(),
                    list_modal: manager.list_modal_open(),
                    editor_modal: manager.editor_modal_open(),
                })
            }
        }
    }
}
