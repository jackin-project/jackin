// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn launch_env_runner_uses_wider_bounded_timeout() {
    let runner = OpCli::new_launch_env();

    assert_eq!(runner.binary, OP_DEFAULT_BIN);
    assert_eq!(runner.timeout, std::time::Duration::from_mins(2));
    assert_eq!(runner.account, None);
}

#[test]
fn read_rejects_non_op_reference_before_spawn() {
    let runner = OpCli::with_binary("definitely-not-run".into());

    let result = runner.read("not-op://vault/item/field");

    let Err(err) = result else {
        panic!("expected non-op reference to fail");
    };
    assert!(err.to_string().contains("must start with op://"), "{err:#}");
}

#[test]
fn read_rejects_flag_like_reference_segments_before_spawn() {
    let runner = OpCli::with_binary("definitely-not-run".into());

    let result = runner.read("op://vault/-item/field");

    let Err(err) = result else {
        panic!("expected flag-like segment to fail");
    };
    assert!(
        err.to_string().contains("segment looks like a flag"),
        "{err:#}"
    );
}

#[test]
fn op_read_args_include_account_when_pinned() {
    assert_eq!(
        op_read_args("op://vault/item/field", Some("acct-a")),
        vec!["--account", "acct-a", "read", "--", "op://vault/item/field"]
    );
}

#[test]
fn op_read_args_omit_account_when_unpinned() {
    assert_eq!(
        op_read_args("op://vault/item/field", None),
        vec!["read", "--", "op://vault/item/field"]
    );
}

#[test]
fn spawn_failure_is_typed_not_installed_and_keeps_guidance() {
    let err = op_spawn_error(
        "definitely-missing-op-binary",
        &std::io::Error::new(std::io::ErrorKind::NotFound, "No such file"),
    );
    let probe = err
        .downcast_ref::<jackin_core::OpProbeError>()
        .expect("typed OpProbeError source");
    assert!(
        matches!(probe, jackin_core::OpProbeError::NotInstalled { .. }),
        "{probe:?}"
    );
    let display = err.to_string();
    assert!(
        display.contains("failed to spawn 1Password CLI"),
        "operator-visible text must keep spawn guidance: {display}"
    );
    assert!(
        display.contains("is `op` installed"),
        "operator-visible text must keep PATH guidance: {display}"
    );
}

#[test]
fn not_signed_in_stderr_is_typed_and_keeps_signin_guidance() {
    // Construct the same shape `run_op_with_timeout` emits for a not-signed
    // stderr signature, so the display parity + downcast contract is pinned
    // without spawning a real `op`.
    let exit_msg =
        "1Password CLI exited with status 1 running `op account list`: not currently signed in";
    let err = anyhow::Error::new(jackin_core::OpProbeError::NotSignedIn {
        detail: exit_msg.to_owned(),
    })
    .context(format!(
        "1Password CLI is not signed in (running `op account list --format json` returned: {exit_msg}). \
         Run `op signin` in your shell, then retry."
    ));
    let probe = err
        .downcast_ref::<jackin_core::OpProbeError>()
        .expect("typed OpProbeError source");
    assert!(
        matches!(probe, jackin_core::OpProbeError::NotSignedIn { .. }),
        "{probe:?}"
    );
    let display = err.to_string();
    assert!(
        display.contains("not signed in"),
        "display must keep not-signed-in guidance: {display}"
    );
    assert!(
        display.contains("Run `op signin`"),
        "display must keep signin guidance: {display}"
    );
}

#[test]
fn text_file_busy_retry_eventually_succeeds() {
    let attempts = std::cell::Cell::new(0);

    let result = retry_text_file_busy_result(|| {
        let next = attempts.get() + 1;
        attempts.set(next);
        if next < 3 {
            return Err(std::io::Error::from_raw_os_error(TEXT_FILE_BUSY_OS_ERROR));
        }
        Ok("ok")
    })
    .expect("retry should recover");

    assert_eq!(result, "ok");
    assert_eq!(attempts.get(), 3);
}
