//! Model state for the 1Password picker.
//!
//! The runner is intentionally absent from `OpPickerState`: the host crate
//! owns the runner and injects it at load-execution time (see the binary's
//! `console/tui/op_picker/load.rs`). Pending-load slots use `()` as the
//! runner type; the host layer swaps in the real runner when converting
//! `take_pending_request` output into an `OpPickerPendingLoad` for execution.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use jackin_tui::components::TextInputState;
use jackin_tui::runtime::BlockingSubscription;
use tui_widget_list::ListState;

use crate::{
    FieldDisplayRow, FieldLabelOrigin, OpLoadState, OpPickerAccount, OpPickerCache, OpPickerField,
    OpPickerFieldRef, OpPickerItem, OpPickerItemRef, OpPickerLoadRequest, OpPickerLoadResult,
    OpPickerMode, OpPickerPendingLoad, OpPickerStage, OpPickerVault, OpPickerVaultRef,
    build_op_picker_ref, field_display_rows_for_picker, filtered_accounts, filtered_fields,
    filtered_item_choices, filtered_items, filtered_vaults, naming_stage_input_for_stage,
    section_choices_from_references, selected_account_id,
};

/// Concrete load-result type for the op picker (all four payload variants
/// carry lists of the picker's own account/vault/item/field types).
pub type LoadResult =
    OpPickerLoadResult<OpPickerAccount, OpPickerVault, OpPickerItem, OpPickerField>;

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
    pub field_label_origin: FieldLabelOrigin,
    /// Set by the Field-stage `R` refresh before re-issuing the field
    /// load so the Fields-loaded arm rebuilds the field rows in place
    /// rather than bouncing back to the Section stage (Create mode). The
    /// initial item-selection load leaves it `false` and lands on Section
    /// as usual. Cleared the moment the refreshed fields arrive.
    pub field_refresh_in_place: bool,

    /// In-flight subscription waiting for a background load result.
    pub rx: Option<BlockingSubscription<LoadResult>>,
    /// Pending load request queued by state-mutation methods; the host
    /// crate's load-execution layer picks this up and starts the worker.
    /// Runner is `()` here — the host injects the real runner at execution
    /// time (see the binary's `op_picker/load.rs`).
    pub pending_load: Option<OpPickerPendingLoad<LoadResult, OpPickerLoadRequest, ()>>,
    /// Session-scoped cache shared with `ConsoleState`; the default
    /// constructor allocates a fresh empty one for unit tests.
    pub op_cache: Rc<RefCell<OpPickerCache>>,
}

// `rx` is not `Debug`; skipped fields are plumbing only.
#[allow(clippy::missing_fields_in_debug, reason = "documented residual allow; prefer expect when site is lint-true")]
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

    /// Build an `OpRef` from the picker's currently-selected vault/item/field.
    ///
    /// The `op` field uses UUID-form identifiers from the picker's pane
    /// selections. The `path` field uses human-readable names, with an
    /// inline `Item[subtitle]` annotation when the item shares its name
    /// with another item in the same vault.
    ///
    #[expect(
        clippy::expect_used,
        reason = "op ref commit is reachable only after vault and item selections exist"
    )]
    pub fn build_op_ref_on_commit(&self, field: &OpPickerField) -> jackin_core::OpRef {
        let vault = self
            .selected_vault
            .as_ref()
            .expect("vault must be selected before commit");
        let item = self
            .selected_item
            .as_ref()
            .expect("item must be selected before commit");

        let built = build_op_picker_ref(
            OpPickerVaultRef {
                id: &vault.id,
                name: &vault.name,
            },
            OpPickerItemRef {
                id: &item.id,
                name: &item.name,
                subtitle: &item.subtitle,
            },
            self.items.iter().map(|item| OpPickerItemRef {
                id: &item.id,
                name: &item.name,
                subtitle: &item.subtitle,
            }),
            OpPickerFieldRef {
                id: &field.id,
                label: &field.label,
                reference: &field.reference,
            },
            self.fields.iter().map(|field| OpPickerFieldRef {
                id: &field.id,
                label: &field.label,
                reference: &field.reference,
            }),
        );

        if built.empty_reference_with_sibling_refs {
            jackin_diagnostics::debug_log!(
                "op_picker",
                "empty field.reference for {}/{} (id {}); sibling fields have references — falling back to 3-segment URI",
                vault.name,
                item.name,
                field.id
            );
        }

        jackin_core::OpRef {
            op: built.op,
            path: built.path,
            account: self.selected_account_id(),
            on_demand: false,
        }
    }

    pub fn selected_account_id(&self) -> Option<String> {
        selected_account_id(self.selected_account.as_ref(), |account| {
            account.id.as_str()
        })
    }

    pub fn scroll_selection(&mut self, delta: i16) -> bool {
        match self.stage {
            OpPickerStage::Account => {
                let count = filtered_accounts(&self.filter_buf, &self.accounts).len();
                scroll_select(&mut self.account_list_state, count, delta)
            }
            OpPickerStage::Vault => {
                let count = filtered_vaults(&self.filter_buf, &self.vaults).len();
                scroll_select(&mut self.vault_list_state, count, delta)
            }
            OpPickerStage::Item => {
                let count = filtered_item_choices(&self.filter_buf, &self.items, &self.mode).len();
                scroll_select(&mut self.item_list_state, count, delta)
            }
            OpPickerStage::Section => {
                let count = self.section_choices().len() + 1;
                scroll_select(&mut self.section_list_state, count, delta)
            }
            OpPickerStage::Field => {
                let count = self.build_field_display_rows().len();
                scroll_select(&mut self.field_list_state, count, delta)
            }
            OpPickerStage::NewItemName
            | OpPickerStage::FieldLabel
            | OpPickerStage::NewSectionName => false,
        }
    }
}

#[must_use]
pub(crate) fn list_state_for_count(count: usize) -> ListState {
    let mut list_state = ListState::default();
    list_state.select(crate::first_selection(count));
    list_state
}

fn scroll_select(list_state: &mut ListState, count: usize, delta: i16) -> bool {
    if count == 0 {
        return false;
    }
    let cur = list_state.selected.unwrap_or(0).min(count - 1);
    let next = if delta.is_negative() {
        cur.saturating_sub(usize::from(delta.unsigned_abs()))
    } else {
        cur.saturating_add(usize::from(delta.unsigned_abs()))
            .min(count - 1)
    };
    list_state.select(Some(next));
    next != cur
}
