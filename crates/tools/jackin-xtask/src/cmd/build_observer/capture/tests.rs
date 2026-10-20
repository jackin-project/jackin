// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn bounded_reader_keeps_exact_bytes_and_rejects_nonzero_or_excess_output() {
    let end = Instant::now() + Duration::from_millis(500);
    assert_eq!(
        command("/bin/sh", &["-c", "printf exact"], 5, end).unwrap(),
        b"exact"
    );
    assert!(
        command(
            "/bin/sh",
            &["-c", "printf excess"],
            5,
            Instant::now() + Duration::from_millis(500)
        )
        .is_none()
    );
    assert!(
        command(
            "/bin/sh",
            &["-c", "exit 4"],
            5,
            Instant::now() + Duration::from_millis(500)
        )
        .is_none()
    );
    assert!(command("/bin/sh", &["-c", "printf unavailable"], 20, Instant::now()).is_none());
}

#[test]
fn continuous_helper_output_cannot_starve_the_shared_deadline_or_pipe_cap() {
    let start = Instant::now();
    let output = command(
        "/bin/sh",
        &["-c", "while :; do printf xxxxxxxxxxxxxxxx; done"],
        4096,
        start + Duration::from_millis(500),
    );
    assert!(output.is_none());
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn a_quiet_busy_helper_is_killed_inside_the_same_work_cleanup_budget() {
    let start = Instant::now();
    assert!(
        command(
            "/bin/sh",
            &["-c", "while :; do :; done"],
            16,
            start + Duration::from_millis(200)
        )
        .is_none()
    );
    assert!(start.elapsed() < Duration::from_millis(500));
}
