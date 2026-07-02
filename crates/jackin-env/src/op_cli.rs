use crate::op_runner::OpRunner;
use crate::op_struct::{OpItemCreateParams, OpStructRunner, OpWriteRunner};
use crate::picker::{
    RawOpAccount, RawOpItemDetail, RawOpVault, apply_field_edit, op_section_id,
    resolve_edited_field_ref,
};
use jackin_core::OpRef;
use jackin_core::op_types::{OpAccount, OpField, OpItem, OpVault};

const OP_DEFAULT_BIN: &str = "op";
const OP_DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const OP_LAUNCH_ENV_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
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
    pub fn new() -> Self {
        Self {
            binary: OP_DEFAULT_BIN.to_owned(),
            timeout: OP_DEFAULT_TIMEOUT,
            account: None,
        }
    }

    /// Launch-time operator-env reads run on the foreground path, but real
    /// 1Password app/daemon wakeups can exceed the default 30s budget while
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
    #[expect(
        clippy::duration_suboptimal_units,
        reason = "std has no from_mins; from_secs is the canonical constructor for a 5-minute timeout"
    )]
    pub fn new_interactive() -> Self {
        Self {
            binary: OP_DEFAULT_BIN.to_owned(),
            timeout: std::time::Duration::from_secs(300),
            account: None,
        }
    }

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
mod tests {
    use super::*;

    #[test]
    fn launch_env_runner_uses_wider_bounded_timeout() {
        let runner = OpCli::new_launch_env();

        assert_eq!(runner.binary, OP_DEFAULT_BIN);
        assert_eq!(runner.timeout, std::time::Duration::from_secs(120));
        assert_eq!(runner.account, None);
    }
}

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
fn drain_bounded_stderr(mut stderr: std::process::ChildStderr) -> Vec<u8> {
    use std::io::Read;

    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match stderr.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > OP_STDERR_MAX + 1 {
                    let mut sink = [0u8; 4096];
                    while matches!(stderr.read(&mut sink), Ok(n) if n > 0) {}
                    break;
                }
            }
        }
    }
    buf
}

