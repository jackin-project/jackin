// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::build_op_picker_ref;
use jackin_oppicker::{OpPickerFieldRef, OpPickerItemRef, OpPickerVaultRef, OpSection};

fn vault() -> OpPickerVaultRef<'static> {
    OpPickerVaultRef {
        id: "vault1",
        name: "Engineering",
    }
}

fn item() -> OpPickerItemRef<'static> {
    OpPickerItemRef {
        id: "item9",
        name: "Login",
        subtitle: "production",
    }
}

fn field<'a>(reference: &'a str, section_id: Option<&'a str>) -> OpPickerFieldRef<'a> {
    OpPickerFieldRef {
        id: "field7",
        label: "password",
        reference,
        section_id,
        section_label: None,
    }
}

#[test]
fn build_op_picker_ref_preserves_sections_and_ambiguous_subtitles() {
    let sections = [
        OpSection {
            id: "section1".to_owned(),
            label: "Alpha".to_owned(),
        },
        OpSection {
            id: "section2".to_owned(),
            label: "Beta".to_owned(),
        },
    ];
    let built = build_op_picker_ref(
        vault(),
        item(),
        [item()],
        field("op://vault1/item9/section2/field7", Some("section1")),
        [field("op://vault1/item9/section2/field7", Some("section1"))],
        &sections,
    )
    .expect("explicit section id wins over the reference segment");
    assert_eq!(built.op, "op://vault1/item9/section1/field7");
    assert_eq!(built.path, "Engineering/Login/Alpha/password");
    let shared = [
        OpSection {
            id: "section1".to_owned(),
            label: "Shared".to_owned(),
        },
        OpSection {
            id: "section2".to_owned(),
            label: "Shared".to_owned(),
        },
    ];
    let built = build_op_picker_ref(
        vault(),
        item(),
        [item()],
        field("op://vault1/item9/section2/field7", None),
        [field("op://vault1/item9/section2/field7", None)],
        &shared,
    )
    .expect("exact section id wins over an ambiguous label");
    assert_eq!(built.op, "op://vault1/item9/section2/field7");
    assert_eq!(built.path, "Engineering/Login/Shared/password");
    let owned = [OpSection {
        id: "section1".to_owned(),
        label: "Owner".to_owned(),
    }];
    let built = build_op_picker_ref(
        vault(),
        item(),
        [item()],
        field("op://vault1/item9/owner/field7", None),
        [field("op://vault1/item9/owner/field7", None)],
        &owned,
    )
    .expect("unique label resolves case-insensitively");
    assert_eq!(built.op, "op://vault1/item9/section1/field7");
    assert_eq!(built.path, "Engineering/Login/Owner/password");
    assert!(
        build_op_picker_ref(
            vault(),
            item(),
            [item()],
            field("op://vault1/item9/shared/field7", None),
            [field("op://vault1/item9/shared/field7", None)],
            &shared,
        )
        .is_none(),
        "ambiguous section reference must fail closed"
    );
}
