use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditSaveDisposition {
    SaveNow,
    ConfirmDiscard,
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

#[must_use]
pub const fn save_opens_confirm_modal(plan: EditSaveDisposition) -> bool {
    matches!(plan, EditSaveDisposition::ConfirmDiscard)
}

#[must_use]
pub const fn plan_leave_when_dirty(is_dirty: bool) -> EditSaveDisposition {
    plan_edit_save(is_dirty, true)
}

#[must_use]
pub const fn plan_explicit_save(is_dirty: bool) -> EditSaveDisposition {
    plan_edit_save(is_dirty, false)
}
