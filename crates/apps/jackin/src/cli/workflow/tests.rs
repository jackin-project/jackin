// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::exit_code_for_daemon_code;

#[test]
fn daemon_codes_map_to_documented_exit_codes() {
    assert_eq!(exit_code_for_daemon_code("invalid_request"), 2);
    assert_eq!(exit_code_for_daemon_code("not_found"), 2);
    assert_eq!(exit_code_for_daemon_code("unsupported_version"), 2);
    assert_eq!(exit_code_for_daemon_code("denied"), 3);
    assert_eq!(exit_code_for_daemon_code("unsupported_capability"), 3);
    assert_eq!(exit_code_for_daemon_code("busy"), 4);
    assert_eq!(exit_code_for_daemon_code("state_conflict"), 4);
    assert_eq!(exit_code_for_daemon_code("needs_operator"), 4);
    assert_eq!(exit_code_for_daemon_code("execution_error"), 1);
    assert_eq!(exit_code_for_daemon_code(""), 1);
    assert_eq!(exit_code_for_daemon_code("anything-new"), 1);
}
