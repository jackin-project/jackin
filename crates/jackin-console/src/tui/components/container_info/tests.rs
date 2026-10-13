// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `container_info`.
use super::debug_run_info_state;

#[test]
fn debug_run_info_state_marks_invocation_id_copyable_without_file_rows() {
    let state = debug_run_info_state("0.6.0-test", "invocation-1");
    let rows = state.rows();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].value(), "invocation-1");
    assert!(rows[0].is_copyable());
    assert_eq!(rows[1].value(), "0.6.0-test");
    assert!(rows.iter().all(|row| row.href().is_none()));
}
