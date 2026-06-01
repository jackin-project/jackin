//! Model state for the 1Password picker.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
#[cfg(test)]
use std::sync::Arc;

use jackin_tui::components::TextInputState;
use jackin_tui::runtime::BlockingSubscription;
use ratatui::text::Line;
use tui_widget_list::ListState;

use super::{
    FieldDisplayRow, FieldLabelOrigin, LoadRequest, LoadResult, OpLoadState, OpPickerAccount,
    OpPickerField, OpPickerItem, OpPickerMode, OpPickerStage, OpPickerVault, OpCache,
};
use jackin_console::tui::components::op_picker::{
    OpPickerAccountRef, OpPickerFieldDisplayRef, OpPickerItemRef, OpPickerRenderState,
    OpPickerVaultRef, account_lines, field_display_rows_for_picker, field_lines,
    filtered_accounts, filtered_fields, filtered_item_choices, filtered_items, filtered_vaults,
    item_choice_lines, naming_stage_input_for_stage, section_choices_from_references,
    section_lines, selected_index_for_stage, vault_lines,
};
#[cfg(test)]
use crate::operator_env::OpStructRunner;

pub struct OpPickerState {
    pub stage: OpPickerStage,
    pub filter_buf: String,

    pub accounts: Vec<OpPickerAccount>,
    pub account_list_state: ListState,
    pub selected_account: Option<OpPickerAccount>,

    pub vaults: Vec<OpPickerVault>,
    pub vault_list_state: ListState,
    pub selected_vault: Option<OpPickerVault>,

    pub items: Vec<OpPickerItem>,
    pub item_list_state: ListState,
    pub selected_item: Option<OpPickerItem>,

    pub fields: Vec<OpPickerField>,
    pub field_list_state: ListState,
    pub section_list_state: ListState,
    /// The section chosen on the Section stage (Create mode), scoping the
    /// Field stage. `None` = the unsectioned `(root)` choice. Reset to
    /// `None` whenever a fresh item's fields load.
    pub selected_section: Option<String>,
    /// Section names currently collapsed in the field picker.
    /// Absent => expanded. Cleared whenever a fresh field list loads.
    pub collapsed_sections: HashSet<String>,

    pub load_state: OpLoadState,

    /// Browse vs. Create. Browse is the default for all existing callers.
    pub mode: OpPickerMode,
    /// New-item title input, driven during the `NewItemName` stage.
    pub item_name_input: TextInputState<'static>,
    /// Field-label input, driven during the `FieldLabel` stage.
    pub field_label_input: TextInputState<'static>,
    /// New-section name input, driven during the `NewSectionName` stage.
    pub section_name_input: TextInputState<'static>,
    /// Captured by the New-section flow, consumed when the final
    /// `OpPickerSelection` is built at commit.
    pub pending_section: Option<String>,
    /// The stage the `FieldLabel` sub-stage was entered from, so its Esc
    /// returns to the right origin (Create mode has three entry points).
    pub(super) field_label_origin: FieldLabelOrigin,
    /// Set by the Field-stage `R` refresh before re-issuing the field
    /// load so the Fields-loaded arm rebuilds the field rows in place
    /// rather than bouncing back to the Section stage (Create mode). The
    /// initial item-selection load leaves it `false` and lands on Section
    /// as usual. Cleared the moment the refreshed fields arrive.
    pub(super) field_refresh_in_place: bool,

    /// Test-only injected runner. Production chooses its service runner outside
    /// TUI state when executing the pending typed load request.
    #[cfg(test)]
    pub(super) runner: Arc<dyn OpStructRunner + Send + Sync>,
    pub(super) rx: Option<BlockingSubscription<LoadResult>>,
    pub(super) pending_load: Option<OpPickerPendingLoad>,
    /// Session-scoped cache shared with `ConsoleState`; the default
    /// constructor allocates a fresh empty one for unit tests.
    pub(super) op_cache: Rc<RefCell<OpCache>>,
}

pub(in crate::console) struct OpPickerPendingLoad {
    pub cached: Option<LoadResult>,
    pub request: LoadRequest,
    #[cfg(test)]
    pub runner: Arc<dyn OpStructRunner + Send + Sync>,
}

// rx and test runner aren't Debug; skipped fields are plumbing only.
#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for OpPickerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpPickerState")
            .field("stage", &self.stage)
            .field("filter_buf", &self.filter_buf)
            .field("accounts", &self.accounts)
            .field("selected_account", &self.selected_account)
            .field("vaults", &self.vaults)
            .field("selected_vault", &self.selected_vault)
            .field("items", &self.items)
            .field("selected_item", &self.selected_item)
            .field("fields", &self.fields)
            .field("selected_section", &self.selected_section)
            .field("collapsed_sections", &self.collapsed_sections)
            .field("load_state", &self.load_state)
            .field("mode", &self.mode)
            .field("pending_section", &self.pending_section)
            .finish_non_exhaustive()
    }
}

impl OpPickerState {
    pub fn filtered_accounts(&self) -> Vec<&OpPickerAccount> {
        filtered_accounts(&self.filter_buf, &self.accounts)
    }

