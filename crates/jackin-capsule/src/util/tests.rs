// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `util`.
use super::*;
use std::io::Write;
#[cfg(unix)]
use std::time::Instant;

#[test]
fn returns_none_when_path_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");
    assert_eq!(read_text_bounded(&missing, 1024), None);
}

#[test]
fn returns_full_contents_below_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("small.txt");
    std::fs::write(&p, b"hello").unwrap();
    assert_eq!(read_text_bounded(&p, 1024).as_deref(), Some("hello"));
}

#[test]
fn truncates_at_cap_when_file_larger() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("big.txt");
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(&vec![b'a'; 4096]).unwrap();
    let result = read_text_bounded(&p, 64).expect("read succeeds");
    assert_eq!(result.len(), 64, "must respect the cap and truncate");
    assert!(result.chars().all(|c| c == 'a'));
}

#[test]
fn returns_none_on_invalid_utf8() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("binary.bin");
    std::fs::write(&p, [0xff, 0xfe, 0xfd]).unwrap();
    assert_eq!(read_text_bounded(&p, 1024), None);
}

#[cfg(unix)]
fn shell_probe(script: &str) -> jackin_process::ExecRequest {
    jackin_process::ExecRequest::new("/bin/sh", ["-c", script])
        .stderr_mode(jackin_process::StdioMode::Null)
}

#[cfg(unix)]
#[test]
fn finite_probe_accepts_healthy_trimmed_output() {
    assert_eq!(
        command_stdout_trimmed_with_timeout(
            &shell_probe("printf '  healthy\n'"),
            Duration::from_secs(2),
        )
        .as_deref(),
        Some("healthy"),
    );
}

#[cfg(unix)]
#[test]
fn finite_probe_deadline_covers_descendant_held_stdout() {
    // The leader exits immediately; the descendant keeps its inherited pipe.
    let started = Instant::now();
    assert_eq!(
        command_stdout_trimmed_with_timeout(
            &shell_probe("sleep 30 & printf healthy"),
            Duration::from_millis(100),
        ),
        None,
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[cfg(unix)]
#[test]
fn finite_probe_rejects_oversized_output_instead_of_truncating() {
    let started = Instant::now();
    assert_eq!(
        command_stdout_trimmed_with_timeout(
            &shell_probe("dd if=/dev/zero bs=1024 count=65 2>/dev/null; sleep 30"),
            Duration::from_secs(10),
        ),
        None,
    );
    // Overflow aborts immediately; it does not wait for the larger deadline.
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[cfg(unix)]
#[test]
fn finite_probe_enforces_requested_deadline_on_running_leader() {
    let started = Instant::now();
    assert_eq!(
        command_stdout_trimmed_with_timeout(&shell_probe("sleep 30"), Duration::from_millis(100),),
        None,
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}
