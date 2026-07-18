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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::protocol::control::{ClientMsg, ControlRequest, ServerMsg, frame};
use crate::socket::SOCKET_PATH;

/// State for the exec credential picker dialog shown by the daemon's TUI.
#[derive(Debug, Clone)]
pub struct ExecPickerState {
    pub command: String,
    pub args: Vec<String>,
    pub items: Vec<ExecPickerItem>,
    pub cursor: usize,
}

/// A single on-demand credential row in the picker. Carries the underlying
/// [`ExecBinding`] verbatim (so a confirm sends it back unchanged) plus a
/// human-readable display label and the operator's selection state.
#[derive(Debug, Clone)]
pub struct ExecPickerItem {
    /// The binding sent to the host resolver if this row is selected.
    pub binding: jackin_protocol::ExecBinding,
    /// Human-readable label (the source for `op`/`env`, the name for literals).
    /// Never a resolved secret value.
    pub display: String,
    /// Whether the operator has selected this item.
    pub selected: bool,
}

impl ExecPickerState {
    /// Build the picker for a `jackin-exec <command> [args…]` invocation from
    /// the workspace's on-demand bindings. Every binding becomes one unselected
    /// row; the operator toggles the ones the command needs. The display label
    /// is the source for `op`/`env` kinds (never a resolved secret) and the
    /// name for literals.
    #[must_use]
    pub fn from_bindings(
        command: String,
        args: Vec<String>,
        bindings: &[jackin_protocol::ExecBinding],
    ) -> Self {
        let items = bindings
            .iter()
            .map(|b| {
                // Literals have no meaningful source to show; everything else
                // displays its source (op:// path or $VAR), never a secret.
                let display = if b.kind == jackin_protocol::ExecKind::Literal {
                    b.name.clone()
                } else {
                    b.source.clone()
                };
                ExecPickerItem {
                    binding: b.clone(),
                    display,
                    selected: false,
                }
            })
            .collect();
        Self {
            command,
            args,
            items,
            cursor: 0,
        }
    }

    /// Returns the selected items as host.sock credential bindings.
    pub fn selected_refs(&self) -> Vec<jackin_protocol::ExecBinding> {
        self.items
            .iter()
            .filter(|i| i.selected)
            .map(|i| i.binding.clone())
            .collect()
    }

    pub fn toggle_cursor(&mut self) {
        if let Some(item) = self.items.get_mut(self.cursor) {
            item.selected = !item.selected;
        }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.items.len() {
            self.cursor += 1;
        }
    }
}

/// Read a 4-byte-BE-length-prefixed payload, bounding the declared length by
/// `max`. The mirror of [`frame`] for the read side, shared by the host.sock
/// and control-socket clients here. No read timeout: both callers intentionally
/// block for as long as the operator takes (picker confirm, `op` Touch ID).
async fn read_framed(stream: &mut UnixStream, max: usize) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    anyhow::ensure!(
        len <= max,
        "framed reply too large: {len} bytes (max {max})"
    );
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;
    Ok(body)
}

/// Resolve on-demand credentials via the host.sock listener.
/// `host_sock_path` is `/jackin/run/host.sock` inside the container.
///
/// Uses the shared `jackin_protocol` wire types ([`CredRequest`]/[`CredReply`])
/// and the canonical [`frame`] encoder, so the host.sock and control-socket
/// paths cannot drift. The read intentionally has no timeout — the host
/// resolver may block on `op read`'s Touch ID prompt for as long as the
/// operator takes.
pub async fn resolve_credentials(
    host_sock_path: &str,
    refs: Vec<jackin_protocol::ExecBinding>,
) -> Result<std::collections::BTreeMap<String, String>> {
    use jackin_protocol::{CredReply, CredRequest};

    if refs.is_empty() {
        return Ok(std::collections::BTreeMap::default());
    }

    const METHOD: &str = "jackin.host.Credentials/Resolve";
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(METHOD),
        },
    ];
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_CLIENT, &attrs).ok();
    let result = async {
        let mut stream = jackin_diagnostics::operation::connection_attempt(
            jackin_telemetry::schema::enums::ConnectionPeerType::HostDaemon,
            UnixStream::connect(host_sock_path),
        )
        .await
        .with_context(|| format!("connecting to host credential resolver at {host_sock_path}"))?;

        let mut ctx = jackin_protocol::TelemetryContext::v1();
        if let Some(operation) = operation.as_ref() {
            operation
                .span()
                .in_scope(|| jackin_telemetry::propagation::inject(&mut ctx));
        } else {
            jackin_telemetry::propagation::inject(&mut ctx);
        }
        stream.write_all(&frame(&CredRequest { ctx, refs })).await?;

        const MAX_REPLY: usize = 1024 * 1024;
        let reply_body = read_framed(&mut stream, MAX_REPLY)
            .await
            .context("reading host.sock reply")?;

        match serde_json::from_slice::<CredReply>(&reply_body).context("parsing host.sock reply")? {
            CredReply::Ok { values } => Ok(values),
            CredReply::Error { error } => bail!("{error}"),
        }
    }
    .await;
    if let Some(operation) = operation {
        operation.complete(
            if result.is_ok() {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            },
            result
                .as_ref()
                .err()
                .map(|_| jackin_telemetry::schema::enums::ErrorType::RpcError),
        );
    }
    result
}

