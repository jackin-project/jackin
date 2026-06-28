//! Host-side Claude CLI helpers used by the
//! `jackin workspace claude-token setup` orchestrator.
//!
//! Three responsibilities:
//!
//! 1. **Probe** that `claude` is on the operator's `PATH` and capture
//!    a version string for the diagnostic banner. Same shape as
//!    `OpRunner::probe` — single install-link error if missing rather
//!    than letting the orchestrator fail later with a confusing
//!    process-spawn error.
//! 2. **Parse** the token line emitted by `claude setup-token` after
//!    the operator completes the browser auth handshake.
//! 3. **Capture** the token interactively under a PTY. The OAuth
//!    flow needs a real terminal (claude refuses if stdout is piped
//!    to a non-tty), so we open a PTY pair, run `claude setup-token`
//!    on the slave end, and stream the master end through a redactor
//!    that forwards URL / instructions to the operator's stderr but
//!    hides the token line. The captured token is held in
//!    [`secrecy::SecretString`] for the rest of its life — never
//!    on stdout, never on argv, never on disk.
//!
//! Roadmap: `docs/src/content/docs/reference/roadmap/workspace-claude-token-setup.mdx`

use std::process::Command;

const CLAUDE_DEFAULT_BIN: &str = "claude";

/// Result of probing `<binary> --version` on the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeProbe {
    /// Exact binary path / name that succeeded (`claude` by default).
    pub binary: String,
    /// Captured version string, e.g. `"2.1.4"`. Already trimmed.
    /// Format follows upstream `claude --version` output and may
    /// change; the orchestrator displays it verbatim and never
    /// version-gates on it.
    pub version: String,
}

/// Probe `<binary> --version` on the host and return the captured
/// version string. The default binary is `claude`; tests inject an
/// alternative path via [`probe_with_binary`].
///
/// Errors carry an actionable install-hint suffix because operators
/// hitting this path typically have not yet installed the upstream
/// CLI on the machine running jackin.
pub fn probe_claude_cli() -> anyhow::Result<ClaudeProbe> {
    probe_with_binary(CLAUDE_DEFAULT_BIN)
}

/// Test-injectable variant. Production callers use [`probe_claude_cli`].
pub fn probe_with_binary(binary: &str) -> anyhow::Result<ClaudeProbe> {
    let mut command = Command::new(binary);
    command.arg("--version");
    #[expect(
        clippy::disallowed_methods,
        reason = "Claude CLI probe is wrapped by spawn_blocking on launch paths"
    )]
    let out = command.output().map_err(|e| {
        anyhow::anyhow!(
            "failed to spawn Claude CLI {binary:?}: {e} \
                 (install with `npm i -g @anthropic-ai/claude-code` or see \
                 https://docs.anthropic.com/en/docs/claude-code)"
        )
    })?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let code = out
            .status
            .code()
            .map_or_else(|| "signal".to_owned(), |c| c.to_string());
        anyhow::bail!(
            "`{binary} --version` exited with {code} (stderr: {})",
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let version = parse_version_line(&stdout).ok_or_else(|| {
        anyhow::anyhow!(
            "could not parse Claude CLI version from output: {:?}",
            stdout.trim()
        )
    })?;

    Ok(ClaudeProbe {
        binary: binary.to_owned(),
        version,
    })
}

/// Extract a bare semver-ish string from `claude --version` output.
///
/// Upstream output today looks like `2.1.4 (Claude Code)` — the
/// leading whitespace-delimited token is the version. Hold this
/// parser tolerant: future upstream output changes shouldn't break
/// the probe, only the displayed version.
fn parse_version_line(stdout: &str) -> Option<String> {
    stdout.split_whitespace().next().map(str::to_owned)
}

