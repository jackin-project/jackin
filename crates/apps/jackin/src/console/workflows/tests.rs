// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::tui::screens::workflows::model::ObserveTarget;

use super::require_session_target;

fn target(instance: Option<&str>, session: Option<i64>) -> ObserveTarget {
    ObserveTarget {
        run_id: "run-1".to_owned(),
        worker_id: "implementer".to_owned(),
        instance_id: instance.map(str::to_owned),
        session_id: session,
        capsule_epoch: Some(2),
        process_identity: Some("pid:9".to_owned()),
    }
}

#[test]
fn target_requires_recorded_instance_and_session() {
    require_session_target(&target(Some("inst-1"), Some(7))).unwrap();
    // The host must never guess a session: every bad target fails.
    for bad in [
        target(None, Some(7)),
        target(Some(""), Some(7)),
        target(Some("inst-1"), None),
        target(Some("inst-1"), Some(-1)),
    ] {
        require_session_target(&bad).unwrap_err();
    }
}
