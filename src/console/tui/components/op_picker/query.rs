//! Derived list/query helpers for the 1Password picker.

use super::{
    FieldDisplayRow, OpPickerStage, OpPickerState, browse_field_display_rows,
    create_field_display_rows, matches_filter, section_choices_from_references,
};
use crate::operator_env::{OpAccount, OpField, OpItem, OpVault};

impl OpPickerState {
    pub fn filtered_accounts(&self) -> Vec<&OpAccount> {
        self.accounts
            .iter()
            .filter(|account| {
                matches_filter(
                    &self.filter_buf,
                    [account.email.as_str(), account.url.as_str()],
                )
            })
            .collect()
    }

    pub fn filtered_vaults(&self) -> Vec<&OpVault> {
        self.vaults
            .iter()
            .filter(|vault| matches_filter(&self.filter_buf, [vault.name.as_str()]))
            .collect()
    }

    pub fn filtered_items(&self) -> Vec<&OpItem> {
        self.items
            .iter()
            .filter(|item| {
                matches_filter(
                    &self.filter_buf,
                    [item.name.as_str(), item.subtitle.as_str()],
                )
            })
            .collect()
    }

    /// Filtered items, followed by a trailing `None` sentinel (the
    /// `+ New item` row) in Create mode. Browse mode emits no sentinel.
    pub fn filtered_item_choices(&self) -> Vec<Option<&OpItem>> {
        let mut out: Vec<Option<&OpItem>> = self.filtered_items().into_iter().map(Some).collect();
        if self.mode.is_create() {
            out.push(None);
        }
        out
    }

    pub fn filtered_fields(&self) -> Vec<&OpField> {
        self.fields
            .iter()
            .filter(|field| {
                matches_filter(&self.filter_buf, [field.label.as_str(), field.id.as_str()])
            })
            .collect()
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
        if self.mode.is_create() {
            return self.build_create_field_rows();
        }
        let visible = self.filtered_fields();
        browse_field_display_rows(
            visible.iter().map(|field| field.reference.as_str()),
            &self.collapsed_sections,
        )
    }

    fn build_create_field_rows(&self) -> Vec<FieldDisplayRow> {
        let visible = self.filtered_fields();
        create_field_display_rows(
            visible.iter().map(|field| field.reference.as_str()),
            self.selected_section.as_deref(),
        )
    }

    /// The input box for the current naming sub-stage, or `None` when the
    /// picker is in a list stage. Single source for render, sizing, and footer.
    pub const fn naming_stage_input(
        &self,
    ) -> Option<&jackin_tui::components::TextInputState<'static>> {
        match self.stage {
            OpPickerStage::NewItemName => Some(&self.item_name_input),
            OpPickerStage::FieldLabel => Some(&self.field_label_input),
            OpPickerStage::NewSectionName => Some(&self.section_name_input),
            _ => None,
        }
    }
}
