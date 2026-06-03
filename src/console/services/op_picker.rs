//! Non-TUI 1Password picker services.

use std::sync::Arc;

use jackin_console::tui::components::op_picker::{OpPickerLoadRequest, OpPickerLoadResult};
use jackin_tui::runtime::BlockingSubscription;

use crate::operator_env::{OpAccount, OpField, OpItem, OpStructRunner, OpVault};

pub(crate) type LoadResult = OpPickerLoadResult<OpAccount, OpVault, OpItem, OpField>;

/// Return a ready or background subscription for a typed picker load request.
#[allow(clippy::option_if_let_else, clippy::needless_pass_by_value)]
pub(crate) fn start_load(
    cached: Option<LoadResult>,
    request: OpPickerLoadRequest,
    runner: Arc<dyn OpStructRunner + Send + Sync>,
) -> BlockingSubscription<LoadResult> {
    match cached {
        Some(result) => ready_load(result),
        None => spawn_load(request, runner),
    }
}

/// Execute one typed picker metadata request against the configured `op` runner.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn execute_load_request(
    runner: Arc<dyn OpStructRunner + Send + Sync>,
    request: OpPickerLoadRequest,
) -> LoadResult {
    match request {
        OpPickerLoadRequest::Accounts => LoadResult::Accounts(runner.account_list()),
        OpPickerLoadRequest::Vaults { account_id } => {
            LoadResult::Vaults(runner.vault_list(account_id.as_deref()))
        }
        OpPickerLoadRequest::Items {
            account_id,
            vault_id,
        } => LoadResult::Items(runner.item_list(&vault_id, account_id.as_deref())),
        OpPickerLoadRequest::Fields {
            account_id,
            vault_id,
            item_id,
        } => LoadResult::Fields(runner.item_get(&item_id, &vault_id, account_id.as_deref())),
    }
}

/// Drop the cached item list and field list for the account/vault/item a freshly
/// minted op ref points at, so a reopened picker re-fetches the new entry.
pub(in crate::console) fn invalidate_cache_for_ref(
    op_cache: &std::rc::Rc<std::cell::RefCell<crate::operator_env::OpCache>>,
    op_ref: &crate::operator_env::OpRef,
) {
    let Some(parts) = crate::operator_env::parse_op_reference(&op_ref.op) else {
        return;
    };
    let account = op_ref.account.as_deref();
    let mut cache = op_cache.borrow_mut();
    cache.invalidate_items(account, &parts.vault);
    cache.invalidate_fields(account, &parts.vault, &parts.item);
}

fn ready_load(result: LoadResult) -> BlockingSubscription<LoadResult> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = tx.send(result);
    rx
}

fn spawn_load(
    request: OpPickerLoadRequest,
    runner: Arc<dyn OpStructRunner + Send + Sync>,
) -> BlockingSubscription<LoadResult> {
    jackin_tui::runtime::spawn_named_blocking_subscription("jackin-op-picker-load", move || {
        execute_load_request(runner, request)
    })
}
