// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn hold_blocks_removal_until_released() {
    let dir = tempfile::TempDir::new().unwrap();
    let state = dir.path();
    assert!(list_holds(state).unwrap().is_empty());

    let hold = hold_work(state, "/work/clone", "checkpoint capture", None).unwrap();
    assert_eq!(hold.resource_id, "/work/clone");

    let err = check_removal(state, "/work/clone").expect_err("held removal must fail");
    assert!(format!("{err:#}").contains("preservation hold"));

    // Unrelated resources stay removable.
    check_removal(state, "/work/other").unwrap();

    // Duplicate holds are idempotent.
    hold_work(state, "/work/clone", "checkpoint capture", None).unwrap();
    assert_eq!(list_holds(state).unwrap().len(), 1);

    assert!(release_hold(state, "/work/clone").unwrap());
    assert!(!release_hold(state, "/work/clone").unwrap());
    check_removal(state, "/work/clone").unwrap();
}

#[test]
fn holds_reject_empty_identity() {
    let dir = tempfile::TempDir::new().unwrap();
    hold_work(dir.path(), "", "reason", None).unwrap_err();
    hold_work(dir.path(), "res", "", None).unwrap_err();
}

#[test]
fn holds_survive_reread() {
    let dir = tempfile::TempDir::new().unwrap();
    hold_work(dir.path(), "ckpt:run:0:1", "capture", Some("op-1")).unwrap();
    let holds = list_holds(dir.path()).unwrap();
    assert_eq!(holds.len(), 1);
    assert_eq!(holds[0].operation_id.as_deref(), Some("op-1"));
    // Re-read from disk through a fresh call: durable, not in-memory.
    let reread = check_removal(dir.path(), "ckpt:run:0:1").expect_err("must still hold");
    assert!(format!("{reread:#}").contains("capture"));
}

#[test]
fn workflow_labels_carry_ownership() {
    let labels = workflow_labels("run-9", "implementer");
    assert_eq!(
        labels.get("jackin.workflows/managed").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        labels.get("jackin.workflows/run").map(String::as_str),
        Some("run-9")
    );
    assert_eq!(
        labels.get("jackin.workflows/worker").map(String::as_str),
        Some("implementer")
    );
}
