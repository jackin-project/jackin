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
#[derive(Debug)]
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
    /// Env var name (e.g. `GH_TOKEN`).
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

    /// Parse a binding `kind` string (`"op"`/`"env"`/`"literal"`). Anything
    /// unrecognised maps to `Literal` (returned verbatim, never resolved
    /// through `op` or host env) — the fail-safe choice for an unknown kind.
    pub fn from_kind_str(kind: &str) -> Self {
        match kind {
            "op" => Self::Op,
            "env" => Self::Env,
            _ => Self::Literal,
        }
    }
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
                let kind = ExecItemKind::from_kind_str(&b.kind);
                let display = match kind {
                    ExecItemKind::Literal => b.name.clone(),
                    ExecItemKind::Op | ExecItemKind::Env => b.source.clone(),
                };
                ExecPickerItem {
                    name: b.name.clone(),
                    display,
                    kind,
                    source: b.source.clone(),
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

    /// Returns the set of selected items, formatted for the host.sock request.
    pub fn selected_refs(&self) -> Vec<CredRef> {
        self.items
            .iter()
            .filter(|i| i.selected)
            .map(|i| CredRef {
                name: i.name.clone(),
                kind: i.kind.as_str().to_owned(),
                source: i.source.clone(),
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
}

// ---------------------------------------------------------------------------
// Host.sock protocol types
// ---------------------------------------------------------------------------

/// One on-demand credential ref sent to the host.sock listener.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    refs: Vec<CredRef>,
) -> Result<std::collections::BTreeMap<String, String>> {
    if refs.is_empty() {
        return Ok(std::collections::BTreeMap::default());
    }

    let mut stream = UnixStream::connect(host_sock_path)
        .await
        .with_context(|| format!("connecting to host credential resolver at {host_sock_path}"))?;

    let request = CredRequest { refs };
    let body = serde_json::to_vec(&request)?;
    let len = (body.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&body).await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let reply_len = u32::from_be_bytes(len_buf) as usize;
    const MAX_REPLY: usize = 1024 * 1024;
    anyhow::ensure!(
        reply_len <= MAX_REPLY,
        "host.sock reply too large: {reply_len}"
    );

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
/// Returns (`exit_code`, `stdout`, `stderr`, `redacted_count`).
pub async fn execute_command(
    command: &str,
    args: &[String],
    extra_env: &std::collections::BTreeMap<String, String>,
    secrets_for_redaction: &[String],
) -> Result<(i32, String, String, u32)> {
    use std::process::Stdio;

    let mut cmd = tokio::process::Command::new(command);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

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

    // Decode to UTF-8. `from_utf8` reuses the child's buffer on the common
    // (valid-UTF-8) path; only invalid bytes pay the lossy re-allocation.
    let mut stdout = into_utf8(output.stdout);
    let mut stderr = into_utf8(output.stderr);

    // Redact secret values from the FULL output, before capping. Capping first
    // would let a secret straddling the cap boundary survive: its tail gets
    // truncated away so the leading prefix no longer matches `secret` and the
    // replace misses it, leaking a verbatim partial secret to the caller.
    let mut redacted_count = 0u32;
    for secret in secrets_for_redaction {
        if secret.is_empty() {
            continue;
        }
        // Plain value redaction — count and replace each stream independently
        // so a stream with no hit skips its replace scan.
        let out_hits = stdout.matches(secret.as_str()).count();
        let err_hits = stderr.matches(secret.as_str()).count();
        if out_hits > 0 {
            stdout = stdout.replace(secret.as_str(), "[redacted by jackin']");
        }
        if err_hits > 0 {
            stderr = stderr.replace(secret.as_str(), "[redacted by jackin']");
        }
        redacted_count += (out_hits + err_hits) as u32;
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

/// Run `jackin-exec` and return the result as a captured struct instead of
/// writing to stdout/stderr and calling `process::exit`. Used by the MCP
/// server to return structured output to Claude Code.
pub async fn run_capture(args: &[String]) -> Result<ExecCapture> {
    if args.is_empty() {
        bail!("usage: jackin-exec <command> [args…]");
    }

    let command = args[0].clone();
    let cmd_args = args[1..].to_vec();

    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .with_context(|| format!("connecting to capsule socket at {SOCKET_PATH}"))?;

    let msg = ClientMsg::ExecCommand {
        command,
        args: cmd_args,
    };
    let framed = frame(&msg);
    stream
        .write_all(&framed)
        .await
        .context("sending ExecCommand")?;

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
#[allow(clippy::exit)]
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
