// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

use anyhow::{Context, Result, anyhow};
use jackin_process::{RetryPolicy, StdioMode};

use super::{build_exec_request, finish_with_cleanup, parse_getconf_path};

#[test]
fn parse_getconf_path_accepts_single_line_paths_and_rejects_malformed_output() {
    let expected = OsString::from("/tools with spaces/bin");
    for output in [
        &b"/tools with spaces/bin"[..],
        &b"/tools with spaces/bin\n"[..],
        &b"/tools with spaces/bin\r\n"[..],
    ] {
        assert_eq!(parse_getconf_path(output), Some(expected.clone()));
    }

    for output in [
        &b""[..],
        &b"\n"[..],
        &b"\r\n"[..],
        &b"/first\nsecond\n"[..],
        &b"/first\rsecond"[..],
        &b"/first\0second"[..],
        &b"/tools/\xff\n"[..],
    ] {
        assert_eq!(parse_getconf_path(output), None);
    }
}

#[test]
fn exec_request_preserves_capture_stream_environment_and_retry_modes() -> Result<()> {
    let argv = [
        OsString::from("rustup"),
        OsString::from("run"),
        OsString::from("nightly-2026-10-18"),
        OsString::from("rustc"),
        OsString::from("-Vv"),
    ];
    let cwd = PathBuf::from("repo with spaces/crates/tools/jackin-lints");
    let env = BTreeMap::from([
        (OsString::from("CARGO_HOME"), OsString::from("cargo home")),
        (OsString::from("PATH"), OsString::from("tool bin")),
    ]);
    let expected_env = env.clone().into_iter().collect::<Vec<_>>();

    for (capture, stdout_mode) in [(true, StdioMode::Capture), (false, StdioMode::Inherit)] {
        let request = build_exec_request(&argv, &cwd, &env, capture)?;
        assert_eq!(request.program, PathBuf::from("rustup"));
        assert_eq!(request.args, argv[1..]);
        assert_eq!(request.cwd.as_deref(), Some(cwd.as_path()));
        assert_eq!(request.stdin, None);
        assert_eq!(request.env, expected_env);
        assert!(request.env_remove.is_empty());
        assert!(request.env_clear);
        assert_eq!(request.stdin_mode, StdioMode::Inherit);
        assert_eq!(request.stdout_mode, stdout_mode);
        assert_eq!(request.stderr_mode, StdioMode::Inherit);
        assert_eq!(request.timeout, None);
        assert_eq!(request.stdout_limit, None);
        assert_eq!(request.stderr_limit, None);
        assert_eq!(request.retry, RetryPolicy::none());
    }

    Ok(())
}

#[test]
fn cleanup_result_combinations_preserve_body_and_cleanup_errors() -> Result<()> {
    assert_eq!(finish_with_cleanup(Ok(17), Ok(()))?, 17);

    let body_error = finish_with_cleanup::<()>(
        Err(anyhow!(std::io::Error::other("body source")).context("body context")),
        Ok(()),
    )
    .err()
    .context("expected body error")?;
    assert_eq!(format!("{body_error:#}"), "body context: body source");
    assert_eq!(body_error.root_cause().to_string(), "body source");

    let cleanup_error = finish_with_cleanup(Ok(17), Err(std::io::Error::other("cleanup marker")))
        .err()
        .context("expected cleanup error")?;
    let cleanup_chain = format!("{cleanup_error:#}");
    assert!(cleanup_chain.contains("removing isolated Dylint library directory"));
    assert!(cleanup_chain.contains("cleanup marker"));

    let combined_error = finish_with_cleanup::<()>(
        Err(anyhow!(std::io::Error::other("body source")).context("body context")),
        Err(std::io::Error::other("cleanup marker")),
    )
    .err()
    .context("expected combined error")?;
    let combined_chain = format!("{combined_error:#}");
    assert!(combined_chain.contains("body context"));
    assert!(combined_chain.contains("removing isolated Dylint library directory also failed"));
    assert!(combined_chain.contains("cleanup marker"));
    assert_eq!(combined_error.root_cause().to_string(), "body source");

    Ok(())
}
