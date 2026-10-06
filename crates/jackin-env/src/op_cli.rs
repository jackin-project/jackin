// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::op_runner::OpRunner;
use crate::op_struct::{OpItemCreateParams, OpStructRunner, OpWriteRunner};
use crate::picker::{
    RawOpAccount, RawOpItemDetail, RawOpVault, apply_field_edit, matches_field_target,
    op_section_id, resolve_edited_field_ref,
};
use jackin_core::OpRef;
use jackin_core::{OpAccount, OpField, OpItem, OpVault};

const OP_DEFAULT_BIN: &str = "op";
const OP_DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const OP_LAUNCH_ENV_TIMEOUT: std::time::Duration = std::time::Duration::from_mins(2);
pub(crate) const OP_STDERR_MAX: usize = 4 * 1024;
const OP_SPAWN_RETRIES: usize = 5;
const TEXT_FILE_BUSY_OS_ERROR: i32 = 26;

/// Production `OpRunner` that shells out to the 1Password CLI.
///
/// Tests inject a different runner (e.g. `TestOpRunner`) rather than
/// using an env-var seam — keeps the crate `unsafe_code = "forbid"`
/// lint intact and tests free of process-env mutation.
#[derive(Debug, Clone)]
pub struct OpCli {
    pub(super) binary: String,
    pub(super) timeout: std::time::Duration,
    /// Pinned 1P account forwarded as `op --account <id>` on every
    /// invocation. `None` lets `op` fall back to its default-account
    /// context. Write paths set this so the minted ref records the
    /// account it was created under (`OpRef::account`); reads rebind to
    /// the ref's own account via `read_with_account` so multi-account
    /// vaults resolve regardless of which account was last
    /// `op signin`-ed.
    pub(super) account: Option<String>,
}

impl OpCli {
    /// Default runner: `op` binary, 30s timeout, default-account context.
    pub fn new() -> Self {
        Self {
            binary: OP_DEFAULT_BIN.to_owned(),
            timeout: OP_DEFAULT_TIMEOUT,
            account: None,
        }
    }

    /// Launch-time operator-env reads run on the foreground path, but real
    /// 1Password app/daemon wake-up delays can exceed the default 30s budget while
    /// still completing successfully. Keep this finite and below the fully
    /// interactive SSO budget so a wedged `op` still fails with a bounded error.
    pub fn new_launch_env() -> Self {
        Self {
            binary: OP_DEFAULT_BIN.to_owned(),
            timeout: OP_LAUNCH_ENV_TIMEOUT,
            account: None,
        }
    }

    /// Short-timeout variant for startup availability checks. A 3-second
    /// ceiling prevents `jackin console` from hanging when `op` is installed
    /// but biometric-blocked or network-stalled at launch time. A false
    /// negative here is acceptable — the picker shows an error panel if `op`
    /// later fails.
    pub fn new_probe() -> Self {
        Self {
            binary: OP_DEFAULT_BIN.to_owned(),
            timeout: std::time::Duration::from_secs(3),
            account: None,
        }
    }

    /// Long-timeout variant for interactive TUI flows where the operator may
    /// need to complete SSO (Okta, SAML, etc.) in a browser before `op`
    /// returns. Five minutes covers typical SSO redirect + approval round-trips.
    pub fn new_interactive() -> Self {
        Self {
            binary: OP_DEFAULT_BIN.to_owned(),
            timeout: std::time::Duration::from_mins(5),
            account: None,
        }
    }

    /// Runner pinned to a custom `op` binary path (default timeout).
    pub const fn with_binary(binary: String) -> Self {
        Self {
            binary,
            timeout: OP_DEFAULT_TIMEOUT,
            account: None,
        }
    }

    /// Pin every `op` invocation to a specific account. UUID, label,
    /// or email — `op` accepts all three. Pass `None` to clear.
    #[must_use]
    pub fn with_account(mut self, account: Option<String>) -> Self {
        self.account = account;
        self
    }

    #[cfg(test)]
    #[expect(
        dead_code,
        reason = "test constructor is used by selected op-cli test builds"
    )]
    pub(super) const fn with_binary_and_timeout(
        binary: String,
        timeout: std::time::Duration,
    ) -> Self {
        Self {
            binary,
            timeout,
            account: None,
        }
    }
}

impl Default for OpCli {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;

fn format_exit_status(status: std::process::ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| "signal".to_owned(), |c| c.to_string())
}

