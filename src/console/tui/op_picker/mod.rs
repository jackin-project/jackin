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
mod query;
pub mod render;
mod selection;
mod state;

pub(crate) use selection::build_op_ref_on_commit;
pub use state::OpPickerState;

pub use jackin_console::tui::components::op_picker::{
    FieldDisplayRow, FieldLabelOrigin, FieldStageBackPlan, NamingStagePlan, OpLoadState, OpPickerAccount, OpPickerCache,
    OpPickerError, OpPickerFatalState, OpPickerField, OpPickerFieldRef, OpPickerItem,
    OpPickerItemRef, OpPickerLoadRequest, OpPickerLoadResult, OpPickerMode, OpPickerStage,
    OpPickerVault, OpPickerVaultRef, SectionCollapseIntent, browse_field_display_rows, build_op_picker_ref,
    create_field_display_rows, field_display_rows_for_picker, filtered_accounts, filtered_fields,
    filter_reset_selection_for_stage, filtered_item_choices, filtered_items, filtered_vaults,
    field_label_cancel_plan, field_stage_back_plan, matches_filter, naming_stage_input_for_stage,
    new_item_name_commit_plan, new_section_name_commit_plan, section_choices_from_references,
    section_header_collapse_target,
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
