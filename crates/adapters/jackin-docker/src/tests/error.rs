// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::DockerError;

#[test]
fn command_timeout_message_names_program_and_secs() {
    let error = DockerError::CommandTimeout {
        secs: 30.0,
        program: "docker".to_owned(),
    };
    assert_eq!(error.to_string(), "command timed out after 30s: docker");
}

#[test]
fn http_status_message_names_url_and_status() {
    let error = DockerError::HttpStatus {
        url: "https://example.com/v1".to_owned(),
        status: "404 Not Found".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "https://example.com/v1 failed: HTTP 404 Not Found"
    );
}

#[test]
fn exec_nonzero_message_names_container_code_and_output() {
    let error = DockerError::ExecNonZero {
        container: "jackin-run-1".to_owned(),
        exit_code: 137,
        output: "killed".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "exec in jackin-run-1 exited with code 137: killed"
    );
}

#[test]
fn range_unsupported_message_names_url() {
    let error = DockerError::RangeUnsupported {
        url: "https://example.com/blob".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "server at https://example.com/blob does not support Range requests; cannot download in parallel"
    );
}

#[test]
fn plain_message_round_trips_text() {
    let error = DockerError::Message("boom".to_owned());
    assert_eq!(error.to_string(), "boom");
}
