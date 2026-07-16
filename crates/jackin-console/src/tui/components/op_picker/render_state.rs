//! Adapter from op-picker state to render-only row builders.

use ratatui::text::Line;

use super::{
    OpLoadState, OpPickerAccountRef, OpPickerFieldDisplayRef, OpPickerItemRef, OpPickerRenderState,
    OpPickerStage, OpPickerVaultRef, account_lines, field_lines, item_choice_lines, section_lines,
    selected_entity_label_or_empty, selected_index_for_stage, vault_lines,
};

impl OpPickerRenderState for crate::tui::op_picker::OpPickerState {
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
        selected_entity_label_or_empty(self.selected_account.as_ref(), |account| {
            account.email.as_str()
        })
    }

    fn selected_vault_name(&self) -> &str {
        selected_entity_label_or_empty(self.selected_vault.as_ref(), |vault| vault.name.as_str())
    }

    fn selected_item_name(&self) -> &str {
        selected_entity_label_or_empty(self.selected_item.as_ref(), |item| item.name.as_str())
    }

    fn selected_item_subtitle(&self) -> &str {
        selected_entity_label_or_empty(self.selected_item.as_ref(), |item| item.subtitle.as_str())
    }

    fn naming_stage_input(&self) -> Option<&jackin_console_oppicker::TextInputState<'static>> {
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
