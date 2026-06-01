//! Input/update handlers for the 1Password picker.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use jackin_console::tui::components::list_helpers::{
    clamp_selection, cycle_select, first_selection, list_state_for_count, selected_choice,
};
use jackin_tui::ModalOutcome;
use jackin_tui::components::TextInputState;

use super::{
    FieldDisplayRow, FieldLabelOrigin, OpField, OpItem, OpLoadState, OpPickerError,
    OpPickerSelection, OpPickerStage, OpPickerState, build_op_ref_on_commit,
    filter_reset_selection_for_stage,
};

impl OpPickerState {
    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        // Naming sub-stages are pure text input (no async load), so the
        // load-state guards must not swallow their keys.
        match self.stage {
            OpPickerStage::NewItemName => return self.handle_new_item_name_key(key),
            OpPickerStage::FieldLabel => return self.handle_field_label_key(key),
            OpPickerStage::NewSectionName => return self.handle_new_section_name_key(key),
            _ => {}
        }

        if matches!(self.load_state, OpLoadState::Error(OpPickerError::Fatal(_))) {
            return if matches!(key.code, KeyCode::Esc) {
                ModalOutcome::Cancel
            } else {
                ModalOutcome::Continue
            };
        }

        if matches!(self.load_state, OpLoadState::Loading { .. }) {
            return if matches!(key.code, KeyCode::Esc) {
                ModalOutcome::Cancel
            } else {
                ModalOutcome::Continue
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

    fn handle_account_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match key.code {
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Re-fires the probe so add/remove of signed-in
                // accounts mid-session is picked up without restart.
                self.op_cache.borrow_mut().invalidate_accounts();
                self.accounts.clear();
                self.account_list_state = list_state_for_count(0);
                self.selected_account = None;
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
                if let Some(a) = selected_choice(&visible, self.account_list_state.selected) {
                    let a = (*a).clone();
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

    fn handle_vault_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match key.code {
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let account_id = self.selected_account_id();
                self.op_cache
                    .borrow_mut()
                    .invalidate_vaults(account_id.as_deref());
                self.vaults.clear();
                self.vault_list_state = list_state_for_count(0);
                self.selected_vault = None;
                self.start_vault_load(account_id);
                ModalOutcome::Continue
            }
            KeyCode::Esc => {
                // `self.accounts` is non-empty iff this is a multi-account
                // session (see the invariant in `handle_accounts_loaded`).
                if self.accounts.len() > 1 {
                    self.stage = OpPickerStage::Account;
                    self.filter_buf.clear();
                    self.selected_vault = None;
                    self.vaults.clear();
                    self.vault_list_state = list_state_for_count(0);
                    // Discard banners from the prior vault load so they
                    // don't bleed into the Account pane.
                    self.load_state = OpLoadState::Ready;
                    return ModalOutcome::Continue;
                }
                ModalOutcome::Cancel
            }
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
                if let Some(v) = selected_choice(&visible, self.vault_list_state.selected) {
                    let v = (*v).clone();
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

    fn handle_item_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match key.code {
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let account_id = self.selected_account_id();
                let vault_id = self
                    .selected_vault
                    .as_ref()
                    .map(|v| v.id.clone())
                    .unwrap_or_default();
                self.op_cache
                    .borrow_mut()
                    .invalidate_items(account_id.as_deref(), &vault_id);
                self.items.clear();
                self.item_list_state = list_state_for_count(0);
                self.start_item_load(vault_id, account_id);
                ModalOutcome::Continue
            }
            KeyCode::Esc => {
                self.stage = OpPickerStage::Vault;
                self.filter_buf.clear();
                self.items.clear();
                self.selected_item = None;
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
                let picked: Option<Option<OpItem>> =
                    selected_choice(&visible, self.item_list_state.selected)
                        .map(|choice| choice.map(Clone::clone));
                match picked {
                    Some(Some(item)) => {
                        let item_id = item.id.clone();
                        let vault_id = self
                            .selected_vault
                            .as_ref()
                            .map(|v| v.id.clone())
                            .unwrap_or_default();
                        let account_id = self.selected_account_id();
                        self.selected_item = Some(item);
                        self.start_field_load(item_id, vault_id, account_id);
                    }
                    Some(None) => {
                        self.stage = OpPickerStage::NewItemName;
                    }
                    None => {}
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
    fn handle_section_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        let choices = self.section_choices();
        let sentinel_idx = choices.len();
        match key.code {
            KeyCode::Esc => {
                // Mirror the Field-stage Esc back to Item.
                self.stage = OpPickerStage::Item;
                self.filter_buf.clear();
                self.fields.clear();
                self.collapsed_sections.clear();
                self.selected_section = None;
                self.selected_item = None;
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
                if self.section_list_state.selected.unwrap_or(0) == sentinel_idx {
                    self.section_name_input = TextInputState::new("Section name", "");
                    self.stage = OpPickerStage::NewSectionName;
                } else if let Some(choice) =
                    selected_choice(&choices, self.section_list_state.selected)
                {
                    self.selected_section.clone_from(choice);
                    self.stage = OpPickerStage::Field;
                    self.filter_buf.clear();
                    let n = self.build_field_display_rows().len();
                    self.field_list_state.select(first_selection(n));
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
        if self.mode.is_create() {
            self.stage = OpPickerStage::Section;
            self.selected_section = None;
            // `section_choices()` + the `+ New section` sentinel always
            // yield at least two rows, so index 0 is always valid.
            self.section_list_state = list_state_for_count(self.section_choices().len() + 1);
        } else {
            self.stage = OpPickerStage::Item;
            self.fields.clear();
            self.collapsed_sections.clear();
            self.selected_item = None;
        }
    }

    fn handle_field_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match key.code {
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let account_id = self.selected_account_id();
                let vault_id = self
                    .selected_vault
                    .as_ref()
                    .map(|v| v.id.clone())
                    .unwrap_or_default();
                let item_id = self
                    .selected_item
                    .as_ref()
                    .map(|i| i.id.clone())
                    .unwrap_or_default();
                self.op_cache.borrow_mut().invalidate_fields(
                    account_id.as_deref(),
                    &vault_id,
                    &item_id,
                );
                self.fields.clear();
                self.field_list_state = list_state_for_count(0);
                self.collapsed_sections.clear();
                // In-place refresh: the operator is already on the Field
                // stage with a chosen section. Flag the reload so the
                // Fields-loaded arm rebuilds the rows here instead of
                // kicking back to Section (Create mode). No-op in Browse.
                self.field_refresh_in_place = self.mode.is_create();
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
                if let Some(FieldDisplayRow::SectionHeader { name, .. }) =
                    self.build_field_display_rows().into_iter().nth(cur)
                {
                    self.set_section_collapsed(name, true);
                }
                ModalOutcome::Continue
            }
            KeyCode::Right => {
                let cur = self.field_list_state.selected.unwrap_or(0);
                if let Some(FieldDisplayRow::SectionHeader { name, .. }) =
                    self.build_field_display_rows().into_iter().nth(cur)
                {
                    self.set_section_collapsed(name, false);
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
                match self.build_field_display_rows().into_iter().nth(cur) {
                    Some(FieldDisplayRow::SectionHeader { name, .. }) => {
                        self.toggle_section_collapse(name);
                    }
                    Some(FieldDisplayRow::Field { field_idx }) => {
                        if let Some(field) = visible.get(field_idx) {
                            return ModalOutcome::Commit(self.commit_existing_field(field));
                        }
                    }
                    Some(FieldDisplayRow::NewFieldSentinel) => {
                        // The Field stage is scoped to the chosen section, so
                        // the new field lands there too.
                        self.pending_section = self.selected_section.clone();
                        self.field_label_origin = FieldLabelOrigin::NewField;
                        self.stage = OpPickerStage::FieldLabel;
                    }
                    // Create mode no longer surfaces NewSectionSentinel on the
                    // Field stage; section creation lives on the Section stage.
                    Some(FieldDisplayRow::NewSectionSentinel) | None => {}
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

    fn handle_new_item_name_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match self.item_name_input.handle_key(key) {
            ModalOutcome::Cancel => {
                self.stage = OpPickerStage::Item;
                ModalOutcome::Continue
            }
            // selected_item stays None => FieldLabel commit takes the new-item path.
            ModalOutcome::Commit(_) => {
                self.field_label_origin = FieldLabelOrigin::NewItem;
                self.stage = OpPickerStage::FieldLabel;
                ModalOutcome::Continue
            }
            ModalOutcome::Continue => ModalOutcome::Continue,
        }
    }

    fn handle_new_section_name_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match self.section_name_input.handle_key(key) {
            ModalOutcome::Cancel => {
                // The `+ New section` entry point lives on the Section stage.
                self.stage = OpPickerStage::Section;
                ModalOutcome::Continue
            }
            ModalOutcome::Commit(name) => {
                // Trim so a whitespace-padded section name can't reach the
                // op section label / derived id.
                self.pending_section = Some(name.trim().to_string());
                self.field_label_origin = FieldLabelOrigin::NewSection;
                self.stage = OpPickerStage::FieldLabel;
                ModalOutcome::Continue
            }
            ModalOutcome::Continue => ModalOutcome::Continue,
        }
    }

    fn handle_field_label_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match self.field_label_input.handle_key(key) {
            ModalOutcome::Cancel => {
                self.stage = self.field_label_origin.cancel_stage();
                // The section was staged immediately before this stage
                // (new-section name or the drilled section for a new field);
                // backing out discards that choice so it cannot leak into a
                // later commit on a different path.
                self.pending_section = None;
                ModalOutcome::Continue
            }
            ModalOutcome::Commit(label) => {
                let vault = self
                    .selected_vault
                    .clone()
                    .expect("vault set before field-label commit");
                // Trim the field label so leading/trailing whitespace can't
                // reach the op field id/label (item_name is trimmed too).
                let field_label = label.trim().to_string();
                if let Some(item) = self.selected_item.clone() {
                    ModalOutcome::Commit(OpPickerSelection::EditItemField {
                        account: self.selected_account.clone(),
                        vault,
                        item,
                        section: self.pending_section.take(),
                        // Typed label = a new field to append.
                        field: crate::operator_env::FieldTarget::New { label: field_label },
                    })
                } else {
                    ModalOutcome::Commit(OpPickerSelection::NewItem {
                        account: self.selected_account.clone(),
                        vault,
                        item_name: self.item_name_input.trimmed_value(),
                        section: self.pending_section.take(),
                        field_label,
                    })
                }
            }
            ModalOutcome::Continue => ModalOutcome::Continue,
        }
    }

    fn toggle_section_collapse(&mut self, name: String) {
        let collapsed = self.collapsed_sections.contains(name.as_str());
        self.set_section_collapsed(name, !collapsed);
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
    fn commit_existing_field(&self, field: &OpField) -> OpPickerSelection {
        if self.mode.is_create() {
            return OpPickerSelection::EditItemField {
                account: self.selected_account.clone(),
                vault: self
                    .selected_vault
                    .clone()
                    .expect("vault set before field commit"),
                item: self
                    .selected_item
                    .clone()
                    .expect("item set before field commit"),
                section: self.selected_section.clone(),
                field: crate::operator_env::FieldTarget::Existing {
                    id: field.id.clone(),
                    label: field.label.clone(),
                },
            };
        }
        OpPickerSelection::Existing(build_op_ref_on_commit(self, field))
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