/// Long-lived OAuth token prefix emitted by `claude setup-token`.
///
/// Documented at <https://code.claude.com/docs/en/iam>. Centralised
/// so the orchestrator's stdout scanner, the `secrecy::SecretString`
/// debug stripper, and any future "looks like a token" log sanitiser
/// stay in sync.
pub const TOKEN_PREFIX: &str = "sk-ant-oat01-";

/// RAII guard that puts the operator's terminal into raw mode for
/// the lifetime of [`capture_setup_token_with_binary`] and restores
/// cooked mode on drop — including on panic via stack unwind.
///
/// Raw mode is required for the PTY pump-through to feel like a
/// direct `claude setup-token` invocation: keystrokes need to reach
/// claude byte-for-byte (single-key prompts, OAuth code paste,
/// Ctrl-C) and the host shell must not echo or line-buffer the
/// terminal capability responses claude solicits via DA1 / XTVERSION
/// queries (those responses must flow into the PTY, not paint as
/// visible garbage on the operator's screen).
///
/// `enable_raw_mode` returns `Err` when stdin is not a tty (CI,
/// piped invocation). We swallow that error; the cooked-mode path
/// still works for non-interactive callers — only the live OAuth
/// flow needs raw mode.
struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Self {
        drop(crossterm::terminal::enable_raw_mode());
        Self
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        // Surface restore failure: a stuck raw-mode terminal swallows
        // line editing until the operator runs `stty sane` or opens
        // a new shell. The error must not panic out of Drop.
        if let Err(e) = crossterm::terminal::disable_raw_mode() {
            crate::output::stderr_line(format_args!(
                "[jackin] warning: failed to restore cooked terminal mode: {e} \
                 (run `stty sane` if your terminal misbehaves)"
            ));
        }
    }
}

/// Drive `claude setup-token` interactively under a PTY and return
/// the captured token wrapped in [`secrecy::SecretString`].
///
/// The function:
///
/// 1. Opens a pty pair (`portable-pty::native_pty_system`).
/// 2. Spawns `claude setup-token` on the slave end so the upstream
///    CLI sees a real terminal and proceeds with the OAuth flow.
/// 3. Reads chunks from the master end. Every chunk is scanned for
///    [`TOKEN_PREFIX`]. When found, the token is extracted (up to
///    the first whitespace / control char) into the secret and the
///    matching span is replaced with `<redacted>` before the chunk
///    is forwarded to the operator's stderr — so the operator still
///    sees the OAuth URL / instructions but never the token.
/// 4. Waits for the child to exit naturally so the OAuth round-trip
///    completes (`claude` writes additional banner lines after the
///    token; killing on first match would leave them unflushed and
///    corrupt the operator's terminal).
///
/// Errors:
/// - PTY allocation / spawn failures bubble up with the usual
///   install-hint suffix.
/// - Child exits non-zero (operator hit Ctrl-C, network failed, OAuth
///   denied) — surfaced verbatim with stderr trail.
/// - Child exits clean but no token line was ever emitted — the
///   operator is told to re-run with `--debug` so the raw output is
///   inspectable.
pub fn capture_setup_token() -> anyhow::Result<secrecy::SecretString> {
    capture_setup_token_with_binary(CLAUDE_DEFAULT_BIN)
}

