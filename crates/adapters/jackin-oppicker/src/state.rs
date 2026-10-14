use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::{LoadSubscription, TextInputState};
use termrock::interaction::{CollectionItem, CollectionState};

use crate::{
    FieldDisplayRow, FieldLabelOrigin, OpLoadState, OpPickerAccount, OpPickerCache, OpPickerField,
    OpPickerFieldRef, OpPickerItem, OpPickerItemRef, OpPickerLoadRequest, OpPickerLoadResult,
    OpPickerMode, OpPickerPendingLoad, OpPickerStage, OpPickerVault, OpPickerVaultRef, OpSection,
    build_op_picker_ref, field_display_rows_for_picker, filtered_accounts, filtered_fields,
    filtered_item_choices, filtered_items, filtered_vaults, naming_stage_input_for_stage,
    section_label_for_id, selected_account_id,
};

pub type LoadResult =
    OpPickerLoadResult<OpPickerAccount, OpPickerVault, OpPickerItem, OpPickerField>;

pub struct OpPickerState {
    pub stage: OpPickerStage,
    pub filter_buf: String,

    pub accounts: Vec<OpPickerAccount>,
    pub account_list_state: CollectionState<usize>,
    pub selected_account: Option<OpPickerAccount>,

    pub vaults: Vec<OpPickerVault>,
    pub vault_list_state: CollectionState<usize>,
    pub selected_vault: Option<OpPickerVault>,

    pub items: Vec<OpPickerItem>,
    pub item_list_state: CollectionState<usize>,
    pub selected_item: Option<OpPickerItem>,

    pub fields: Vec<OpPickerField>,
    pub sections: Vec<OpSection>,
    pub field_list_state: CollectionState<usize>,
    pub section_list_state: CollectionState<usize>,
    pub selected_section: Option<OpSection>,
    pub collapsed_sections: HashSet<String>,

    pub load_state: OpLoadState,

    pub mode: OpPickerMode,
    pub item_name_input: TextInputState<'static>,
    pub field_label_input: TextInputState<'static>,
    pub section_name_input: TextInputState<'static>,
    pub pending_section: Option<jackin_core::OpSectionTarget>,
    pub field_label_origin: FieldLabelOrigin,
    pub field_refresh_in_place: bool,

    pub rx: Option<LoadSubscription<LoadResult>>,
    pub pending_load: Option<OpPickerPendingLoad<LoadResult, OpPickerLoadRequest, ()>>,
    pub op_cache: Rc<RefCell<OpPickerCache>>,
}

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
            .field("sections", &self.sections)
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

    pub fn filtered_item_choices(&self) -> Vec<Option<&OpPickerItem>> {
        filtered_item_choices(&self.filter_buf, &self.items, &self.mode)
    }

    pub fn filtered_fields(&self) -> Vec<&OpPickerField> {
        filtered_fields(&self.filter_buf, &self.fields)
    }

    pub fn section_choices(&self) -> Vec<Option<OpSection>> {
        let mut choices = vec![None];
        let mut seen_ids = HashSet::new();
        for section in &self.sections {
            if seen_ids.insert(section.id.as_str()) {
                choices.push(Some(section.clone()));
            }
        }
        choices
    }

    pub fn build_field_display_rows(&self) -> Vec<FieldDisplayRow> {
        field_display_rows_for_picker(
            &self.mode,
            &self.filter_buf,
            &self.fields,
            &self.sections,
            self.selected_section
                .as_ref()
                .map(|section| section.id.as_str()),
            &self.collapsed_sections,
        )
    }

    pub const fn naming_stage_input(&self) -> Option<&TextInputState<'static>> {
        naming_stage_input_for_stage(
            self.stage,
            &self.item_name_input,
            &self.field_label_input,
            &self.section_name_input,
        )
    }

    #[expect(
        clippy::expect_used,
        reason = "op ref commit is reachable only after vault and item selections exist"
    )]
    pub fn build_op_ref_on_commit(&self, field: &OpPickerField) -> Option<jackin_core::OpRef> {
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
                section_id: field.section_id.as_deref(),
                section_label: field
                    .section_id
                    .as_deref()
                    .and_then(|section_id| section_label_for_id(section_id, &self.sections)),
            },
            self.fields.iter().map(|field| OpPickerFieldRef {
                id: &field.id,
                label: &field.label,
                reference: &field.reference,
                section_id: field.section_id.as_deref(),
                section_label: field
                    .section_id
                    .as_deref()
                    .and_then(|section_id| section_label_for_id(section_id, &self.sections)),
            }),
            &self.sections,
        )?;

        Some(jackin_core::OpRef {
            op: built.op,
            path: built.path,
            account: self.selected_account_id(),
            on_demand: false,
        })
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
pub(crate) fn collection_state_for_count(count: usize) -> CollectionState<usize> {
    let mut state = CollectionState::new();
    state.set_active(crate::first_selection(count));
    state
}

#[must_use]
pub(crate) fn index_collection_items(count: usize) -> Vec<CollectionItem<usize>> {
    (0..count)
        .map(|index| CollectionItem::new(index, String::new()))
        .collect()
}

fn scroll_select(list_state: &mut CollectionState<usize>, count: usize, delta: i16) -> bool {
    if count == 0 {
        let changed = list_state.active().is_some();
        list_state.set_active(None);
        return changed;
    }
    let current = list_state.active().copied().unwrap_or(0).min(count - 1);
    let delta = isize::from(delta);
    let next = if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta.unsigned_abs()).min(count - 1)
    };
    list_state.set_active(Some(next));
    next != current
}
