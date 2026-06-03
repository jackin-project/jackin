//! Operator-controlled env resolution: four config layers, three value
//! syntaxes (`op://`, `$NAME` / `${NAME}`, literal), and merging onto
//! the manifest-resolved env at launch.

pub trait OpRunner {
    fn read(&self, reference: &str) -> anyhow::Result<String>;

    /// Read pinned to a specific 1Password account. The production
    /// `OpCli` rebinds itself to `account` before invoking `op` so a
    /// ref whose vault lives in a non-default account resolves. Default
    /// ignores `account` and delegates to `read`, keeping mock runners
    /// trivial.
    fn read_with_account(&self, reference: &str, _account: Option<&str>) -> anyhow::Result<String> {
        self.read(reference)
    }

    /// Probed once per launch so a missing `op` surfaces as a single
    /// install-link error rather than one-per-key noise. Default no-op
    /// keeps mock runners trivial.
    fn probe(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Resolve a single [`EnvValue`] to its final string, dispatching on the
/// enum variant rather than lexical string prefix.
///
/// - `EnvValue::Plain` passes through `$VAR` / `${VAR}` expansion via
///   the host environment; bare `op://...` strings stored as `Plain` are
///   **not** resolved and flow to the container literally.
/// - `EnvValue::OpRef` shells out to `op read <op>` using the canonical
///   UUID URI; failures are wrapped with the human-readable `path` for
///   actionable error messages.
///
/// `layer_label` / `var_name` are used only in error messages.
///
/// Only structural `EnvValue::OpRef` triggers `op read`. Bare
/// `op://...` strings stored as `EnvValue::Plain` flow to the
/// container literally.
pub fn resolve_env_value<R, H>(
    layer_label: &str,
    var_name: &str,
    value: &EnvValue,
    op_runner: &R,
    host_env: H,
) -> anyhow::Result<String>
where
    R: OpRunner + ?Sized,
    H: FnMut(&str) -> Result<String, std::env::VarError>,
{
    match value {
        EnvValue::Plain(s) => dispatch_plain(layer_label, var_name, s, host_env),
        EnvValue::OpRef(r) => op_runner
            .read_with_account(&r.op, r.account.as_deref())
            .map_err(|e| {
                anyhow::anyhow!(
                    "{layer_label} env var {var_name:?}: 1Password reference {:?} failed: {e}",
                    r.path
                )
            }),
    }
}

/// Resolve a plain string value: `$NAME` / `${NAME}` → host env lookup,
/// otherwise verbatim. `op://...` strings are intentionally NOT resolved
/// here — that branch lives exclusively in [`resolve_env_value`] for
/// `EnvValue::OpRef`.
fn dispatch_plain<H>(
    layer_label: &str,
    var_name: &str,
    value: &str,
    mut host_env: H,
) -> anyhow::Result<String>
where
    H: FnMut(&str) -> Result<String, std::env::VarError>,
{
    if let Some(host_name) = parse_host_ref(value) {
        return host_env(host_name).map_err(|_| {
            anyhow::anyhow!(
                "{layer_label} env var {var_name:?}: host env var {host_name:?} is not set"
            )
        });
    }
    Ok(value.to_string())
}

/// Parse `$NAME` or `${NAME}` and return the name. Rejects bare `$`,
/// unmatched braces, and non-identifier characters.
fn parse_host_ref(value: &str) -> Option<&str> {
    if let Some(rest) = value.strip_prefix("${")
        && let Some(name) = rest.strip_suffix('}')
        && is_valid_env_name(name)
    {
        return Some(name);
    }

    if let Some(name) = value.strip_prefix('$')
        && !name.is_empty()
        && is_valid_env_name(name)
    {
        return Some(name);
    }

    None
}

/// Re-exported from `jackin-core` — canonical definitions live there.
pub use jackin_core::{EnvValue, FieldTarget, OpRef};

pub use jackin_console::op_reference::{OpReferenceParts, parse_op_reference};

fn is_valid_env_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

const OP_DEFAULT_BIN: &str = "op";
const OP_DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const OP_STDERR_MAX: usize = 4 * 1024;
const OP_SPAWN_RETRIES: usize = 5;
const TEXT_FILE_BUSY_OS_ERROR: i32 = 26;

/// Production `OpRunner` that shells out to the 1Password CLI.
///
/// Tests inject a different runner (e.g. `TestOpRunner`) rather than
/// using an env-var seam — keeps the crate `unsafe_code = "forbid"`
/// lint intact and tests free of process-env mutation.
#[derive(Clone)]
pub struct OpCli {
    binary: String,
    timeout: std::time::Duration,
    /// Pinned 1P account forwarded as `op --account <id>` on every
    /// invocation. `None` lets `op` fall back to its default-account
    /// context. Write paths set this so the minted ref records the
    /// account it was created under (`OpRef::account`); reads rebind to
    /// the ref's own account via `read_with_account` so multi-account
    /// vaults resolve regardless of which account was last
    /// `op signin`-ed.
    account: Option<String>,
}

impl OpCli {
    pub fn new() -> Self {
        Self {
            binary: OP_DEFAULT_BIN.to_string(),
            timeout: OP_DEFAULT_TIMEOUT,
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
            binary: OP_DEFAULT_BIN.to_string(),
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
            binary: OP_DEFAULT_BIN.to_string(),
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
    const fn with_binary_and_timeout(binary: String, timeout: std::time::Duration) -> Self {
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

fn format_exit_status(status: std::process::ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| "signal".to_string(), |c| c.to_string())
}

/// Truncate stderr to ~`OP_STDERR_MAX` bytes, rounding down to a UTF-8
/// char boundary so a multi-byte codepoint at the cut point cannot
/// panic on the error path.
fn truncate_stderr(stderr: &str) -> String {
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
            let mut guard = child.lock().expect("child mutex poisoned");
            let Some(c) = guard.as_mut() else {
                return;
            };
            let status_opt = match c.try_wait() {
                Ok(Some(s)) => {
                    let _ = guard.take();
                    Some(Ok(s))
                }
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            };
            drop(guard);
            match status_opt {
                Some(r) => {
                    let _ = tx.send(r);
                    return;
                }
                None => std::thread::sleep(poll),
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
                account: account.map(str::to_string),
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
        let mut stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let timeout = self.timeout;

        let stdout_handle = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf);
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
                let killed = child.lock().expect("child mutex poisoned").take();
                if let Some(mut c) = killed {
                    let _ = c.kill();
                    let _ = c.wait();
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

/// Structural `op` queries used by the picker.
///
/// Distinct from [`OpRunner`] (single-value resolution): the picker is
/// a metadata browser and must never deserialize a secret value — see
/// [`RawOpField`].
pub trait OpStructRunner {
    /// Doubles as the sign-in probe before any other call.
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>>;
    /// `account = None` lets `op` use its default-account context.
    fn vault_list(&self, account: Option<&str>) -> anyhow::Result<Vec<OpVault>>;
    fn item_list(&self, vault_id: &str, account: Option<&str>) -> anyhow::Result<Vec<OpItem>>;
    fn item_get(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<Vec<OpField>>;
}

pub fn default_op_struct_runner() -> std::sync::Arc<dyn OpStructRunner + Send + Sync> {
    std::sync::Arc::new(OpCli::new())
}

/// `id` is the `account_uuid` accepted by `op --account <id>`. `email`
/// and `url` feed the picker's Account pane.
pub type OpAccount = jackin_console::tui::components::op_picker::OpPickerAccount;

pub type OpVault = jackin_console::tui::components::op_picker::OpPickerVault;

/// `name` comes from JSON `title`; `subtitle` from
/// `additional_information` (login username/email, empty on secure
/// notes) — used to disambiguate items sharing a title.
pub type OpItem = jackin_console::tui::components::op_picker::OpPickerItem;

/// Field metadata only — the value is intentionally absent.
///
/// `reference` is the verbatim `op://...` 1Password emits per field;
/// the picker commits this rather than synthesizing a path from
/// display names (synthesis was wrong for sections, names containing
/// `/`, or whitespace).
pub type OpField = jackin_console::tui::components::op_picker::OpPickerField;

pub type OpCache = jackin_console::tui::components::op_picker::OpPickerCache;

// Accept either `id` or `account_uuid` so the probe works against
// current and older op CLI shapes. `email` / `url` default to empty
// because older `op` versions may omit them.
#[derive(serde::Deserialize)]
struct RawOpAccount {
    #[serde(alias = "account_uuid")]
    id: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    url: String,
}

#[derive(serde::Deserialize)]
struct RawOpVault {
    id: String,
    name: String,
}

#[derive(serde::Deserialize)]
struct RawOpItem {
    id: String,
    title: String,
    // Missing on secure notes and other non-login item types.
    #[serde(default)]
    additional_information: String,
}

#[derive(serde::Deserialize)]
struct RawOpItemDetail {
    #[serde(default)]
    fields: Vec<RawOpField>,
}

// SAFETY: 'value' is intentionally absent from this struct. The picker is a
// metadata browser; serde must not deserialize secret values into memory.
// Any change adding a `value` field here breaks the picker's trust model.
//
// `reference` IS deserialized: the string `op://...` that 1Password's
// CLI emits per field is metadata, not a credential, and the picker
// commits it verbatim instead of synthesizing a path from display
// names (which mishandled section nesting and `/`/whitespace in
// names).
#[derive(serde::Deserialize)]
struct RawOpField {
    id: String,
    #[serde(default)]
    label: String,
    #[serde(rename = "type", default)]
    field_type: String,
    #[serde(default)]
    purpose: String,
    #[serde(default)]
    reference: String,
}

impl From<RawOpAccount> for OpAccount {
    fn from(raw: RawOpAccount) -> Self {
        Self {
            id: raw.id,
            email: raw.email,
            url: raw.url,
        }
    }
}

impl From<RawOpVault> for OpVault {
    fn from(raw: RawOpVault) -> Self {
        Self {
            id: raw.id,
            name: raw.name,
        }
    }
}

impl From<RawOpItem> for OpItem {
    fn from(raw: RawOpItem) -> Self {
        Self {
            id: raw.id,
            name: raw.title,
            subtitle: raw.additional_information,
        }
    }
}

impl From<RawOpField> for OpField {
    fn from(raw: RawOpField) -> Self {
        let concealed = raw.field_type == "CONCEALED" || raw.purpose == "PASSWORD";
        Self {
            id: raw.id,
            label: raw.label,
            field_type: raw.field_type,
            concealed,
            reference: raw.reference,
        }
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
    let mut stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
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
            let killed = child.lock().expect("child mutex poisoned").take();
            if let Some(mut c) = killed {
                let _ = c.kill();
                let _ = c.wait();
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
        let raw: Vec<RawOpItem> = serde_json::from_slice(&bytes)
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

/// Mutating 1Password operations used by the workspace-token setup
/// orchestrator.
///
/// Held in a separate trait from [`OpStructRunner`] so the read-only
/// SAFETY contract on the picker's `OpCache` cannot be accidentally
/// widened by a future `item_create` impl that decides to memoise
/// its return value.
///
/// All write paths take secret material on **stdin**, never on argv —
/// `op item create login.password=value` is forbidden because that
/// places the secret in `/proc/<pid>/cmdline` where any process on the
/// host with the right uid can read it. Implementations must use
/// `op item create login.password[password]=-` (or the equivalent
/// `--field`) and pipe the value through stdin.
///
/// See `docs/src/content/docs/reference/roadmap/workspace-claude-token-setup.mdx`
/// for the operator-facing flow this trait powers.
pub trait OpWriteRunner {
    /// Create an item and return the canonical `op://...` reference
    /// pointing at the named field. `value` lands on the child's
    /// stdin — never on argv.
    ///
    /// `category` is an `op` item category in the underscore form the
    /// CLI accepts (`"API_CREDENTIAL"`, `"PASSWORD"`, `"SECURE_NOTE"`;
    /// see `op item template list`). `notes_plain` populates the
    /// item's free-form notes block (used by the orchestrator to
    /// stamp `{workspace, host, created, expires, token_sha256_prefix}`).
    fn item_create(&self, params: OpItemCreateParams<'_>) -> anyhow::Result<OpRef>;

    /// Overwrite (or add) a single field in an existing 1Password item.
    ///
    /// For [`FieldTarget::Existing`] the field is located by its exact op
    /// id, its `value` is overwritten, its type is set to `CONCEALED`, and
    /// its existing section placement is left untouched — overwriting a
    /// value must never re-parent the field. For [`FieldTarget::New`] the
    /// field is located by label (overwrite if present); if no such field
    /// exists a new `CONCEALED` field is appended, placed in `section` when
    /// one is given. All other fields and item metadata are preserved.
    ///
    /// The secret value reaches `op` via stdin (GET → modify in-process
    /// → EDIT via stdin), following the same never-on-argv contract as
    /// `item_create`. The implementation issues two `op` invocations:
    /// 1. `op item get <id> --vault <vault> --format json` — fetch the
    ///    full item template.
    /// 2. `op item edit <id> --vault <vault> --format json` — pipe the
    ///    modified template back on stdin.
    fn item_field_set(
        &self,
        item_id: &str,
        vault_id: &str,
        target: &FieldTarget,
        value: &str,
        section: Option<&str>,
    ) -> anyhow::Result<OpRef>;

    /// Delete an item entirely. Used by
    /// `jackin workspace claude-token revoke --delete-op-item` and
    /// by the rotate flow to remove the prior 1P item once the new
    /// one is wired and validated.
    fn item_delete(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Read an item's `tags` array. Used by the rotate flow to decide
    /// whether the prior item is jackin-owned (and therefore safe to
    /// delete) versus an item the operator adopted via `--reuse` /
    /// interactive edit-in-place (which jackin must not delete, since it
    /// may hold the operator's other fields).
    fn item_tags(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<Vec<String>>;
}

/// Parameters for [`OpWriteRunner::item_create`]. Borrowed-form to
/// match the existing `OpStructRunner` style and avoid cloning every
/// string at the call site.
///
/// The `op` account is pinned on the [`OpCli`] instance via
/// [`OpCli::with_account`] before the call — there is no per-call
/// override, mirroring how [`OpRunner::read`] consumes
/// [`OpCli::account`].
#[derive(Debug, Clone, Copy)]
pub struct OpItemCreateParams<'a> {
    pub vault_id: &'a str,
    pub title: &'a str,
    /// `op` item category in the underscore form (e.g.
    /// [`crate::workspace::token_setup::DEFAULT_ITEM_CATEGORY`]).
    pub category: &'a str,
    /// Field label (`"token"`, `"password"`, etc.).
    pub field_label: &'a str,
    /// Field value — lands on stdin, never on argv.
    pub value: &'a str,
    /// Optional `notesPlain` block (provenance metadata stamp).
    pub notes_plain: Option<&'a str>,
    /// `op` item tags applied at create time so list/search filters
    /// can find every jackin-managed item.
    pub tags: &'a [&'a str],
    /// Optional 1Password section label. When set, the field is placed
    /// in a section with this label; when `None`, the field is unsectioned.
    pub section: Option<&'a str>,
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

/// Slug a 1Password section label into a deterministic section id:
/// lowercase, collapse each run of non-alphanumeric characters into a
/// single `_`, and trim leading/trailing `_`. Empty results fall back
/// to `"section"` so the id is always a valid non-empty identifier.
fn op_section_id(label: &str) -> String {
    let mut id = String::with_capacity(label.len());
    let mut pending_underscore = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_underscore && !id.is_empty() {
                id.push('_');
            }
            pending_underscore = false;
            id.push(ch.to_ascii_lowercase());
        } else {
            pending_underscore = true;
        }
    }
    if id.is_empty() {
        "section".to_string()
    } else {
        id
    }
}

/// Apply a single concealed-field edit to a parsed `op item get` JSON
/// value in place, ready to pipe back to `op item edit`.
///
/// [`FieldTarget::Existing`] is located by its exact op id, so a same-
/// labeled field in another section is never clobbered, and the field's
/// existing `section` is left untouched — overwriting a value must not
/// re-parent the field (GUI-created section ids are opaque, not the
/// `label` slug). A stale id (gone since it was picked) bails loudly
/// rather than appending a stray field. [`FieldTarget::New`] places a new
/// `CONCEALED` field (overwriting a same-label field if one exists),
/// in `section` when one is supplied, registering that section if missing.
fn apply_field_edit(
    item: &mut serde_json::Value,
    target: &FieldTarget,
    value: &str,
    section: Option<&str>,
) -> anyhow::Result<()> {
    let fields = item["fields"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("item has no `fields` array"))?;

    let label = target.label();
    let found = match target {
        FieldTarget::Existing { id, .. } => {
            fields.iter_mut().find(|f| f["id"].as_str() == Some(id))
        }
        FieldTarget::New { label } => fields.iter_mut().find(|f| {
            f["label"].as_str() == Some(label.as_str()) || f["id"].as_str() == Some(label.as_str())
        }),
    };

    let section_id = section.map(op_section_id);
    let mut appended_in_section = false;
    match (found, target) {
        (Some(field), _) => {
            field["value"] = serde_json::Value::String(value.to_string());
            field["type"] = serde_json::Value::String("CONCEALED".to_string());
        }
        // A specific field id was requested but is gone (renamed/deleted in
        // 1Password since it was picked, or read from a stale cache). Fail
        // loudly instead of appending a stray label-named field — the
        // read-back would then miss the id and error anyway, but only after
        // mutating the operator's item.
        (None, FieldTarget::Existing { id, .. }) => anyhow::bail!(
            "field id {id:?} not found in the item — it may have been renamed or deleted in \
             1Password since it was picked; re-open the picker to refresh and retry"
        ),
        (None, FieldTarget::New { .. }) => {
            let mut field = serde_json::json!({
                "id": label,
                "label": label,
                "type": "CONCEALED",
                "value": value,
            });
            if let Some(id) = section_id.as_deref() {
                field["section"] = serde_json::json!({ "id": id });
                appended_in_section = true;
            }
            fields.push(field);
        }
    }

    // Register the section only when a new field was actually placed in
    // it; an overwrite never creates or moves sections.
    if appended_in_section && let (Some(id), Some(label)) = (section_id.as_deref(), section) {
        if !item["sections"].is_array() {
            item["sections"] = serde_json::Value::Array(Vec::new());
        }
        let sections = item["sections"]
            .as_array_mut()
            .expect("sections coerced to array above");
        if !sections.iter().any(|s| s["id"].as_str() == Some(id)) {
            sections.push(serde_json::json!({ "id": id, "label": label }));
        }
    }
    Ok(())
}

impl OpWriteRunner for OpCli {
    #[allow(clippy::too_many_lines)]
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
        let _ = run_op_with_timeout(&self.binary, &args, self.timeout)?;
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
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        Ok(tags)
    }

    fn item_field_set(
        &self,
        item_id: &str,
        vault_id: &str,
        target: &FieldTarget,
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

/// Locate the edited field in the JSON `op item edit` returns and build the
/// UUID-form `OpRef`. [`FieldTarget::Existing`] matches by the exact id
/// (stable across the edit); [`FieldTarget::New`] matches by label (case-
/// insensitive), since `op` assigns the new field's id. The `op://` ref is
/// built from UUIDs (vault/item/field ids) so it survives renames; `path`
/// carries the human-readable names for display, same three-segment shape.
fn resolve_edited_field_ref(
    updated: &serde_json::Value,
    target: &FieldTarget,
    vault_id: &str,
    item_id: &str,
    account: Option<String>,
) -> anyhow::Result<OpRef> {
    let label = target.label();
    let updated_fields = updated["fields"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("updated item has no `fields` array"))?;

    let field = updated_fields
        .iter()
        .find(|f| match target {
            FieldTarget::Existing { id, .. } => f["id"].as_str() == Some(id),
            FieldTarget::New { label } => {
                f["label"]
                    .as_str()
                    .is_some_and(|l| l.eq_ignore_ascii_case(label))
                    || f["id"].as_str() == Some(label)
            }
        })
        .ok_or_else(|| {
            let labels: Vec<&str> = updated_fields
                .iter()
                .filter_map(|f| f["label"].as_str())
                .collect();
            anyhow::anyhow!(
                "`op item edit` returned no field matching {target:?}; \
                 observed labels: {labels:?}"
            )
        })?;

    let vid = updated["vault"]["id"].as_str().unwrap_or(vault_id);
    let iid = updated["id"].as_str().unwrap_or(item_id);
    let fid = field["id"].as_str().unwrap_or(label);
    let op_uri = format!("op://{vid}/{iid}/{fid}");

    let vault_name = updated["vault"]["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(vault_id);
    let item_title = updated["title"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(item_id);
    let field_label_display = field["label"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(label);
    let path = format!("{vault_name}/{item_title}/{field_label_display}");

    Ok(OpRef {
        op: op_uri,
        path,
        account,
    })
}

/// Source layer of an env value, attached to error messages and
/// launch diagnostics so the operator can locate the offending entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvLayer {
    Global,
    Role(String),
    Workspace(String),
    WorkspaceRole { workspace: String, role: String },
}

impl std::fmt::Display for EnvLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Global => write!(f, "global [env]"),
            Self::Role(name) => write!(f, "role {name:?} [env]"),
            Self::Workspace(name) => write!(f, "workspace {name:?} [env]"),
            Self::WorkspaceRole { workspace, role } => {
                write!(f, "workspace {workspace:?} → role {role:?} [env]")
            }
        }
    }
}

/// Later-wins merge. Order, low → high priority:
/// global → role → workspace → workspace-role.
pub fn merge_layers(
    global: &std::collections::BTreeMap<String, EnvValue>,
    role: &std::collections::BTreeMap<String, EnvValue>,
    workspace: &std::collections::BTreeMap<String, EnvValue>,
    workspace_role: &std::collections::BTreeMap<String, EnvValue>,
) -> std::collections::BTreeMap<String, EnvValue> {
    let mut merged = std::collections::BTreeMap::new();
    for layer in [global, role, workspace, workspace_role] {
        for (k, v) in layer {
            merged.insert(k.clone(), v.clone());
        }
    }
    merged
}

/// Reject operator env maps that declare any reserved runtime name.
/// Runs at config-load time so misconfigurations fail before launch.
/// Conflicts across every layer are aggregated into one error.
pub fn validate_reserved_names(config: &crate::config::AppConfig) -> anyhow::Result<()> {
    let mut offenses: Vec<String> = Vec::new();
    let mut record = |layer: EnvLayer, env: &std::collections::BTreeMap<String, EnvValue>| {
        for key in env.keys() {
            if crate::env_model::is_reserved(key) {
                offenses.push(format!(
                    "  - {key:?} is reserved by the jackin runtime; declared in {layer}"
                ));
            }
        }
    };

    record(EnvLayer::Global, &config.env);
    for (role_name, role_source) in &config.roles {
        record(EnvLayer::Role(role_name.clone()), &role_source.env);
    }
    for (ws_name, ws) in &config.workspaces {
        record(EnvLayer::Workspace(ws_name.clone()), &ws.env);
        for (role_name, override_) in &ws.roles {
            record(
                EnvLayer::WorkspaceRole {
                    workspace: ws_name.clone(),
                    role: role_name.clone(),
                },
                &override_.env,
            );
        }
    }

    if offenses.is_empty() {
        return Ok(());
    }

    anyhow::bail!(
        "operator env map contains {} reserved runtime name(s):\n{}\n\
         These names are fixed by jackin and cannot be overridden. Remove them \
         from your config.toml.",
        offenses.len(),
        offenses.join("\n")
    )
}

/// Resolve a user-supplied `op://...` URI into a canonical [`OpRef`].
///
/// Accepts all official 1Password URI forms: names, UUIDs, mixed, with
/// optional subtitle filter `Item[subtitle]`, optional 4th section segment,
/// and optional query suffix (`?attribute=otp` etc.). Errors on ambiguity,
/// missing items or fields, or unsupported `${VAR}` substitution syntax.
///
/// The caller must probe `op` CLI availability before calling this
/// (e.g. via [`OpRunner::probe`]).
///
/// `account` pins every underlying `op` query (`vault list`, `item
/// list`, `item get`) to a specific 1Password account. Required when
/// the operator runs more than one signed-in account: a name-based
/// `op://...` reference can otherwise resolve a coincidentally-named
/// item from the default account instead of the intended one. Pass
/// `None` when the call has no account context (e.g. ambient
/// `op://...` resolution where the operator has not pinned an
/// account).
#[allow(clippy::too_many_lines)]
pub fn resolve_op_uri_to_ref(
    input: &str,
    op: &dyn OpStructRunner,
    account: Option<&str>,
) -> anyhow::Result<OpRef> {
    use anyhow::{anyhow, bail};

    if !input.starts_with("op://") {
        bail!("not an op:// reference: {input}");
    }
    if input.contains("${") {
        bail!(
            "jackin does not support shell variable substitution inside `op://` URIs \
             (`{input}`). Use a plain string value, or substitute before passing."
        );
    }

    // Peel off optional `?attribute=...` / `?attr=...` / `?ssh-format=...` suffix.
    let (path_part, query) = input
        .find('?')
        .map_or((input, None), |i| (&input[..i], Some(&input[i..])));
    let body = path_part.strip_prefix("op://").unwrap();
    let segs: Vec<&str> = body.split('/').collect();
    let (vault_seg, item_seg, section_seg, field_seg) = match segs.as_slice() {
        [v, i, f] => (*v, *i, None::<&str>, *f),
        [v, i, s, f] => (*v, *i, Some(*s), *f),
        _ => bail!("malformed op:// URI (expected 3 or 4 path segments): {input}"),
    };

    // Item segment may carry [subtitle] filter — jackin's display extension.
    // Nested condition makes map_or awkward; allow the if-let pattern here.
    #[allow(clippy::option_if_let_else)]
    let (item_name, subtitle_filter): (&str, Option<&str>) = if let Some(open) = item_seg.rfind('[')
    {
        if item_seg.ends_with(']') && open < item_seg.len() - 1 {
            (
                &item_seg[..open],
                Some(&item_seg[open + 1..item_seg.len() - 1]),
            )
        } else {
            (item_seg, None)
        }
    } else {
        (item_seg, None)
    };

    // Resolve vault by name (case-insensitive) or UUID.
    let vaults = op.vault_list(account)?;
    let vault = vaults
        .iter()
        .find(|v| v.name.eq_ignore_ascii_case(vault_seg) || v.id == vault_seg)
        .ok_or_else(|| anyhow!("vault not found: {vault_seg:?}"))?;

    // Resolve items in this vault, then filter by name (case-insensitive) or
    // UUID, and by subtitle filter when present.
    let items = op.item_list(&vault.id, account)?;
    let mut matches: Vec<&OpItem> = items
        .iter()
        .filter(|i| {
            let name_match = i.name.eq_ignore_ascii_case(item_name) || i.id == item_name;
            let subtitle_match = match subtitle_filter {
                None => true,
                // `#<prefix>` → match against item ID prefix (from disambig suggestion).
                Some(s) if s.starts_with('#') => i.id.starts_with(&s[1..]),
                Some(s) => i.subtitle.eq_ignore_ascii_case(s),
            };
            name_match && subtitle_match
        })
        .collect();

    if matches.is_empty() {
        let suffix = subtitle_filter
            .map(|s| format!("[{s}]"))
            .unwrap_or_default();
        bail!(
            "item {name:?} not found in vault {vault_name:?}",
            name = format!("{item_name}{suffix}"),
            vault_name = vault.name
        );
    }
    if matches.len() > 1 {
        let suggestions: Vec<String> = matches
            .iter()
            .map(|i| {
                let label = if i.subtitle.is_empty() {
                    let id_prefix: String = i.id.chars().take(8).collect();
                    format!("{}[#{}]", i.name, id_prefix)
                } else {
                    format!("{}[{}]", i.name, i.subtitle)
                };
                let section_part = section_seg.map(|s| format!("/{s}")).unwrap_or_default();
                let q = query.unwrap_or("");
                format!("  op://{}/{label}{section_part}/{field_seg}{q}", vault.name)
            })
            .collect();
        bail!(
            "{n} items named {name:?} in vault {vault_name:?}. Disambiguate with:\n{lines}",
            n = matches.len(),
            name = item_name,
            vault_name = vault.name,
            lines = suggestions.join("\n")
        );
    }
    let item = matches.pop().unwrap();

    // Resolve field by label (case-insensitive) or UUID.
    let fields = op.item_get(&item.id, &vault.id, account)?;
    let field = fields
        .iter()
        .find(|f| f.label.eq_ignore_ascii_case(field_seg) || f.id == field_seg)
        .ok_or_else(|| {
            anyhow!(
                "field {field_seg:?} not found in item {name:?}",
                name = item.name
            )
        })?;

    // Compute ambiguity for path snapshot (same rule as picker).
    let item_name_collides = items.iter().any(|i| i.id != item.id && i.name == item.name);
    let safe_to_embed = !item.name.contains('[') && !item.name.contains(']');
    let item_segment = if item_name_collides && safe_to_embed && !item.subtitle.is_empty() {
        format!("{}[{}]", item.name, item.subtitle)
    } else {
        item.name.clone()
    };

    // Use field.reference (1Password's canonical emission) as the authoritative
    // source for the section segment, mirroring build_op_ref_on_commit.
    let section_from_field = parse_op_reference(&field.reference).and_then(|p| p.section);

    let canonical_section = match (section_seg, section_from_field) {
        // field.reference has a section: use canonical (1Password) form
        // regardless of whether the user also typed a section. This covers:
        //   - (Some(_), Some(s)): both present → prefer field.reference's form.
        //   - (None, Some(s)): 3-segment input but field lives in a section;
        //     pick it up so the result matches the picker's output.
        (_, Some(s)) => Some(s),
        // User typed a section but the field's reference has none — should not
        // happen for sectioned fields; trust the user input as a fallback.
        (Some(user_s), None) => Some(user_s.to_string()),
        // No section anywhere: 3-segment URI.
        (None, None) => None,
    };

    // Mirror picker's empty-label fallback: use field.id when label is empty.
    let field_label = if field.label.is_empty() {
        field.id.as_str()
    } else {
        field.label.as_str()
    };

    let q_suffix = query.unwrap_or("");
    let (op_uri, display_path) = canonical_section.as_deref().map_or_else(
        || {
            (
                format!("op://{}/{}/{}{q_suffix}", vault.id, item.id, field.id),
                format!("{}/{}/{}{q_suffix}", vault.name, item_segment, field_label),
            )
        },
        |s| {
            (
                format!("op://{}/{}/{}/{}{q_suffix}", vault.id, item.id, s, field.id),
                format!(
                    "{}/{}/{}/{}{q_suffix}",
                    vault.name, item_segment, s, field_label
                ),
            )
        },
    );

    Ok(OpRef {
        op: op_uri,
        path: display_path,
        account: account.map(str::to_string),
    })
}

fn record_layer(
    attributed: &mut std::collections::BTreeMap<String, (EnvLayer, EnvValue)>,
    layer: &EnvLayer,
    env: &std::collections::BTreeMap<String, EnvValue>,
) {
    for (k, v) in env {
        attributed.insert(k.clone(), (layer.clone(), v.clone()));
    }
}

/// Build the (key → (layer, value)) attribution map by walking the
/// four config layers in precedence order — global, role, workspace,
/// workspace-role — for the given `(role, workspace)` selection.
/// Later layers overwrite earlier ones, so the final layer attached
/// to each key is the one that wins resolution.
fn build_attributed_layers(
    config: &crate::config::AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&str>,
) -> std::collections::BTreeMap<String, (EnvLayer, EnvValue)> {
    let mut attributed: std::collections::BTreeMap<String, (EnvLayer, EnvValue)> =
        std::collections::BTreeMap::new();

    record_layer(&mut attributed, &EnvLayer::Global, &config.env);
    if let Some(role_name) = role_selector
        && let Some(a) = config.roles.get(role_name)
    {
        record_layer(
            &mut attributed,
            &EnvLayer::Role(role_name.to_string()),
            &a.env,
        );
    }
    if let Some(ws_name) = workspace_name
        && let Some(ws) = config.workspaces.get(ws_name)
    {
        record_layer(
            &mut attributed,
            &EnvLayer::Workspace(ws_name.to_string()),
            &ws.env,
        );
        if let Some(role_name) = role_selector
            && let Some(ov) = ws.roles.get(role_name)
        {
            let ws_role_layer = EnvLayer::WorkspaceRole {
                workspace: ws_name.to_string(),
                role: role_name.to_string(),
            };
            record_layer(&mut attributed, &ws_role_layer, &ov.env);
        }
    }

    attributed
}

/// Look up the raw (unresolved) declaration value for `key` in the
/// operator env config layers, using the same precedence as
/// `resolve_operator_env` (global < role < workspace < workspace-role).
pub fn lookup_operator_env_raw(
    config: &crate::config::AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&str>,
    key: &str,
) -> Option<String> {
    build_attributed_layers(config, role_selector, workspace_name)
        .remove(key)
        .map(|(_, value)| value.as_display_str().to_string())
}

/// Env var Claude Code reads for the long-lived OAuth token.
///
/// Centralised so [`crate::workspace::token_setup`], the launch
/// diagnostic in [`crate::runtime::launch`], and
/// [`crate::agent::Agent::required_env_var`] stay in sync. See
/// <https://code.claude.com/docs/en/iam> for upstream precedence
/// semantics.
pub const CLAUDE_OAUTH_TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// Walk the env layers for the given `(role, workspace)` pair and
/// resolve every value. Resolution failures across layers are
/// aggregated into one error.
pub fn resolve_operator_env(
    config: &crate::config::AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&str>,
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    // Each `op://` ref pins its own account at read time
    // (`OpRef::account`), so the runner carries no instance-level account.
    let runner = OpCli::new();
    resolve_operator_env_with(config, role_selector, workspace_name, &runner, |name| {
        std::env::var(name)
    })
}

/// `?Sized` so callers can pass `&dyn OpRunner` (used by
/// `LoadOptions::op_runner` in `src/runtime/launch.rs`).
pub fn resolve_operator_env_with<R, H>(
    config: &crate::config::AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&str>,
    op_runner: &R,
    mut host_env: H,
) -> anyhow::Result<std::collections::BTreeMap<String, String>>
where
    R: OpRunner + ?Sized,
    H: FnMut(&str) -> Result<String, std::env::VarError>,
{
    let attributed = build_attributed_layers(config, role_selector, workspace_name);

    let mut resolved = std::collections::BTreeMap::new();
    let mut errors: Vec<String> = Vec::new();

    // Probe op CLI once up front when any value is an OpRef, so a
    // missing op surfaces as one install-link error not N.
    let uses_op = attributed
        .values()
        .any(|(_, v)| matches!(v, EnvValue::OpRef(_)));
    if uses_op && let Err(e) = op_runner.probe() {
        anyhow::bail!("operator env resolution aborted: {e}");
    }

    for (key, (layer, value)) in &attributed {
        let layer_label = format!("{layer}");
        match resolve_env_value(&layer_label, key, value, op_runner, &mut host_env) {
            Ok(v) => {
                resolved.insert(key.clone(), v);
            }
            Err(e) => errors.push(format!("  - {e}")),
        }
    }

    if errors.is_empty() {
        return Ok(resolved);
    }

    anyhow::bail!(
        "operator env resolution failed for {} var(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}

/// Print a launch diagnostic to stderr. Values are NEVER printed —
/// normal mode is counts only, debug mode is reference strings or the
/// `literal` placeholder; the layer that supplied each key is shown.
pub fn print_launch_diagnostic(
    config: &crate::config::AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&str>,
    resolved: &std::collections::BTreeMap<String, String>,
    debug: bool,
) {
    let mut out = Vec::new();
    write_launch_diagnostic(
        &mut out,
        config,
        role_selector,
        workspace_name,
        resolved,
        debug,
    )
    .expect("writing to Vec<u8> is infallible");
    emit_launch_diagnostic(
        std::str::from_utf8(&out).expect("diagnostic formatter emits UTF-8"),
        debug,
        &mut std::io::stderr(),
    );
}

fn emit_launch_diagnostic<W: std::io::Write>(rendered: &str, debug: bool, stderr: &mut W) {
    if let Some(run) = crate::diagnostics::active_run() {
        run.compact("operator_env", rendered.trim_end());
    }
    if debug || crate::tui::rich_terminal_owned() {
        return;
    }
    let _ = stderr.write_all(rendered.as_bytes());
}

#[cfg(test)]
fn format_launch_diagnostic_for_test(
    config: &crate::config::AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&str>,
    resolved: &std::collections::BTreeMap<String, String>,
    debug: bool,
) -> String {
    let mut out = Vec::new();
    write_launch_diagnostic(
        &mut out,
        config,
        role_selector,
        workspace_name,
        resolved,
        debug,
    )
    .unwrap();
    String::from_utf8(out).unwrap()
}

fn write_launch_diagnostic<W: std::io::Write>(
    w: &mut W,
    config: &crate::config::AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&str>,
    resolved: &std::collections::BTreeMap<String, String>,
    debug: bool,
) -> std::io::Result<()> {
    let mut attributed = build_attributed_layers(config, role_selector, workspace_name);
    // Drop keys not in `resolved` — those failed to dispatch.
    attributed.retain(|k, _| resolved.contains_key(k));

    if debug {
        writeln!(w, "[jackin] operator env:")?;
        let key_width = attributed
            .keys()
            .map(String::len)
            .max()
            .unwrap_or(0)
            .min(40);
        let raw_width = attributed
            .values()
            .map(|(_, v)| classify_env_value(v).len())
            .max()
            .unwrap_or(0)
            .min(40);
        for (key, (layer, value)) in &attributed {
            let kind = classify_env_value(value);
            writeln!(w, "  {key:key_width$}  {kind:raw_width$}  ({layer})")?;
        }
        return Ok(());
    }

    let (mut op_count, mut host_count, mut literal_count) = (0u32, 0u32, 0u32);
    for (_, value) in attributed.values() {
        match ValueKind::of_env_value(value) {
            ValueKind::Op => op_count += 1,
            ValueKind::Host => host_count += 1,
            ValueKind::Literal => literal_count += 1,
        }
    }
    writeln!(
        w,
        "[jackin] operator env: {} resolved ({} op://, {} host ref, {} literal)",
        attributed.len(),
        op_count,
        host_count,
        literal_count
    )?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum ValueKind {
    Op,
    Host,
    Literal,
}

impl ValueKind {
    fn of_env_value(value: &EnvValue) -> Self {
        match value {
            EnvValue::OpRef(_) => Self::Op,
            EnvValue::Plain(s) => {
                if parse_host_ref(s).is_some() {
                    Self::Host
                } else {
                    Self::Literal
                }
            }
        }
    }
}

/// Value-free label: `OpRef` emits the canonical `op://` URI; `$NAME`
/// host refs are returned verbatim; literals collapse to `"literal"` so
/// the value never reaches stderr.
fn classify_env_value(value: &EnvValue) -> String {
    match value {
        EnvValue::OpRef(r) => r.op.clone(),
        EnvValue::Plain(s) => {
            if parse_host_ref(s).is_some() {
                s.clone()
            } else {
                "literal".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests;
