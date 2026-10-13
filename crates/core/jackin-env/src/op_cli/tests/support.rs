// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn create_params<'a>(
    field_label: &'a str,
    section: Option<&'a str>,
) -> OpItemCreateParams<'a> {
    OpItemCreateParams {
        vault_id: "vault-id",
        title: "Build token",
        category: "API_CREDENTIAL",
        field_label,
        value: "secret fixture",
        notes_plain: None,
        tags: &[],
        section,
    }
}

pub(super) fn created_item_with_fields(
    fields: Vec<RawCreatedItemField>,
    sections: Vec<RawCreatedItemSection>,
) -> RawCreatedItem {
    RawCreatedItem {
        id: "item-id".to_owned(),
        title: "Build token".to_owned(),
        vault: RawCreatedItemVault {
            id: "vault-id".to_owned(),
            name: "Private".to_owned(),
        },
        fields,
        sections,
    }
}

pub(super) fn created_field(id: &str, label: &str, section: Option<&str>) -> RawCreatedItemField {
    RawCreatedItemField {
        id: id.to_owned(),
        label: label.to_owned(),
        section: section.map(|id| RawCreatedItemFieldSection { id: id.to_owned() }),
    }
}

pub(super) fn created_section(id: &str, label: &str) -> RawCreatedItemSection {
    RawCreatedItemSection {
        id: id.to_owned(),
        label: label.to_owned(),
    }
}
