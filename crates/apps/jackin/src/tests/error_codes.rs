// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::error::{ErrorCode, JackinError};

#[test]
fn error_code_labels_match_canonical_ids() {
    let cases = [
        (ErrorCode::E001, "E001"),
        (ErrorCode::E002, "E002"),
        (ErrorCode::E003, "E003"),
        (ErrorCode::E004, "E004"),
        (ErrorCode::E005, "E005"),
        (ErrorCode::E006, "E006"),
        (ErrorCode::E007, "E007"),
        (ErrorCode::E008, "E008"),
        (ErrorCode::E009, "E009"),
        (ErrorCode::E010, "E010"),
        (ErrorCode::E011, "E011"),
        (ErrorCode::E012, "E012"),
        (ErrorCode::E013, "E013"),
        (ErrorCode::E015, "E015"),
        (ErrorCode::E016, "E016"),
    ];
    for (code, expected) in cases {
        assert_eq!(code.as_str(), expected);
    }
    let mut labels: Vec<&str> = cases.iter().map(|(_, label)| *label).collect();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), cases.len());
}

#[test]
fn workspace_not_found_message_carries_name_and_code() {
    let message = JackinError::WorkspaceNotFound {
        name: "demo".to_owned(),
    }
    .user_message();
    assert_eq!(message.code, ErrorCode::E007);
    assert_eq!(message.code.as_str(), "E007");
    assert!(!message.headline.is_empty());
    assert!(
        message
            .more_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("demo"))
    );
}

#[test]
fn auth_and_conflict_errors_map_to_stable_codes() {
    assert_eq!(
        JackinError::GhAuthFailed.user_message().code.as_str(),
        "E012"
    );
    let message = JackinError::ContainerNameConflict {
        name: "jk-abc123xy".to_owned(),
    }
    .user_message();
    assert_eq!(message.code, ErrorCode::E009);
    assert_eq!(message.code.as_str(), "E009");
    assert!(
        message
            .more_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("jk-abc123xy"))
    );
}
