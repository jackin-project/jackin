// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, sync::atomic::Ordering};

use super::{LiveRepository, OwnedSnapshot, SetupControls};

#[test]
fn recovery_controls_collect_new_path_fixes_after_setup_cancellation() {
    let fixture = super::fixture();
    let live = LiveRepository::capture_at(fixture.path()).expect("capture fixture");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize staged tree");

    let new_path = snapshot.root.join("recovery-created.txt");
    fs::write(&new_path, "recovery patch\n").expect("write recovery-created file");
    snapshot
        .controls
        .cancellation()
        .store(true, Ordering::SeqCst);
    let recovery_controls = SetupControls::for_test(std::time::Duration::from_secs(10));
    assert!(snapshot.controls.cancellation().load(Ordering::SeqCst));
    assert!(!recovery_controls.cancellation().load(Ordering::SeqCst));
    recovery_controls
        .check_start()
        .expect("fresh recovery deadline remains live after setup cancellation");
    let patch = snapshot
        .collect_fixes(&tree, &recovery_controls)
        .expect("fresh recovery controls admit new-path staging and diff capture");
    assert!(
        String::from_utf8_lossy(&patch).contains("recovery patch"),
        "new-file fix remains available after setup cancellation"
    );
}
