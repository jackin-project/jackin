// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use super::*;

#[test]
fn host_open_command_rejects_non_http_urls() {
    assert!(host_open_command("file:///tmp/report.html").is_none());
    assert!(host_open_command("javascript:alert(1)").is_none());
}

#[test]
fn host_open_command_accepts_http_urls() {
    let Some((_program, args)) = host_open_command_with_policy(
        "https://github.com/jackin-project/jackin/actions/runs/1",
        None,
    ) else {
        panic!("http(s) URL should produce a host opener command on supported test platforms");
    };
    assert!(args.iter().any(|arg| arg.contains("github.com")));
}

#[test]
fn host_open_command_accepts_mailto_urls() {
    let Some((_program, args)) = host_open_command_with_policy("mailto:operator@example.com", None)
    else {
        panic!("mailto URL should produce a host opener command on supported test platforms");
    };
    assert!(args.iter().any(|arg| arg == "mailto:operator@example.com"));
}

#[test]
fn host_open_command_honors_open_links_opt_out() {
    assert!(
        host_open_command_with_policy(
            "https://github.com/jackin-project/jackin/actions/runs/1",
            Some("deny"),
        )
        .is_none()
    );
}

#[test]
fn host_reveal_command_matches_current_platform() {
    let path = Path::new("/tmp/jackin/report.txt");
    let command = host_reveal_command(path).expect("current platform should support reveal");

    if cfg!(target_os = "macos") {
        assert_eq!(command.0, "open");
        assert_eq!(command.1, vec!["-R", "/tmp/jackin/report.txt"]);
    } else if cfg!(target_os = "linux") {
        assert_eq!(command.0, "xdg-open");
        assert_eq!(command.1, vec!["/tmp/jackin"]);
    } else if cfg!(target_os = "windows") {
        assert_eq!(command.0, "explorer.exe");
        assert_eq!(command.1, vec!["/select,/tmp/jackin/report.txt"]);
    }
}

#[test]
fn host_file_open_command_matches_current_platform() {
    let path = Path::new("/tmp/jackin/report.txt");
    let command = host_file_open_command(path).expect("current platform should support open");

    if cfg!(target_os = "macos") {
        assert_eq!(command.0, "open");
        assert_eq!(command.1, vec!["/tmp/jackin/report.txt"]);
    } else if cfg!(target_os = "linux") {
        assert_eq!(command.0, "xdg-open");
        assert_eq!(command.1, vec!["/tmp/jackin/report.txt"]);
    } else if cfg!(target_os = "windows") {
        assert_eq!(command.0, "explorer.exe");
        assert_eq!(command.1, vec!["/tmp/jackin/report.txt"]);
    }
}

#[cfg(unix)]
#[test]
fn host_desktop_command_reports_nonzero_exit() {
    run_host_desktop_command("/usr/bin/env", vec!["true".to_owned()], "test opener")
        .expect("successful command should pass");

    let err = run_host_desktop_command("/usr/bin/env", vec!["false".to_owned()], "test opener")
        .expect_err("nonzero command should fail");

    assert!(err.to_string().contains("test opener command"));
}

#[cfg(unix)]
#[test]
fn host_desktop_command_exports_bounded_failure_without_command_material() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        let error = run_host_desktop_command(
            "/usr/bin/env",
            vec!["operator-secret-missing-opener".to_owned()],
            "host opener",
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "host opener command exited unsuccessfully"
        );
    });
    export.force_flush();

    assert_eq!(export.finished_spans().len(), 1);
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("process_exit_nonzero"));
    assert!(!export.contains_span_text("/usr/bin/env"));
    assert!(!export.contains_span_text("operator-secret-missing-opener"));
}
