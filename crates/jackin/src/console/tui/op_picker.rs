//! 1Password vault/item/field picker modal.
//!
//! Drill-down `[Account →] Vault → Item → Field` reachable via `P`
//! from a Secrets row. The Account pane only appears for ≥2 signed-in
//! accounts. Selecting a field commits `OpField::reference` (the
//! `op://...` string `op` itself emits) verbatim — synthesizing the
//! path from display names mishandled sections, slashes, and
//! whitespace.
//!
//! The selected account is recorded on the committed `OpRef` (its `account`
//! field), and launch-time `op read` pins resolution to that account, so a
//! multi-account operator's reference resolves against the account it was
//! authored under rather than whichever happens to be the `op` default.
//!
//! Generic state, input handlers, and load-completion polling are now in
//! `jackin-console`. This module is the host-crate façade: it re-exports the
//! generic types, binds `OpPickerSelection` to the concrete `jackin-core`
//! types, and houses the test-runner thread-local so the binary's tests can
//! inject a `StubRunner` without `jackin-env` becoming a `jackin-console` dep.
//!
//! Behavioral invariants documented in the Developer Reference → Behavioral
//! Specs section of the docs site.

pub(crate) use jackin_console::tui::op_picker::{OpPickerSelection, OpPickerState};

// Used by tests.rs via `use super::*`.
#[cfg(test)]
use jackin_console::tui::components::op_picker::{
    FieldDisplayRow, FieldLabelOrigin, OpLoadState, OpPickerCache, OpPickerError,
    OpPickerFatalState, OpPickerStage,
};
#[cfg(test)]
use jackin_console::tui::op_picker::LoadResult;

// Test-runner thread-local: stores the injected runner so
// `execute_pending_load_for_test` can pass it to `start_load` without
// storing it in `OpPickerState` (which would create a circular dep via the
// `OpStructRunner` trait bound in `jackin-env`).
#[cfg(test)]
thread_local! {
    pub(super) static TEST_RUNNER: std::cell::RefCell<
        Option<std::sync::Arc<dyn jackin_env::OpStructRunner + Send + Sync>>,
    > = const { std::cell::RefCell::new(None) };
}

/// Test constructor helpers — these are free functions (not methods on `OpPickerState`)
/// because the orphan rule prevents adding inherent methods to a type from another crate.
/// Each sets the thread-local runner so `execute_pending_load_for_test` can pick it up.
#[cfg(test)]
pub(super) fn new_picker_with_runner(
    runner: std::sync::Arc<dyn jackin_env::OpStructRunner + Send + Sync>,
) -> OpPickerState {
    TEST_RUNNER.with(|r| *r.borrow_mut() = Some(runner));
    OpPickerState::new()
}

#[cfg(test)]
pub(super) fn new_picker_with_runner_and_cache(
    runner: std::sync::Arc<dyn jackin_env::OpStructRunner + Send + Sync>,
    op_cache: std::rc::Rc<std::cell::RefCell<OpPickerCache>>,
) -> OpPickerState {
    TEST_RUNNER.with(|r| *r.borrow_mut() = Some(runner));
    OpPickerState::new_with_cache(op_cache)
}

#[cfg(test)]
pub(super) fn new_create_picker_with_runner_and_cache(
    runner: std::sync::Arc<dyn jackin_env::OpStructRunner + Send + Sync>,
    op_cache: std::rc::Rc<std::cell::RefCell<OpPickerCache>>,
    item_name_default: impl Into<String>,
    field_label_default: impl Into<String>,
) -> OpPickerState {
    TEST_RUNNER.with(|r| *r.borrow_mut() = Some(runner));
    OpPickerState::new_create_with_cache(op_cache, item_name_default, field_label_default)
}

#[cfg(test)]
mod tests;