/// Truncate stderr to ~`OP_STDERR_MAX` bytes, rounding down to a UTF-8
/// char boundary so a multi-byte codepoint at the cut point cannot
/// panic on the error path.
pub(crate) fn truncate_stderr(stderr: &str) -> String {
    if stderr.len() <= OP_STDERR_MAX {
        return stderr.to_owned();
    }
    let mut end = OP_STDERR_MAX;
    while !stderr.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… [truncated]", &stderr[..end])
}

/// Drain stderr capped at `OP_STDERR_MAX + 1` bytes; further output is
/// sunk so the child exits cleanly.
fn drain_bounded_stderr(mut stderr: std::process::ChildStderr) -> std::io::Result<Vec<u8>> {
    use std::io::Read;

    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match stderr.read(&mut chunk)? {
            0 => break,
            n => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > OP_STDERR_MAX + 1 {
                    let mut sink = [0u8; 4096];
                    while stderr.read(&mut sink)? > 0 {}
                    break;
                }
            }
        }
    }
    Ok(buf)
}

/// Poll `try_wait` and forward the exit status, releasing the mutex
/// between attempts so the timeout branch can `take` and `kill` the
/// child without contending on a blocking `wait()`.
fn spawn_wait_thread(
    child: std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    tx: std::sync::mpsc::Sender<std::io::Result<std::process::ExitStatus>>,
) {
    jackin_telemetry::spawn::thread_stream("op.wait", move || {
        let poll = std::time::Duration::from_millis(20);
        loop {
            let Ok(mut guard) = child.lock() else {
                drop(tx.send(Err(std::io::Error::other("child mutex poisoned"))));
                return;
            };
            let Some(c) = guard.as_mut() else {
                return;
            };
            let status_opt = match c.try_wait() {
                Ok(Some(s)) => {
                    drop(guard.take());
                    Some(Ok(s))
                }
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            };
            drop(guard);
            match status_opt {
                Some(r) => {
                    drop(tx.send(r));
                    return;
                }
                None => {
                    #[expect(
                        clippy::disallowed_methods,
                        reason = "1Password poll loop runs on its own OS thread"
                    )]
                    std::thread::sleep(poll);
                }
            }
        }
    });
}

fn kill_and_reap(child: &std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>) {
    let Ok(mut guard) = child.lock() else {
        return;
    };
    if let Some(mut child) = guard.take() {
        drop(child.kill());
        drop(child.wait());
    }
}

fn is_text_file_busy(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(TEXT_FILE_BUSY_OS_ERROR)
}

fn retry_text_file_busy_result<T, F>(mut run: F) -> std::io::Result<T>
where
    F: FnMut() -> std::io::Result<T>,
{
    for attempt in 0..OP_SPAWN_RETRIES {
        match run() {
            Ok(value) => return Ok(value),
            Err(error) if is_text_file_busy(&error) && attempt + 1 < OP_SPAWN_RETRIES => {
                #[expect(
                    clippy::disallowed_methods,
                    reason = "launch callers run 1Password spawn retries inside spawn_blocking"
                )]
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("OP_SPAWN_RETRIES is nonzero");
}

fn spawn_op_with_retry<F>(
    mut build: F,
) -> Result<
    (
        std::process::Child,
        crate::process_telemetry::ChildOperation,
    ),
    Box<(std::io::Error, crate::process_telemetry::ChildOperation)>,
>
where
    F: FnMut() -> std::process::Command,
{
    let operation = crate::process_telemetry::ChildOperation::begin(
        jackin_telemetry::schema::enums::ProcessExecutableName::Op,
    );
    match retry_text_file_busy_result(|| {
        let mut command = build();
        command.spawn()
    }) {
        Ok(child) => Ok((child, operation)),
        Err(error) => Err(Box::new((error, operation))),
    }
}

fn op_spawn_error(binary: &str, error: &std::io::Error) -> anyhow::Error {
    // Typed source for cross-crate classification; context keeps the
    // operator-visible wording identical to the pre-typed path.
    let detail = error.to_string();
    let message = format!(
        "failed to spawn 1Password CLI {binary:?}: {error} \
         (is `op` installed and on your PATH? see \
         https://developer.1password.com/docs/cli/)"
    );
    anyhow::Error::new(jackin_core::OpProbeError::NotInstalled { detail }).context(message)
}

fn validate_op_source(source: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        source.starts_with("op://"),
        "invalid op:// reference {source:?}: must start with op://"
    );
    let path = &source["op://".len()..];
    anyhow::ensure!(
        !path.split('/').any(|segment| segment.starts_with('-')),
        "invalid op:// reference: segment looks like a flag in {source:?}"
    );
    Ok(())
}

