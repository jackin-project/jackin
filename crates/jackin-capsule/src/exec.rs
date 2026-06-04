//! `jackin-exec` / `jackin-capsule exec` subcommand.
//!
//! Two roles in this module:
//!
//! 1. **Client binary** (`run`): connects to the capsule daemon via the
//!    control socket, sends `ExecCommand`, waits for `ExecResult` or
//!    `ExecDenied`, and writes the output to the terminal.
//!
//! 2. **Shared types** (`ExecRequest`, `ExecPickerState`, …): used by the
//!    daemon to carry exec work from the socket handler into the event loop.

use anyhow::{Context as _, Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::oneshot;

use crate::protocol::control::{ClientMsg, ServerMsg, frame};
use crate::socket::SOCKET_PATH;

// ---------------------------------------------------------------------------
// Shared types (daemon ↔ socket handler)
// ---------------------------------------------------------------------------

/// A pending exec request forwarded from the socket handler into the
/// daemon event loop. The socket handler awaits `response_rx` for the
/// daemon's answer; the daemon resolves credentials and executes the
/// command, then sends the result through `response_tx`.
pub struct ExecRequest {
    pub command: String,
    pub args: Vec<String>,
    pub response_tx: oneshot::Sender<ExecOutcome>,
}

/// The outcome of a `jackin-exec` invocation sent from daemon → socket handler.
#[derive(Debug)]
pub enum ExecOutcome {
    Result {
        exit_code: i32,
        stdout: String,
        stderr: String,
        redacted_count: u32,
    },
    Denied {
        reason: String,
    },
}

/// State for the exec credential picker dialog shown by the daemon's TUI.
#[derive(Debug, Clone)]
pub struct ExecPickerState {
    pub command: String,
    pub args: Vec<String>,
    pub items: Vec<ExecPickerItem>,
    pub cursor: usize,
}

/// A single on-demand credential that can be attached to the exec command.
#[derive(Debug, Clone)]
pub struct ExecPickerItem {
    /// Env var name (e.g. "GH_TOKEN").
    pub name: String,
    /// Human-readable label: `OpRef.path` or `Extended.value`.
    pub display: String,
    /// Source kind for the host.sock request.
    pub kind: ExecItemKind,
    /// The raw source value (`op://` URI, `$VAR`, or literal).
    pub source: String,
    /// Whether the operator has selected this item.
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecItemKind {
    /// Resolved via `op read <source>` on the host.
    Op,
    /// Resolved from the host's environment (`$VAR` syntax).
    Env,
    /// Already resolved — return `source` verbatim.
    Literal,
}

impl ExecItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Op => "op",
            Self::Env => "env",
            Self::Literal => "literal",
        }
    }
}

