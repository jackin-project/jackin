// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn info(id: &str, status: AppleContainerStatus) -> AppleContainerInfo {
    AppleContainerInfo {
        id: id.to_owned(),
        status,
        launch_generation: None,
    }
}

#[tokio::test]
async fn stable_running_and_stopped_states_record_the_matching_outcome() {
    for (status, expected) in [
        (
            AppleContainerStatus::Running,
            AttachOutcome::still_running(),
        ),
        (AppleContainerStatus::Stopped, AttachOutcome::stopped(17)),
    ] {
        let recorded = std::cell::RefCell::new(Vec::new());
        record_attach_outcome_with(
            "jk-a",
            Some(17),
            || async move { Ok(Some(info("jk-a", status))) },
            |outcome| {
                recorded.borrow_mut().push(outcome);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(*recorded.borrow(), vec![expected]);
    }
}

#[tokio::test]
async fn absent_inventory_does_not_call_manifest_recorder() {
    let calls = std::cell::Cell::new(0);
    record_attach_outcome_with(
        "jk-a",
        Some(0),
        || async { Ok(None) },
        |_| {
            calls.set(calls.get() + 1);
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(calls.get(), 0);
}

#[tokio::test]
async fn unknown_and_stopping_observations_leave_existing_manifest_untouched() {
    for status in [
        AppleContainerStatus::Unknown,
        AppleContainerStatus::Stopping,
    ] {
        let calls = std::cell::Cell::new(0);
        record_attach_outcome_with(
            "jk-a",
            Some(9),
            || async move { Ok(Some(info("jk-a", status))) },
            |_| {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(calls.get(), 0);
    }
}

#[tokio::test]
async fn inventory_error_and_mismatched_id_do_not_call_manifest_recorder() {
    let error_calls = std::cell::Cell::new(0);
    let error = record_attach_outcome_with(
        "jk-a",
        Some(0),
        || async { Err(anyhow::anyhow!("fixture inventory error")) },
        |_| {
            error_calls.set(error_calls.get() + 1);
            Ok(())
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("fixture inventory error"));
    assert_eq!(error_calls.get(), 0);

    let mismatch_calls = std::cell::Cell::new(0);
    assert!(
        record_attach_outcome_with(
            "jk-a",
            Some(0),
            || async { Ok(Some(info("jk-a-extra", AppleContainerStatus::Running))) },
            |_| {
                mismatch_calls.set(mismatch_calls.get() + 1);
                Ok(())
            },
        )
        .await
        .is_err()
    );
    assert_eq!(mismatch_calls.get(), 0);
}

#[tokio::test]
async fn manifest_record_failure_is_best_effort() {
    let calls = std::cell::Cell::new(0);
    let result = record_attach_outcome_with(
        "jk-a",
        Some(4),
        || async { Ok(Some(info("jk-a", AppleContainerStatus::Stopped))) },
        |_| {
            calls.set(calls.get() + 1);
            Err(anyhow::anyhow!("fixture manifest write error"))
        },
    )
    .await;
    result.unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn unknown_and_stopping_status_have_no_attach_outcome() {
    assert_eq!(
        outcome_for_status(AppleContainerStatus::Unknown, Some(3)),
        None
    );
    assert_eq!(
        outcome_for_status(AppleContainerStatus::Stopping, Some(3)),
        None
    );
}

#[test]
fn stopped_without_observed_exit_code_uses_failure_sentinel() {
    assert_eq!(
        outcome_for_status(AppleContainerStatus::Stopped, None),
        Some(AttachOutcome::stopped(-1))
    );
}