fn op_read_args<'a>(reference: &'a str, account: Option<&'a str>) -> Vec<&'a str> {
    let mut args = Vec::new();
    push_account_arg(&mut args, account);
    args.extend_from_slice(&["read", "--", reference]);
    args
}

impl OpRunner for OpCli {
    fn read_with_account(&self, reference: &str, account: Option<&str>) -> anyhow::Result<String> {
        // A per-ref account overrides the instance default so a workspace
        // holding refs from several accounts resolves each against its own.
        match account {
            Some(_) => Self {
                binary: self.binary.clone(),
                timeout: self.timeout,
                account: account.map(str::to_owned),
            }
            .read(reference),
            None => self.read(reference),
        }
    }

    fn read(&self, reference: &str) -> anyhow::Result<String> {
        use std::io::Read;
        use std::process::{Command, Stdio};

        validate_op_source(reference)?;

        let (mut child, operation) = spawn_op_with_retry(|| {
            let mut cmd = Command::new(&self.binary);
            cmd.args(op_read_args(reference, self.account.as_deref()))
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            cmd
        })
        .map_err(|failure| {
            let (error, operation) = *failure;
            operation.spawn_failed();
            op_spawn_error(&self.binary, &error)
        })?;

        // Channel-and-thread wait pattern so we avoid a new async dep,
        // and the wait thread never holds the mutex across a blocking
        // wait — see spawn_wait_thread.
        let (tx, rx) = std::sync::mpsc::channel();
        let Some(mut stdout) = child.stdout.take() else {
            drop(child.kill());
            drop(child.wait());
            operation.io_failed();
            anyhow::bail!("1Password CLI stdout pipe missing");
        };
        let Some(stderr) = child.stderr.take() else {
            drop(child.kill());
            drop(child.wait());
            operation.io_failed();
            anyhow::bail!("1Password CLI stderr pipe missing");
        };
        let timeout = self.timeout;

        let stdout_handle = jackin_telemetry::spawn::thread_stream("op.stdout", move || {
            let mut buf = Vec::new();
            stdout.read_to_end(&mut buf).map(|_| buf)
        });
        let stderr_handle = jackin_telemetry::spawn::thread_stream("op.stderr", move || {
            drain_bounded_stderr(stderr)
        });

        let child = std::sync::Arc::new(std::sync::Mutex::new(Some(child)));
        spawn_wait_thread(std::sync::Arc::clone(&child), tx);

        let status = match rx.recv_timeout(timeout) {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                kill_and_reap(&child);
                operation.io_failed();
                anyhow::bail!("1Password CLI wait failed for {reference:?}: {e}");
            }
            Err(_) => {
                // Child may have exited between recv_timeout expiring
                // and the take below (yielding Err(InvalidInput) on
                // kill), which is not a real failure. Reap so pipes
                // close and reader threads exit.
                kill_and_reap(&child);
                operation.timed_out();
                anyhow::bail!(
                    "1Password CLI timed out after {}s resolving {reference:?}",
                    timeout.as_secs()
                );
            }
        };

        let stdout_bytes = stdout_handle
            .join()
            .unwrap_or_else(|_| Err(std::io::Error::other("stdout reader panicked")));
        let stderr_bytes = stderr_handle
            .join()
            .unwrap_or_else(|_| Err(std::io::Error::other("stderr reader panicked")));
        let (Ok(stdout_bytes), Ok(stderr_bytes)) = (stdout_bytes, stderr_bytes) else {
            operation.io_failed();
            anyhow::bail!("1Password CLI output pipe read failed");
        };

        if status.success() {
            operation.complete_status(status);
            // `op read` appends a trailing newline as CLI convention;
            // strip exactly one so a secret ending in a real newline
            // (e.g. PEM block) survives.
            let mut stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
            if stdout.ends_with('\n') {
                stdout.pop();
                if stdout.ends_with('\r') {
                    stdout.pop();
                }
            }
            return Ok(stdout);
        }

        let stderr = String::from_utf8_lossy(&stderr_bytes);
        let stderr_trimmed = truncate_stderr(&stderr);
        operation.complete_status(status);
        anyhow::bail!(
            "1Password CLI exited with status {} resolving {reference:?}: {}",
            format_exit_status(status),
            stderr_trimmed.trim()
        )
    }

    fn probe(&self) -> anyhow::Result<()> {
        // Route through the timeout helper so a wedged `op` (network
        // stall, biometric prompt held open) cannot freeze the caller.
        run_op_with_timeout(&self.binary, &["--version"], self.timeout).map_err(|e| {
            // Preserve the install-link hint on spawn-error paths.
            let msg = e.to_string();
            if msg.contains("developer.1password.com") {
                e
            } else {
                anyhow::anyhow!(
                    "1Password CLI probe (`{} --version`) failed: {msg} — \
                     see https://developer.1password.com/docs/cli/",
                    self.binary
                )
            }
        })?;
        Ok(())
    }
}

