// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn binary_policy_explicitly_excludes_developer_tools() {
    assert_eq!(
        lifecycle_policy(BinaryKind::Host),
        LifecyclePolicy::Product(AppMode::OneShot)
    );
    assert_eq!(
        lifecycle_policy(BinaryKind::Role),
        LifecyclePolicy::Product(AppMode::OneShot)
    );
    assert_eq!(
        lifecycle_policy(BinaryKind::BuildCapsuleDeveloperTool),
        LifecyclePolicy::DeveloperExcluded
    );
}

#[test]
fn result_classification_aligns_exit_outcome_and_error() {
    assert_eq!(classify_result(&Ok(())), ResultClassification::SUCCESS);
    let cancelled = Err(jackin_runtime::runtime::progress::LaunchCancelled::err());
    assert_eq!(
        classify_result(&cancelled),
        ResultClassification::CANCELLATION
    );
    let timeout = Err(anyhow::Error::new(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "timeout",
    )));
    assert_eq!(
        classify_result(&timeout),
        ResultClassification {
            exit_code: 1,
            outcome: OutcomeValue::Timeout,
            error_type: Some(ErrorType::Timeout),
        }
    );
    for code in [
        crate::error::ErrorCode::E001,
        crate::error::ErrorCode::E002,
        crate::error::ErrorCode::E003,
        crate::error::ErrorCode::E004,
        crate::error::ErrorCode::E005,
        crate::error::ErrorCode::E006,
        crate::error::ErrorCode::E007,
        crate::error::ErrorCode::E008,
        crate::error::ErrorCode::E009,
        crate::error::ErrorCode::E010,
        crate::error::ErrorCode::E011,
        crate::error::ErrorCode::E012,
        crate::error::ErrorCode::E013,
        crate::error::ErrorCode::E014,
        crate::error::ErrorCode::E015,
        crate::error::ErrorCode::E016,
    ] {
        assert!(!code.telemetry_error().as_str().is_empty());
    }
}
