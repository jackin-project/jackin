//! Input/update handlers for the 1Password picker.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use jackin_tui::ModalOutcome;

use crate::state::{OpPickerState, list_state_for_count};
use crate::{
    AccountStageCommitPlan, ExistingFieldCommitSelectionInput, FieldStageCommitPlan,
    ItemStageCommitPlan, OpLoadState, OpPickerBlockedLoadKeyPlan, OpPickerCoreSelection,
    OpPickerField, OpPickerItem, OpPickerStage, SectionCollapseIntent, SectionStageCommitPlan,
    VaultStageBackPlan, VaultStageCommitPlan, account_stage_commit_plan,
    account_stage_refresh_plan, blocked_load_key_plan, existing_field_commit_plan,
    existing_field_commit_selection, field_label_cancel_plan, field_label_commit_plan,
    field_label_commit_selection, field_stage_back_plan, field_stage_commit_plan,
    field_stage_refresh_plan, filter_reset_selection_for_stage, item_stage_back_plan,
    item_stage_commit_plan, item_stage_refresh_plan, new_item_name_commit_plan,
    new_section_name_commit_plan, section_header_collapse_target, section_name_input_state,
    section_stage_back_plan, section_stage_commit_plan, vault_stage_back_plan,
    vault_stage_commit_plan, vault_stage_refresh_plan,
};

use crate::first_selection;

impl OpPickerState {
    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerCoreSelection> {
        // Naming sub-stages are pure text input (no async load), so the
        // load-state guards must not swallow their keys.
        match self.stage {
            OpPickerStage::NewItemName => return self.handle_new_item_name_key(key),
            OpPickerStage::FieldLabel => return self.handle_field_label_key(key),
            OpPickerStage::NewSectionName => return self.handle_new_section_name_key(key),
            _ => {}
        }

        if let Some(plan) =
            blocked_load_key_plan(&self.load_state, matches!(key.code, KeyCode::Esc))
        {
            return match plan {
                OpPickerBlockedLoadKeyPlan::Cancel => ModalOutcome::Cancel,
                OpPickerBlockedLoadKeyPlan::Continue => ModalOutcome::Continue,
            };
        }

        match self.stage {
            OpPickerStage::Account => self.handle_account_key(key),
            OpPickerStage::Vault => self.handle_vault_key(key),
            OpPickerStage::Item => self.handle_item_key(key),
            OpPickerStage::Section => self.handle_section_key(key),
            OpPickerStage::Field => self.handle_field_key(key),
            OpPickerStage::NewItemName
            | OpPickerStage::FieldLabel
            | OpPickerStage::NewSectionName => ModalOutcome::Continue,
        }
    }