/// Shared timeout primitive used by [`OpCli::probe`] and
/// [`run_op_json`]. Returns stdout bytes on success; failure stderr is
/// untouched so callers can pattern-match (see [`run_op_json`]).
fn run_op_with_timeout(
    binary: &str,
    args: &[&str],
    timeout: std::time::Duration,
) -> anyhow::Result<Vec<u8>> {
    run_op_with_timeout_inner(binary, args, timeout, OpTransportFault::None)
}

#[derive(Clone, Copy)]
enum OpTransportFault {
    None,
    #[cfg(test)]
    MissingStdout,
    #[cfg(test)]
    MissingStderr,
    #[cfg(test)]
    Wait,
    #[cfg(test)]
    StdoutRead,
    #[cfg(test)]
    StderrRead,
}

fn run_op_with_timeout_inner(
    binary: &str,
    args: &[&str],
    timeout: std::time::Duration,
    fault: OpTransportFault,
) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    #[cfg(not(test))]
    let _ = fault;

    let (mut child, operation) = spawn_op_with_retry(|| {
        let mut command = Command::new(binary);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    })
    .map_err(|failure| {
        let (error, operation) = *failure;
        operation.spawn_failed();
        op_spawn_error(binary, &error)
    })?;

    #[cfg(test)]
    if matches!(fault, OpTransportFault::MissingStdout) {
        drop(child.stdout.take());
    }
    #[cfg(test)]
    if matches!(fault, OpTransportFault::MissingStderr) {
        drop(child.stderr.take());
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let Some(mut stdout) = child.stdout.take() else {
        drop(child.kill());
        drop(child.wait());
        operation.io_failed();
        anyhow::bail!("1Password CLI stdout pipe missing");
    };
    let Some(stderr) = child.stderr.take() else {
        drop(child.kill());
        drop(child.wait());
        operation.io_failed();
        anyhow::bail!("1Password CLI stderr pipe missing");
    };

    let stdout_handle = jackin_telemetry::spawn::thread_stream("op.stdout", move || {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).map(|_| buf)
    });
    let stderr_handle =
        jackin_telemetry::spawn::thread_stream("op.stderr", move || drain_bounded_stderr(stderr));

    let child = std::sync::Arc::new(std::sync::Mutex::new(Some(child)));
    #[cfg(test)]
    if matches!(fault, OpTransportFault::Wait) {
        drop(tx.send(Err(std::io::Error::other("injected wait failure"))));
    } else {
        spawn_wait_thread(std::sync::Arc::clone(&child), tx);
    }
    #[cfg(not(test))]
    spawn_wait_thread(std::sync::Arc::clone(&child), tx);

    let cmd_label = format!("op {}", args.join(" "));
    let status = match rx.recv_timeout(timeout) {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            kill_and_reap(&child);
            operation.io_failed();
            let message = format!("1Password CLI wait failed for `{cmd_label}`: {e}");
            return Err(anyhow::Error::new(jackin_core::OpProbeError::Other {
                message: message.clone(),
            })
            .context(message));
        }
        Err(_) => {
            kill_and_reap(&child);
            operation.timed_out();
            let seconds = timeout.as_secs();
            let message = format!("1Password CLI timed out after {seconds}s running `{cmd_label}`");
            return Err(
                anyhow::Error::new(jackin_core::OpProbeError::Timeout { seconds }).context(message),
            );
        }
    };

    let stdout_bytes = stdout_handle
        .join()
        .unwrap_or_else(|_| Err(std::io::Error::other("stdout reader panicked")));
    let stderr_bytes = stderr_handle
        .join()
        .unwrap_or_else(|_| Err(std::io::Error::other("stderr reader panicked")));
    #[cfg(test)]
    let stdout_bytes = if matches!(fault, OpTransportFault::StdoutRead) {
        Err(std::io::Error::other("injected stdout read failure"))
    } else {
        stdout_bytes
    };
    #[cfg(test)]
    let stderr_bytes = if matches!(fault, OpTransportFault::StderrRead) {
        Err(std::io::Error::other("injected stderr read failure"))
    } else {
        stderr_bytes
    };
    let (Ok(stdout_bytes), Ok(stderr_bytes)) = (stdout_bytes, stderr_bytes) else {
        operation.io_failed();
        anyhow::bail!("1Password CLI output pipe read failed");
    };

    if status.success() {
        operation.complete_status(status);
        return Ok(stdout_bytes);
    }

    let stderr = String::from_utf8_lossy(&stderr_bytes);
    let stderr_trimmed = truncate_stderr(&stderr);
    let stderr_msg = stderr_trimmed.trim();
    operation.complete_status(status);
    let message = format!(
        "1Password CLI exited with status {} running `{cmd_label}`: {stderr_msg}",
        format_exit_status(status),
    );
    // Single place that inspects op's stderr wording for the not-signed-in
    // class; consumers classify via downcast, not substring re-parse.
    if message.contains("not currently signed") || message.contains("no accounts") {
        return Err(anyhow::Error::new(jackin_core::OpProbeError::NotSignedIn {
            detail: message.clone(),
        })
        .context(format!(
            "1Password CLI is not signed in (running `{cmd_label}` returned: {message}). \
                 Run `op signin` in your shell, then retry."
        )));
    }
    Err(anyhow::Error::new(jackin_core::OpProbeError::Other {
        message: message.clone(),
    })
    .context(message))
}

