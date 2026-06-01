//! Shared 1Password picker modal state enums.

use std::collections::HashSet;

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

#[derive(Debug, Clone)]
pub enum OpPickerFatalState {
    NotInstalled,
    NotSignedIn,
    NoVaults,
    GenericFatal { message: String },
}

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

pub fn matches_filter<'a>(filter: &str, values: impl IntoIterator<Item = &'a str>) -> bool {
    if filter.is_empty() {
        return true;
    }
    let needle = filter.to_lowercase();
    values
        .into_iter()
        .any(|value| value.to_lowercase().contains(&needle))
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
