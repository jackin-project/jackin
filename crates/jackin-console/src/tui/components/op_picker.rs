//! Shared 1Password picker modal state enums.

use std::collections::HashSet;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub use crate::tui::components::list_helpers::matches_filter;
use crate::tui::components::list_helpers::first_selection;
use crate::tui::components::spinner::SPINNER_FRAMES;
use jackin_tui::components::scrollable_panel::render_selected_lines_in_area;
use jackin_tui::components::{Panel, PanelFocus, TextInputState};
use jackin_tui::theme::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

pub fn item_name_input_state<'a>(
    item_default: impl Into<String>,
) -> TextInputState<'a> {
    TextInputState::new("Item name", item_default)
}

pub fn field_label_input_state<'a>(
    field_default: impl Into<String>,
) -> TextInputState<'a> {
    TextInputState::new("Field label", field_default)
}

pub fn section_name_input_state<'a>(
    initial: impl Into<String>,
) -> TextInputState<'a> {
    TextInputState::new("Section name", initial)
}

/// Browse-only vs. creation-enabled picker mode.
#[derive(Debug, Clone)]
pub enum OpPickerMode {
    /// Pick an existing field only.
    Browse,
    /// Enable item/field/section creation rows and naming sub-stages.
    Create {
        item_name_default: String,
        field_label_default: String,
    },
}