/// Wraps [`run_op_with_timeout`]. Not-signed-in classification is applied
/// at the origin inside that helper; this remains the shared JSON probe
/// entry so call sites stay identical.
fn run_op_json(
    binary: &str,
    args: &[&str],
    timeout: std::time::Duration,
) -> anyhow::Result<Vec<u8>> {
    run_op_with_timeout(binary, args, timeout)
}

/// Append `--account <id>` to an `op` argument vector when an account is
/// pinned, so every subcommand builder emits the flag identically.
fn push_account_arg<'a>(args: &mut Vec<&'a str>, account: Option<&'a str>) {
    if let Some(id) = account {
        args.push("--account");
        args.push(id);
    }
}

impl OpStructRunner for OpCli {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        let bytes = run_op_json(
            &self.binary,
            &["account", "list", "--format", "json"],
            self.timeout,
        )?;
        let raw: Vec<RawOpAccount> = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("failed to parse `op account list` JSON: {e}"))?;
        Ok(raw.into_iter().map(OpAccount::from).collect())
    }

    fn vault_list(&self, account: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        let mut args: Vec<&str> = vec!["vault", "list"];
        push_account_arg(&mut args, account);
        args.extend_from_slice(&["--format", "json"]);
        let bytes = run_op_json(&self.binary, &args, self.timeout)?;
        let raw: Vec<RawOpVault> = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("failed to parse `op vault list` JSON: {e}"))?;
        Ok(raw.into_iter().map(OpVault::from).collect())
    }

    fn item_list(&self, vault_id: &str, account: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        let mut args: Vec<&str> = vec!["item", "list", "--vault", vault_id];
        push_account_arg(&mut args, account);
        args.extend_from_slice(&["--format", "json"]);
        let bytes = run_op_json(&self.binary, &args, self.timeout)?;
        let raw: Vec<crate::picker::RawOpItem> = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("failed to parse `op item list` JSON: {e}"))?;
        Ok(raw.into_iter().map(OpItem::from).collect())
    }

    fn item_get(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<jackin_core::OpItemDetail<OpField>> {
        let mut args: Vec<&str> = vec!["item", "get", item_id, "--vault", vault_id];
        push_account_arg(&mut args, account);
        args.extend_from_slice(&["--format", "json"]);
        let bytes = run_op_json(&self.binary, &args, self.timeout)?;
        let detail: RawOpItemDetail = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("failed to parse `op item get` JSON: {e}"))?;
        Ok(jackin_core::OpItemDetail {
            fields: detail.fields.into_iter().map(OpField::from).collect(),
            sections: detail.sections.into_iter().map(Into::into).collect(),
        })
    }
}

/// JSON shape returned by `op item create --format json`. Only the
/// fields jackin needs to construct an [`OpRef`] are deserialized.
#[derive(serde::Deserialize)]
struct RawCreatedItem {
    id: String,
    title: String,
    vault: RawCreatedItemVault,
    #[serde(default)]
    fields: Vec<RawCreatedItemField>,
    #[serde(default)]
    sections: Vec<RawCreatedItemSection>,
}

#[derive(serde::Deserialize)]
struct RawCreatedItemVault {
    id: String,
    #[serde(default)]
    name: String,
}

#[derive(serde::Deserialize)]
struct RawCreatedItemField {
    id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    section: Option<RawCreatedItemFieldSection>,
}

#[derive(serde::Deserialize)]
struct RawCreatedItemFieldSection {
    id: String,
}

#[derive(serde::Deserialize)]
struct RawCreatedItemSection {
    id: String,
    #[serde(default)]
    label: String,
}

