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
//! `OpStructRunner` calls run through blocking workers, results routed
//! through one-shot subscriptions; the spinner ticks until the receiver
//! yields. Probe / vault-list failures fork into four fatal panels
//! (not installed, not signed in, no vaults, generic).

mod input;
mod load;
mod state;

pub use state::OpPickerState;

pub use jackin_console::tui::components::op_picker::{
    AccountStageCommitPlan, AccountStageRefreshPlan, ExistingFieldCommitPlan,
    FieldDisplayRow, FieldLabelCommitPlan, FieldLabelOrigin, FieldStageBackPlan,
    FieldStageCommitPlan, FieldStageRefreshPlan, ItemStageBackPlan, ItemStageCommitPlan,
    ItemStageRefreshPlan, NamingStagePlan, OpLoadState, OpPickerAccount, OpPickerCache,
    OpPickerError, OpPickerFatalState, OpPickerField, OpPickerFieldRef, OpPickerItem,
    OpPickerItemRef, OpPickerLoadRequest, OpPickerLoadResult, OpPickerMode,
    OpPickerPendingLoad as GenericOpPickerPendingLoad, OpPickerStage, OpPickerVault,
    OpPickerVaultRef, SectionCollapseIntent, SectionStageBackPlan,
    SectionStageCommitPlan, VaultStageBackPlan, VaultStageCommitPlan, VaultStageRefreshPlan,
    account_stage_commit_plan, account_stage_refresh_plan, browse_field_display_rows,
    build_op_picker_ref, create_field_display_rows, existing_field_commit_plan,
    field_display_rows_for_picker, field_label_cancel_plan, field_label_commit_plan,
    field_stage_back_plan, field_stage_commit_plan, field_stage_refresh_plan,
    filter_reset_selection_for_stage, filtered_accounts, filtered_fields, filtered_item_choices,
    filtered_items, filtered_vaults, item_stage_back_plan, item_stage_commit_plan,
    item_stage_refresh_plan, matches_filter, naming_stage_input_for_stage,
    new_item_name_commit_plan, new_section_name_commit_plan, section_choices_from_references,
    section_header_collapse_target, section_stage_back_plan, section_stage_commit_plan,
    vault_stage_back_plan, vault_stage_commit_plan, vault_stage_refresh_plan,
};

pub type OpPickerSelection = jackin_console::tui::components::op_picker::OpPickerSelection<
    crate::operator_env::OpRef,
    crate::operator_env::OpAccount,
    crate::operator_env::OpVault,
    crate::operator_env::OpItem,
    crate::operator_env::FieldTarget,
>;

type LoadResult = OpPickerLoadResult<OpPickerAccount, OpPickerVault, OpPickerItem, OpPickerField>;

type LoadRequest = OpPickerLoadRequest;

#[cfg(test)]
type PendingLoadRunner = std::sync::Arc<dyn crate::operator_env::OpStructRunner + Send + Sync>;
#[cfg(not(test))]
type PendingLoadRunner = ();

pub(in crate::console) type OpPickerPendingLoad =
    GenericOpPickerPendingLoad<LoadResult, LoadRequest, PendingLoadRunner>;

pub type OpAccount = OpPickerAccount;
pub type OpVault = OpPickerVault;
pub type OpItem = OpPickerItem;
pub type OpField = OpPickerField;
pub type OpCache = OpPickerCache;

impl Default for OpPickerState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
