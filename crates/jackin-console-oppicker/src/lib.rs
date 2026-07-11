//! Pure model and planning helpers for the 1Password picker.

pub mod input;
pub mod load;
pub mod state;

use std::collections::HashSet;

use jackin_tui::components::TextInputState;

pub use state::{LoadResult, OpPickerState};

/// Concrete selection type for the picker: all five type parameters are bound
/// to `jackin-core` types already available in this crate.
pub type OpPickerCoreSelection = OpPickerSelection<
    jackin_core::OpRef,
    jackin_core::op_types::OpAccount,
    jackin_core::op_types::OpVault,
    jackin_core::op_types::OpItem,
    jackin_core::FieldTarget,
>;

pub const fn first_selection(count: usize) -> Option<usize> {
    if count == 0 { None } else { Some(0) }
}

pub fn matches_filter<const N: usize>(filter: &str, haystacks: [&str; N]) -> bool {
    if filter.is_empty() {
        return true;
    }
    let f = filter.to_lowercase();
    haystacks
        .into_iter()
        .any(|haystack| haystack.to_lowercase().contains(&f))
}

pub fn item_name_input_state<'a>(item_default: impl Into<String>) -> TextInputState<'a> {
    TextInputState::new("Item name", item_default)
}

pub fn field_label_input_state<'a>(field_default: impl Into<String>) -> TextInputState<'a> {
    TextInputState::new("Field label", field_default)
}

pub fn section_name_input_state<'a>(initial: impl Into<String>) -> TextInputState<'a> {
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

pub fn probe_load_error_state(message: impl Into<String>) -> OpLoadState {
    OpLoadState::Error(classify_probe_error_message(message))
}

/// Classify a process/probe failure carried as `anyhow::Error`, preferring a
/// typed [`jackin_core::OpProbeError`] source when present.
pub fn probe_load_error_from_anyhow(error: &anyhow::Error) -> OpLoadState {
    OpLoadState::Error(classify_probe_error(error))
}

pub fn recoverable_load_error_state(message: impl Into<String>) -> OpLoadState {
    OpLoadState::Error(OpPickerError::Recoverable {
        message: message.into(),
    })
}

pub fn disconnected_worker_error_state() -> OpLoadState {
    recoverable_load_error_state(background_worker_disconnected_error_message())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpPickerBlockedLoadKeyPlan {
    Cancel,
    Continue,
}

pub fn blocked_load_key_plan(
    load_state: &OpLoadState,
    is_escape: bool,
) -> Option<OpPickerBlockedLoadKeyPlan> {
    match load_state {
        OpLoadState::Loading { .. } | OpLoadState::Error(OpPickerError::Fatal(_)) => {
            Some(if is_escape {
                OpPickerBlockedLoadKeyPlan::Cancel
            } else {
                OpPickerBlockedLoadKeyPlan::Continue
            })
        }
        OpLoadState::Idle
        | OpLoadState::Ready
        | OpLoadState::Error(OpPickerError::Recoverable { .. }) => None,
    }
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

pub fn selected_account_id<Account>(
    selected_account: Option<&Account>,
    account_id: impl FnOnce(&Account) -> &str,
) -> Option<String> {
    selected_account.map(|account| account_id(account).to_owned())
}

pub fn selected_account_id_ref<'a, Account>(
    selected_account: Option<&'a Account>,
    account_id: impl FnOnce(&'a Account) -> &'a str,
) -> Option<&'a str> {
    selected_account.map(account_id)
}

pub fn selected_entity_id_or_default<Entity>(
    selected_entity: Option<&Entity>,
    entity_id: impl FnOnce(&Entity) -> &str,
) -> String {
    selected_entity
        .map(|entity| entity_id(entity).to_owned())
        .unwrap_or_default()
}

pub fn selected_entity_label_or_empty<'a, Entity>(
    selected_entity: Option<&'a Entity>,
    label: impl FnOnce(&'a Entity) -> &'a str,
) -> &'a str {
    selected_entity.map_or("", label)
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

pub use jackin_core::op_types::{OpAccount as OpPickerAccount, OpVault as OpPickerVault};
/// Re-exported from `jackin-core` — canonical definitions live there so
/// `jackin-env` no longer depends on `jackin-console` for data types.
pub use jackin_core::op_types::{OpField as OpPickerField, OpItem as OpPickerItem};

/// Session-scoped metadata cache for picker drill-down panes.
pub type OpPickerCache =
    jackin_core::op_cache::OpCache<OpPickerAccount, OpPickerVault, OpPickerItem, OpPickerField>;

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
        OpPickerStage::NewItemName | OpPickerStage::FieldLabel | OpPickerStage::NewSectionName => {
            None
        }
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

#[allow(
    clippy::struct_excessive_bools,
    reason = "Orthogonal Esc-back mutation flags for the op-picker Field stage — \
              each bool is an independent state reset (section pointer, field buffer, \
              collapsed sections, selected item, section list) consumed individually \
              by the input dispatcher. Bundling into bitflags would lose naming at \
              the read site without changing observable behavior."
)]
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

#[allow(
    clippy::struct_excessive_bools,
    reason = "Orthogonal refresh-mutation flags for the op-picker Field stage — \
              each bool is an independent state update (clear fields, reset list, \
              clear collapsed sections, in-place reload) consumed individually by \
              the input dispatcher. Bundling into bitflags would lose naming at \
              the read site without changing observable behavior."
)]
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

