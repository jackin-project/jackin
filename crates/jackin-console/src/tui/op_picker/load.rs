//! External load execution adapter for the 1Password picker.

use std::sync::Arc;

use jackin_env::OpStructRunner;
use jackin_tui::runtime::BlockingSubscription;

use super::model::{OpPickerLoadRequest, OpPickerLoadResult};
use super::{LoadResult, OpPickerState};

impl crate::tui::model::ConsoleAnimationTick for OpPickerState {
    fn tick_active_animation(&mut self) -> bool {
        self.tick()
    }
}

/// Return a ready or background subscription for a typed picker load request.
/// Caller supplies `runner`; the pending load from `take_pending_load` provides
/// `cached` and `request`.
#[allow(clippy::option_if_let_else, clippy::needless_pass_by_value)]
pub fn start_load(
    cached: Option<LoadResult>,
    request: OpPickerLoadRequest,
    runner: Arc<dyn OpStructRunner + Send + Sync>,
) -> BlockingSubscription<LoadResult> {
    match cached {
        Some(result) => jackin_tui::runtime::ready_blocking_subscription(result),
        None => jackin_tui::runtime::spawn_named_blocking_subscription(
            "jackin-op-picker-load",
            move || execute_load_request(runner, request),
        ),
    }
}

/// Execute one typed picker metadata request against the configured `op` runner.
#[allow(clippy::needless_pass_by_value)]
pub fn execute_load_request(
    runner: Arc<dyn OpStructRunner + Send + Sync>,
    request: OpPickerLoadRequest,
) -> LoadResult {
    match request {
        OpPickerLoadRequest::Accounts => OpPickerLoadResult::Accounts(runner.account_list()),
        OpPickerLoadRequest::Vaults { account_id } => {
            OpPickerLoadResult::Vaults(runner.vault_list(account_id.as_deref()))
        }
        OpPickerLoadRequest::Items {
            account_id,
            vault_id,
        } => OpPickerLoadResult::Items(runner.item_list(&vault_id, account_id.as_deref())),
        OpPickerLoadRequest::Fields {
            account_id,
            vault_id,
            item_id,
        } => {
            OpPickerLoadResult::Fields(runner.item_get(&item_id, &vault_id, account_id.as_deref()))
        }
    }
}