fn created_item_reference(
    raw: &RawCreatedItem,
    params: &OpItemCreateParams<'_>,
    vault_id: &str,
    template_section_id: Option<&str>,
    account: Option<String>,
) -> anyhow::Result<OpRef> {
    anyhow::ensure!(
        raw.vault.id == vault_id,
        "`op item create` returned vault id {:?}, expected {:?}; item id {:?}",
        raw.vault.id,
        vault_id,
        raw.id
    );

    let (section_id, section_label) = match (params.section, template_section_id) {
        (Some(requested_label), Some(template_id)) => {
            let matching_sections: Vec<&RawCreatedItemSection> = raw
                .sections
                .iter()
                .filter(|section| section.id == template_id)
                .collect();
            let [section] = matching_sections.as_slice() else {
                anyhow::bail!(
                    "`op item create` returned {} section records for requested section id {:?}; \
                     the item was created (id {:?}) but jackin cannot identify its field — \
                     delete by hand in 1Password and re-run setup.",
                    matching_sections.len(),
                    template_id,
                    raw.id
                );
            };
            let display_label = if section.label.is_empty() {
                requested_label
            } else {
                section.label.as_str()
            };
            (Some(template_id), Some(display_label))
        }
        (None, None) => (None, None),
        _ => anyhow::bail!("`op item create` section label and submitted section id do not agree"),
    };

    // Labels are presentation metadata and may repeat. Use the exact section
    // identity submitted in the template, then require one matching field; a
    // duplicate or missing response must never select an arbitrary ID.
    let matching_fields: Vec<&RawCreatedItemField> = raw
        .fields
        .iter()
        .filter(|field| {
            field.label.eq_ignore_ascii_case(params.field_label)
                && field.section.as_ref().map(|section| section.id.as_str()) == section_id
        })
        .collect();
    let field = match matching_fields.as_slice() {
        [field] => *field,
        [] => {
            let labels: Vec<&str> = raw
                .fields
                .iter()
                .map(|field| field.label.as_str())
                .collect();
            anyhow::bail!(
                "`op item create` returned no field with label {:?} in the requested section; \
                 observed labels: {labels:?}. The item was created (id {:?}) but jackin cannot \
                 reference its field — delete by hand in 1Password and re-run setup.",
                params.field_label,
                raw.id
            );
        }
        _ => anyhow::bail!(
            "`op item create` returned {} fields with label {:?} in the requested section; \
             the item was created (id {:?}) but jackin cannot choose an unambiguous field — \
             delete by hand in 1Password and re-run setup.",
            matching_fields.len(),
            params.field_label,
            raw.id
        ),
    };
    anyhow::ensure!(
        !field.id.is_empty(),
        "`op item create` returned no field ID for label {:?}",
        params.field_label
    );

    let op_uri = jackin_core::build_op_reference(
        &raw.vault.id,
        &raw.id,
        section_id.as_deref(),
        &field.id,
    )
    .ok_or_else(|| {
        anyhow::anyhow!(
            "`op item create` returned an ID that cannot be represented in the created `op://` reference; item id {:?}",
            raw.id
        )
    })?;

    let vault_name = if raw.vault.name.is_empty() {
        vault_id
    } else {
        raw.vault.name.as_str()
    };
    let section_path = section_label
        .map(|label| format!("{}/", jackin_core::encode_op_breadcrumb_segment(label)))
        .unwrap_or_default();
    let field_label = if field.label.is_empty() {
        params.field_label
    } else {
        field.label.as_str()
    };
    let path = format!(
        "{}/{}/{}{}",
        jackin_core::encode_op_breadcrumb_segment(vault_name),
        jackin_core::encode_op_breadcrumb_segment(&raw.title),
        section_path,
        jackin_core::encode_op_breadcrumb_segment(field_label)
    );

    Ok(OpRef {
        op: op_uri,
        path,
        account,
        on_demand: false,
    })
}