    pub fn filtered_vaults(&self) -> Vec<&OpPickerVault> {
        filtered_vaults(&self.filter_buf, &self.vaults)
    }

    pub fn filtered_items(&self) -> Vec<&OpPickerItem> {
        filtered_items(&self.filter_buf, &self.items)
    }

    /// Filtered items, followed by a trailing `None` sentinel (the
    /// `+ New item` row) in Create mode. Browse mode emits no sentinel.
    pub fn filtered_item_choices(&self) -> Vec<Option<&OpPickerItem>> {
        filtered_item_choices(&self.filter_buf, &self.items, &self.mode)
    }

    pub fn filtered_fields(&self) -> Vec<&OpPickerField> {
        filtered_fields(&self.filter_buf, &self.fields)
    }

    /// Distinct sections present in the loaded fields, in first-appearance
    /// order, with a leading `None` (`(root)`) entry. Drives the Section
    /// stage list (Create mode). The render appends a `+ New section`
    /// sentinel after these choices.
    pub fn section_choices(&self) -> Vec<Option<String>> {
        section_choices_from_references(self.fields.iter().map(|field| field.reference.as_str()))
    }

    /// Build the ordered display rows for the field picker.
    ///
    /// Browse mode: unsectioned fields are emitted first; each named section
    /// follows with a collapsible `SectionHeader` row.
    ///
    /// Create mode: the Field stage is already scoped to `selected_section`,
    /// so rows are just that section's fields followed by `+ New field`.
    pub fn build_field_display_rows(&self) -> Vec<FieldDisplayRow> {
        field_display_rows_for_picker(
            &self.mode,
            &self.filter_buf,
            &self.fields,
            self.selected_section.as_deref(),
            &self.collapsed_sections,
        )
    }

    /// The input box for the current naming sub-stage, or `None` when the
    /// picker is in a list stage. Single source for render, sizing, and footer.
    pub const fn naming_stage_input(&self) -> Option<&TextInputState<'static>> {
        naming_stage_input_for_stage(
            self.stage,
            &self.item_name_input,
            &self.field_label_input,
            &self.section_name_input,
        )
    }
}

impl OpPickerRenderState for OpPickerState {
    fn stage(&self) -> OpPickerStage {
        self.stage
    }

    fn load_state(&self) -> &OpLoadState {
        &self.load_state
    }

    fn filter_buffer(&self) -> &str {
        &self.filter_buf
    }

    fn account_count(&self) -> usize {
        self.accounts.len()
    }

    fn selected_account_email(&self) -> &str {
        self.selected_account
            .as_ref()
            .map_or("", |account| account.email.as_str())
    }

    fn selected_vault_name(&self) -> &str {
        self.selected_vault
            .as_ref()
            .map_or("", |vault| vault.name.as_str())
    }

    fn selected_item_name(&self) -> &str {
        self.selected_item
            .as_ref()
            .map_or("", |item| item.name.as_str())
    }

    fn selected_item_subtitle(&self) -> &str {
        self.selected_item
            .as_ref()
            .map_or("", |item| item.subtitle.as_str())
    }

    fn naming_stage_input(&self) -> Option<&TextInputState<'static>> {
        Self::naming_stage_input(self)
    }

    fn account_lines(&self) -> Vec<Line<'static>> {
        account_lines(
            self.filtered_accounts()
                .into_iter()
                .map(|account| OpPickerAccountRef {
                    email: &account.email,
                    url: &account.url,
                }),
            self.account_list_state.selected,
        )
    }

    fn vault_lines(&self) -> Vec<Line<'static>> {
        vault_lines(
            self.filtered_vaults()
                .into_iter()
                .map(|vault| OpPickerVaultRef {
                    id: &vault.id,
                    name: &vault.name,
                }),
            self.vault_list_state.selected,
        )
    }

    fn item_lines(&self) -> Vec<Line<'static>> {
        item_choice_lines(
            self.filtered_item_choices().into_iter().map(|choice| {
                choice.map(|item| OpPickerItemRef {
                    id: &item.id,
                    name: &item.name,
                    subtitle: &item.subtitle,
                })
            }),
            self.item_list_state.selected,
        )
    }

    fn section_lines(&self) -> Vec<Line<'static>> {
        section_lines(self.section_choices(), self.section_list_state.selected)
    }

    fn field_lines(&self) -> Vec<Line<'static>> {
        field_lines(
            self.build_field_display_rows(),
            self.filtered_fields()
                .into_iter()
                .map(|field| OpPickerFieldDisplayRef {
                    id: &field.id,
                    label: &field.label,
                    field_type: &field.field_type,
                    concealed: field.concealed,
                }),
            &self.collapsed_sections,
            self.field_list_state.selected,
        )
    }

    fn selected_index(&self) -> Option<usize> {
        selected_index_for_stage(
            self.stage,
            self.account_list_state.selected,
            self.vault_list_state.selected,
            self.item_list_state.selected,
            self.section_list_state.selected,
            self.field_list_state.selected,
        )
    }
}
