//! Derived list/query helpers for the 1Password picker.

use super::{
    FieldDisplayRow, OpPickerAccount, OpPickerField, OpPickerItem, OpPickerState,
    OpPickerVault, field_display_rows_for_picker, filtered_accounts, filtered_fields,
    filtered_item_choices, filtered_items, filtered_vaults, naming_stage_input_for_stage,
    section_choices_from_references,
};

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
    pub const fn naming_stage_input(
        &self,
    ) -> Option<&jackin_tui::components::TextInputState<'static>> {
        naming_stage_input_for_stage(
            self.stage,
            &self.item_name_input,
            &self.field_label_input,
            &self.section_name_input,
        )
    }
}