impl OpWriteRunner for OpCli {
    fn item_create(&self, params: OpItemCreateParams<'_>) -> anyhow::Result<OpRef> {
        anyhow::ensure!(
            params.section.is_none_or(|section| !section.is_empty()),
            "section label must not be empty; cannot create a valid 1Password reference"
        );
        let vaults = self.vault_list(self.account.as_deref())?;
        let vault_id = if let Some(vault) = vaults.iter().find(|vault| vault.id == params.vault_id)
        {
            vault.id.clone()
        } else {
            let matches: Vec<_> = vaults
                .iter()
                .filter(|vault| vault.name.eq_ignore_ascii_case(params.vault_id))
                .collect();
            anyhow::ensure!(
                matches.len() == 1,
                "vault {:?} resolved to {} vaults; select a vault by ID and retry",
                params.vault_id,
                matches.len()
            );
            matches
                .first()
                .map(|vault| vault.id.clone())
                .ok_or_else(|| anyhow::anyhow!("vault {:?} was not found", params.vault_id))?
        };
        anyhow::ensure!(
            jackin_core::is_valid_op_reference_path_component(&vault_id),
            "vault id {vault_id:?} cannot be represented as one `op://` path component"
        );

        // Build the JSON template. `op item create -` reads it from
        // stdin so the secret value never crosses argv. Tags and
        // notesPlain ride along inside the same template — neither
        // is sensitive but consolidating into one stdin payload
        // keeps the argv invocation deterministic and free of
        // operator-supplied content.
        let template_section_id = params.section.map(op_section_id);
        let template_section_id = template_section_id.as_deref();
        anyhow::ensure!(
            template_section_id.is_none_or(jackin_core::is_valid_op_reference_path_component),
            "generated section ID cannot be represented as one `op://` path component"
        );
        // `op` assigns the new item's and field's IDs. Preflight the complete
        // path with the resolved vault and exact submitted section ID; validate
        // the CLI-returned generated IDs against the same builder below.
        anyhow::ensure!(
            jackin_core::build_op_reference(
                &vault_id,
                "jackin-pending-item",
                template_section_id,
                "jackin-pending-field"
            )
            .is_some(),
            "cannot build a valid `op://` reference from the item template"
        );
        let mut field = serde_json::json!({
            // Ask `op` for a unique stable field ID; the label stays display
            // metadata and never serves as canonical URI identity.
            "id": "",
            "label": params.field_label,
            "type": "CONCEALED",
            "value": params.value,
        });
        let mut template = serde_json::json!({
            "title": params.title,
            "category": params.category,
            "tags": params.tags,
            "notesPlain": params.notes_plain.unwrap_or(""),
        });
        if let (Some(label), Some(section_id)) = (params.section, template_section_id) {
            template["sections"] = serde_json::json!([{ "id": section_id, "label": label }]);
            field["section"] = serde_json::json!({ "id": section_id });
        }
        template["fields"] = serde_json::json!([field]);
        let body = serde_json::to_vec(&template)
            .map_err(|e| anyhow::anyhow!("failed to encode op item template: {e}"))?;

        let mut args = Vec::new();
        push_account_arg(&mut args, self.account.as_deref());
        args.extend_from_slice(&[
            "item",
            "create",
            "--vault",
            vault_id.as_str(),
            "--format",
            "json",
            "-",
        ]);
        let mut request = jackin_process::ExecRequest::new(&self.binary, &args);
        request.stdin = Some(body);
        request.timeout = Some(self.timeout);
        let out = crate::process_telemetry::exec_sync_op_with_retry(&request, OP_SPAWN_RETRIES)?;
        if out.timed_out {
            anyhow::bail!("1Password CLI item create timed out");
        }
        if !out.success {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!(
                "`op item create` exited with status {}: {}",
                out.code
                    .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
                truncate_stderr(&stderr).trim()
            );
        }

        // SAFETY: `op item create --format json` echoes the created
        // item's fields back, including the secret `value` for
        // CONCEALED fields. We deserialize via `RawCreatedItem`
        // (which intentionally omits `value`) and never embed the
        // raw stdout bytes in any error message — the
        // field-not-found arm below lists labels and ids only.
        let raw: RawCreatedItem = serde_json::from_slice(&out.stdout).map_err(|e| {
            anyhow::anyhow!(
                "failed to parse `op item create` JSON: {e} \
                 (item may have been created but its layout is unrecognised; \
                 inspect or delete by hand in 1Password)"
            )
        })?;
        created_item_reference(
            &raw,
            &params,
            &vault_id,
            template_section_id,
            self.account.clone(),
        )
    }