pub fn capture_setup_token_with_binary(binary: &str) -> anyhow::Result<secrecy::SecretString> {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};
    use secrecy::SecretString;
    use std::io::{Read, Write};

    // Match the operator's terminal geometry so claude's banner
    // wraps the same way it would in a direct invocation. Fall
    // back to a sane default when stdout is not a tty (CI, piped
    // invocation).
    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 24));
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| anyhow::anyhow!("failed to allocate pty: {e}"))?;

    let cmd = CommandBuilder::new(binary);
    let mut child = pair
        .slave
        .spawn_command({
            let mut c = cmd;
            c.arg("setup-token");
            c
        })
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to spawn Claude CLI {binary:?} setup-token: {e} \
             (install with `npm i -g @anthropic-ai/claude-code` or see \
             https://docs.anthropic.com/en/docs/claude-code)"
            )
        })?;

    // Drop the slave handle on the parent side so the child becomes
    // the only owner — its EOF on exit closes the master side and
    // wakes our reader.
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| anyhow::anyhow!("failed to clone pty reader: {e}"))?;
    let mut master_writer = pair
        .master
        .take_writer()
        .map_err(|e| anyhow::anyhow!("failed to take pty writer: {e}"))?;

    // Put the operator's terminal into raw mode so single
    // keystrokes (`c` to copy the OAuth URL, the OAuth code paste,
    // Ctrl-C) and terminal capability responses to claude's
    // escape queries flow straight into the PTY without being
    // filtered, echoed, or line-buffered by the host shell.
    // Without this, the PTY layer breaks claude's interactive
    // contract: keys never reach claude and DA1/XTVERSION query
    // responses leak as visible garbage on the operator's screen.
    // Drop restores cooked mode even on panic.
    let _raw_guard = RawModeGuard::enter();

    // Pump operator stdin → PTY master in a detached worker. The
    // master is closed when the child exits, so the next byte
    // written from this thread fails and the thread exits
    // naturally; we do not need an explicit stop signal. Reads use
    // `std::io::stdin()` (not `lock()`) so the global stdin lock
    // is not held across calls — a later jackin step that wants
    // to read stdin can do so without deadlocking on this thread.
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        loop {
            match std::io::stdin().read(&mut byte) {
                Ok(0) => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    // A real read error (BrokenPipe, EIO, terminal
                    // detach) silently terminates the pump. Without
                    // this notice, claude appears to hang while the
                    // operator's keystrokes are dropped on the floor.
                    crate::output::stderr_line(format_args!(
                        "[jackin] warning: stdin pump terminated mid-flow: {e} \
                         (the OAuth prompt may now be unresponsive — Ctrl-C and retry)"
                    ));
                    break;
                }
                Ok(_) => {
                    if master_writer.write_all(&byte).is_err() {
                        break;
                    }
                    if master_writer.flush().is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut captured: Option<String> = None;
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let mut stderr = std::io::stderr();
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                drain_pty_buffer(&mut buf, &mut captured, &mut stderr);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => {
                drop(stderr.flush());
                drop(child.kill());
                drop(child.wait());
                anyhow::bail!(
                    "PTY read failed while capturing `{binary} setup-token` output: {e} \
                     (any captured token must be considered compromised; re-run setup)"
                );
            }
        }
    }
    // Flush any tail bytes that did not end with a newline.
    if !buf.is_empty() {
        forward_redacted_line(&buf, &mut captured, &mut stderr);
        buf.clear();
    }
    drop(stderr.flush());

    let status = child
        .wait()
        .map_err(|e| anyhow::anyhow!("failed to wait on `claude setup-token`: {e}"))?;
    if !status.success() {
        anyhow::bail!(
            "`{binary} setup-token` exited with non-zero status (operator may have \
             cancelled, network failed, or upstream OAuth was denied)"
        );
    }

    captured.map(SecretString::from).ok_or_else(|| {
        anyhow::anyhow!(
            "`{binary} setup-token` exited without emitting a token line. \
             Re-run with --debug to inspect the raw output."
        )
    })
}

/// Pull complete `\n`-terminated lines from `buf`, capture any token
/// match into `captured`, and forward the rest (with the matching
/// span replaced by `<redacted>`) to `out`.
fn drain_pty_buffer(
    buf: &mut Vec<u8>,
    captured: &mut Option<String>,
    out: &mut impl std::io::Write,
) {
    while let Some(nl) = buf.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = buf.drain(..=nl).collect();
        forward_redacted_line(&line, captured, out);
    }
}