    fn handle_account_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerCoreSelection> {
        match key.code {
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char('r' | 'R') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Re-fires the probe so add/remove of signed-in
                // accounts mid-session is picked up without restart.
                let plan = account_stage_refresh_plan();
                self.op_cache.borrow_mut().invalidate_accounts();
                if plan.clear_accounts {
                    self.accounts.clear();
                }
                if plan.reset_account_list {
                    self.account_list_state = list_state_for_count(0);
                }
                if plan.clear_selected_account {
                    self.selected_account = None;
                }
                self.start_account_load();
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                let n = self.filtered_accounts().len();
                cycle_select(&mut self.account_list_state, n, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_accounts().len();
                cycle_select(&mut self.account_list_state, n, 1);
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                self.filter_buf.pop();
                self.reset_selection_for_filter(OpPickerStage::Account);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                let visible = self.filtered_accounts();
                let picked = selected_choice(&visible, self.account_list_state.selected)
                    .map(|a| (*a).clone());
                if let AccountStageCommitPlan::ExistingAccount(a) =
                    account_stage_commit_plan(picked)
                {
                    let id = a.id.clone();
                    self.selected_account = Some(a);
                    self.start_vault_load(Some(id));
                }
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                self.reset_selection_for_filter(OpPickerStage::Account);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    fn handle_vault_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerCoreSelection> {
        match key.code {
            KeyCode::Char('r' | 'R') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let account_id = self.selected_account_id();
                let plan = vault_stage_refresh_plan();
                self.op_cache
                    .borrow_mut()
                    .invalidate_vaults(account_id.as_deref());
                if plan.clear_vaults {
                    self.vaults.clear();
                }
                if plan.reset_vault_list {
                    self.vault_list_state = list_state_for_count(0);
                }
                if plan.clear_selected_vault {
                    self.selected_vault = None;
                }
                self.start_vault_load(account_id);
                ModalOutcome::Continue
            }
            KeyCode::Esc => match vault_stage_back_plan(self.accounts.len()) {
                VaultStageBackPlan::BackToAccount {
                    stage,
                    clear_selected_vault,
                    clear_vaults,
                    reset_vault_list,
                    ready_load_state,
                } => {
                    self.stage = stage;
                    self.filter_buf.clear();
                    if clear_selected_vault {
                        self.selected_vault = None;
                    }
                    if clear_vaults {
                        self.vaults.clear();
                    }
                    if reset_vault_list {
                        self.vault_list_state = list_state_for_count(0);
                    }
                    if ready_load_state {
                        self.load_state = OpLoadState::Ready;
                    }
                    ModalOutcome::Continue
                }
                VaultStageBackPlan::Cancel => ModalOutcome::Cancel,
            },
            KeyCode::Up => {
                let n = self.filtered_vaults().len();
                cycle_select(&mut self.vault_list_state, n, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_vaults().len();
                cycle_select(&mut self.vault_list_state, n, 1);
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                self.filter_buf.pop();
                self.reset_selection_for_filter(OpPickerStage::Vault);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                let visible = self.filtered_vaults();
                let picked =
                    selected_choice(&visible, self.vault_list_state.selected).map(|v| (*v).clone());
                if let VaultStageCommitPlan::ExistingVault(v) = vault_stage_commit_plan(picked) {
                    let id = v.id.clone();
                    let account_id = self.selected_account_id();
                    self.selected_vault = Some(v);
                    self.start_item_load(id, account_id);
                }
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                self.reset_selection_for_filter(OpPickerStage::Vault);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    fn handle_item_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerCoreSelection> {
        match key.code {
            KeyCode::Char('r' | 'R') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let account_id = self.selected_account_id();
                let vault_id = self.selected_vault_id_or_default();
                self.op_cache
                    .borrow_mut()
                    .invalidate_items(account_id.as_deref(), &vault_id);
                let plan = item_stage_refresh_plan();
                if plan.clear_items {
                    self.items.clear();
                }
                if plan.reset_item_list {
                    self.item_list_state = list_state_for_count(0);
                }
                self.start_item_load(vault_id, account_id);
                ModalOutcome::Continue
            }
            KeyCode::Esc => {
                let plan = item_stage_back_plan();
                self.stage = plan.stage;
                self.filter_buf.clear();
                if plan.clear_items {
                    self.items.clear();
                }
                if plan.clear_selected_item {
                    self.selected_item = None;
                }
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                let n = self.filtered_item_choices().len();
                cycle_select(&mut self.item_list_state, n, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_item_choices().len();
                cycle_select(&mut self.item_list_state, n, 1);
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                self.filter_buf.pop();
                self.reset_selection_for_filter(OpPickerStage::Item);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                // `None` is the `+ New item` sentinel (Create mode only).
                let visible = self.filtered_item_choices();
                let picked: Option<Option<OpPickerItem>> =
                    selected_choice(&visible, self.item_list_state.selected)
                        .map(|choice| choice.map(Clone::clone));
                match item_stage_commit_plan(picked) {
                    ItemStageCommitPlan::ExistingItem(item) => {
                        let item_id = item.id.clone();
                        let vault_id = self.selected_vault_id_or_default();
                        let account_id = self.selected_account_id();
                        self.selected_item = Some(item);
                        self.start_field_load(item_id, vault_id, account_id);
                    }
                    ItemStageCommitPlan::NewItemName => {
                        self.stage = OpPickerStage::NewItemName;
                    }
                    ItemStageCommitPlan::NoSelection => {}
                }
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                self.reset_selection_for_filter(OpPickerStage::Item);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    /// Create-mode Section stage: pick `(root)` / an existing section /
    /// `+ New section`. The list has `section_choices().len()` choice rows
    /// followed by a single `+ New section` sentinel. No filtering; sections
    /// are few, so `Char` input is ignored here.
    fn handle_section_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerCoreSelection> {
        let choices = self.section_choices();
        let sentinel_idx = choices.len();
        match key.code {
            KeyCode::Esc => {
                let plan = section_stage_back_plan();
                self.stage = plan.stage;
                self.filter_buf.clear();
                if plan.clear_fields {
                    self.fields.clear();
                }
                if plan.clear_collapsed_sections {
                    self.collapsed_sections.clear();
                }
                if plan.clear_selected_section {
                    self.selected_section = None;
                }
                if plan.clear_selected_item {
                    self.selected_item = None;
                }
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                cycle_select(&mut self.section_list_state, sentinel_idx + 1, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                cycle_select(&mut self.section_list_state, sentinel_idx + 1, 1);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                match section_stage_commit_plan(self.section_list_state.selected, &choices) {
                    SectionStageCommitPlan::NewSectionName => {
                        self.section_name_input = section_name_input_state("");
                        self.stage = OpPickerStage::NewSectionName;
                    }
                    SectionStageCommitPlan::ExistingSection { selected_section } => {
                        self.selected_section = selected_section;
                        self.stage = OpPickerStage::Field;
                        self.filter_buf.clear();
                        let n = self.build_field_display_rows().len();
                        self.field_list_state.select(first_selection(n));
                    }
                    SectionStageCommitPlan::NoSelection => {}
                }
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    /// Esc back-nav from the Field stage. Create mode steps back to the
    /// Section stage (keeping the loaded fields); Browse steps back to Item.
    fn field_stage_back(&mut self) {
        self.filter_buf.clear();
        let plan = field_stage_back_plan(&self.mode);
        self.stage = plan.stage;
        if plan.reset_selected_section {
            self.selected_section = None;
        }
        if plan.reset_section_list {
            self.section_list_state = list_state_for_count(self.section_choices().len() + 1);
        }
        if plan.clear_fields {
            self.fields.clear();
        }
        if plan.clear_collapsed_sections {
            self.collapsed_sections.clear();
        }
        if plan.clear_selected_item {
            self.selected_item = None;
        }
    }

    fn handle_field_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerCoreSelection> {
        match key.code {
            KeyCode::Char('r' | 'R') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let account_id = self.selected_account_id();
                let vault_id = self.selected_vault_id_or_default();
                let item_id = self.selected_item_id_or_default();
                self.op_cache.borrow_mut().invalidate_fields(
                    account_id.as_deref(),
                    &vault_id,
                    &item_id,
                );
                let plan = field_stage_refresh_plan(&self.mode);
                if plan.clear_fields {
                    self.fields.clear();
                }
                if plan.reset_field_list {
                    self.field_list_state = list_state_for_count(0);
                }
                if plan.clear_collapsed_sections {
                    self.collapsed_sections.clear();
                }
                self.field_refresh_in_place = plan.refresh_in_place;
                self.start_field_load(item_id, vault_id, account_id);
                ModalOutcome::Continue
            }
            KeyCode::Esc => {
                self.field_stage_back();
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                let n = self.build_field_display_rows().len();
                cycle_select(&mut self.field_list_state, n, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.build_field_display_rows().len();
                cycle_select(&mut self.field_list_state, n, 1);
                ModalOutcome::Continue
            }
            KeyCode::Left => {
                let cur = self.field_list_state.selected.unwrap_or(0);
                let rows = self.build_field_display_rows();
                if let Some((name, collapsed)) = section_header_collapse_target(
                    rows.get(cur),
                    &self.collapsed_sections,
                    SectionCollapseIntent::Collapse,
                ) {
                    self.set_section_collapsed(name, collapsed);
                }
                ModalOutcome::Continue
            }
            KeyCode::Right => {
                let cur = self.field_list_state.selected.unwrap_or(0);
                let rows = self.build_field_display_rows();
                if let Some((name, collapsed)) = section_header_collapse_target(
                    rows.get(cur),
                    &self.collapsed_sections,
                    SectionCollapseIntent::Expand,
                ) {
                    self.set_section_collapsed(name, collapsed);
                }
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                self.filter_buf.pop();
                self.reset_selection_for_filter(OpPickerStage::Field);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                let visible = self.filtered_fields();
                let cur = self.field_list_state.selected.unwrap_or(0);
                let rows = self.build_field_display_rows();
                match field_stage_commit_plan(
                    rows.get(cur),
                    &self.collapsed_sections,
                    self.selected_section.as_deref(),
                ) {
                    FieldStageCommitPlan::ToggleSection { name, collapsed } => {
                        self.set_section_collapsed(name, collapsed);
                    }
                    FieldStageCommitPlan::ExistingField { field_idx } => {
                        if let Some(field) = visible.get(field_idx) {
                            return ModalOutcome::Commit(self.commit_existing_field(field));
                        }
                    }
                    FieldStageCommitPlan::NewField {
                        pending_section,
                        field_label_origin,
                        stage,
                    } => {
                        self.pending_section = pending_section;
                        self.field_label_origin = field_label_origin;
                        self.stage = stage;
                    }
                    FieldStageCommitPlan::NoSelection => {}
                }
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                self.reset_selection_for_filter(OpPickerStage::Field);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    fn handle_new_item_name_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerCoreSelection> {
        match self.item_name_input.handle_key(key) {
            ModalOutcome::Cancel => {
                self.stage = OpPickerStage::Item;
                ModalOutcome::Continue
            }
            // selected_item stays None => FieldLabel commit takes the new-item path.
            ModalOutcome::Commit(_) => {
                let plan = new_item_name_commit_plan();
                self.stage = plan.stage;
                if let Some(origin) = plan.field_label_origin {
                    self.field_label_origin = origin;
                }
                ModalOutcome::Continue
            }
            ModalOutcome::Continue => ModalOutcome::Continue,
        }
    }

    fn handle_new_section_name_key(
        &mut self,
        key: KeyEvent,
    ) -> ModalOutcome<OpPickerCoreSelection> {
        match self.section_name_input.handle_key(key) {
            ModalOutcome::Cancel => {
                // The `+ New section` entry point lives on the Section stage.
                self.stage = OpPickerStage::Section;
                ModalOutcome::Continue
            }
            ModalOutcome::Commit(name) => {
                let plan = new_section_name_commit_plan(&name);
                self.stage = plan.stage;
                self.pending_section = plan.pending_section;
                if let Some(origin) = plan.field_label_origin {
                    self.field_label_origin = origin;
                }
                ModalOutcome::Continue
            }
            ModalOutcome::Continue => ModalOutcome::Continue,
        }
    }

    fn handle_field_label_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerCoreSelection> {
        #[expect(
            clippy::expect_used,
            reason = "field-label commit is reachable only after a vault selection exists"
        )]
        match self.field_label_input.handle_key(key) {
            ModalOutcome::Cancel => {
                let plan = field_label_cancel_plan(self.field_label_origin);
                self.stage = plan.stage;
                if plan.clear_pending_section {
                    self.pending_section = None;
                }
                ModalOutcome::Continue
            }
            ModalOutcome::Commit(label) => {
                let vault = self
                    .selected_vault
                    .clone()
                    .expect("vault set before field-label commit");
                let plan = field_label_commit_plan(
                    self.selected_account.clone(),
                    vault,
                    self.selected_item.clone(),
                    self.pending_section.take(),
                    self.item_name_input.trimmed_value(),
                    &label,
                );
                ModalOutcome::Commit(field_label_commit_selection(plan, |label| {
                    jackin_core::FieldTarget::New { label }
                }))
            }
            ModalOutcome::Continue => ModalOutcome::Continue,
        }
    }

    /// Collapse (`collapsed = true`) or expand a section header, then clamp
    /// the field selection so it never dangles past the new row count.
    /// All three entry points (Enter toggle, Left collapse, Right expand)
    /// route here so the selection clamp stays in lockstep with the rows.
    fn set_section_collapsed(&mut self, name: String, collapsed: bool) {
        if collapsed {
            self.collapsed_sections.insert(name);
        } else {
            self.collapsed_sections.remove(name.as_str());
        }
        let new_len = self.build_field_display_rows().len();
        self.field_list_state
            .select(clamp_selection(self.field_list_state.selected, new_len));
    }

    /// Browse: commit the field's `op://` reference. Create: overwrite the
    /// field by its exact id. The consumer matches on `field_id` and preserves
    /// the field's existing section, so `selected_section` rides along only
    /// for display, not placement.
    #[expect(
        clippy::expect_used,
        reason = "existing-field commit is reachable only after vault and item selections exist"
    )]
    fn commit_existing_field(&self, field: &OpPickerField) -> OpPickerCoreSelection {
        let plan = existing_field_commit_plan(
            &self.mode,
            &field.id,
            &field.label,
            self.selected_section.clone(),
        );
        existing_field_commit_selection(
            plan,
            ExistingFieldCommitSelectionInput {
                account: self.selected_account.clone(),
                vault: self
                    .selected_vault
                    .clone()
                    .expect("vault set before field commit"),
                item: self
                    .selected_item
                    .clone()
                    .expect("item set before field commit"),
            },
            || self.build_op_ref_on_commit(field),
            |id, label| jackin_core::FieldTarget::Existing { id, label },
        )
    }

    fn reset_selection_for_filter(&mut self, stage: OpPickerStage) {
        let Some(selection) = filter_reset_selection_for_stage(
            stage,
            self.filtered_accounts().len(),
            self.filtered_vaults().len(),
            self.filtered_item_choices().len(),
            self.build_field_display_rows().len(),
        ) else {
            return;
        };
        match stage {
            OpPickerStage::Account => self.account_list_state.select(selection),
            OpPickerStage::Vault => self.vault_list_state.select(selection),
            OpPickerStage::Item => self.item_list_state.select(selection),
            OpPickerStage::Field => self.field_list_state.select(selection),
            _ => {}
        }
    }
}

fn cycle_select(list_state: &mut tui_widget_list::ListState, count: usize, delta: i32) {
    if count == 0 {
        return;
    }
    let cur = list_state.selected.unwrap_or(0);
    let next = if delta < 0 {
        if cur == 0 { count - 1 } else { cur - 1 }
    } else if cur + 1 >= count {
        0
    } else {
        cur + 1
    };
    list_state.select(Some(next));
}

#[must_use]
fn selected_choice<T>(choices: &[T], selected: Option<usize>) -> Option<&T> {
    selected.and_then(|index| choices.get(index))
}

#[must_use]
const fn clamp_selection(selected: Option<usize>, count: usize) -> Option<usize> {
    if count == 0 {
        None
    } else if let Some(selected) = selected {
        if selected >= count {
            Some(count - 1)
        } else {
            Some(selected)
        }
    } else {
        None
    }
}