    fn item_delete(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<()> {
        // Per-call account override beats the OpCli's pinned account
        // so a caller can target a specific 1P account even when the
        // workspace is unscoped. Read-side `OpStructRunner::item_get`
        // does NOT consult `self.account` — that asymmetry is
        // deliberate: the read path is driven by the picker, which
        // sets the account on the call itself.
        let effective_account = account.or(self.account.as_deref());
        let mut args: Vec<&str> = Vec::new();
        push_account_arg(&mut args, effective_account);
        args.extend_from_slice(&["item", "delete", item_id, "--vault", vault_id]);
        drop(run_op_with_timeout(&self.binary, &args, self.timeout)?);
        Ok(())
    }

    fn item_tags(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<Vec<String>> {
        let effective_account = account.or(self.account.as_deref());
        let mut args: Vec<&str> = Vec::new();
        push_account_arg(&mut args, effective_account);
        args.extend_from_slice(&[
            "item", "get", item_id, "--vault", vault_id, "--format", "json",
        ]);
        let raw = run_op_with_timeout(&self.binary, &args, self.timeout)
            .map_err(|e| anyhow::anyhow!("`op item get` (tags) failed: {e}"))?;
        let item: serde_json::Value = serde_json::from_slice(&raw)
            .map_err(|e| anyhow::anyhow!("failed to parse `op item get` JSON: {e}"))?;
        let tags = item["tags"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        Ok(tags)
    }

    fn item_field_set(
        &self,
        item_id: &str,
        vault_id: &str,
        target: &jackin_core::FieldTarget,
        value: &str,
        section: Option<&jackin_core::OpSectionTarget>,
    ) -> anyhow::Result<OpRef> {
        anyhow::ensure!(
            jackin_core::is_valid_op_reference_path_component(vault_id),
            "vault id {vault_id:?} cannot be represented as one `op://` path component"
        );
        anyhow::ensure!(
            jackin_core::is_valid_op_reference_path_component(item_id),
            "item id {item_id:?} cannot be represented as one `op://` path component"
        );

        // Step 1: fetch the full item JSON so we can modify one field
        // while preserving all other fields and metadata.
        let mut get_args: Vec<&str> = Vec::new();
        push_account_arg(&mut get_args, self.account.as_deref());
        get_args.extend_from_slice(&[
            "item", "get", item_id, "--vault", vault_id, "--format", "json",
        ]);
        let raw_bytes = run_op_with_timeout(&self.binary, &get_args, self.timeout)
            .map_err(|e| anyhow::anyhow!("`op item get` failed: {e}"))?;

        // Step 2: parse as a generic JSON value so we can manipulate the
        // `fields` array without discarding unrecognised properties.
        let mut item: serde_json::Value = serde_json::from_slice(&raw_bytes)
            .map_err(|e| anyhow::anyhow!("failed to parse `op item get` JSON: {e}"))?;

        let edit = apply_field_edit(&mut item, target, value, section)?;
        let edited_field_exists = item["fields"].as_array().is_some_and(|fields| {
            fields
                .iter()
                .any(|field| matches_field_target(field, target, edit.section_id.as_deref()))
        });
        anyhow::ensure!(
            edited_field_exists,
            "edited field disappeared before preflight"
        );
        let preflight_field_id = edit
            .existing_field_id
            .as_deref()
            .unwrap_or("jackin-pending-field");
        anyhow::ensure!(
            jackin_core::build_op_reference(
                vault_id,
                item_id,
                edit.section_id.as_deref(),
                preflight_field_id
            )
            .is_some(),
            "cannot build a valid `op://` reference from the existing 1Password IDs; re-open the picker to refresh and retry"
        );

        let body = serde_json::to_vec(&item)
            .map_err(|e| anyhow::anyhow!("failed to re-encode item JSON: {e}"))?;

        // Step 3: pipe the modified item JSON to `op item edit <id>`.
        // `op item edit` takes the item as a positional and reads a JSON
        // template from stdin (the documented `cat updated.json | op item
        // edit <id>` form), so the secret value rides in stdin, never on
        // argv. The item id must be the positional — `-` would be parsed
        // as the item name, not a stdin sentinel (that is the create-only
        // convention). `--template` is mutually exclusive with piped
        // input, so it is intentionally not passed.
        let mut args = Vec::new();
        push_account_arg(&mut args, self.account.as_deref());
        args.extend_from_slice(&[
            "item", "edit", item_id, "--vault", vault_id, "--format", "json",
        ]);
        let mut request = jackin_process::ExecRequest::new(&self.binary, &args);
        request.stdin = Some(body);
        request.timeout = Some(self.timeout);
        let out = crate::process_telemetry::exec_sync_op_with_retry(&request, OP_SPAWN_RETRIES)?;
        if out.timed_out {
            anyhow::bail!("1Password CLI item edit timed out");
        }
        if !out.success {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!(
                "`op item edit` exited with status {}: {}",
                out.code
                    .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
                truncate_stderr(&stderr).trim()
            );
        }

        // Step 4: parse the returned item JSON and build the ref.
        let updated: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| anyhow::anyhow!("failed to parse `op item edit` JSON: {e}"))?;

        resolve_edited_field_ref(
            &updated,
            target,
            vault_id,
            item_id,
            self.account.clone(),
            &edit,
            section,
        )
    }
}
