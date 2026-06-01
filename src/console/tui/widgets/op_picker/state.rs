//! Model state for the 1Password picker.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use jackin_tui::components::TextInputState;
use jackin_tui::runtime::BlockingSubscription;
use tui_widget_list::ListState;

use super::{FieldLabelOrigin, LoadRequest, LoadResult, OpLoadState, OpPickerMode, OpPickerStage};
use crate::operator_env::{OpAccount, OpCache, OpField, OpItem, OpStructRunner, OpVault};

pub struct OpPickerState {
    pub stage: OpPickerStage,
    pub filter_buf: String,

    pub accounts: Vec<OpAccount>,
    pub account_list_state: ListState,
    pub selected_account: Option<OpAccount>,

    pub vaults: Vec<OpVault>,
    pub vault_list_state: ListState,
    pub selected_vault: Option<OpVault>,

    pub items: Vec<OpItem>,
    pub item_list_state: ListState,
    pub selected_item: Option<OpItem>,

    pub fields: Vec<OpField>,
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

    /// `Arc` so spawned worker threads share the same trait object
    /// (test injectees included).
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
    pub runner: Arc<dyn OpStructRunner + Send + Sync>,
}

// runner / rx aren't Debug; skipped fields are plumbing only.
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
