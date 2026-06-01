//! Non-TUI 1Password picker services.

use std::sync::Arc;

use jackin_console::tui::components::op_picker::{
    OpPickerLoadRequest, OpPickerLoadResult,
};

use crate::operator_env::{OpAccount, OpField, OpItem, OpStructRunner, OpVault};

pub type LoadResult = OpPickerLoadResult<OpAccount, OpVault, OpItem, OpField>;

/// Execute one typed picker metadata request against the configured `op` runner.
pub fn execute_load_request(
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
