//! 1Password vault/item/field picker modal — generic state, input handlers,
//! and load-execution helpers.

pub mod input;
pub mod load;
pub mod state;

pub use load::{execute_load_request, start_load};
pub use state::{LoadResult, OpPickerState};

pub fn cli_available() -> bool {
    use jackin_env::OpRunner as _;
    jackin_env::OpCli::new_probe().probe().is_ok()
}

pub fn start_ref_validation(
    op_ref: jackin_core::OpRef,
) -> jackin_tui::runtime::BlockingSubscription<anyhow::Result<()>> {
    use jackin_env::OpRunner as _;
    let runner = jackin_env::OpCli::new().with_account(op_ref.account.clone());
    let op = op_ref.op;
    jackin_tui::runtime::spawn_blocking_subscription(move || runner.read(&op).map(|_| ()))
}

/// Concrete selection type for the picker: all five type parameters are bound
/// to `jackin-core` types already available in this crate.
pub type OpPickerSelection = crate::tui::components::op_picker::OpPickerSelection<
    jackin_core::OpRef,
    jackin_core::op_types::OpAccount,
    jackin_core::op_types::OpVault,
    jackin_core::op_types::OpItem,
    jackin_core::FieldTarget,
>;

// Test-runner thread-local: stores the injected runner so
// `execute_pending_load_for_test` in tests can pass it to `start_load` without
// storing it in `OpPickerState` (which would require propagating the
// `OpStructRunner` trait bound through state types).
#[cfg(test)]
thread_local! {
    pub(crate) static TEST_RUNNER: std::cell::RefCell<
        Option<std::sync::Arc<dyn jackin_env::OpStructRunner + Send + Sync>>,
    > = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn new_picker_with_runner(
    runner: std::sync::Arc<dyn jackin_env::OpStructRunner + Send + Sync>,
) -> OpPickerState {
    TEST_RUNNER.with(|r| *r.borrow_mut() = Some(runner));
    OpPickerState::new()
}

#[cfg(test)]
pub(crate) fn new_picker_with_runner_and_cache(
    runner: std::sync::Arc<dyn jackin_env::OpStructRunner + Send + Sync>,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::tui::components::op_picker::OpPickerCache>>,
) -> OpPickerState {
    TEST_RUNNER.with(|r| *r.borrow_mut() = Some(runner));
    OpPickerState::new_with_cache(op_cache)
}

#[cfg(test)]
pub(crate) fn new_create_picker_with_runner_and_cache(
    runner: std::sync::Arc<dyn jackin_env::OpStructRunner + Send + Sync>,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::tui::components::op_picker::OpPickerCache>>,
    item_name_default: impl Into<String>,
    field_label_default: impl Into<String>,
) -> OpPickerState {
    TEST_RUNNER.with(|r| *r.borrow_mut() = Some(runner));
    OpPickerState::new_create_with_cache(op_cache, item_name_default, field_label_default)
}

// Re-exported into test scope via `use super::*` from tests.rs.
#[cfg(test)]
use crate::tui::components::op_picker::{
    FieldDisplayRow, FieldLabelOrigin, OpLoadState, OpPickerError, OpPickerFatalState,
    OpPickerStage,
};

#[cfg(test)]
mod tests;
