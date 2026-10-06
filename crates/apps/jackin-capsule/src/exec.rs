// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `jackin-exec` / `jackin-capsule exec` subcommand.
//!
//! Two roles in this module:
//!
//! 1. **Client binary** (`run`): connects to the capsule daemon via the
//!    control socket, sends `ExecCommand`, waits for `ExecResult` or
//!    `ExecDenied`, and writes the output to the terminal.
//!
//! 2. **Shared types** (`ExecPickerState`, …) and helpers
//!    (`resolve_credentials`, `execute_command`): used by the daemon to drive
//!    the credential picker and run the approved command. The host.sock wire
//!    types (`ExecBinding`, `CredRequest`, `CredReply`) live in `jackin-protocol`.

use anyhow::{Context as _, Result, bail};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

use crate::protocol::control::{ClientMsg, ControlRequest, ServerMsg, frame};
use crate::socket::SOCKET_PATH;

mod command;
mod picker;

pub(crate) use command::read_framed;
#[cfg(test)]
pub(crate) use command::{cap_output, redact_pem};
pub use command::{execute_command, resolve_credentials};
pub use picker::{ExecPickerItem, ExecPickerState};

// ---------------------------------------------------------------------------
// Client binary entry point
// ---------------------------------------------------------------------------

/// Result of an exec call when captured (for MCP tool integration).
#[derive(Debug)]
pub struct ExecCapture {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub redacted_count: u32,
    pub denied: Option<String>,
}

fn exec_control_request(
    command: String,
    args: Vec<String>,
    ctx: jackin_protocol::TelemetryContext,
    session_capability: Option<String>,
) -> ControlRequest {
    let msg = ClientMsg::ExecCommand { command, args };
    ControlRequest {
        ctx,
        session_capability,
        msg,
    }
}

fn inherited_session_capability() -> Option<String> {
    std::env::var(jackin_protocol::SESSION_CAPABILITY_ENV)
        .ok()
        .filter(|value| !value.is_empty())
}

/// Run `jackin-exec` and return the result as a captured struct instead of
/// writing to stdout/stderr and calling `process::exit`. Used by the MCP
/// server to return structured output to Claude Code.
/// # Errors
///
/// Returns an error when command arguments are invalid, the daemon cannot be
/// reached, or the command request fails.
pub async fn run_capture(args: &[String]) -> Result<ExecCapture> {
    if args.is_empty() {
        bail!("usage: jackin-exec <command> [args…]");
    }

    let command = args[0].clone();
    let cmd_args = args[1..].to_vec();

    let msg = ClientMsg::ExecCommand {
        command,
        args: cmd_args,
    };
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(msg.rpc_method()),
        },
    ];
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_CLIENT, &attrs).ok();
    let mut ctx = jackin_protocol::TelemetryContext::v1();
    if let Some(operation) = operation.as_ref() {
        operation
            .span()
            .in_scope(|| jackin_telemetry::propagation::inject(&mut ctx));
    } else {
        jackin_telemetry::propagation::inject(&mut ctx);
    }
    let request = match msg {
        ClientMsg::ExecCommand { command, args } => {
            exec_control_request(command, args, ctx, inherited_session_capability())
        }
        _ => unreachable!("constructed ExecCommand above"),
    };
    let result = async {
        let mut stream = jackin_diagnostics::operation::connection_attempt(
            jackin_telemetry::schema::enums::ConnectionPeerType::CapsuleControl,
            UnixStream::connect(SOCKET_PATH),
        )
        .await
        .with_context(|| format!("connecting to capsule socket at {SOCKET_PATH}"))?;
        jackin_protocol::capsule_transport::client_handshake_async(&mut stream)
            .await
            .context("negotiating Capsule control transport")?;
        stream
            .write_all(&frame(&request))
            .await
            .context("sending ExecCommand")?;

        const MAX_REPLY: usize = 8 * 1024 * 1024;
        let body = read_framed(&mut stream, MAX_REPLY)
            .await
            .context("reading ExecResult")?;
        serde_json::from_slice::<ServerMsg>(&body).context("parsing ExecResult")
    }
    .await;
    if let Some(operation) = operation {
        let (outcome, error_type) = match &result {
            Ok(ServerMsg::ExecResult { exit_code: 0, .. }) => {
                (jackin_telemetry::schema::enums::OutcomeValue::Success, None)
            }
            Ok(ServerMsg::ExecDenied { .. }) => (
                jackin_telemetry::schema::enums::OutcomeValue::Cancellation,
                None,
            ),
            _ => (
                jackin_telemetry::schema::enums::OutcomeValue::Failure,
                Some(jackin_telemetry::schema::enums::ErrorType::RpcError),
            ),
        };
        operation.complete(outcome, error_type);
    }

    match result? {
        ServerMsg::ExecResult {
            exit_code,
            stdout,
            stderr,
            redacted_count,
        } => Ok(ExecCapture {
            exit_code,
            stdout,
            stderr,
            redacted_count,
            denied: None,
        }),
        ServerMsg::ExecDenied { reason } => Ok(ExecCapture {
            exit_code: 1,
            stdout: String::new(),
            stderr: String::new(),
            redacted_count: 0,
            denied: Some(reason),
        }),
        other => bail!("unexpected reply to ExecCommand: {other:?}"),
    }
}

/// Entry point for `jackin-capsule exec <command> [args…]`
/// and the `jackin-exec <command> [args…]` symlink form.
///
/// Thin terminal wrapper over [`run_capture`]: the socket round-trip lives
/// there; `run` only renders the captured result to stdout/stderr and exits
/// with the child's code.
#[expect(
    clippy::exit,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
/// # Errors
///
/// Returns an error when command arguments are invalid, the daemon request
/// fails, or output cannot be written.
pub async fn run(args: &[String]) -> Result<()> {
    let capture = run_capture(args).await?;

    if let Some(reason) = capture.denied {
        use std::io::Write as _;
        writeln!(std::io::stderr(), "[jackin-exec] denied: {reason}")
            .context("writing denial to stderr")?;
        std::process::exit(1);
    }

    use std::io::Write as _;
    if !capture.stdout.is_empty() {
        std::io::stdout()
            .write_all(capture.stdout.as_bytes())
            .context("writing stdout")?;
    }
    if !capture.stderr.is_empty() {
        std::io::stderr()
            .write_all(capture.stderr.as_bytes())
            .context("writing stderr")?;
    }
    if capture.redacted_count > 0 {
        writeln!(
            std::io::stderr(),
            "[jackin-exec] {} secret pattern(s) redacted from output",
            capture.redacted_count
        )
        .context("writing redaction notice to stderr")?;
    }
    std::process::exit(capture.exit_code);
}

#[cfg(test)]
mod tests;