/// Poll `try_wait` and forward the exit status, releasing the mutex
/// between attempts so the timeout branch can `take` and `kill` the
/// child without contending on a blocking `wait()`.
fn spawn_wait_thread(
    child: std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    tx: std::sync::mpsc::Sender<std::io::Result<std::process::ExitStatus>>,
) {
    std::thread::spawn(move || {
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

fn is_text_file_busy(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(TEXT_FILE_BUSY_OS_ERROR)
}

fn spawn_op_with_retry<F>(mut build: F) -> std::io::Result<std::process::Child>
where
    F: FnMut() -> std::process::Command,
{
    for attempt in 0..OP_SPAWN_RETRIES {
        let mut command = build();
        match command.spawn() {
            Ok(child) => return Ok(child),
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

fn op_spawn_error(binary: &str, error: &std::io::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "failed to spawn 1Password CLI {binary:?}: {error} \
         (is `op` installed and on your PATH? see \
         https://developer.1password.com/docs/cli/)"
    )
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

        let mut child = spawn_op_with_retry(|| {
            let mut cmd = Command::new(&self.binary);
            if let Some(account) = self.account.as_deref() {
                cmd.args(["--account", account]);
            }
            cmd.args(["read", reference])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            cmd
        })
        .map_err(|error| op_spawn_error(&self.binary, &error))?;

        // Channel-and-thread wait pattern so we avoid a new async dep,
        // and the wait thread never holds the mutex across a blocking
        // wait — see spawn_wait_thread.
        let (tx, rx) = std::sync::mpsc::channel();
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("1Password CLI stdout pipe missing"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("1Password CLI stderr pipe missing"))?;
        let timeout = self.timeout;

        let stdout_handle = std::thread::spawn(move || {
            let mut buf = Vec::new();
            drop(stdout.read_to_end(&mut buf));
            buf
        });
        let stderr_handle = std::thread::spawn(move || drain_bounded_stderr(stderr));

        let child = std::sync::Arc::new(std::sync::Mutex::new(Some(child)));
        spawn_wait_thread(std::sync::Arc::clone(&child), tx);

        let status = match rx.recv_timeout(timeout) {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                anyhow::bail!("1Password CLI wait failed for {reference:?}: {e}");
            }
            Err(_) => {
                // Child may have exited between recv_timeout expiring
                // and the take below (yielding Err(InvalidInput) on
                // kill), which is not a real failure. Reap so pipes
                // close and reader threads exit.
                let killed = child
                    .lock()
                    .map_err(|_| anyhow::anyhow!("child mutex poisoned"))?
                    .take();
                if let Some(mut c) = killed {
                    drop(c.kill());
                    drop(c.wait());
                }
                anyhow::bail!(
                    "1Password CLI timed out after {}s resolving {reference:?}",
                    timeout.as_secs()
                );
            }
        };

        let stdout_bytes = stdout_handle.join().unwrap_or_default();
        let stderr_bytes = stderr_handle.join().unwrap_or_default();

        if status.success() {
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
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut child = spawn_op_with_retry(|| {
        let mut command = Command::new(binary);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    })
    .map_err(|error| op_spawn_error(binary, &error))?;

    let (tx, rx) = std::sync::mpsc::channel();
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("1Password CLI stdout pipe missing"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("1Password CLI stderr pipe missing"))?;

    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        drop(stdout.read_to_end(&mut buf));
        buf
    });
    let stderr_handle = std::thread::spawn(move || drain_bounded_stderr(stderr));

    let child = std::sync::Arc::new(std::sync::Mutex::new(Some(child)));
    spawn_wait_thread(std::sync::Arc::clone(&child), tx);

    let cmd_label = format!("op {}", args.join(" "));
    let status = match rx.recv_timeout(timeout) {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            anyhow::bail!("1Password CLI wait failed for `{cmd_label}`: {e}");
        }
        Err(_) => {
            let killed = child
                .lock()
                .map_err(|_| anyhow::anyhow!("child mutex poisoned"))?
                .take();
            if let Some(mut c) = killed {
                drop(c.kill());
                drop(c.wait());
            }
            anyhow::bail!(
                "1Password CLI timed out after {}s running `{cmd_label}`",
                timeout.as_secs()
            );
        }
    };

    let stdout_bytes = stdout_handle.join().unwrap_or_default();
    let stderr_bytes = stderr_handle.join().unwrap_or_default();

    if status.success() {
        return Ok(stdout_bytes);
    }

    let stderr = String::from_utf8_lossy(&stderr_bytes);
    let stderr_trimmed = truncate_stderr(&stderr);
    let stderr_msg = stderr_trimmed.trim();
    anyhow::bail!(
        "1Password CLI exited with status {} running `{cmd_label}`: {stderr_msg}",
        format_exit_status(status),
    )
}

/// Wraps [`run_op_with_timeout`] and additionally rewrites the
/// "not signed in" / "no accounts" stderr signature into a dedicated
/// error message the picker pattern-matches on.
fn run_op_json(
    binary: &str,
    args: &[&str],
    timeout: std::time::Duration,
) -> anyhow::Result<Vec<u8>> {
    let cmd_label = format!("op {}", args.join(" "));
    run_op_with_timeout(binary, args, timeout).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("not currently signed") || msg.contains("no accounts") {
            anyhow::anyhow!(
                "1Password CLI is not signed in (running `{cmd_label}` returned: {msg}). \
                 Run `op signin` in your shell, then retry."
            )
        } else {
            e
        }
    })
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
    ) -> anyhow::Result<Vec<OpField>> {
        let mut args: Vec<&str> = vec!["item", "get", item_id, "--vault", vault_id];
        push_account_arg(&mut args, account);
        args.extend_from_slice(&["--format", "json"]);
        let bytes = run_op_json(&self.binary, &args, self.timeout)?;
        let detail: RawOpItemDetail = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("failed to parse `op item get` JSON: {e}"))?;
        Ok(detail.fields.into_iter().map(OpField::from).collect())
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
}