impl ExecPickerState {
    /// Returns the set of selected items, formatted for the host.sock request.
    pub fn selected_refs(&self) -> Vec<serde_json::Value> {
        self.items
            .iter()
            .filter(|i| i.selected)
            .map(|i| {
                serde_json::json!({
                    "name": i.name,
                    "kind": i.kind.as_str(),
                    "source": i.source,
                })
            })
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

    pub fn has_items(&self) -> bool {
        !self.items.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Host.sock protocol types
// ---------------------------------------------------------------------------

/// One on-demand credential ref sent to the host.sock listener.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CredRef {
    pub name: String,
    pub kind: String,
    pub source: String,
}

/// Request sent capsule → host.sock.
#[derive(Debug, serde::Serialize)]
pub struct CredRequest {
    pub refs: Vec<CredRef>,
}

/// Success response from host.sock → capsule.
#[derive(Debug, serde::Deserialize)]
pub struct CredResponse {
    pub values: std::collections::BTreeMap<String, String>,
}

/// Error response from host.sock → capsule.
#[derive(Debug, serde::Deserialize)]
pub struct CredError {
    pub error: String,
}

/// Resolve on-demand credentials via the host.sock listener.
/// `host_sock_path` is `/jackin/run/host.sock` inside the container.
pub async fn resolve_credentials(
    host_sock_path: &str,
    refs: Vec<serde_json::Value>,
) -> Result<std::collections::BTreeMap<String, String>> {
    if refs.is_empty() {
        return Ok(Default::default());
    }

    let mut stream = UnixStream::connect(host_sock_path)
        .await
        .with_context(|| format!("connecting to host credential resolver at {host_sock_path}"))?;

    let request = serde_json::json!({ "refs": refs });
    let body = serde_json::to_vec(&request)?;
    let len = (body.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&body).await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let reply_len = u32::from_be_bytes(len_buf) as usize;
    const MAX_REPLY: usize = 1024 * 1024;
    anyhow::ensure!(reply_len <= MAX_REPLY, "host.sock reply too large: {reply_len}");

    let mut reply_body = vec![0u8; reply_len];
    stream.read_exact(&mut reply_body).await?;

    // Try success response first, then error.
    if let Ok(ok) = serde_json::from_slice::<CredResponse>(&reply_body) {
        return Ok(ok.values);
    }
    if let Ok(err) = serde_json::from_slice::<CredError>(&reply_body) {
        bail!("{}", err.error);
    }
    bail!("unrecognised host.sock response");
}

/// Execute a command with the given environment additions.
/// Returns (exit_code, stdout, stderr).
pub async fn execute_command(
    command: &str,
    args: &[String],
    extra_env: &std::collections::BTreeMap<String, String>,
    secrets_for_redaction: &[String],
) -> Result<(i32, String, String, u32)> {
    use std::process::Stdio;

    let mut cmd = tokio::process::Command::new(command);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (k, v) in extra_env {
        cmd.env(k, v);
    }

    let output = cmd
        .spawn()
        .with_context(|| format!("spawning {command:?}"))?
        .wait_with_output()
        .await
        .with_context(|| format!("waiting for {command:?}"))?;

    let exit_code = output.status.code().unwrap_or(-1);

    // Convert to lossy UTF-8 strings capped at 1 MiB each.
    const MAX_OUTPUT: usize = 1024 * 1024;
    let mut stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if stdout.len() > MAX_OUTPUT {
        stdout.truncate(MAX_OUTPUT);
        stdout.push_str("\n[output truncated — use JACKIN_DEBUG for full output]");
    }
    let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if stderr.len() > MAX_OUTPUT {
        stderr.truncate(MAX_OUTPUT);
        stderr.push_str("\n[output truncated — use JACKIN_DEBUG for full output]");
    }

    // Redact secret values from output.
    let mut redacted_count = 0u32;
    for secret in secrets_for_redaction {
        if secret.is_empty() {
            continue;
        }
        // Plain value redaction.
        let count_before = stdout.matches(secret.as_str()).count()
            + stderr.matches(secret.as_str()).count();
        if count_before > 0 {
            stdout = stdout.replace(secret.as_str(), "[redacted by jackin']");
            stderr = stderr.replace(secret.as_str(), "[redacted by jackin']");
            redacted_count += count_before as u32;
        }
        // PEM block redaction.
        if secret.contains("BEGIN") && secret.contains("PRIVATE KEY") {
            let pem_pattern = "-----BEGIN";
            if stdout.contains(pem_pattern) || stderr.contains(pem_pattern) {
                // Simple heuristic: redact any PEM block.
                redact_pem(&mut stdout, &mut redacted_count);
                redact_pem(&mut stderr, &mut redacted_count);
            }
        }
    }

    Ok((exit_code, stdout, stderr, redacted_count))
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

/// Entry point for `jackin-capsule exec <command> [args…]`
/// and the `jackin-exec <command> [args…]` symlink form.
pub async fn run(args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("usage: jackin-exec <command> [args…]");
    }

    let command = args[0].clone();
    let cmd_args = args[1..].to_vec();

    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .with_context(|| format!("connecting to capsule socket at {SOCKET_PATH}"))?;

    // Control channel: write length-prefixed JSON.
    let msg = ClientMsg::ExecCommand {
        command: command.clone(),
        args: cmd_args,
    };
    let framed = frame(&msg);
    stream
        .write_all(&framed)
        .await
        .context("sending ExecCommand")?;

    // Read 4-byte length prefix then JSON body.
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .context("reading ExecResult length")?;
    let len = u32::from_be_bytes(len_buf) as usize;
    const MAX_REPLY: usize = 8 * 1024 * 1024;
    if len > MAX_REPLY {
        bail!("ExecResult reply too large: {len} bytes");
    }
    let mut body = vec![0u8; len];
    stream
        .read_exact(&mut body)
        .await
        .context("reading ExecResult body")?;

    let reply: ServerMsg = serde_json::from_slice(&body).context("parsing ExecResult")?;

    match reply {
        ServerMsg::ExecResult {
            exit_code,
            stdout,
            stderr,
            redacted_count,
        } => {
            use std::io::Write as _;
            if !stdout.is_empty() {
                std::io::stdout()
                    .write_all(stdout.as_bytes())
                    .context("writing stdout")?;
            }
            if !stderr.is_empty() {
                std::io::stderr()
                    .write_all(stderr.as_bytes())
                    .context("writing stderr")?;
            }
            if redacted_count > 0 {
                eprintln!(
                    "[jackin-exec] {redacted_count} secret pattern(s) redacted from output"
                );
            }
            std::process::exit(exit_code);
        }
        ServerMsg::ExecDenied { reason } => {
            eprintln!("[jackin-exec] denied: {reason}");
            std::process::exit(1);
        }
        other => {
            bail!("unexpected reply to ExecCommand: {other:?}");
        }
    }
}
