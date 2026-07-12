//! Shared edit-save plan types for workspace editor and settings screens.
//!
//! Converges the dual save models (R-edit-model-convergence) onto one
//! plan → apply vocabulary so both surfaces describe pending mutations
//! the same way before calling into `ConfigEditor` / service ports.

use std::fmt;

/// Outcome of a save-key / save-action decision (shared by editor + settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditSaveDisposition {
    /// Persist pending edits immediately.
    SaveNow,
    /// Open discard / keep / cancel confirmation.
    ConfirmDiscard,
    /// No pending dirty state — no-op.
    Noop,
}

impl fmt::Display for EditSaveDisposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SaveNow => write!(f, "save_now"),
            Self::ConfirmDiscard => write!(f, "confirm_discard"),
            Self::Noop => write!(f, "noop"),
        }
    }
}

/// Pure save planner: given dirty + confirm-required flags, pick disposition.
///
/// Used by workspace-editor `save_key_plan` and settings save paths so the
/// branching policy lives in one place.
#[must_use]
pub const fn plan_edit_save(is_dirty: bool, needs_confirm_on_leave: bool) -> EditSaveDisposition {
    if !is_dirty {
        EditSaveDisposition::Noop
    } else if needs_confirm_on_leave {
        EditSaveDisposition::ConfirmDiscard
    } else {
        EditSaveDisposition::SaveNow
    }
}

/// Whether a planned save should open the shared save/discard/cancel modal.
#[must_use]
pub const fn save_opens_confirm_modal(plan: EditSaveDisposition) -> bool {
    matches!(plan, EditSaveDisposition::ConfirmDiscard)
}

/// Leave key (Esc / `q`): dirty → confirm discard; clean → no pending save work.
#[must_use]
pub const fn plan_leave_when_dirty(is_dirty: bool) -> EditSaveDisposition {
    plan_edit_save(is_dirty, true)
}

/// Explicit save key (`s`): dirty → save now; clean → noop.
#[must_use]
pub const fn plan_explicit_save(is_dirty: bool) -> EditSaveDisposition {
    plan_edit_save(is_dirty, false)
}

#[cfg(test)]
#[path = "edit_save/tests.rs"]
mod tests;