/// Execute a command with the given environment additions.
/// Returns (`exit_code`, `stdout`, `stderr`, `redacted_count`).
///
/// Transport is [`jackin_process`] with **no read timeout** — the operator
/// (or `op` Touch ID) may take arbitrarily long; see also `resolve_credentials`.
pub async fn execute_command(
    command: &str,
    args: &[String],
    extra_env: &std::collections::BTreeMap<String, String>,
    secrets_for_redaction: &[&str],
) -> Result<(i32, String, String, u32)> {
    let mut request = jackin_process::ExecRequest::new(command, args).no_timeout();
    request = request.envs(extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let output = crate::process_telemetry::exec_async_as(
        &request,
        jackin_telemetry::schema::enums::ProcessExecutableName::ConfiguredCommand,
    )
    .await
    .context("running configured command")?;

    let exit_code = output.code.unwrap_or(-1);

    // Decode to UTF-8. `from_utf8` reuses the child's buffer on the common
    // (valid-UTF-8) path; only invalid bytes pay the lossy re-allocation.
    let mut stdout = into_utf8(output.stdout);
    let mut stderr = into_utf8(output.stderr);

    // Redact secret values from the FULL output, before capping. Capping first
    // would let a secret straddling the cap boundary survive: its tail gets
    // truncated away so the leading prefix no longer matches `secret` and the
    // replace misses it, leaking a verbatim partial secret to the caller.
    let mut redacted_count = 0u32;
    for &secret in secrets_for_redaction {
        if secret.is_empty() {
            continue;
        }
        // Plain value redaction — count and replace each stream independently
        // so a stream with no hit skips its replace scan.
        let out_hits = stdout.matches(secret).count();
        let err_hits = stderr.matches(secret).count();
        if out_hits > 0 {
            stdout = stdout.replace(secret, "[redacted by jackin']");
        }
        if err_hits > 0 {
            stderr = stderr.replace(secret, "[redacted by jackin']");
        }
        redacted_count += (out_hits + err_hits) as u32;
    }
    // PEM block redaction is global — `redact_pem` scrubs *any* PEM block, not a
    // specific secret's — so run it once per stream when any key-type secret is
    // present, rather than re-scanning inside the per-secret loop above.
    if secrets_for_redaction
        .iter()
        .any(|s| s.contains("BEGIN") && s.contains("PRIVATE KEY"))
    {
        redact_pem(&mut stdout, &mut redacted_count);
        redact_pem(&mut stderr, &mut redacted_count);
    }

    // Cap returned output at 1 MiB per stream, after redaction so truncation
    // cannot expose secret material.
    const MAX_OUTPUT: usize = 1024 * 1024;
    cap_output(&mut stdout, MAX_OUTPUT);
    cap_output(&mut stderr, MAX_OUTPUT);

    Ok((exit_code, stdout, stderr, redacted_count))
}

/// Decode child output as UTF-8, reusing the buffer when valid and falling
/// back to a lossy copy only for invalid byte sequences.
fn into_utf8(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Cap `s` at `max` bytes, rounding down to a UTF-8 char boundary so the
/// truncation never panics mid-codepoint, and append a marker.
fn cap_output(s: &mut String, max: usize) {
    if s.len() <= max {
        return;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s.push_str("\n[output truncated at 1 MiB]");
}

fn redact_pem(s: &mut String, count: &mut u32) {
    let begin = "-----BEGIN";
    let end = "-----";
    while let Some(start) = s.find(begin) {
        if let Some(end_idx) = s[start..].find("-----END") {
            // Find closing "-----" after "-----END"
            if let Some(close) = s[start + end_idx + 8..].find(end) {
                let remove_end = start + end_idx + 8 + close + end.len();
                s.replace_range(start..remove_end, "[key material redacted by jackin']");
                *count += 1;
                continue;
            }
        }
        break;
    }
}

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
) -> ControlRequest {
    let msg = ClientMsg::ExecCommand { command, args };
    ControlRequest { ctx, msg }
}

/// Run `jackin-exec` and return the result as a captured struct instead of
/// writing to stdout/stderr and calling `process::exit`. Used by the MCP
/// server to return structured output to Claude Code.
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
        ClientMsg::ExecCommand { command, args } => exec_control_request(command, args, ctx),
        _ => unreachable!("constructed ExecCommand above"),
    };
    let result = async {
        let mut stream = jackin_diagnostics::operation::connection_attempt(
            jackin_telemetry::schema::enums::ConnectionPeerType::CapsuleControl,
            UnixStream::connect(SOCKET_PATH),
        )
        .await
        .with_context(|| format!("connecting to capsule socket at {SOCKET_PATH}"))?;
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
