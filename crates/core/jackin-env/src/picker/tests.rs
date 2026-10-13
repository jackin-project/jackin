// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::apply_field_edit;
use jackin_core::{FieldTarget, OpSection, OpSectionTarget};

#[test]
fn duplicate_same_section_label_is_rejected_without_mutating_item() {
    let mut item = serde_json::json!({
        "sections": [{ "id": "credentials", "label": "Credentials" }],
        "fields": [
            { "id": "first", "label": "API token", "section": { "id": "credentials" }, "value": "old-first" },
            { "id": "second", "label": "API token", "section": { "id": "credentials" }, "value": "old-second" }
        ]
    });
    let before = item.clone();
    let target = FieldTarget::New {
        label: "API token".to_owned(),
    };
    let section = OpSectionTarget::Existing(OpSection {
        id: "credentials".to_owned(),
        label: "Credentials".to_owned(),
    });

    let result = apply_field_edit(&mut item, &target, "new-secret", Some(&section));
    assert!(
        result.is_err(),
        "duplicate section-scoped targets must fail"
    );
    let message = result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(message.contains("ambiguous"), "{message}");
    assert_eq!(item, before, "the rejected edit must be mutation-free");
}

#[test]
fn duplicate_existing_field_ids_are_rejected_without_mutating_item() {
    let mut item = serde_json::json!({
        "fields": [
            { "id": "field-id", "label": "First", "value": "old-first" },
            { "id": "field-id", "label": "Second", "value": "old-second" }
        ]
    });
    let before = item.clone();
    let target = FieldTarget::Existing {
        id: "field-id".to_owned(),
        label: "First".to_owned(),
    };

    let result = apply_field_edit(&mut item, &target, "new-secret", None);
    assert!(result.is_err(), "duplicate opaque IDs must fail");
    let message = result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(message.contains("ambiguous"), "{message}");
    assert_eq!(item, before, "the rejected edit must be mutation-free");
}

#[test]
fn duplicate_opaque_section_records_are_rejected_before_edit() {
    let mut item = serde_json::json!({
        "sections": [
            { "id": "credentials", "label": "Credentials" },
            { "id": "credentials", "label": "Credentials" }
        ],
        "fields": [
            { "id": "field-id", "label": "Token", "reference": "op://vault/item/credentials/field-id", "value": "old" }
        ]
    });
    let before = item.clone();
    let target = FieldTarget::Existing {
        id: "field-id".to_owned(),
        label: "Token".to_owned(),
    };

    let result = apply_field_edit(&mut item, &target, "new-secret", None);
    assert!(
        result.is_err(),
        "duplicate section IDs must fail before write"
    );
    let message = result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(message.contains("duplicate section records"), "{message}");
    assert_eq!(item, before, "the rejected edit must be mutation-free");
}

#[test]
fn same_label_in_another_section_does_not_match_selected_field() {
    let mut item = serde_json::json!({
        "sections": [
            { "id": "credentials", "label": "Credentials" },
            { "id": "metadata", "label": "Metadata" }
        ],
        "fields": [
            { "id": "credential-token", "label": "Token", "section": { "id": "credentials" }, "value": "credential-old" },
            { "id": "metadata-token", "label": "Token", "section": { "id": "metadata" }, "value": "metadata-old" }
        ]
    });
    let target = FieldTarget::New {
        label: "Token".to_owned(),
    };
    let section = OpSectionTarget::Existing(OpSection {
        id: "credentials".to_owned(),
        label: "Credentials".to_owned(),
    });

    let result = apply_field_edit(&mut item, &target, "credential-new", Some(&section));
    assert!(
        result.is_ok(),
        "one selected-section match should be writable"
    );
    let Some(edit) = result.ok() else {
        return;
    };

    assert_eq!(edit.existing_field_id.as_deref(), Some("credential-token"));
    assert_eq!(
        item.pointer("/fields/0/value")
            .and_then(serde_json::Value::as_str),
        Some("credential-new")
    );
    assert_eq!(
        item.pointer("/fields/1/value")
            .and_then(serde_json::Value::as_str),
        Some("metadata-old")
    );
}
