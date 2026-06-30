//! `UsageDialogTab` type (and eventually usage method family) extracted from
//! the dialog coordinator. Re-exported from parent for `super::*` in tests.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageDialogTab {
    Overview,
    Provider,
}