impl OpWriteRunner for OpCli {
    #[expect(
        clippy::too_many_lines,
        reason = "pending extraction — tracked in codebase-readability roadmap"
    )]
    fn item_create(&self, params: OpItemCreateParams<'_>) -> anyhow::Result<OpRef> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        // Build the JSON template. `op item create -` reads it from
        // stdin so the secret value never crosses argv. Tags and
        // notesPlain ride along inside the same template — neither
        // is sensitive but consolidating into one stdin payload
        // keeps the argv invocation deterministic and free of
        // operator-supplied content.
        let mut field = serde_json::json!({
            "id": params.field_label,
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
        if let Some(label) = params.section {
            let section_id = op_section_id(label);
            template["sections"] = serde_json::json!([{ "id": section_id, "label": label }]);
            field["section"] = serde_json::json!({ "id": section_id });
        }
        template["fields"] = serde_json::json!([field]);
        let body = serde_json::to_vec(&template)
            .map_err(|e| anyhow::anyhow!("failed to encode op item template: {e}"))?;

        let mut child = spawn_op_with_retry(|| {
            let mut command = Command::new(&self.binary);
            if let Some(account) = self.account.as_deref() {
                command.args(["--account", account]);
            }
            command.args([
                "item",
                "create",
                "--vault",
                params.vault_id,
                "--format",
                "json",
            ]);
            command.arg("-");
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command
        })
        .map_err(|error| op_spawn_error(&self.binary, &error))?;

        // Write the template body to the child's stdin and drop the
        // handle so `op` sees EOF and proceeds. Scoping the stdin
        // borrow with `take()` ensures the pipe is closed before we
        // call `wait_with_output()` — leaving it open would deadlock
        // if `op` waits for EOF before printing JSON.
        //
        // If the write fails (typically `EPIPE` because `op` rejected
        // the template body and exited), drain its stderr before
        // surfacing the error so the operator sees the real cause
        // (auth failure, vault permission, schema mismatch) instead
        // of a generic "stdin write failed".
        if let Some(mut stdin) = child.stdin.take()
            && let Err(e) = stdin.write_all(&body)
        {
            drop(stdin);
            let captured = child.wait_with_output().ok();
            let stderr_msg = captured
                .as_ref()
                .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
                .unwrap_or_default();
            anyhow::bail!(
                "failed to write op item template to stdin: {e} (op stderr: {})",
                truncate_stderr(&stderr_msg).trim()
            );
        }

        let out = child
            .wait_with_output()
            .map_err(|e| anyhow::anyhow!("1Password CLI wait failed: {e}"))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!(
                "`op item create` exited with status {}: {}",
                format_exit_status(out.status),
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

        // Locate the field by case-insensitive label match — the
        // template `id` we sent is what `op` echoes back as the
        // field id, but downstream callers expect to look up by
        // operator-visible `field_label`.
        let field = raw
            .fields
            .iter()
            .find(|f| f.label.eq_ignore_ascii_case(params.field_label))
            .ok_or_else(|| {
                let labels: Vec<&str> = raw.fields.iter().map(|f| f.label.as_str()).collect();
                anyhow::anyhow!(
                    "`op item create` returned no field with label {:?}; \
                     observed labels: {labels:?}. The item was created (id {:?}) \
                     but jackin cannot reference its field — delete by hand in \
                     1Password and re-run setup.",
                    params.field_label,
                    raw.id,
                )
            })?;

        // Always use UUID-based op:// so the reference is stable even if
        // the vault, item, or field is renamed. `path` carries the
        // human-readable names for display only — it must have the same
        // three-segment structure as the `op` URI.
        let op_uri = format!("op://{}/{}/{}", raw.vault.id, raw.id, field.id);

        let vault_name = if raw.vault.name.is_empty() {
            raw.vault.id.as_str()
        } else {
            raw.vault.name.as_str()
        };
        let path = format!("{}/{}/{}", vault_name, raw.title, params.field_label);

        Ok(OpRef {
            op: op_uri,
            path,
            account: self.account.clone(),
            on_demand: false,
        })
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
        section: Option<&str>,
    ) -> anyhow::Result<OpRef> {
        use std::io::Write;
        use std::process::Stdio;

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

        apply_field_edit(&mut item, target, value, section)?;

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
        let mut child = spawn_op_with_retry(|| {
            use std::process::Command;
            let mut command = Command::new(&self.binary);
            if let Some(acc) = self.account.as_deref() {
                command.args(["--account", acc]);
            }
            command.args([
                "item", "edit", item_id, "--vault", vault_id, "--format", "json",
            ]);
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command
        })
        .map_err(|e| op_spawn_error(&self.binary, &e))?;

        if let Some(mut stdin) = child.stdin.take()
            && let Err(e) = stdin.write_all(&body)
        {
            drop(stdin);
            let captured = child.wait_with_output().ok();
            let stderr_msg = captured
                .as_ref()
                .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
                .unwrap_or_default();
            anyhow::bail!(
                "failed to write op item template to stdin: {e} (op stderr: {})",
                truncate_stderr(&stderr_msg).trim()
            );
        }

        let out = child
            .wait_with_output()
            .map_err(|e| anyhow::anyhow!("1Password CLI wait failed: {e}"))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!(
                "`op item edit` exited with status {}: {}",
                format_exit_status(out.status),
                truncate_stderr(&stderr).trim()
            );
        }

        // Step 4: parse the returned item JSON and build the ref.
        let updated: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| anyhow::anyhow!("failed to parse `op item edit` JSON: {e}"))?;

        resolve_edited_field_ref(&updated, target, vault_id, item_id, self.account.clone())
    }
}
