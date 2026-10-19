// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn timeout_closes_probe_before_detached_worker_resumes() {
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (late_result_tx, late_result_rx) = std::sync::mpsc::channel();

    let result = run_probe_with_liveness(Duration::from_millis(50), move |liveness| {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        let mut payload_replaced = false;
        let committed = liveness.commit_if_current(&mut || {
            payload_replaced = true;
            true
        });
        late_result_tx
            .send((
                liveness.is_current(),
                liveness.admit_operation(),
                committed,
                payload_replaced,
            ))
            .unwrap();
    });

    assert_eq!(result, Err(ProbeBudgetExpired));
    started_rx
        .recv()
        .expect("detached worker started before its budget expired");
    release_tx.send(()).unwrap();
    assert_eq!(
        late_result_rx.recv().unwrap(),
        (false, false, None, false),
        "timeout must close the fence, refuse admission, and skip the payload replace"
    );
}

#[test]
fn open_fence_commit_runs_payload_replace() {
    let liveness = ProbeLiveness::new();
    assert!(liveness.is_current());
    assert!(liveness.admit_operation());
    let mut payload_replaced = false;
    let committed = liveness.commit_if_current(&mut || {
        payload_replaced = true;
        true
    });
    assert_eq!(committed, Some(true));
    assert!(payload_replaced);
}