#[allow(
    clippy::struct_excessive_bools,
    reason = "Orthogonal Esc-back mutation flags for the op-picker Section stage — \
              each bool is an independent state reset (field buffer, collapsed \
              sections, selected section, selected item) consumed individually by \
              the input dispatcher. Bundling into bitflags would lose naming at \
              the read site without changing observable behavior."
)]
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
        .map_or(SectionStageCommitPlan::NoSelection, |selected_section| {
            SectionStageCommitPlan::ExistingSection { selected_section }
        })
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
    picked.map_or(
        VaultStageCommitPlan::NoSelection,
        VaultStageCommitPlan::ExistingVault,
    )
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
    picked.map_or(
        AccountStageCommitPlan::NoSelection,
        AccountStageCommitPlan::ExistingAccount,
    )
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
    ToggleSection {
        name: String,
        collapsed: bool,
    },
    ExistingField {
        field_idx: usize,
    },
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
                .map_or(FieldStageCommitPlan::NoSelection, |(name, collapsed)| {
                    FieldStageCommitPlan::ToggleSection { name, collapsed }
                })
        }
        Some(FieldDisplayRow::Field { field_idx }) => FieldStageCommitPlan::ExistingField {
            field_idx: *field_idx,
        },
        Some(FieldDisplayRow::NewFieldSentinel) => FieldStageCommitPlan::NewField {
            pending_section: selected_section.map(str::to_owned),
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
        pending_section: Some(name.trim().to_owned()),
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
    let field_label = raw_label.trim().to_owned();
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
            field_id: field_id.to_owned(),
            field_label: field_label.to_owned(),
        };
    }
    ExistingFieldCommitPlan::ExistingReference
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
        OpPickerStage::Account => "1Password".to_owned(),
        OpPickerStage::Vault => {
            if multi_account {
                account_email.to_owned()
            } else {
                "1Password".to_owned()
            }
        }
        OpPickerStage::Item
        | OpPickerStage::NewItemName
        | OpPickerStage::FieldLabel
        | OpPickerStage::NewSectionName => {
            if multi_account {
                format!("{account_email} \u{2192} {vault_name}")
            } else {
                vault_name.to_owned()
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

/// Classify a probe `anyhow::Error` via typed [`jackin_core::OpProbeError`]
/// when attached as a source; falls back to substring matching for
/// stringified transports.
pub fn classify_probe_error(error: &anyhow::Error) -> OpPickerError {
    if let Some(probe) = error.downcast_ref::<jackin_core::OpProbeError>() {
        return match probe {
            jackin_core::OpProbeError::NotInstalled { .. } => {
                OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
            }
            jackin_core::OpProbeError::NotSignedIn { .. } => {
                OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
            }
            // Timeout has no dedicated picker fatal state; same GenericFatal
            // path the substring classifier used for timeout wording.
            jackin_core::OpProbeError::Timeout { .. }
            | jackin_core::OpProbeError::Other { .. } => {
                OpPickerError::Fatal(OpPickerFatalState::GenericFatal {
                    message: error.to_string(),
                })
            }
        };
    }
    classify_probe_error_message(error.to_string())
}

/// Fallback classifier for string-only transports (and unit tests). Prefer
/// [`classify_probe_error`] when an `anyhow::Error` is available so typed
/// sources win over message wording.
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
pub fn section_choices_from_references<S>(
    references: impl IntoIterator<Item = S>,
) -> Vec<Option<String>>
where
    S: AsRef<str>,
{
    let mut out: Vec<Option<String>> = vec![None];
    for reference in references {
        if let Some(name) = jackin_core::op_reference::parse_op_reference(reference.as_ref())
            .and_then(|parts| parts.section)
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
pub fn browse_field_display_rows<S>(
    references: impl IntoIterator<Item = S>,
    collapsed_sections: &HashSet<String>,
) -> Vec<FieldDisplayRow>
where
    S: AsRef<str>,
{
    let mut unsectioned: Vec<usize> = Vec::new();
    let mut sections: Vec<(String, Vec<usize>)> = Vec::new();

    for (idx, reference) in references.into_iter().enumerate() {
        match jackin_core::op_reference::parse_op_reference(reference.as_ref())
            .and_then(|parts| parts.section)
        {
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
pub fn create_field_display_rows<S>(
    references: impl IntoIterator<Item = S>,
    selected_section: Option<&str>,
) -> Vec<FieldDisplayRow>
where
    S: AsRef<str>,
{
    let mut rows: Vec<FieldDisplayRow> = references
        .into_iter()
        .enumerate()
        .filter(|(_, reference)| {
            let section = jackin_core::op_reference::parse_op_reference(reference.as_ref())
                .and_then(|parts| parts.section);
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

pub fn filtered_vaults<'a>(filter: &str, vaults: &'a [OpPickerVault]) -> Vec<&'a OpPickerVault> {
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
    let mut out: Vec<Option<&OpPickerItem>> = filtered_items(filter, items)
        .into_iter()
        .map(Some)
        .collect();
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
        selected_item.name.to_owned()
    };

    if let Some(section_name) = jackin_core::op_reference::parse_op_reference(field.reference)
        .and_then(|parts| parts.section)
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
