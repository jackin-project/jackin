// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

/// Drop cached item list and field list for the account/vault/item a freshly
/// minted op ref points at, so a reopened picker re-fetches the new entry.
pub fn invalidate_cache_for_ref(
    op_cache: &std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
    op_ref: &jackin_core::OpRef,
) {
    let Some(parts) = jackin_core::op_reference::parse_op_reference(&op_ref.op) else {
        return;
    };
    let account = op_ref.account.as_deref();
    let mut cache = op_cache.borrow_mut();
    cache.invalidate_items(account, &parts.vault);
    cache.invalidate_fields(account, &parts.vault, &parts.item);
}

/// Poll all active op-picker loads across every active manager stage and list
/// modal. Starts pending loads and routes completed ones into picker state.
///
/// Returns `true` if any picker state changed (caller should redraw).
pub fn poll_picker_loads(state: &mut crate::tui::state::ManagerState<'_>) -> bool {
    use crate::tui::state::{ManagerStage, Modal, SettingsAuthModal, SettingsEnvModal};
    let mut dirty = false;
    if let Some(Modal::OpPicker { state }) = state.list_modal.as_mut() {
        dirty |= poll_op_picker_load(state);
    }
    if let ManagerStage::Editor(editor) = &mut state.stage
        && let Some(Modal::OpPicker { state }) = editor.modal.as_mut()
    {
        dirty |= poll_op_picker_load(state);
    }
    if let ManagerStage::Settings(settings) = &mut state.stage
        && let Some(SettingsEnvModal::OpPicker { state }) = settings.env.modal.as_mut()
    {
        dirty |= poll_op_picker_load(state);
    }
    if let ManagerStage::Settings(settings) = &mut state.stage
        && let Some(SettingsAuthModal::OpPicker { state }) = settings.auth.modal_mut()
    {
        dirty |= poll_op_picker_load(state);
    }
    dirty
}

fn poll_op_picker_load(state: &mut OpPickerState) -> bool {
    let mut dirty = execute_op_picker_pending_load(state);
    dirty |= state.poll_load();
    dirty |= execute_op_picker_pending_load(state);
    dirty
}

fn execute_op_picker_pending_load(state: &mut OpPickerState) -> bool {
    let Some(pending) = state.take_pending_load() else {
        return false;
    };
    let rx = start_load(
        pending.cached,
        pending.request,
        jackin_env::default_op_struct_runner(),
    );
    state.attach_load_receiver(rx);
    true
}

// Re-exported into test scope via `use super::*` from tests.rs.
#[cfg(test)]
use crate::tui::components::op_picker::{
    FieldDisplayRow, FieldLabelOrigin, OpLoadState, OpPickerError, OpPickerFatalState,
    OpPickerStage,
};

#[cfg(test)]
mod tests;