/// Scan `line` for [`TOKEN_PREFIX`]; capture the first match into
/// `captured` and forward `line` to `out` with the matching span
/// replaced by `<redacted>`. Lines without a match are forwarded
/// verbatim.
///
/// The claude CLI sometimes embeds ANSI cursor-movement sequences
/// (e.g. `\x1b[1B` cursor-down) inside the token display to achieve
/// a two-row visual layout. The token bytes themselves are
/// contiguous alphanumeric/`-`/`_` characters, but separated by
/// escape sequences. This function skips those escapes while
/// collecting the actual token content, so the full token is
/// captured even when ANSI sequences split its display.
fn forward_redacted_line(
    line: &[u8],
    captured: &mut Option<String>,
    out: &mut impl std::io::Write,
) {
    let prefix = TOKEN_PREFIX.as_bytes();
    let Some(start) = line.windows(prefix.len()).position(|w| w == prefix) else {
        drop(out.write_all(line));
        return;
    };
    // Walk from `start`, skipping ANSI escape sequences, collecting
    // alphanumeric + '-' + '_' bytes as token content.
    //
    // The claude CLI wraps the token display across two visual rows
    // using cursor-down escapes followed by a formatting space before
    // the next token segment. `in_escape_gap` tracks whether we just
    // consumed an escape, which allows us to skip those interstitial
    // spaces without treating them as token terminators.
    let mut token = String::new();
    let mut i = start;
    let mut in_escape_gap = false;
    while i < line.len() {
        let b = line[i];
        if b == b'\x1b' {
            i = skip_ansi_escape(line, i);
            in_escape_gap = true;
        } else if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            token.push(b as char);
            in_escape_gap = false;
            i += 1;
        } else if b == b' ' && in_escape_gap {
            // Formatting space between an escape sequence and the
            // next token segment — skip without adding to token.
            i += 1;
        } else {
            break;
        }
    }
    let token_bytes_end = i;
    if token.is_empty() {
        token = TOKEN_PREFIX.to_owned();
    }
    if captured.is_none() {
        *captured = Some(token);
    }
    drop(out.write_all(&line[..start]));
    drop(out.write_all(b"<redacted>"));
    drop(out.write_all(&line[token_bytes_end..]));
}

/// Advance `pos` past one ANSI/VT escape sequence starting at `bytes[pos]`.
/// Handles CSI (`\x1b[`), OSC (`\x1b]`), and bare two-byte escapes.
/// Returns the index of the first byte after the sequence, or `bytes.len()`
/// if the sequence runs to end-of-slice.
//
// Hand-rolled per AGENTS.md "Prefer libraries" carve-out for
// trivially-small parsers: `vte` (the canonical alternative used by
// alacritty / wezterm) is a 4 KLOC state machine with a callback API
// that would dominate this 30-line single-call-site helper. Replace
// with `vte` if upstream output ever needs DCS / SOS / PM / APC
// support — the current redactor only needs to skip past the cursor-
// movement CSIs claude emits between the prefix and token body.
fn skip_ansi_escape(bytes: &[u8], pos: usize) -> usize {
    let rest = &bytes[pos..];
    if rest.len() < 2 {
        return bytes.len();
    }
    match rest[1] {
        b'[' => {
            // CSI: \x1b[ <params> <letter>
            let mut i = 2;
            while i < rest.len() && !rest[i].is_ascii_alphabetic() {
                i += 1;
            }
            pos + i + rest.get(i).map_or(0, |_| 1)
        }
        b']' => {
            // OSC: \x1b] <text> \x07  or  \x1b] <text> \x1b\\
            let mut i = 2;
            while i < rest.len() {
                if rest[i] == b'\x07' {
                    return pos + i + 1;
                }
                if rest[i] == b'\x1b' && rest.get(i + 1) == Some(&b'\\') {
                    return pos + i + 2;
                }
                i += 1;
            }
            bytes.len()
        }
        _ => pos + 2, // bare two-byte escape (\x1b7, \x1b8, \x1bM, ...)
    }
}

#[cfg(test)]
mod tests;