impl OpPickerMode {
    pub const fn is_create(&self) -> bool {
        matches!(self, Self::Create { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpPickerStage {
    Account,
    Vault,
    Item,
    Section,
    Field,
    NewItemName,
    FieldLabel,
    NewSectionName,
}

impl OpPickerStage {
    pub const fn is_naming(self) -> bool {
        matches!(
            self,
            Self::NewItemName | Self::FieldLabel | Self::NewSectionName
        )
    }

    pub const fn is_filterable(self) -> bool {
        matches!(self, Self::Account | Self::Vault | Self::Item | Self::Field)
    }
}

/// Which creation path entered the field-label sub-stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldLabelOrigin {
    NewItem,
    NewField,
    NewSection,
}

impl FieldLabelOrigin {
    pub const fn cancel_stage(self) -> OpPickerStage {
        match self {
            Self::NewItem => OpPickerStage::NewItemName,
            Self::NewField => OpPickerStage::Field,
            Self::NewSection => OpPickerStage::NewSectionName,
        }
    }
}

#[derive(Debug, Clone)]
pub enum OpLoadState {
    Idle,
    Loading { spinner_tick: u8 },
    Ready,
    Error(OpPickerError),
}

#[derive(Debug, Clone)]
pub enum OpPickerError {
    Fatal(OpPickerFatalState),
    Recoverable { message: String },
}

pub fn background_worker_disconnected_error_message() -> &'static str {
    "background worker disconnected"
}

#[derive(Debug, Clone)]
pub enum OpPickerFatalState {
    NotInstalled,
    NotSignedIn,
    NoVaults,
    GenericFatal { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountsLoadedPlan {
    NotSignedIn,
    SelectSingleAccount,
    ShowAccountPane,
}

pub const fn accounts_loaded_plan(account_count: usize) -> AccountsLoadedPlan {
    match account_count {
        0 => AccountsLoadedPlan::NotSignedIn,
        1 => AccountsLoadedPlan::SelectSingleAccount,
        _ => AccountsLoadedPlan::ShowAccountPane,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultsLoadedPlan {
    NoVaults,
    ShowVaultPane { selected: Option<usize> },
}

pub const fn vaults_loaded_plan(vault_count: usize) -> VaultsLoadedPlan {
    if vault_count == 0 {
        VaultsLoadedPlan::NoVaults
    } else {
        VaultsLoadedPlan::ShowVaultPane {
            selected: first_selection(vault_count),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemsLoadedPlan {
    pub selected: Option<usize>,
}

pub const fn items_loaded_plan(item_count: usize) -> ItemsLoadedPlan {
    ItemsLoadedPlan {
        selected: first_selection(item_count),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldsLoadedPlan {
    RefreshFieldPane {
        field_selected: Option<usize>,
        clear_refresh_in_place: bool,
    },
    ShowSectionPane {
        stage: OpPickerStage,
        section_selected: Option<usize>,
        clear_selected_section: bool,
    },
    ShowFieldPane {
        field_selected: Option<usize>,
        clear_selected_section: bool,
    },
}

pub const fn fields_loaded_plan(
    mode: &OpPickerMode,
    refresh_in_place: bool,
    section_choice_count: usize,
    field_display_count: usize,
) -> FieldsLoadedPlan {
    if refresh_in_place {
        return FieldsLoadedPlan::RefreshFieldPane {
            field_selected: first_selection(field_display_count),
            clear_refresh_in_place: true,
        };
    }
    if mode.is_create() {
        return FieldsLoadedPlan::ShowSectionPane {
            stage: OpPickerStage::Section,
            section_selected: first_selection(section_choice_count + 1),
            clear_selected_section: true,
        };
    }
    FieldsLoadedPlan::ShowFieldPane {
        field_selected: first_selection(field_display_count),
        clear_selected_section: true,
    }
}

pub fn sort_fields_by_concealed_first<Field>(
    fields: &mut [Field],
    mut concealed: impl FnMut(&Field) -> bool,
) {
    fields.sort_by_key(|field| !concealed(field));
}

/// Background load completion routed back into the picker.
#[derive(Debug)]
pub enum OpPickerLoadResult<Account, Vault, Item, Field> {
    Accounts(anyhow::Result<Vec<Account>>),
    Vaults(anyhow::Result<Vec<Vault>>),
    Items(anyhow::Result<Vec<Item>>),
    Fields(anyhow::Result<Vec<Field>>),
}

/// Typed request for external 1Password metadata loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpPickerLoadRequest {
    Accounts,
    Vaults {
        account_id: Option<String>,
    },
    Items {
        account_id: Option<String>,
        vault_id: String,
    },
    Fields {
        account_id: Option<String>,
        vault_id: String,
        item_id: String,
    },
}

/// Typed pending load request emitted by picker state and executed by the
/// owning non-TUI service adapter.
#[derive(Debug)]
pub struct OpPickerPendingLoad<LoadResult, LoadRequest, Runner> {
    pub cached: Option<LoadResult>,
    pub request: LoadRequest,
    pub runner: Runner,
}

/// What the operator chose when the picker commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpPickerSelection<Reference, Account, Vault, Item, FieldTarget> {
    /// An existing field was chosen.
    Existing(Reference),
    /// Create a brand-new item in the vault.
    NewItem {
        account: Option<Account>,
        vault: Vault,
        item_name: String,
        section: Option<String>,
        field_label: String,
    },
    /// Write/append a field in an existing item.
    EditItemField {
        account: Option<Account>,
        vault: Vault,
        item: Item,
        section: Option<String>,
        field: FieldTarget,
    },
}

/// 1Password account metadata displayed by the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpPickerAccount {
    pub id: String,
    pub email: String,
    pub url: String,
}

/// 1Password vault metadata displayed by the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpPickerVault {
    pub id: String,
    pub name: String,
}

/// 1Password item metadata displayed by the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpPickerItem {
    pub id: String,
    pub name: String,
    pub subtitle: String,
}

/// 1Password field metadata displayed by the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpPickerField {
    pub id: String,
    pub label: String,
    pub field_type: String,
    pub concealed: bool,
    pub reference: String,
}

/// Session-scoped metadata cache for picker drill-down panes.
pub type OpPickerCache =
    crate::op_cache::OpCache<OpPickerAccount, OpPickerVault, OpPickerItem, OpPickerField>;

/// A single row in the field-picker display list.
#[derive(Debug, Clone)]
pub enum FieldDisplayRow {
    /// A collapsible section header derived from the `op://` reference.
    SectionHeader { name: String, field_count: usize },
    /// A selectable field row. The index points into the filtered fields.
    Field { field_idx: usize },
    /// `+ New field` creation row.
    NewFieldSentinel,
    /// `+ New section` creation row.
    NewSectionSentinel,
}

#[derive(Debug, Clone, Copy)]
pub struct OpPickerAccountRef<'a> {
    pub email: &'a str,
    pub url: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct OpPickerVaultRef<'a> {
    pub id: &'a str,
    pub name: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct OpPickerItemRef<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub subtitle: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct OpPickerFieldRef<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub reference: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct OpPickerFieldDisplayRef<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub field_type: &'a str,
    pub concealed: bool,
}

pub trait OpPickerRenderState {
    fn stage(&self) -> OpPickerStage;
    fn load_state(&self) -> &OpLoadState;
    fn filter_buffer(&self) -> &str;
    fn account_count(&self) -> usize;
    fn selected_account_email(&self) -> &str;
    fn selected_vault_name(&self) -> &str;
    fn selected_item_name(&self) -> &str;
    fn selected_item_subtitle(&self) -> &str;
    fn naming_stage_input(&self) -> Option<&TextInputState<'static>>;
    fn account_lines(&self) -> Vec<Line<'static>>;
    fn vault_lines(&self) -> Vec<Line<'static>>;
    fn item_lines(&self) -> Vec<Line<'static>>;
    fn section_lines(&self) -> Vec<Line<'static>>;
    fn field_lines(&self) -> Vec<Line<'static>>;
    fn selected_index(&self) -> Option<usize>;
}

pub const fn selected_index_for_stage(
    stage: OpPickerStage,
    account: Option<usize>,
    vault: Option<usize>,
    item: Option<usize>,
    section: Option<usize>,
    field: Option<usize>,
) -> Option<usize> {
    match stage {
        OpPickerStage::Account => account,
        OpPickerStage::Vault => vault,
        OpPickerStage::Item => item,
        OpPickerStage::Section => section,
        OpPickerStage::Field => field,
        OpPickerStage::NewItemName
        | OpPickerStage::FieldLabel
        | OpPickerStage::NewSectionName => None,
    }
}

pub const fn naming_stage_input_for_stage<'a>(
    stage: OpPickerStage,
    item_name: &'a TextInputState<'static>,
    field_label: &'a TextInputState<'static>,
    section_name: &'a TextInputState<'static>,
) -> Option<&'a TextInputState<'static>> {
    match stage {
        OpPickerStage::NewItemName => Some(item_name),
        OpPickerStage::FieldLabel => Some(field_label),
        OpPickerStage::NewSectionName => Some(section_name),
        _ => None,
    }
}

pub const fn filter_reset_selection_for_stage(
    stage: OpPickerStage,
    account_count: usize,
    vault_count: usize,
    item_count: usize,
    field_count: usize,
) -> Option<Option<usize>> {
    match stage {
        OpPickerStage::Account => Some(first_selection(account_count)),
        OpPickerStage::Vault => Some(first_selection(vault_count)),
        OpPickerStage::Item => Some(first_selection(item_count)),
        OpPickerStage::Field => Some(first_selection(field_count)),
        OpPickerStage::Section
        | OpPickerStage::NewItemName
        | OpPickerStage::FieldLabel
        | OpPickerStage::NewSectionName => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldStageBackPlan {
    pub stage: OpPickerStage,
    pub reset_selected_section: bool,
    pub clear_fields: bool,
    pub clear_collapsed_sections: bool,
    pub clear_selected_item: bool,
    pub reset_section_list: bool,
}

pub const fn field_stage_back_plan(mode: &OpPickerMode) -> FieldStageBackPlan {
    if mode.is_create() {
        FieldStageBackPlan {
            stage: OpPickerStage::Section,
            reset_selected_section: true,
            clear_fields: false,
            clear_collapsed_sections: false,
            clear_selected_item: false,
            reset_section_list: true,
        }
    } else {
        FieldStageBackPlan {
            stage: OpPickerStage::Item,
            reset_selected_section: false,
            clear_fields: true,
            clear_collapsed_sections: true,
            clear_selected_item: true,
            reset_section_list: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldStageRefreshPlan {
    pub clear_fields: bool,
    pub reset_field_list: bool,
    pub clear_collapsed_sections: bool,
    pub refresh_in_place: bool,
}

pub const fn field_stage_refresh_plan(mode: &OpPickerMode) -> FieldStageRefreshPlan {
    FieldStageRefreshPlan {
        clear_fields: true,
        reset_field_list: true,
        clear_collapsed_sections: true,
        refresh_in_place: mode.is_create(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionStageBackPlan {
    pub stage: OpPickerStage,
    pub clear_fields: bool,
    pub clear_collapsed_sections: bool,
    pub clear_selected_section: bool,
    pub clear_selected_item: bool,
}

pub const fn section_stage_back_plan() -> SectionStageBackPlan {
    SectionStageBackPlan {
        stage: OpPickerStage::Item,
        clear_fields: true,
        clear_collapsed_sections: true,
        clear_selected_section: true,
        clear_selected_item: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionStageCommitPlan {
    NewSectionName,
    ExistingSection { selected_section: Option<String> },
    NoSelection,
}

pub fn section_stage_commit_plan(
    selected: Option<usize>,
    choices: &[Option<String>],
) -> SectionStageCommitPlan {
    let selected = selected.unwrap_or(0);
    if selected == choices.len() {
        return SectionStageCommitPlan::NewSectionName;
    }
    choices
        .get(selected)
        .cloned()
        .map(|selected_section| SectionStageCommitPlan::ExistingSection { selected_section })
        .unwrap_or(SectionStageCommitPlan::NoSelection)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemStageBackPlan {
    pub stage: OpPickerStage,
    pub clear_items: bool,
    pub clear_selected_item: bool,
}

pub const fn item_stage_back_plan() -> ItemStageBackPlan {
    ItemStageBackPlan {
        stage: OpPickerStage::Vault,
        clear_items: true,
        clear_selected_item: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemStageCommitPlan<Item> {
    ExistingItem(Item),
    NewItemName,
    NoSelection,
}

pub fn item_stage_commit_plan<Item>(picked: Option<Option<Item>>) -> ItemStageCommitPlan<Item> {
    match picked {
        Some(Some(item)) => ItemStageCommitPlan::ExistingItem(item),
        Some(None) => ItemStageCommitPlan::NewItemName,
        None => ItemStageCommitPlan::NoSelection,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemStageRefreshPlan {
    pub clear_items: bool,
    pub reset_item_list: bool,
}

pub const fn item_stage_refresh_plan() -> ItemStageRefreshPlan {
    ItemStageRefreshPlan {
        clear_items: true,
        reset_item_list: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultStageBackPlan {
    BackToAccount {
        stage: OpPickerStage,
        clear_selected_vault: bool,
        clear_vaults: bool,
        reset_vault_list: bool,
        ready_load_state: bool,
    },
    Cancel,
}

pub const fn vault_stage_back_plan(account_count: usize) -> VaultStageBackPlan {
    if account_count > 1 {
        VaultStageBackPlan::BackToAccount {
            stage: OpPickerStage::Account,
            clear_selected_vault: true,
            clear_vaults: true,
            reset_vault_list: true,
            ready_load_state: true,
        }
    } else {
        VaultStageBackPlan::Cancel
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultStageCommitPlan<Vault> {
    ExistingVault(Vault),
    NoSelection,
}

pub fn vault_stage_commit_plan<Vault>(picked: Option<Vault>) -> VaultStageCommitPlan<Vault> {
    picked
        .map(VaultStageCommitPlan::ExistingVault)
        .unwrap_or(VaultStageCommitPlan::NoSelection)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultStageRefreshPlan {
    pub clear_vaults: bool,
    pub reset_vault_list: bool,
    pub clear_selected_vault: bool,
}

pub const fn vault_stage_refresh_plan() -> VaultStageRefreshPlan {
    VaultStageRefreshPlan {
        clear_vaults: true,
        reset_vault_list: true,
        clear_selected_vault: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountStageRefreshPlan {
    pub clear_accounts: bool,
    pub reset_account_list: bool,
    pub clear_selected_account: bool,
}

pub const fn account_stage_refresh_plan() -> AccountStageRefreshPlan {
    AccountStageRefreshPlan {
        clear_accounts: true,
        reset_account_list: true,
        clear_selected_account: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountStageCommitPlan<Account> {
    ExistingAccount(Account),
    NoSelection,
}

pub fn account_stage_commit_plan<Account>(
    picked: Option<Account>,
) -> AccountStageCommitPlan<Account> {
    picked
        .map(AccountStageCommitPlan::ExistingAccount)
        .unwrap_or(AccountStageCommitPlan::NoSelection)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionCollapseIntent {
    Collapse,
    Expand,
    Toggle,
}

pub fn section_header_collapse_target(
    row: Option<&FieldDisplayRow>,
    collapsed_sections: &HashSet<String>,
    intent: SectionCollapseIntent,
) -> Option<(String, bool)> {
    let Some(FieldDisplayRow::SectionHeader { name, .. }) = row else {
        return None;
    };
    let collapsed = match intent {
        SectionCollapseIntent::Collapse => true,
        SectionCollapseIntent::Expand => false,
        SectionCollapseIntent::Toggle => !collapsed_sections.contains(name.as_str()),
    };
    Some((name.clone(), collapsed))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldStageCommitPlan {
    ToggleSection { name: String, collapsed: bool },
    ExistingField { field_idx: usize },
    NewField {
        pending_section: Option<String>,
        field_label_origin: FieldLabelOrigin,
        stage: OpPickerStage,
    },
    NoSelection,
}

pub fn field_stage_commit_plan(
    row: Option<&FieldDisplayRow>,
    collapsed_sections: &HashSet<String>,
    selected_section: Option<&str>,
) -> FieldStageCommitPlan {
    match row {
        Some(FieldDisplayRow::SectionHeader { .. }) => {
            section_header_collapse_target(row, collapsed_sections, SectionCollapseIntent::Toggle)
                .map(|(name, collapsed)| FieldStageCommitPlan::ToggleSection { name, collapsed })
                .unwrap_or(FieldStageCommitPlan::NoSelection)
        }
        Some(FieldDisplayRow::Field { field_idx }) => FieldStageCommitPlan::ExistingField {
            field_idx: *field_idx,
        },
        Some(FieldDisplayRow::NewFieldSentinel) => FieldStageCommitPlan::NewField {
            pending_section: selected_section.map(str::to_string),
            field_label_origin: FieldLabelOrigin::NewField,
            stage: OpPickerStage::FieldLabel,
        },
        Some(FieldDisplayRow::NewSectionSentinel) | None => FieldStageCommitPlan::NoSelection,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingStagePlan {
    pub stage: OpPickerStage,
    pub field_label_origin: Option<FieldLabelOrigin>,
    pub pending_section: Option<String>,
    pub clear_pending_section: bool,
}

pub const fn new_item_name_commit_plan() -> NamingStagePlan {
    NamingStagePlan {
        stage: OpPickerStage::FieldLabel,
        field_label_origin: Some(FieldLabelOrigin::NewItem),
        pending_section: None,
        clear_pending_section: false,
    }
}

pub fn new_section_name_commit_plan(name: &str) -> NamingStagePlan {
    NamingStagePlan {
        stage: OpPickerStage::FieldLabel,
        field_label_origin: Some(FieldLabelOrigin::NewSection),
        pending_section: Some(name.trim().to_string()),
        clear_pending_section: false,
    }
}

pub const fn field_label_cancel_plan(origin: FieldLabelOrigin) -> NamingStagePlan {
    NamingStagePlan {
        stage: origin.cancel_stage(),
        field_label_origin: None,
        pending_section: None,
        clear_pending_section: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldLabelCommitPlan<Account, Vault, Item> {
    NewItem {
        account: Option<Account>,
        vault: Vault,
        item_name: String,
        section: Option<String>,
        field_label: String,
    },
    EditItemField {
        account: Option<Account>,
        vault: Vault,
        item: Item,
        section: Option<String>,
        field_label: String,
    },
}

pub fn field_label_commit_plan<Account, Vault, Item>(
    account: Option<Account>,
    vault: Vault,
    item: Option<Item>,
    pending_section: Option<String>,
    item_name: String,
    raw_label: &str,
) -> FieldLabelCommitPlan<Account, Vault, Item> {
    let field_label = raw_label.trim().to_string();
    if let Some(item) = item {
        return FieldLabelCommitPlan::EditItemField {
            account,
            vault,
            item,
            section: pending_section,
            field_label,
        };
    }
    FieldLabelCommitPlan::NewItem {
        account,
        vault,
        item_name,
        section: pending_section,
        field_label,
    }
}

pub fn field_label_commit_selection<Reference, Account, Vault, Item, FieldTarget>(
    plan: FieldLabelCommitPlan<Account, Vault, Item>,
    new_field_target: impl FnOnce(String) -> FieldTarget,
) -> OpPickerSelection<Reference, Account, Vault, Item, FieldTarget> {
    match plan {
        FieldLabelCommitPlan::EditItemField {
            account,
            vault,
            item,
            section,
            field_label,
        } => OpPickerSelection::EditItemField {
            account,
            vault,
            item,
            section,
            field: new_field_target(field_label),
        },
        FieldLabelCommitPlan::NewItem {
            account,
            vault,
            item_name,
            section,
            field_label,
        } => OpPickerSelection::NewItem {
            account,
            vault,
            item_name,
            section,
            field_label,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExistingFieldCommitPlan {
    ExistingReference,
    EditItemField {
        section: Option<String>,
        field_id: String,
        field_label: String,
    },
}

pub fn existing_field_commit_plan(
    mode: &OpPickerMode,
    field_id: &str,
    field_label: &str,
    selected_section: Option<String>,
) -> ExistingFieldCommitPlan {
    if mode.is_create() {
        return ExistingFieldCommitPlan::EditItemField {
            section: selected_section,
            field_id: field_id.to_string(),
            field_label: field_label.to_string(),
        };
    }
    ExistingFieldCommitPlan::ExistingReference
}

pub struct ExistingFieldCommitSelectionInput<Account, Vault, Item> {
    pub account: Option<Account>,
    pub vault: Vault,
    pub item: Item,
}

pub fn existing_field_commit_selection<Reference, Account, Vault, Item, FieldTarget>(
    plan: ExistingFieldCommitPlan,
    input: ExistingFieldCommitSelectionInput<Account, Vault, Item>,
    existing_reference: impl FnOnce() -> Reference,
    existing_field_target: impl FnOnce(String, String) -> FieldTarget,
) -> OpPickerSelection<Reference, Account, Vault, Item, FieldTarget> {
    match plan {
        ExistingFieldCommitPlan::EditItemField {
            section,
            field_id,
            field_label,
        } => OpPickerSelection::EditItemField {
            account: input.account,
            vault: input.vault,
            item: input.item,
            section,
            field: existing_field_target(field_id, field_label),
        },
        ExistingFieldCommitPlan::ExistingReference => {
            OpPickerSelection::Existing(existing_reference())
        }
    }
}

pub fn render_picker(frame: &mut Frame, area: Rect, state: &impl OpPickerRenderState) {
    frame.render_widget(ratatui::widgets::Clear, area);
    match state.load_state() {
        OpLoadState::Error(OpPickerError::Fatal(fatal)) => render_fatal(frame, area, fatal),
        OpLoadState::Loading { spinner_tick } => render_loading(frame, area, state, *spinner_tick),
        OpLoadState::Idle
        | OpLoadState::Ready
        | OpLoadState::Error(OpPickerError::Recoverable { .. }) => {
            render_pane(frame, area, state);
        }
    }
}

fn render_pane(frame: &mut Frame, area: Rect, state: &impl OpPickerRenderState) {
    let multi_account = state.account_count() > 1;

    if let Some(input) = state.naming_stage_input() {
        jackin_tui::components::text_input::render_text_input(frame, area, input);
        return;
    }

    let title = breadcrumb_title(
        state.stage(),
        multi_account,
        state.selected_account_email(),
        state.selected_vault_name(),
        state.selected_item_name(),
    );
    let title_with_spaces = format!(" {title} ");
    let block = Panel::new()
        .title(&title_with_spaces)
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let banner_height: u16 = match state.load_state() {
        OpLoadState::Error(OpPickerError::Recoverable { .. }) => 2,
        _ => 0,
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(banner_height),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    if banner_height > 0
        && let OpLoadState::Error(OpPickerError::Recoverable { message }) = state.load_state()
    {
        let truncated: String = message.chars().take(120).collect();
        let line = Line::from(vec![
            Span::styled(
                "Error: ",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(truncated, Style::default().fg(PHOSPHOR_DIM)),
        ]);
        frame.render_widget(Paragraph::new(line), rows[0]);
    }

    jackin_tui::components::render_filter_input(frame, rows[1], state.filter_buffer());

    let list_lines = match state.stage() {
        OpPickerStage::Account => state.account_lines(),
        OpPickerStage::Vault => state.vault_lines(),
        OpPickerStage::Item => state.item_lines(),
        OpPickerStage::Section => state.section_lines(),
        OpPickerStage::Field => state.field_lines(),
        OpPickerStage::NewItemName | OpPickerStage::FieldLabel | OpPickerStage::NewSectionName => {
            Vec::new()
        }
    };
    if list_lines.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "(no matches)",
            Style::default().fg(PHOSPHOR_DIM),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(para, rows[3]);
    } else {
        render_selected_lines_in_area(frame, rows[3], list_lines, state.selected_index());
    }
}

fn render_loading(
    frame: &mut Frame,
    area: Rect,
    state: &impl OpPickerRenderState,
    tick: u8,
) {
    let multi_account = state.account_count() > 1;
    let title = breadcrumb_title(
        loading_title_stage(state.stage()),
        multi_account,
        state.selected_account_email(),
        state.selected_vault_name(),
        state.selected_item_name(),
    );
    let title_with_spaces = format!(" {title} ");
    let block = Panel::new()
        .title(&title_with_spaces)
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let glyph = SPINNER_FRAMES[(tick as usize) % SPINNER_FRAMES.len()];
    let descriptor = loading_descriptor(
        state.stage(),
        multi_account,
        state.selected_account_email(),
        state.selected_vault_name(),
        state.selected_item_name(),
        state.selected_item_subtitle(),
    );

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let body = Line::from(vec![
        Span::styled(glyph.to_string(), Style::default().fg(PHOSPHOR_GREEN)),
        Span::raw("  "),
        Span::styled(descriptor, Style::default().fg(PHOSPHOR_DIM)),
    ]);
    frame.render_widget(Paragraph::new(body).alignment(Alignment::Center), rows[1]);
}

pub fn render_fatal(frame: &mut Frame, area: Rect, fatal: &OpPickerFatalState) {
    let block = Panel::new()
        .title(" 1Password ")
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(fatal_body_lines(fatal)).alignment(Alignment::Center),
        rows[1],
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltOpPickerRef {
    pub op: String,
    pub path: String,
    pub empty_reference_with_sibling_refs: bool,
}

/// Multi-account titles lead with the chosen account's email so the
/// operator can see which account they're drilling into; single-account
/// titles omit it.
pub fn breadcrumb_title(
    stage: OpPickerStage,
    multi_account: bool,
    account_email: &str,
    vault_name: &str,
    item_name: &str,
) -> String {
    match stage {
        OpPickerStage::Account => "1Password".to_string(),
        OpPickerStage::Vault => {
            if multi_account {
                account_email.to_string()
            } else {
                "1Password".to_string()
            }
        }
        OpPickerStage::Item
        | OpPickerStage::NewItemName
        | OpPickerStage::FieldLabel
        | OpPickerStage::NewSectionName => {
            if multi_account {
                format!("{account_email} \u{2192} {vault_name}")
            } else {
                vault_name.to_string()
            }
        }
        OpPickerStage::Section | OpPickerStage::Field => {
            if multi_account {
                format!("{account_email} \u{2192} {vault_name} \u{2192} {item_name}")
            } else {
                format!("{vault_name} \u{2192} {item_name}")
            }
        }
    }
}

/// Classifies by stderr substring because the root picker receives
/// process errors through `anyhow::Error` rather than typed variants.
pub fn classify_probe_error_message(message: impl Into<String>) -> OpPickerError {
    let message = message.into();
    if message.contains("failed to spawn") {
        OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
    } else if message.contains("not signed in")
        || message.contains("not currently signed")
        || message.contains("no accounts")
    {
        OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
    } else {
        OpPickerError::Fatal(OpPickerFatalState::GenericFatal { message })
    }
}

/// Distinct sections present in loaded `op://` field references, in
/// first-appearance order, with a leading `None` (`(root)`) entry.
pub fn section_choices_from_references<'a>(
    references: impl IntoIterator<Item = &'a str>,
) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = vec![None];
    for reference in references {
        if let Some(name) =
            crate::op_reference::parse_op_reference(reference).and_then(|parts| parts.section)
            && !out
                .iter()
                .any(|section| section.as_deref() == Some(name.as_str()))
        {
            out.push(Some(name));
        }
    }
    out
}

/// Build browse-mode field rows from the currently visible field
/// references. Returned `field_idx` values index into the visible-field
/// list supplied by the caller.
pub fn browse_field_display_rows<'a>(
    references: impl IntoIterator<Item = &'a str>,
    collapsed_sections: &HashSet<String>,
) -> Vec<FieldDisplayRow> {
    let mut unsectioned: Vec<usize> = Vec::new();
    let mut sections: Vec<(String, Vec<usize>)> = Vec::new();

    for (idx, reference) in references.into_iter().enumerate() {
        match crate::op_reference::parse_op_reference(reference).and_then(|parts| parts.section) {
            None => unsectioned.push(idx),
            Some(name) => {
                if let Some(entry) = sections.iter_mut().find(|(section, _)| section == &name) {
                    entry.1.push(idx);
                } else {
                    sections.push((name, vec![idx]));
                }
            }
        }
    }

    let mut rows = Vec::new();

    for idx in unsectioned {
        rows.push(FieldDisplayRow::Field { field_idx: idx });
    }

    for (section_name, indices) in sections {
        let count = indices.len();
        rows.push(FieldDisplayRow::SectionHeader {
            name: section_name.clone(),
            field_count: count,
        });
        if !collapsed_sections.contains(section_name.as_str()) {
            for idx in indices {
                rows.push(FieldDisplayRow::Field { field_idx: idx });
            }
        }
    }

    rows
}

/// Build create-mode field rows scoped to `selected_section`. Returned
/// `field_idx` values index into the visible-field list supplied by the
/// caller. A trailing `+ New field` sentinel is always present.
pub fn create_field_display_rows<'a>(
    references: impl IntoIterator<Item = &'a str>,
    selected_section: Option<&str>,
) -> Vec<FieldDisplayRow> {
    let mut rows: Vec<FieldDisplayRow> = references
        .into_iter()
        .enumerate()
        .filter(|(_, reference)| {
            let section =
                crate::op_reference::parse_op_reference(reference).and_then(|parts| parts.section);
            section.as_deref() == selected_section
        })
        .map(|(idx, _)| FieldDisplayRow::Field { field_idx: idx })
        .collect();
    rows.push(FieldDisplayRow::NewFieldSentinel);
    rows
}

pub fn filtered_accounts<'a>(
    filter: &str,
    accounts: &'a [OpPickerAccount],
) -> Vec<&'a OpPickerAccount> {
    accounts
        .iter()
        .filter(|account| matches_filter(filter, [account.email.as_str(), account.url.as_str()]))
        .collect()
}

pub fn filtered_vaults<'a>(
    filter: &str,
    vaults: &'a [OpPickerVault],
) -> Vec<&'a OpPickerVault> {
    vaults
        .iter()
        .filter(|vault| matches_filter(filter, [vault.name.as_str()]))
        .collect()
}

pub fn filtered_items<'a>(filter: &str, items: &'a [OpPickerItem]) -> Vec<&'a OpPickerItem> {
    items
        .iter()
        .filter(|item| matches_filter(filter, [item.name.as_str(), item.subtitle.as_str()]))
        .collect()
}

pub fn filtered_item_choices<'a>(
    filter: &str,
    items: &'a [OpPickerItem],
    mode: &OpPickerMode,
) -> Vec<Option<&'a OpPickerItem>> {
    let mut out: Vec<Option<&OpPickerItem>> =
        filtered_items(filter, items).into_iter().map(Some).collect();
    if mode.is_create() {
        out.push(None);
    }
    out
}

pub fn filtered_fields<'a>(filter: &str, fields: &'a [OpPickerField]) -> Vec<&'a OpPickerField> {
    fields
        .iter()
        .filter(|field| matches_filter(filter, [field.label.as_str(), field.id.as_str()]))
        .collect()
}

pub fn field_display_rows_for_picker(
    mode: &OpPickerMode,
    filter: &str,
    fields: &[OpPickerField],
    selected_section: Option<&str>,
    collapsed_sections: &HashSet<String>,
) -> Vec<FieldDisplayRow> {
    let visible = filtered_fields(filter, fields);
    if mode.is_create() {
        create_field_display_rows(
            visible.iter().map(|field| field.reference.as_str()),
            selected_section,
        )
    } else {
        browse_field_display_rows(
            visible.iter().map(|field| field.reference.as_str()),
            collapsed_sections,
        )
    }
}

/// Build the committed `op://` value and display path from the picker
/// cache values. UUID-form `op` segments are paired with human-readable
/// path segments, preserving a section segment from the field reference
/// when 1Password supplies one.
pub fn build_op_picker_ref<'a>(
    vault: OpPickerVaultRef<'a>,
    selected_item: OpPickerItemRef<'a>,
    items_in_vault: impl IntoIterator<Item = OpPickerItemRef<'a>>,
    field: OpPickerFieldRef<'a>,
    fields_in_item: impl IntoIterator<Item = OpPickerFieldRef<'a>>,
) -> BuiltOpPickerRef {
    let item_name_collides = items_in_vault
        .into_iter()
        .any(|item| item.id != selected_item.id && item.name == selected_item.name);
    let safe_to_embed = !selected_item.name.contains('[') && !selected_item.name.contains(']');
    let item_segment = if item_name_collides && safe_to_embed && !selected_item.subtitle.is_empty()
    {
        format!("{}[{}]", selected_item.name, selected_item.subtitle)
    } else {
        selected_item.name.to_string()
    };

    if let Some(section_name) =
        crate::op_reference::parse_op_reference(field.reference).and_then(|parts| parts.section)
    {
        return BuiltOpPickerRef {
            op: format!(
                "op://{}/{}/{}/{}",
                vault.id, selected_item.id, section_name, field.id
            ),
            path: format!(
                "{}/{}/{}/{}",
                vault.name, item_segment, section_name, field.label
            ),
            empty_reference_with_sibling_refs: false,
        };
    }

    let label = if field.label.is_empty() {
        field.id
    } else {
        field.label
    };
    let empty_reference_with_sibling_refs = field.reference.is_empty()
        && fields_in_item
            .into_iter()
            .any(|sibling| sibling.id != field.id && !sibling.reference.is_empty());

    BuiltOpPickerRef {
        op: format!("op://{}/{}/{}", vault.id, selected_item.id, field.id),
        path: format!("{}/{}/{}", vault.name, item_segment, label),
        empty_reference_with_sibling_refs,
    }
}

/// `+ New X` creation row, styled like picker list rows.
pub fn sentinel_line(text: &str, is_selected: bool) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8} " } else { "  " };
    let style = if is_selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(PHOSPHOR_DIM)
    };
    Line::from(Span::styled(format!("{prefix}{text}"), style))
}

pub fn account_lines<'a>(
    accounts: impl IntoIterator<Item = OpPickerAccountRef<'a>>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    accounts
        .into_iter()
        .enumerate()
        .map(|(i, account)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let label_style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(vec![
                Span::styled(format!("{prefix}{}", account.email), label_style),
                Span::raw("  "),
                Span::styled(
                    format!("({})", account.url),
                    Style::default().fg(PHOSPHOR_DIM),
                ),
            ])
        })
        .collect()
}

pub fn vault_lines<'a>(
    vaults: impl IntoIterator<Item = OpPickerVaultRef<'a>>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    vaults
        .into_iter()
        .enumerate()
        .map(|(i, vault)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(Span::styled(format!("{prefix}{}", vault.name), style))
        })
        .collect()
}

pub fn item_choice_lines<'a>(
    item_choices: impl IntoIterator<Item = Option<OpPickerItemRef<'a>>>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    item_choices
        .into_iter()
        .enumerate()
        .map(|(i, choice)| {
            let is_selected = Some(i) == selected;
            choice.map_or_else(
                || sentinel_line("+ New item", is_selected),
                |item| {
                    let prefix = if is_selected { "\u{25b8} " } else { "  " };
                    let title_style = if is_selected {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(WHITE)
                    };
                    let mut spans = vec![
                        Span::styled(prefix, title_style),
                        Span::styled(item.name.to_string(), title_style),
                    ];
                    if !item.subtitle.is_empty() {
                        let dim = Style::default().fg(PHOSPHOR_DIM);
                        spans.push(Span::styled(" (", dim));
                        spans.push(Span::styled(item.subtitle.to_string(), dim));
                        spans.push(Span::styled(")", dim));
                    }
                    Line::from(spans)
                },
            )
        })
        .collect()
}

/// Render section-stage rows: `(root)`, named sections, then a creation
/// sentinel.
pub fn section_lines(
    choices: impl IntoIterator<Item = Option<String>>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    let choices: Vec<Option<String>> = choices.into_iter().collect();
    let sentinel_idx = choices.len();
    let mut lines: Vec<Line<'static>> = choices
        .into_iter()
        .enumerate()
        .map(|(i, choice)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            let label = choice.unwrap_or_else(|| "(root)".to_string());
            Line::from(Span::styled(format!("{prefix}{label}"), style))
        })
        .collect();
    lines.push(sentinel_line(
        "+ New section",
        Some(sentinel_idx) == selected,
    ));
    lines
}

pub fn field_lines<'a>(
    rows: impl IntoIterator<Item = FieldDisplayRow>,
    fields: impl IntoIterator<Item = OpPickerFieldDisplayRef<'a>>,
    collapsed_sections: &HashSet<String>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    let fields: Vec<OpPickerFieldDisplayRef<'a>> = fields.into_iter().collect();
    let label_w = fields
        .iter()
        .map(|field| field_display_label(*field).chars().count())
        .max()
        .unwrap_or(0)
        .max(8);

    rows.into_iter()
        .enumerate()
        .map(|(row_i, row)| {
            let is_selected = Some(row_i) == selected;
            match row {
                FieldDisplayRow::SectionHeader { name, field_count } => {
                    section_header_line(&name, field_count, collapsed_sections, is_selected)
                }
                FieldDisplayRow::Field { field_idx } => {
                    let Some(field) = fields.get(field_idx).copied() else {
                        return Line::default();
                    };
                    field_line(field, label_w, is_selected)
                }
                FieldDisplayRow::NewFieldSentinel => sentinel_line("+ New field", is_selected),
                FieldDisplayRow::NewSectionSentinel => sentinel_line("+ New section", is_selected),
            }
        })
        .collect()
}

fn section_header_line(
    name: &str,
    field_count: usize,
    collapsed_sections: &HashSet<String>,
    is_selected: bool,
) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8}  " } else { "   " };
    let arrow = if collapsed_sections.contains(name) {
        "\u{25b6}"
    } else {
        "\u{25bc}"
    };
    let style = if is_selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(PHOSPHOR_DIM)
    };
    let count_label = format!(
        "({} {})",
        field_count,
        if field_count == 1 { "field" } else { "fields" }
    );
    Line::from(vec![
        Span::styled(prefix, style),
        Span::styled(arrow, style),
        Span::styled(format!(" {name}  "), style),
        Span::styled(count_label, Style::default().fg(PHOSPHOR_DIM)),
    ])
}

fn field_line(
    field: OpPickerFieldDisplayRef<'_>,
    label_w: usize,
    is_selected: bool,
) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8} " } else { "  " };
    let label = field_display_label(field);
    let pad = label_w.saturating_sub(label.chars().count());
    let label_style = if is_selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    let annotation = if field.concealed {
        "(concealed)".to_string()
    } else {
        format!("({})", field.field_type.to_lowercase())
    };
    Line::from(vec![
        Span::styled(format!("{prefix}{label}"), label_style),
        Span::raw(format!("{}  ", " ".repeat(pad))),
        Span::styled(annotation, Style::default().fg(PHOSPHOR_DIM)),
    ])
}

fn field_display_label(field: OpPickerFieldDisplayRef<'_>) -> String {
    if field.label.is_empty() {
        field.id.to_string()
    } else {
        field.label.to_string()
    }
}

pub fn loading_title_stage(stage: OpPickerStage) -> OpPickerStage {
    if matches!(stage, OpPickerStage::Field) {
        OpPickerStage::Item
    } else {
        stage
    }
}

pub fn loading_descriptor(
    stage: OpPickerStage,
    multi_account: bool,
    account_email: &str,
    vault_name: &str,
    item_name: &str,
    item_subtitle: &str,
) -> String {
    match stage {
        OpPickerStage::Account => "loading accounts\u{2026}".to_string(),
        OpPickerStage::Vault => {
            if multi_account && !account_email.is_empty() {
                format!("loading vaults from {account_email}\u{2026}")
            } else {
                "loading vaults\u{2026}".to_string()
            }
        }
        OpPickerStage::Item => {
            format!("loading items from {vault_name}\u{2026}")
        }
        OpPickerStage::Field => {
            if item_subtitle.is_empty() {
                format!("loading {item_name}\u{2026}")
            } else {
                format!("loading {item_name} ({item_subtitle})\u{2026}")
            }
        }
        OpPickerStage::Section
        | OpPickerStage::NewItemName
        | OpPickerStage::FieldLabel
        | OpPickerStage::NewSectionName => "loading\u{2026}".to_string(),
    }
}

pub fn fatal_body_lines(fatal: &OpPickerFatalState) -> Vec<Line<'static>> {
    match fatal {
        OpPickerFatalState::NotInstalled => vec![
            Line::from(Span::styled(
                "1Password CLI not found.",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Install: brew install 1password-cli (macOS)",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(Span::styled(
                "or visit 1password.com/downloads/command-line/",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "After install, run `op signin`, then press P to retry.",
                Style::default().fg(PHOSPHOR_DIM),
            )),
        ],
        OpPickerFatalState::NotSignedIn => vec![
            Line::from(Span::styled(
                "1Password CLI is not signed in.",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Run `op signin` in your shell, then retry.",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "jackin' uses your existing op session — there is no separate jackin' auth.",
                Style::default().fg(PHOSPHOR_DIM),
            )),
        ],
        OpPickerFatalState::NoVaults => vec![
            Line::from(Span::styled(
                "No vaults available.",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Check 1Password's app integration settings:",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(Span::styled(
                "Settings \u{2192} Developer \u{2192} CLI integration.",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
        ],
        OpPickerFatalState::GenericFatal { message } => {
            let truncated: String = message.chars().take(120).collect();
            vec![
                Line::from(Span::styled(
                    "1Password CLI error.",
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(truncated, Style::default().fg(PHOSPHOR_DIM))),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_worker_disconnected_message_is_component_owned() {
        assert_eq!(
            background_worker_disconnected_error_message(),
            "background worker disconnected",
        );
    }

    #[test]
    fn breadcrumb_omits_pane_type_suffix_multi_account() {
        let title = breadcrumb_title(
            OpPickerStage::Vault,
            true,
            "alice@example.com",
            "ignored",
            "ignored",
        );
        assert_eq!(title, "alice@example.com");
        assert!(!title.contains("Vaults"), "no `Vaults` suffix: {title}");

        let title = breadcrumb_title(
            OpPickerStage::Item,
            true,
            "alice@example.com",
            "Personal",
            "",
        );
        assert_eq!(title, "alice@example.com \u{2192} Personal");
        assert!(!title.contains("Items"));

        let title = breadcrumb_title(
            OpPickerStage::Field,
            true,
            "alice@example.com",
            "Personal",
            "API Keys",
        );
        assert_eq!(
            title,
            "alice@example.com \u{2192} Personal \u{2192} API Keys"
        );
        assert!(!title.contains("Fields"));
    }

    #[test]
    fn breadcrumb_single_account_uses_brand_or_bare_context() {
        let v = breadcrumb_title(OpPickerStage::Vault, false, "", "Personal", "");
        assert_eq!(v, "1Password");

        let i = breadcrumb_title(OpPickerStage::Item, false, "", "Personal", "API Keys");
        assert_eq!(i, "Personal");

        let f = breadcrumb_title(OpPickerStage::Field, false, "", "Personal", "API Keys");
        assert_eq!(f, "Personal \u{2192} API Keys");
    }

    #[test]
    fn breadcrumb_account_pane_is_bare_brand() {
        let title = breadcrumb_title(OpPickerStage::Account, true, "ignored", "", "");
        assert_eq!(title, "1Password");
    }

    #[test]
    fn probe_error_message_classifies_operator_states() {
        assert!(matches!(
            classify_probe_error_message("failed to spawn op"),
            OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
        ));
        assert!(matches!(
            classify_probe_error_message("not currently signed in"),
            OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
        ));
        assert!(matches!(
            classify_probe_error_message("boom"),
            OpPickerError::Fatal(OpPickerFatalState::GenericFatal { .. })
        ));
    }

    #[test]
    fn section_choices_deduplicate_in_first_seen_order() {
        let choices = section_choices_from_references([
            "op://Vault/Item/token",
            "op://Vault/Item/Auth/password",
            "op://Vault/Item/Deploy/key",
            "op://Vault/Item/Auth/otp",
        ]);
        assert_eq!(
            choices,
            vec![None, Some("Auth".to_string()), Some("Deploy".to_string())]
        );
    }

    #[test]
    fn browse_field_rows_group_sections_and_respect_collapse() {
        let mut collapsed = HashSet::new();
        collapsed.insert("Auth".to_string());
        let rows = browse_field_display_rows(
            [
                "op://Vault/Item/root",
                "op://Vault/Item/Auth/password",
                "op://Vault/Item/Auth/otp",
                "op://Vault/Item/Deploy/key",
            ],
            &collapsed,
        );
        assert!(matches!(rows[0], FieldDisplayRow::Field { field_idx: 0 }));
        assert!(matches!(
            rows[1],
            FieldDisplayRow::SectionHeader {
                ref name,
                field_count: 2
            } if name == "Auth"
        ));
        assert!(matches!(
            rows[2],
            FieldDisplayRow::SectionHeader {
                ref name,
                field_count: 1
            } if name == "Deploy"
        ));
        assert!(matches!(rows[3], FieldDisplayRow::Field { field_idx: 3 }));
    }

    #[test]
    fn create_field_rows_scope_to_section_and_add_sentinel() {
        let rows = create_field_display_rows(
            [
                "op://Vault/Item/root",
                "op://Vault/Item/Auth/password",
                "op://Vault/Item/Auth/otp",
            ],
            Some("Auth"),
        );
        assert!(matches!(rows[0], FieldDisplayRow::Field { field_idx: 1 }));
        assert!(matches!(rows[1], FieldDisplayRow::Field { field_idx: 2 }));
        assert!(matches!(rows[2], FieldDisplayRow::NewFieldSentinel));
    }

    #[test]
    fn selected_index_routes_by_visible_stage() {
        assert_eq!(
            selected_index_for_stage(
                OpPickerStage::Account,
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
            ),
            Some(1)
        );
        assert_eq!(
            selected_index_for_stage(
                OpPickerStage::Field,
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
            ),
            Some(5)
        );
        assert_eq!(
            selected_index_for_stage(
                OpPickerStage::FieldLabel,
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
            ),
            None
        );
    }

    #[test]
    fn naming_stage_input_routes_by_naming_stage() {
        let item = item_name_input_state("");
        let field = field_label_input_state("");
        let section = section_name_input_state("");

        assert_eq!(
            naming_stage_input_for_stage(OpPickerStage::NewItemName, &item, &field, &section)
                .map(|input| input.label.as_str()),
            Some("Item name")
        );
        assert_eq!(
            naming_stage_input_for_stage(OpPickerStage::FieldLabel, &item, &field, &section)
                .map(|input| input.label.as_str()),
            Some("Field label")
        );
        assert_eq!(
            naming_stage_input_for_stage(OpPickerStage::NewSectionName, &item, &field, &section)
                .map(|input| input.label.as_str()),
            Some("Section name")
        );
        assert!(
            naming_stage_input_for_stage(OpPickerStage::Field, &item, &field, &section).is_none()
        );
    }

    #[test]
    fn filter_reset_selection_routes_only_filterable_stages() {
        assert_eq!(
            filter_reset_selection_for_stage(OpPickerStage::Account, 2, 3, 4, 5),
            Some(Some(0))
        );
        assert_eq!(
            filter_reset_selection_for_stage(OpPickerStage::Item, 2, 3, 0, 5),
            Some(None)
        );
        assert_eq!(
            filter_reset_selection_for_stage(OpPickerStage::Section, 2, 3, 4, 5),
            None
        );
        assert_eq!(
            filter_reset_selection_for_stage(OpPickerStage::FieldLabel, 2, 3, 4, 5),
            None
        );
    }

    #[test]
    fn field_stage_back_plan_preserves_create_mode_sections() {
        assert_eq!(
            field_stage_back_plan(&OpPickerMode::Create {
                item_name_default: String::new(),
                field_label_default: String::new(),
            }),
            FieldStageBackPlan {
                stage: OpPickerStage::Section,
                reset_selected_section: true,
                clear_fields: false,
                clear_collapsed_sections: false,
                clear_selected_item: false,
                reset_section_list: true,
            }
        );
        assert_eq!(
            field_stage_back_plan(&OpPickerMode::Browse),
            FieldStageBackPlan {
                stage: OpPickerStage::Item,
                reset_selected_section: false,
                clear_fields: true,
                clear_collapsed_sections: true,
                clear_selected_item: true,
                reset_section_list: false,
            }
        );
    }

    #[test]
    fn field_stage_refresh_plan_tracks_create_mode_in_place_reload() {
        assert_eq!(
            field_stage_refresh_plan(&OpPickerMode::Browse),
            FieldStageRefreshPlan {
                clear_fields: true,
                reset_field_list: true,
                clear_collapsed_sections: true,
                refresh_in_place: false,
            }
        );
        assert_eq!(
            field_stage_refresh_plan(&OpPickerMode::Create {
                item_name_default: String::new(),
                field_label_default: String::new(),
            }),
            FieldStageRefreshPlan {
                clear_fields: true,
                reset_field_list: true,
                clear_collapsed_sections: true,
                refresh_in_place: true,
            }
        );
    }

    #[test]
    fn section_stage_back_plan_returns_to_item() {
        assert_eq!(
            section_stage_back_plan(),
            SectionStageBackPlan {
                stage: OpPickerStage::Item,
                clear_fields: true,
                clear_collapsed_sections: true,
                clear_selected_section: true,
                clear_selected_item: true,
            }
        );
    }

    #[test]
    fn section_stage_commit_plan_resolves_sentinel_and_choices() {
        let choices = vec![None, Some("api".to_string())];

        assert_eq!(
            section_stage_commit_plan(Some(0), &choices),
            SectionStageCommitPlan::ExistingSection {
                selected_section: None
            }
        );
        assert_eq!(
            section_stage_commit_plan(Some(1), &choices),
            SectionStageCommitPlan::ExistingSection {
                selected_section: Some("api".to_string())
            }
        );
        assert_eq!(
            section_stage_commit_plan(Some(2), &choices),
            SectionStageCommitPlan::NewSectionName
        );
        assert_eq!(
            section_stage_commit_plan(Some(3), &choices),
            SectionStageCommitPlan::NoSelection
        );
    }

    #[test]
    fn item_stage_back_plan_returns_to_vault() {
        assert_eq!(
            item_stage_back_plan(),
            ItemStageBackPlan {
                stage: OpPickerStage::Vault,
                clear_items: true,
                clear_selected_item: true,
            }
        );
    }

    #[test]
    fn item_stage_commit_plan_routes_existing_new_and_empty() {
        assert_eq!(
            item_stage_commit_plan(Some(Some("item"))),
            ItemStageCommitPlan::ExistingItem("item")
        );
        assert_eq!(
            item_stage_commit_plan::<&str>(Some(None)),
            ItemStageCommitPlan::NewItemName
        );
        assert_eq!(
            item_stage_commit_plan::<&str>(None),
            ItemStageCommitPlan::NoSelection
        );
    }

    #[test]
    fn item_stage_refresh_plan_clears_loaded_state() {
        assert_eq!(
            item_stage_refresh_plan(),
            ItemStageRefreshPlan {
                clear_items: true,
                reset_item_list: true,
            }
        );
    }

    #[test]
    fn vault_stage_back_plan_handles_single_and_multi_account() {
        assert_eq!(vault_stage_back_plan(1), VaultStageBackPlan::Cancel);
        assert_eq!(
            vault_stage_back_plan(2),
            VaultStageBackPlan::BackToAccount {
                stage: OpPickerStage::Account,
                clear_selected_vault: true,
                clear_vaults: true,
                reset_vault_list: true,
                ready_load_state: true,
            }
        );
    }

    #[test]
    fn vault_stage_commit_plan_routes_existing_and_empty() {
        assert_eq!(
            vault_stage_commit_plan(Some("vault")),
            VaultStageCommitPlan::ExistingVault("vault")
        );
        assert_eq!(
            vault_stage_commit_plan::<&str>(None),
            VaultStageCommitPlan::NoSelection
        );
    }

    #[test]
    fn vault_stage_refresh_plan_clears_loaded_state() {
        assert_eq!(
            vault_stage_refresh_plan(),
            VaultStageRefreshPlan {
                clear_vaults: true,
                reset_vault_list: true,
                clear_selected_vault: true,
            }
        );
    }

    #[test]
    fn account_stage_refresh_plan_clears_loaded_state() {
        assert_eq!(
            account_stage_refresh_plan(),
            AccountStageRefreshPlan {
                clear_accounts: true,
                reset_account_list: true,
                clear_selected_account: true,
            }
        );
    }

    #[test]
    fn account_stage_commit_plan_routes_existing_and_empty() {
        assert_eq!(
            account_stage_commit_plan(Some("account")),
            AccountStageCommitPlan::ExistingAccount("account")
        );
        assert_eq!(
            account_stage_commit_plan::<&str>(None),
            AccountStageCommitPlan::NoSelection
        );
    }

    #[test]
    fn section_header_collapse_target_routes_only_headers() {
        let row = FieldDisplayRow::SectionHeader {
            name: "Auth".to_string(),
            field_count: 2,
        };
        let mut collapsed = HashSet::new();

        assert_eq!(
            section_header_collapse_target(
                Some(&row),
                &collapsed,
                SectionCollapseIntent::Collapse
            ),
            Some(("Auth".to_string(), true))
        );
        assert_eq!(
            section_header_collapse_target(Some(&row), &collapsed, SectionCollapseIntent::Expand),
            Some(("Auth".to_string(), false))
        );
        assert_eq!(
            section_header_collapse_target(Some(&row), &collapsed, SectionCollapseIntent::Toggle),
            Some(("Auth".to_string(), true))
        );

        collapsed.insert("Auth".to_string());
        assert_eq!(
            section_header_collapse_target(Some(&row), &collapsed, SectionCollapseIntent::Toggle),
            Some(("Auth".to_string(), false))
        );
        assert_eq!(
            section_header_collapse_target(
                Some(&FieldDisplayRow::NewFieldSentinel),
                &collapsed,
                SectionCollapseIntent::Toggle,
            ),
            None
        );
    }

    #[test]
    fn field_stage_commit_plan_routes_row_kinds() {
        let row = FieldDisplayRow::SectionHeader {
            name: "Auth".to_string(),
            field_count: 2,
        };
        let collapsed = HashSet::new();
        assert_eq!(
            field_stage_commit_plan(Some(&row), &collapsed, Some("Auth")),
            FieldStageCommitPlan::ToggleSection {
                name: "Auth".to_string(),
                collapsed: true,
            }
        );

        assert_eq!(
            field_stage_commit_plan(
                Some(&FieldDisplayRow::Field { field_idx: 3 }),
                &collapsed,
                Some("Auth"),
            ),
            FieldStageCommitPlan::ExistingField { field_idx: 3 }
        );
        assert_eq!(
            field_stage_commit_plan(Some(&FieldDisplayRow::NewFieldSentinel), &collapsed, None),
            FieldStageCommitPlan::NewField {
                pending_section: None,
                field_label_origin: FieldLabelOrigin::NewField,
                stage: OpPickerStage::FieldLabel,
            }
        );
        assert_eq!(
            field_stage_commit_plan(Some(&FieldDisplayRow::NewSectionSentinel), &collapsed, None),
            FieldStageCommitPlan::NoSelection
        );
    }

    #[test]
    fn naming_stage_plans_name_next_stage_and_pending_section() {
        assert_eq!(
            new_item_name_commit_plan(),
            NamingStagePlan {
                stage: OpPickerStage::FieldLabel,
                field_label_origin: Some(FieldLabelOrigin::NewItem),
                pending_section: None,
                clear_pending_section: false,
            }
        );
        assert_eq!(
            new_section_name_commit_plan("  Deploy  "),
            NamingStagePlan {
                stage: OpPickerStage::FieldLabel,
                field_label_origin: Some(FieldLabelOrigin::NewSection),
                pending_section: Some("Deploy".to_string()),
                clear_pending_section: false,
            }
        );
        assert_eq!(
            field_label_cancel_plan(FieldLabelOrigin::NewField),
            NamingStagePlan {
                stage: OpPickerStage::Field,
                field_label_origin: None,
                pending_section: None,
                clear_pending_section: true,
            }
        );
    }

    #[test]
    fn matches_filter_accepts_empty_or_any_matching_value() {
        assert!(matches_filter("", ["anything"]));
        assert!(matches_filter("api", ["Stripe", "API token"]));
        assert!(matches_filter(
            "example",
            ["alice@example.com", "https://example.test"]
        ));
        assert!(!matches_filter("missing", ["one", "two"]));
    }

    #[test]
    fn build_op_picker_ref_uses_uuid_op_and_clean_path_for_unique_item() {
        let built = build_op_picker_ref(
            OpPickerVaultRef {
                id: "v_uuid",
                name: "Private",
            },
            OpPickerItemRef {
                id: "i_uuid",
                name: "Stripe",
                subtitle: "",
            },
            [OpPickerItemRef {
                id: "i_uuid",
                name: "Stripe",
                subtitle: "",
            }],
            OpPickerFieldRef {
                id: "f_uuid",
                label: "api key",
                reference: "op://Private/Stripe/api key",
            },
            [OpPickerFieldRef {
                id: "f_uuid",
                label: "api key",
                reference: "op://Private/Stripe/api key",
            }],
        );
        assert_eq!(built.op, "op://v_uuid/i_uuid/f_uuid");
        assert_eq!(built.path, "Private/Stripe/api key");
        assert!(!built.empty_reference_with_sibling_refs);
    }

    #[test]
    fn build_op_picker_ref_preserves_sections_and_ambiguous_subtitles() {
        let built = build_op_picker_ref(
            OpPickerVaultRef {
                id: "v_uuid",
                name: "Private",
            },
            OpPickerItemRef {
                id: "i_a",
                name: "Claude",
                subtitle: "alice@example.com",
            },
            [
                OpPickerItemRef {
                    id: "i_a",
                    name: "Claude",
                    subtitle: "alice@example.com",
                },
                OpPickerItemRef {
                    id: "i_b",
                    name: "Claude",
                    subtitle: "bob@example.com",
                },
            ],
            OpPickerFieldRef {
                id: "f_uuid",
                label: "token",
                reference: "op://Private/Claude/Auth/token",
            },
            [OpPickerFieldRef {
                id: "f_uuid",
                label: "token",
                reference: "op://Private/Claude/Auth/token",
            }],
        );
        assert_eq!(built.op, "op://v_uuid/i_a/Auth/f_uuid");
        assert_eq!(built.path, "Private/Claude[alice@example.com]/Auth/token");
    }

    #[test]
    fn build_op_picker_ref_flags_empty_reference_with_sibling_refs() {
        let built = build_op_picker_ref(
            OpPickerVaultRef {
                id: "v_uuid",
                name: "Private",
            },
            OpPickerItemRef {
                id: "i_uuid",
                name: "MyItem",
                subtitle: "",
            },
            [OpPickerItemRef {
                id: "i_uuid",
                name: "MyItem",
                subtitle: "",
            }],
            OpPickerFieldRef {
                id: "f_noref",
                label: "notes",
                reference: "",
            },
            [
                OpPickerFieldRef {
                    id: "f_noref",
                    label: "notes",
                    reference: "",
                },
                OpPickerFieldRef {
                    id: "f_sectioned",
                    label: "password",
                    reference: "op://Private/MyItem/Auth/password",
                },
            ],
        );
        assert_eq!(built.op, "op://v_uuid/i_uuid/f_noref");
        assert_eq!(built.path, "Private/MyItem/notes");
        assert!(built.empty_reference_with_sibling_refs);
    }

    #[test]
    fn section_lines_append_new_section_sentinel() {
        let lines = section_lines([None, Some("Auth".to_string())], Some(2));
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[0].spans[0].content.as_ref(),
            "  (root)",
            "root choice renders first"
        );
        assert_eq!(
            lines[1].spans[0].content.as_ref(),
            "  Auth",
            "named section renders second"
        );
        assert_eq!(
            lines[2].spans[0].content.as_ref(),
            "\u{25b8} + New section",
            "sentinel renders last and selected"
        );
    }

    #[test]
    fn account_vault_and_item_lines_apply_selected_prefixes() {
        let account = account_lines(
            [OpPickerAccountRef {
                email: "alice@example.com",
                url: "alice.1password.com",
            }],
            Some(0),
        );
        assert_eq!(
            account[0].spans[0].content.as_ref(),
            "\u{25b8} alice@example.com"
        );
        assert_eq!(
            account[0].spans[2].content.as_ref(),
            "(alice.1password.com)"
        );

        let vault = vault_lines(
            [OpPickerVaultRef {
                id: "v1",
                name: "Private",
            }],
            None,
        );
        assert_eq!(vault[0].spans[0].content.as_ref(), "  Private");

        let items = item_choice_lines(
            [
                Some(OpPickerItemRef {
                    id: "i1",
                    name: "Claude",
                    subtitle: "alice@example.com",
                }),
                None,
            ],
            Some(1),
        );
        assert_eq!(items[0].spans[1].content.as_ref(), "Claude");
        assert_eq!(items[0].spans[3].content.as_ref(), "alice@example.com");
        assert_eq!(items[1].spans[0].content.as_ref(), "\u{25b8} + New item");
    }

    #[test]
    fn field_lines_render_headers_fields_and_sentinels() {
        let mut collapsed = HashSet::new();
        collapsed.insert("Auth".to_string());
        let lines = field_lines(
            [
                FieldDisplayRow::SectionHeader {
                    name: "Auth".to_string(),
                    field_count: 1,
                },
                FieldDisplayRow::Field { field_idx: 0 },
                FieldDisplayRow::NewFieldSentinel,
            ],
            [OpPickerFieldDisplayRef {
                id: "f1",
                label: "token",
                field_type: "CONCEALED",
                concealed: true,
            }],
            &collapsed,
            Some(1),
        );

        assert_eq!(lines[0].spans[1].content.as_ref(), "\u{25b6}");
        assert_eq!(lines[1].spans[0].content.as_ref(), "\u{25b8} token");
        assert_eq!(lines[1].spans[2].content.as_ref(), "(concealed)");
        assert_eq!(lines[2].spans[0].content.as_ref(), "  + New field");
    }

    #[test]
    fn loading_descriptor_names_current_load_target() {
        assert_eq!(
            loading_descriptor(OpPickerStage::Account, false, "", "", "", ""),
            "loading accounts\u{2026}"
        );
        assert_eq!(
            loading_descriptor(OpPickerStage::Vault, true, "alice@example.com", "", "", ""),
            "loading vaults from alice@example.com\u{2026}"
        );
        assert_eq!(
            loading_descriptor(
                OpPickerStage::Field,
                false,
                "",
                "",
                "Claude",
                "alice@example.com"
            ),
            "loading Claude (alice@example.com)\u{2026}"
        );
        assert_eq!(
            loading_title_stage(OpPickerStage::Field),
            OpPickerStage::Item
        );
    }

    #[test]
    fn fatal_body_lines_truncate_generic_errors() {
        let long = "x".repeat(140);
        let lines = fatal_body_lines(&OpPickerFatalState::GenericFatal { message: long });
        assert_eq!(lines[0].spans[0].content.as_ref(), "1Password CLI error.");
        assert_eq!(lines[2].spans[0].content.chars().count(), 120);

        let missing = fatal_body_lines(&OpPickerFatalState::NotInstalled);
        assert!(missing.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("brew install"))
        }));
    }

    #[test]
    fn field_label_origin_maps_to_cancel_stage() {
        assert_eq!(
            FieldLabelOrigin::NewItem.cancel_stage(),
            OpPickerStage::NewItemName
        );
        assert_eq!(
            FieldLabelOrigin::NewField.cancel_stage(),
            OpPickerStage::Field
        );
        assert_eq!(
            FieldLabelOrigin::NewSection.cancel_stage(),
            OpPickerStage::NewSectionName
        );
    }

    #[test]
    fn field_label_commit_plan_trims_and_routes_item_presence() {
        assert_eq!(
            field_label_commit_plan(
                Some("account"),
                "vault",
                Some("item"),
                Some("section".to_string()),
                "ignored".to_string(),
                "  token  ",
            ),
            FieldLabelCommitPlan::EditItemField {
                account: Some("account"),
                vault: "vault",
                item: "item",
                section: Some("section".to_string()),
                field_label: "token".to_string(),
            }
        );
        assert_eq!(
            field_label_commit_plan::<&str, &str, &str>(
                None,
                "vault",
                None,
                None,
                "login".to_string(),
                "  password  ",
            ),
            FieldLabelCommitPlan::NewItem {
                account: None,
                vault: "vault",
                item_name: "login".to_string(),
                section: None,
                field_label: "password".to_string(),
            }
        );
    }

    #[test]
    fn field_label_commit_selection_builds_component_owned_selection_shape() {
        let selection = field_label_commit_selection::<&str, &str, &str, &str, (&str, String)>(
            FieldLabelCommitPlan::EditItemField {
                account: Some("account"),
                vault: "vault",
                item: "item",
                section: Some("api".to_string()),
                field_label: "token".to_string(),
            },
            |label| ("new", label),
        );
        assert_eq!(
            selection,
            OpPickerSelection::EditItemField {
                account: Some("account"),
                vault: "vault",
                item: "item",
                section: Some("api".to_string()),
                field: ("new", "token".to_string()),
            }
        );

        let selection = field_label_commit_selection::<&str, &str, &str, &str, (&str, String)>(
            FieldLabelCommitPlan::NewItem {
                account: None,
                vault: "vault",
                item_name: "Login".to_string(),
                section: None,
                field_label: "password".to_string(),
            },
            |label| ("new", label),
        );
        assert_eq!(
            selection,
            OpPickerSelection::NewItem {
                account: None,
                vault: "vault",
                item_name: "Login".to_string(),
                section: None,
                field_label: "password".to_string(),
            }
        );
    }

    #[test]
    fn existing_field_commit_plan_routes_create_mode_to_field_target_data() {
        assert_eq!(
            existing_field_commit_plan(
                &OpPickerMode::Create {
                    item_name_default: String::new(),
                    field_label_default: String::new(),
                },
                "field-id",
                "token",
                Some("api".to_string()),
            ),
            ExistingFieldCommitPlan::EditItemField {
                section: Some("api".to_string()),
                field_id: "field-id".to_string(),
                field_label: "token".to_string(),
            }
        );
        assert_eq!(
            existing_field_commit_plan(&OpPickerMode::Browse, "field-id", "token", None),
            ExistingFieldCommitPlan::ExistingReference,
        );
    }

    #[test]
    fn existing_field_commit_selection_builds_component_owned_selection_shape() {
        let selection = existing_field_commit_selection(
            ExistingFieldCommitPlan::EditItemField {
                section: Some("api".to_string()),
                field_id: "field-id".to_string(),
                field_label: "token".to_string(),
            },
            ExistingFieldCommitSelectionInput {
                account: Some("account"),
                vault: "vault",
                item: "item",
            },
            || "op://unused",
            |id, label| (id, label),
        );
        assert_eq!(
            selection,
            OpPickerSelection::EditItemField {
                account: Some("account"),
                vault: "vault",
                item: "item",
                section: Some("api".to_string()),
                field: ("field-id".to_string(), "token".to_string()),
            }
        );

        let selection = existing_field_commit_selection::<&str, &str, &str, &str, (String, String)>(
            ExistingFieldCommitPlan::ExistingReference,
            ExistingFieldCommitSelectionInput {
                account: None,
                vault: "vault",
                item: "item",
            },
            || "op://vault/item/field",
            |id, label| (id, label),
        );
        assert_eq!(
            selection,
            OpPickerSelection::Existing("op://vault/item/field")
        );
    }

    #[test]
    fn stage_classification_separates_naming_and_filterable_lists() {
        assert!(OpPickerStage::FieldLabel.is_naming());
        assert!(OpPickerStage::NewSectionName.is_naming());
        assert!(!OpPickerStage::Field.is_naming());

        assert!(OpPickerStage::Account.is_filterable());
        assert!(OpPickerStage::Field.is_filterable());
        assert!(!OpPickerStage::Section.is_filterable());
        assert!(!OpPickerStage::FieldLabel.is_filterable());
    }

    #[test]
    fn account_load_completion_plan_keeps_root_adapter_out_of_transition_policy() {
        assert_eq!(accounts_loaded_plan(0), AccountsLoadedPlan::NotSignedIn);
        assert_eq!(
            accounts_loaded_plan(1),
            AccountsLoadedPlan::SelectSingleAccount
        );
        assert_eq!(accounts_loaded_plan(2), AccountsLoadedPlan::ShowAccountPane);
    }

    #[test]
    fn vault_item_and_field_load_completion_plans_keep_root_adapter_out_of_transition_policy() {
        assert_eq!(vaults_loaded_plan(0), VaultsLoadedPlan::NoVaults);
        assert_eq!(
            vaults_loaded_plan(2),
            VaultsLoadedPlan::ShowVaultPane { selected: Some(0) }
        );

        assert_eq!(items_loaded_plan(0).selected, None);
        assert_eq!(items_loaded_plan(3).selected, Some(0));

        assert_eq!(
            fields_loaded_plan(&OpPickerMode::Browse, false, 2, 4),
            FieldsLoadedPlan::ShowFieldPane {
                field_selected: Some(0),
                clear_selected_section: true,
            }
        );
        assert_eq!(
            fields_loaded_plan(
                &OpPickerMode::Create {
                    item_name_default: String::new(),
                    field_label_default: String::new(),
                },
                false,
                2,
                4,
            ),
            FieldsLoadedPlan::ShowSectionPane {
                stage: OpPickerStage::Section,
                section_selected: Some(0),
                clear_selected_section: true,
            }
        );
        assert_eq!(
            fields_loaded_plan(
                &OpPickerMode::Create {
                    item_name_default: String::new(),
                    field_label_default: String::new(),
                },
                true,
                2,
                4,
            ),
            FieldsLoadedPlan::RefreshFieldPane {
                field_selected: Some(0),
                clear_refresh_in_place: true,
            }
        );
    }

    #[test]
    fn field_load_sort_policy_puts_concealed_fields_first() {
        #[derive(Debug, PartialEq, Eq)]
        struct Field {
            label: &'static str,
            concealed: bool,
        }

        let mut fields = vec![
            Field {
                label: "plain",
                concealed: false,
            },
            Field {
                label: "secret",
                concealed: true,
            },
        ];

        sort_fields_by_concealed_first(&mut fields, |field| field.concealed);

        assert_eq!(fields[0].label, "secret");
        assert_eq!(fields[1].label, "plain");
    }
}
