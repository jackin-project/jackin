//! Operator-controlled env resolution: four config layers, three value
//! syntaxes (`op://`, `$NAME` / `${NAME}`, literal), and merging onto
//! the manifest-resolved env at launch.

pub trait OpRunner {
    fn read(&self, reference: &str) -> anyhow::Result<String>;

    /// Probed once per launch so a missing `op` surfaces as a single
    /// install-link error rather than one-per-key noise. Default no-op
    /// keeps mock runners trivial.
    fn probe(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Dispatch a single env value string to the appropriate resolver.
///
/// `op://...` → `op_runner.read`, `$NAME` / `${NAME}` → `host_env`,
/// otherwise verbatim. `layer_label` / `var_name` only feed error
/// messages.
pub fn dispatch_value<R>(
    layer_label: &str,
    var_name: &str,
    value: &str,
    op_runner: &R,
    mut host_env: impl FnMut(&str) -> Result<String, std::env::VarError>,
) -> anyhow::Result<String>
where
    R: OpRunner + ?Sized,
{
    if is_op_reference(value) {
        return op_runner.read(value).map_err(|e| {
            anyhow::anyhow!(
                "{layer_label} env var {var_name:?}: 1Password reference {value:?} failed: {e}"
            )
        });
    }

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

#[must_use]
pub fn is_op_reference(value: &str) -> bool {
    value.starts_with("op://")
}

/// Structured parts of an `op://...` reference.
///
/// Syntax: `op://<vault>/<item>/[<section>/]<field>`. Account scope is
/// not encoded in the path; multi-account picks live separately on
/// `OpPickerState::selected_account`. See
/// <https://developer.1password.com/docs/cli/secret-reference-syntax/>.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpReferenceParts {
    pub vault: String,
    pub item: String,
    pub section: Option<String>,
    pub field: String,
}

#[must_use]
pub fn parse_op_reference(value: &str) -> Option<OpReferenceParts> {
    let path = value.strip_prefix("op://")?;
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        [vault, item, field] => Some(OpReferenceParts {
            vault: (*vault).to_string(),
            item: (*item).to_string(),
            section: None,
            field: (*field).to_string(),
        }),
        [vault, item, section, field] => Some(OpReferenceParts {
            vault: (*vault).to_string(),
            item: (*item).to_string(),
            section: Some((*section).to_string()),
            field: (*field).to_string(),
        }),
        _ => None,
    }
}

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

/// Production `OpRunner` that shells out to the 1Password CLI.
///
/// Tests inject a different runner (e.g. `TestOpRunner`) rather than
/// using an env-var seam — keeps the crate `unsafe_code = "forbid"`
/// lint intact and tests free of process-env mutation.
pub struct OpCli {
    binary: String,
    timeout: std::time::Duration,
}

impl OpCli {
    pub fn new() -> Self {
        Self {
            binary: OP_DEFAULT_BIN.to_string(),
            timeout: OP_DEFAULT_TIMEOUT,
        }
    }

    pub const fn with_binary(binary: String) -> Self {
        Self {
            binary,
            timeout: OP_DEFAULT_TIMEOUT,
        }
    }

    #[cfg(test)]
    const fn with_binary_and_timeout(binary: String, timeout: std::time::Duration) -> Self {
        Self { binary, timeout }
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

impl OpRunner for OpCli {
    fn read(&self, reference: &str) -> anyhow::Result<String> {
        use std::io::Read;
        use std::process::{Command, Stdio};

        let mut child = Command::new(&self.binary)
            .args(["read", reference])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to spawn 1Password CLI {:?}: {e} \
                     (is `op` installed and on your PATH? see \
                     https://developer.1password.com/docs/cli/)",
                    self.binary
                )
            })?;

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

/// `id` is the `account_uuid` accepted by `op --account <id>`. `email`
/// and `url` feed the picker's Account pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpAccount {
    pub id: String,
    pub email: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpVault {
    pub id: String,
    pub name: String,
}

/// `name` comes from JSON `title`; `subtitle` from
/// `additional_information` (login username/email, empty on secure
/// notes) — used to disambiguate items sharing a title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpItem {
    pub id: String,
    pub name: String,
    pub subtitle: String,
}

/// Field metadata only — the value is intentionally absent.
///
/// `reference` is the verbatim `op://...` 1Password emits per field;
/// the picker commits this rather than synthesizing a path from
/// display names (synthesis was wrong for sections, names containing
/// `/`, or whitespace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpField {
    pub id: String,
    pub label: String,
    pub field_type: String,
    pub concealed: bool,
    pub reference: String,
}

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

    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to spawn 1Password CLI {binary:?}: {e} \
                 (is `op` installed and on your PATH? see \
                 https://developer.1password.com/docs/cli/)"
            )
        })?;

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
        if let Some(id) = account {
            args.push("--account");
            args.push(id);
        }
        args.extend_from_slice(&["--format", "json"]);
        let bytes = run_op_json(&self.binary, &args, self.timeout)?;
        let raw: Vec<RawOpVault> = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("failed to parse `op vault list` JSON: {e}"))?;
        Ok(raw.into_iter().map(OpVault::from).collect())
    }

    fn item_list(&self, vault_id: &str, account: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        let mut args: Vec<&str> = vec!["item", "list", "--vault", vault_id];
        if let Some(id) = account {
            args.push("--account");
            args.push(id);
        }
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
        if let Some(id) = account {
            args.push("--account");
            args.push(id);
        }
        args.extend_from_slice(&["--format", "json"]);
        let bytes = run_op_json(&self.binary, &args, self.timeout)?;
        let detail: RawOpItemDetail = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("failed to parse `op item get` JSON: {e}"))?;
        Ok(detail.fields.into_iter().map(OpField::from).collect())
    }
}

/// Source layer of an env value, attached to error messages and
/// launch diagnostics so the operator can locate the offending entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvLayer {
    Global,
    Agent(String),
    Workspace(String),
    WorkspaceAgent { workspace: String, agent: String },
}

impl std::fmt::Display for EnvLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Global => write!(f, "global [env]"),
            Self::Agent(name) => write!(f, "agent {name:?} [env]"),
            Self::Workspace(name) => write!(f, "workspace {name:?} [env]"),
            Self::WorkspaceAgent { workspace, agent } => {
                write!(f, "workspace {workspace:?} → agent {agent:?} [env]")
            }
        }
    }
}

/// Later-wins merge. Order, low → high priority:
/// global → agent → workspace → workspace-agent.
pub fn merge_layers(
    global: &std::collections::BTreeMap<String, String>,
    agent: &std::collections::BTreeMap<String, String>,
    workspace: &std::collections::BTreeMap<String, String>,
    workspace_agent: &std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    let mut merged = std::collections::BTreeMap::new();
    for layer in [global, agent, workspace, workspace_agent] {
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
    let mut record = |layer: EnvLayer, env: &std::collections::BTreeMap<String, String>| {
        for key in env.keys() {
            if crate::env_model::is_reserved(key) {
                offenses.push(format!(
                    "  - {key:?} is reserved by the jackin runtime; declared in {layer}"
                ));
            }
        }
    };

    record(EnvLayer::Global, &config.env);
    for (agent_name, agent_source) in &config.agents {
        record(EnvLayer::Agent(agent_name.clone()), &agent_source.env);
    }
    for (ws_name, ws) in &config.workspaces {
        record(EnvLayer::Workspace(ws_name.clone()), &ws.env);
        for (agent_name, override_) in &ws.agents {
            record(
                EnvLayer::WorkspaceAgent {
                    workspace: ws_name.clone(),
                    agent: agent_name.clone(),
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

/// (key → (layer, `raw_value`)) precedence-merged across the four
/// config layers — global, agent, workspace, workspace-agent — for the
/// given `(agent, workspace)` selection. Later layers overwrite earlier
/// ones, so the final layer attached to each key is the one that wins.
fn build_attributed_layers(
    config: &crate::config::AppConfig,
    agent_selector: Option<&str>,
    workspace_name: Option<&str>,
) -> std::collections::BTreeMap<String, (EnvLayer, String)> {
    let mut attributed: std::collections::BTreeMap<String, (EnvLayer, String)> =
        std::collections::BTreeMap::new();

    let mut record = |layer: EnvLayer, env: &std::collections::BTreeMap<String, String>| {
        for (k, v) in env {
            attributed.insert(k.clone(), (layer.clone(), v.clone()));
        }
    };

    record(EnvLayer::Global, &config.env);
    if let Some(agent_name) = agent_selector
        && let Some(a) = config.agents.get(agent_name)
    {
        record(EnvLayer::Agent(agent_name.to_string()), &a.env);
    }
    if let Some(ws_name) = workspace_name
        && let Some(ws) = config.workspaces.get(ws_name)
    {
        record(EnvLayer::Workspace(ws_name.to_string()), &ws.env);
        if let Some(agent_name) = agent_selector
            && let Some(ov) = ws.agents.get(agent_name)
        {
            record(
                EnvLayer::WorkspaceAgent {
                    workspace: ws_name.to_string(),
                    agent: agent_name.to_string(),
                },
                &ov.env,
            );
        }
    }

    attributed
}

/// Walk the env layers for the given `(agent, workspace)` pair and
/// resolve every value. Resolution failures across layers are
/// aggregated into one error.
pub fn resolve_operator_env(
    config: &crate::config::AppConfig,
    agent_selector: Option<&str>,
    workspace_name: Option<&str>,
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    resolve_operator_env_with(
        config,
        agent_selector,
        workspace_name,
        &OpCli::new(),
        |name| std::env::var(name),
    )
}

/// `?Sized` so callers can pass `&dyn OpRunner` (used by
/// `LoadOptions::op_runner` in `src/runtime/launch.rs`).
pub fn resolve_operator_env_with<R, H>(
    config: &crate::config::AppConfig,
    agent_selector: Option<&str>,
    workspace_name: Option<&str>,
    op_runner: &R,
    mut host_env: H,
) -> anyhow::Result<std::collections::BTreeMap<String, String>>
where
    R: OpRunner + ?Sized,
    H: FnMut(&str) -> Result<String, std::env::VarError>,
{
    let attributed = build_attributed_layers(config, agent_selector, workspace_name);

    let mut resolved = std::collections::BTreeMap::new();
    let mut errors: Vec<String> = Vec::new();

    // Probe op CLI once up front when any value uses op://, so a
    // missing op surfaces as one install-link error not N.
    let uses_op = attributed.values().any(|(_, v)| is_op_reference(v));
    if uses_op && let Err(e) = op_runner.probe() {
        anyhow::bail!("operator env resolution aborted: {e}");
    }

    for (key, (layer, raw_value)) in &attributed {
        let layer_label = format!("{layer}");
        match dispatch_value(&layer_label, key, raw_value, op_runner, &mut host_env) {
            Ok(value) => {
                resolved.insert(key.clone(), value);
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
    agent_selector: Option<&str>,
    workspace_name: Option<&str>,
    resolved: &std::collections::BTreeMap<String, String>,
    debug: bool,
) {
    use std::io::Write;
    let mut out = Vec::new();
    write_launch_diagnostic(
        &mut out,
        config,
        agent_selector,
        workspace_name,
        resolved,
        debug,
    )
    .expect("writing to Vec<u8> is infallible");
    let _ = std::io::stderr().write_all(&out);
}

#[cfg(test)]
fn format_launch_diagnostic_for_test(
    config: &crate::config::AppConfig,
    agent_selector: Option<&str>,
    workspace_name: Option<&str>,
    resolved: &std::collections::BTreeMap<String, String>,
    debug: bool,
) -> String {
    let mut out = Vec::new();
    write_launch_diagnostic(
        &mut out,
        config,
        agent_selector,
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
    agent_selector: Option<&str>,
    workspace_name: Option<&str>,
    resolved: &std::collections::BTreeMap<String, String>,
    debug: bool,
) -> std::io::Result<()> {
    let mut attributed = build_attributed_layers(config, agent_selector, workspace_name);
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
            .map(|(_, v)| classify_value(v).len())
            .max()
            .unwrap_or(0)
            .min(40);
        for (key, (layer, raw_value)) in &attributed {
            let kind = classify_value(raw_value);
            writeln!(w, "  {key:key_width$}  {kind:raw_width$}  ({layer})")?;
        }
        return Ok(());
    }

    let (mut op_count, mut host_count, mut literal_count) = (0u32, 0u32, 0u32);
    for (_, raw) in attributed.values() {
        match ValueKind::of(raw) {
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
    fn of(raw: &str) -> Self {
        if is_op_reference(raw) {
            Self::Op
        } else if parse_host_ref(raw).is_some() {
            Self::Host
        } else {
            Self::Literal
        }
    }
}

/// Value-free label: `op://...` and `$NAME` returned verbatim (the
/// reference is not secret, the resolved value is); literals collapse
/// to `"literal"` so the value never reaches stderr.
fn classify_value(raw: &str) -> String {
    match ValueKind::of(raw) {
        ValueKind::Op | ValueKind::Host => raw.to_string(),
        ValueKind::Literal => "literal".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_op_reference_recognizes_prefix() {
        assert!(is_op_reference("op://Personal/api/token"));
        assert!(is_op_reference("op://acct/Personal/api/token"));
        assert!(!is_op_reference("plain-literal"));
        assert!(!is_op_reference("$HOST"));
        assert!(!is_op_reference("${HOST}"));
        assert!(!is_op_reference(""));
        assert!(!is_op_reference("op:/missing"));
    }

    #[test]
    fn parse_op_reference_three_segments() {
        let parts = parse_op_reference("op://Vault/Item/field").unwrap();
        assert_eq!(parts.vault, "Vault");
        assert_eq!(parts.item, "Item");
        assert_eq!(parts.section, None);
        assert_eq!(parts.field, "field");
    }

    #[test]
    fn parse_op_reference_handles_section_in_4_segment() {
        let parts = parse_op_reference("op://Personal/Item/Auth/password").unwrap();
        assert_eq!(parts.vault, "Personal");
        assert_eq!(parts.item, "Item");
        assert_eq!(parts.section, Some("Auth".to_string()));
        assert_eq!(parts.field, "password");
    }

    #[test]
    fn parse_op_reference_invalid_segment_count() {
        assert!(parse_op_reference("plain").is_none());
        assert!(parse_op_reference("op://only/two").is_none());
        assert!(parse_op_reference("op://a/b/c/d/e").is_none());
        assert!(parse_op_reference("op://").is_none());
    }

    #[test]
    fn dispatch_literal_value_returns_literal() {
        let out = dispatch_value(
            "global",
            "FOO",
            "plain-literal",
            &TestOpRunner::forbidden(),
            |n| panic!("host env should not be queried for literal; got {n}"),
        )
        .unwrap();
        assert_eq!(out, "plain-literal");
    }

    #[test]
    fn dispatch_host_ref_dollar_name_reads_host_env() {
        let out = dispatch_value(
            "global",
            "MY_VAR",
            "$OPERATOR_HOST_SOURCE",
            &TestOpRunner::forbidden(),
            |name| {
                assert_eq!(name, "OPERATOR_HOST_SOURCE");
                Ok("from-host".to_string())
            },
        )
        .unwrap();
        assert_eq!(out, "from-host");
    }

    #[test]
    fn dispatch_host_ref_braced_reads_host_env() {
        let out = dispatch_value(
            "global",
            "MY_VAR",
            "${OPERATOR_HOST_SOURCE}",
            &TestOpRunner::forbidden(),
            |name| {
                assert_eq!(name, "OPERATOR_HOST_SOURCE");
                Ok("braced".to_string())
            },
        )
        .unwrap();
        assert_eq!(out, "braced");
    }

    /// Set-but-empty (Unix semantics) passes through unchanged; only
    /// `VarError::NotPresent` is a hard error.
    #[test]
    fn dispatch_host_ref_empty_string_passes_through() {
        let out = dispatch_value(
            "global",
            "MAYBE_EMPTY",
            "$OPERATOR_HOST_EMPTY",
            &TestOpRunner::forbidden(),
            |name| {
                assert_eq!(name, "OPERATOR_HOST_EMPTY");
                Ok(String::new())
            },
        )
        .unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn dispatch_host_ref_missing_returns_clear_error() {
        let err = dispatch_value(
            "workspace \"big-monorepo\"",
            "MY_VAR",
            "$MISSING_HOST_VAR",
            &TestOpRunner::forbidden(),
            |_| Err(std::env::VarError::NotPresent),
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("MY_VAR"), "expected var name in error: {msg}");
        assert!(
            msg.contains("MISSING_HOST_VAR"),
            "expected host var name in error: {msg}"
        );
        assert!(
            msg.contains("workspace \"big-monorepo\""),
            "expected layer name in error: {msg}"
        );
    }

    #[test]
    fn dispatch_op_ref_invokes_op_cli() {
        let runner = TestOpRunner::new(Ok("tok-abc".to_string()));
        let out = dispatch_value(
            "agent \"agent-smith\"",
            "API_TOKEN",
            "op://Personal/api/token",
            &runner,
            |_| panic!("host env should not be queried for op:// refs"),
        )
        .unwrap();
        assert_eq!(out, "tok-abc");
        assert_eq!(
            runner.last_ref().as_deref(),
            Some("op://Personal/api/token")
        );
    }

    struct TestOpRunner {
        response: std::cell::RefCell<Option<anyhow::Result<String>>>,
        last_ref: std::cell::RefCell<Option<String>>,
    }

    impl TestOpRunner {
        fn new(response: anyhow::Result<String>) -> Self {
            Self {
                response: std::cell::RefCell::new(Some(response)),
                last_ref: std::cell::RefCell::new(None),
            }
        }

        fn forbidden() -> Self {
            Self {
                response: std::cell::RefCell::new(None),
                last_ref: std::cell::RefCell::new(None),
            }
        }

        fn last_ref(&self) -> std::cell::Ref<'_, Option<String>> {
            self.last_ref.borrow()
        }
    }

    impl OpRunner for TestOpRunner {
        fn read(&self, reference: &str) -> anyhow::Result<String> {
            *self.last_ref.borrow_mut() = Some(reference.to_string());
            match self.response.borrow_mut().take() {
                Some(r) => r,
                None => panic!("op CLI should not have been invoked"),
            }
        }
    }

    #[test]
    fn op_cli_invokes_binary_and_returns_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op");
        std::fs::write(
            &bin_path,
            "#!/bin/sh\nif [ \"$1\" = \"read\" ] && [ \"$2\" = \"op://Personal/api/token\" ]; then printf '%s' 'tok-123'; exit 0; fi\nexit 99\n",
        )
        .unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let out = runner.read("op://Personal/api/token").unwrap();
        assert_eq!(out, "tok-123");
    }

    #[test]
    fn op_cli_strips_trailing_newline_from_op_read_output() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-newline");
        std::fs::write(&bin_path, "#!/bin/sh\nprintf 'tok-123\\n'\nexit 0\n").unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let out = runner.read("op://Personal/api/token").unwrap();
        assert_eq!(
            out, "tok-123",
            "trailing \\n from op read must be stripped; got {out:?}"
        );
    }

    /// A secret legitimately ending in `\n` (e.g. a PEM block) is sent
    /// by `op read` as value+\n; strip exactly one so inner newlines
    /// survive.
    #[test]
    fn op_cli_strips_only_one_trailing_newline_preserves_value_newline() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-double-newline");
        std::fs::write(&bin_path, "#!/bin/sh\nprintf 'line1\\nline2\\n'\nexit 0\n").unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let out = runner.read("op://Personal/api/multi").unwrap();
        assert_eq!(
            out, "line1\nline2",
            "internal newline must survive while final trailing \\n is stripped"
        );
    }

    #[test]
    fn op_cli_missing_binary_returns_clear_error() {
        let runner = OpCli::with_binary("/nonexistent/op/binary/path".to_string());
        let err = runner.read("op://Personal/api/token").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("op"), "expected binary name in error: {msg}");
        assert!(
            msg.contains("not found")
                || msg.contains("No such file")
                || msg.contains("failed to spawn"),
            "expected a missing-binary hint in error: {msg}"
        );
    }

    #[test]
    fn op_cli_nonzero_exit_propagates_stderr_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-fail");
        std::fs::write(
            &bin_path,
            "#!/bin/sh\n>&2 echo 'item not found: op://Foo/bar'\nexit 1\n",
        )
        .unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let err = runner.read("op://Foo/bar").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("exit"), "expected exit code in error: {msg}");
        assert!(
            msg.contains("item not found"),
            "expected bounded stderr in error: {msg}"
        );
    }

    #[test]
    fn op_cli_large_stderr_is_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-big-stderr");
        std::fs::write(
            &bin_path,
            "#!/bin/sh\npython3 -c \"import sys; sys.stderr.write('X' * 16384)\" 2>&1 1>&2\nexit 1\n",
        )
        .unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let err = runner.read("op://Foo/bar").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.len() < 6 * 1024,
            "expected bounded stderr in error; got {} bytes",
            msg.len()
        );
    }

    #[test]
    fn op_cli_hanging_binary_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-hang");
        std::fs::write(&bin_path, "#!/bin/sh\nsleep 60\n").unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary_and_timeout(
            bin_path.to_string_lossy().to_string(),
            std::time::Duration::from_millis(250),
        );
        let start = std::time::Instant::now();
        let err = runner.read("op://Foo/bar").unwrap_err();
        let elapsed = start.elapsed();

        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "runner must abort before 5s; actual={elapsed:?}"
        );
        assert!(
            err.to_string().contains("timeout") || err.to_string().contains("timed out"),
            "expected timeout in error: {}",
            err
        );
    }

    #[test]
    fn op_cli_probe_succeeds_when_binary_exists_and_exits_zero() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-version");
        std::fs::write(&bin_path, "#!/bin/sh\necho '2.30.0'\nexit 0\n").unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        runner.probe().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn op_cli_probe_times_out_when_binary_hangs() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-version-hang");
        std::fs::write(&bin_path, "#!/bin/sh\nsleep 60\n").unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary_and_timeout(
            bin_path.to_string_lossy().to_string(),
            std::time::Duration::from_millis(250),
        );
        let start = std::time::Instant::now();
        let err = runner.probe().unwrap_err();
        let elapsed = start.elapsed();

        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "probe must abort before 5s; actual={elapsed:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("timeout") || msg.contains("timed out"),
            "expected timeout in error: {msg}"
        );
    }

    #[test]
    fn op_cli_probe_fails_with_install_link_when_binary_missing() {
        let runner = OpCli::with_binary("/nonexistent/op/binary/path".to_string());
        let err = runner.probe().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("1Password") || msg.contains("op"),
            "expected reference to op in error: {msg}"
        );
        assert!(
            msg.contains("developer.1password.com"),
            "expected install link in error: {msg}"
        );
    }

    #[cfg(unix)]
    fn make_executable(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &std::path::Path) {}

    #[test]
    fn truncate_stderr_returns_input_for_short_string() {
        let s = "short error message";
        assert_eq!(truncate_stderr(s), s);
    }

    #[test]
    fn truncate_stderr_truncates_long_ascii_at_boundary() {
        let s: String = "x".repeat(OP_STDERR_MAX + 100);
        let out = truncate_stderr(&s);
        assert!(
            out.starts_with(&s[..OP_STDERR_MAX]),
            "ASCII truncation must keep exactly OP_STDERR_MAX bytes"
        );
        assert!(out.ends_with("[truncated]"));
    }

    /// Multi-byte UTF-8 char straddling `OP_STDERR_MAX` — naive byte
    /// slicing would panic; the boundary walk-back must round down.
    #[test]
    fn truncate_stderr_does_not_panic_on_utf8_boundary() {
        // ASCII padding + 4-byte emoji (`U+1F4A9`) straddling the cap.
        let pad_len = OP_STDERR_MAX - 2;
        let mut s = String::with_capacity(pad_len + 16);
        s.push_str(&"a".repeat(pad_len));
        for _ in 0..10 {
            s.push('\u{1F4A9}');
        }
        assert!(
            !s.is_char_boundary(OP_STDERR_MAX),
            "test fixture must place a non-boundary byte at OP_STDERR_MAX; \
             got is_char_boundary == true"
        );
        let out = truncate_stderr(&s);
        assert!(out.ends_with("[truncated]"));
        let head = out
            .strip_suffix("… [truncated]")
            .expect("truncate marker present");
        assert!(
            head.is_char_boundary(head.len()),
            "truncated head must end on a UTF-8 char boundary"
        );
        assert!(head.len() <= OP_STDERR_MAX);
    }

    use std::collections::BTreeMap;

    fn m(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn merge_empty_layers_returns_empty() {
        let merged = merge_layers(&m(&[]), &m(&[]), &m(&[]), &m(&[]));
        assert!(merged.is_empty());
    }

    #[test]
    fn merge_global_only() {
        let merged = merge_layers(&m(&[("A", "1"), ("B", "2")]), &m(&[]), &m(&[]), &m(&[]));
        assert_eq!(merged.get("A").map(|v| v.as_str()), Some("1"));
        assert_eq!(merged.get("B").map(|v| v.as_str()), Some("2"));
    }

    #[test]
    fn merge_agent_overrides_global() {
        let merged = merge_layers(
            &m(&[("A", "global"), ("B", "global")]),
            &m(&[("B", "agent")]),
            &m(&[]),
            &m(&[]),
        );
        assert_eq!(merged.get("A").map(|v| v.as_str()), Some("global"));
        assert_eq!(merged.get("B").map(|v| v.as_str()), Some("agent"));
    }

    #[test]
    fn merge_workspace_overrides_agent() {
        let merged = merge_layers(
            &m(&[("A", "global")]),
            &m(&[("A", "agent")]),
            &m(&[("A", "workspace")]),
            &m(&[]),
        );
        assert_eq!(merged.get("A").map(|v| v.as_str()), Some("workspace"));
    }

    #[test]
    fn merge_workspace_agent_overrides_workspace() {
        let merged = merge_layers(
            &m(&[("A", "global")]),
            &m(&[("A", "agent")]),
            &m(&[("A", "workspace")]),
            &m(&[("A", "ws-agent")]),
        );
        assert_eq!(merged.get("A").map(|v| v.as_str()), Some("ws-agent"));
    }

    #[test]
    fn merge_preserves_non_overlapping_keys_across_layers() {
        let merged = merge_layers(
            &m(&[("G", "g")]),
            &m(&[("A", "a")]),
            &m(&[("W", "w")]),
            &m(&[("X", "x")]),
        );
        assert_eq!(merged.get("G").map(|v| v.as_str()), Some("g"));
        assert_eq!(merged.get("A").map(|v| v.as_str()), Some("a"));
        assert_eq!(merged.get("W").map(|v| v.as_str()), Some("w"));
        assert_eq!(merged.get("X").map(|v| v.as_str()), Some("x"));
    }

    #[test]
    fn validate_reserved_names_rejects_global_reserved() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env
            .insert("DOCKER_HOST".to_string(), "whatever".to_string());

        let err = validate_reserved_names(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("DOCKER_HOST"), "{msg}");
        assert!(msg.contains("global [env]"), "{msg}");
        assert!(msg.contains("reserved"), "{msg}");
    }

    #[test]
    fn validate_reserved_names_rejects_per_agent_reserved() {
        let mut cfg = crate::config::AppConfig::default();
        let mut agent = crate::config::AgentSource {
            git: "https://example.com/x.git".to_string(),
            trusted: true,
            claude: None,
            env: std::collections::BTreeMap::new(),
        };
        agent
            .env
            .insert("JACKIN_CLAUDE_ENV".to_string(), "whatever".to_string());
        cfg.agents.insert("agent-smith".to_string(), agent);

        let err = validate_reserved_names(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("JACKIN_CLAUDE_ENV"), "{msg}");
        assert!(msg.contains("agent \"agent-smith\""), "{msg}");
    }

    #[test]
    fn validate_reserved_names_rejects_per_workspace_reserved() {
        let mut cfg = crate::config::AppConfig::default();
        let mut ws = crate::workspace::WorkspaceConfig {
            workdir: "/x".to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: "/x".to_string(),
                dst: "/x".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        ws.env
            .insert("DOCKER_TLS_VERIFY".to_string(), "0".to_string());
        cfg.workspaces.insert("big-monorepo".to_string(), ws);

        let err = validate_reserved_names(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("DOCKER_TLS_VERIFY"), "{msg}");
        assert!(msg.contains("workspace \"big-monorepo\""), "{msg}");
    }

    #[test]
    fn validate_reserved_names_rejects_workspace_agent_override_reserved() {
        let mut cfg = crate::config::AppConfig::default();
        let mut override_ = crate::workspace::WorkspaceAgentOverride::default();
        override_
            .env
            .insert("DOCKER_CERT_PATH".to_string(), "/tmp".to_string());
        let mut ws = crate::workspace::WorkspaceConfig {
            workdir: "/x".to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: "/x".to_string(),
                dst: "/x".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        ws.agents.insert("agent-smith".to_string(), override_);
        cfg.workspaces.insert("big-monorepo".to_string(), ws);

        let err = validate_reserved_names(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("DOCKER_CERT_PATH"), "{msg}");
        assert!(
            msg.contains("workspace \"big-monorepo\"") && msg.contains("agent \"agent-smith\""),
            "{msg}"
        );
    }

    #[test]
    fn validate_reserved_names_reports_all_conflicts_in_one_error() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env.insert("DOCKER_HOST".to_string(), "x".to_string());
        cfg.env
            .insert("DOCKER_TLS_VERIFY".to_string(), "y".to_string());

        let err = validate_reserved_names(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("DOCKER_HOST"), "{msg}");
        assert!(msg.contains("DOCKER_TLS_VERIFY"), "{msg}");
    }

    #[test]
    fn validate_reserved_names_accepts_non_reserved() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env.insert("MY_VAR".to_string(), "value".to_string());
        cfg.env
            .insert("OPERATOR_TOKEN".to_string(), "op://...".to_string());

        validate_reserved_names(&cfg).unwrap();
    }

    #[test]
    fn resolve_empty_config_returns_empty_map() {
        let cfg = crate::config::AppConfig::default();
        let resolved =
            resolve_operator_env_with(&cfg, None, None, &TestOpRunner::forbidden(), |_| {
                Err(std::env::VarError::NotPresent)
            })
            .unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn resolve_global_literal_value() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env.insert("FOO".to_string(), "bar".to_string());
        let resolved =
            resolve_operator_env_with(&cfg, None, None, &TestOpRunner::forbidden(), |_| {
                Err(std::env::VarError::NotPresent)
            })
            .unwrap();
        assert_eq!(resolved.get("FOO").map(|v| v.as_str()), Some("bar"));
    }

    #[test]
    fn resolve_layers_apply_in_order_with_workspace_agent_winning() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env.insert("X".to_string(), "global".to_string());

        let mut agent_source = crate::config::AgentSource {
            git: "https://example.com/x.git".to_string(),
            trusted: true,
            claude: None,
            env: std::collections::BTreeMap::new(),
        };
        agent_source
            .env
            .insert("X".to_string(), "agent".to_string());
        cfg.agents.insert("agent-smith".to_string(), agent_source);

        let mut ws = crate::workspace::WorkspaceConfig {
            workdir: "/x".to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: "/x".to_string(),
                dst: "/x".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        ws.env.insert("X".to_string(), "workspace".to_string());
        let mut wsa = crate::workspace::WorkspaceAgentOverride::default();
        wsa.env.insert("X".to_string(), "ws-agent".to_string());
        ws.agents.insert("agent-smith".to_string(), wsa);
        cfg.workspaces.insert("big-monorepo".to_string(), ws);

        let resolved = resolve_operator_env_with(
            &cfg,
            Some("agent-smith"),
            Some("big-monorepo"),
            &TestOpRunner::forbidden(),
            |_| Err(std::env::VarError::NotPresent),
        )
        .unwrap();

        assert_eq!(resolved.get("X").map(|v| v.as_str()), Some("ws-agent"));
    }

    #[test]
    fn resolve_reports_all_failures_in_one_error() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env.insert("A".to_string(), "$MISSING_A".to_string());
        cfg.env.insert("B".to_string(), "$MISSING_B".to_string());

        let err = resolve_operator_env_with(&cfg, None, None, &TestOpRunner::forbidden(), |_| {
            Err(std::env::VarError::NotPresent)
        })
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("\"A\""), "{msg}");
        assert!(msg.contains("\"B\""), "{msg}");
        assert!(msg.contains("MISSING_A"), "{msg}");
        assert!(msg.contains("MISSING_B"), "{msg}");
    }

    #[test]
    fn resolve_probes_op_cli_once_when_any_op_ref_present() {
        struct ProbeCountingRunner {
            probe_calls: std::cell::Cell<u32>,
            read_calls: std::cell::Cell<u32>,
        }
        impl OpRunner for ProbeCountingRunner {
            fn read(&self, _reference: &str) -> anyhow::Result<String> {
                self.read_calls.set(self.read_calls.get() + 1);
                Ok("stub".into())
            }
            fn probe(&self) -> anyhow::Result<()> {
                self.probe_calls.set(self.probe_calls.get() + 1);
                Ok(())
            }
        }

        let mut cfg = crate::config::AppConfig::default();
        cfg.env
            .insert("A".to_string(), "op://Personal/a".to_string());
        cfg.env
            .insert("B".to_string(), "op://Personal/b".to_string());
        let runner = ProbeCountingRunner {
            probe_calls: std::cell::Cell::new(0),
            read_calls: std::cell::Cell::new(0),
        };
        resolve_operator_env_with(&cfg, None, None, &runner, |_| {
            Err(std::env::VarError::NotPresent)
        })
        .unwrap();
        assert_eq!(runner.probe_calls.get(), 1, "probe must fire exactly once");
        assert_eq!(runner.read_calls.get(), 2, "each op:// key is resolved");
    }

    #[test]
    fn resolve_skips_probe_when_no_op_refs_present() {
        struct ProbeCountingRunner {
            probe_calls: std::cell::Cell<u32>,
        }
        impl OpRunner for ProbeCountingRunner {
            fn read(&self, _reference: &str) -> anyhow::Result<String> {
                panic!("read must not be called when no op:// refs exist")
            }
            fn probe(&self) -> anyhow::Result<()> {
                self.probe_calls.set(self.probe_calls.get() + 1);
                Ok(())
            }
        }

        let mut cfg = crate::config::AppConfig::default();
        cfg.env.insert("A".to_string(), "literal".to_string());
        let runner = ProbeCountingRunner {
            probe_calls: std::cell::Cell::new(0),
        };
        resolve_operator_env_with(&cfg, None, None, &runner, |_| {
            Err(std::env::VarError::NotPresent)
        })
        .unwrap();
        assert_eq!(
            runner.probe_calls.get(),
            0,
            "probe must not fire when no op:// refs exist"
        );
    }

    #[test]
    fn resolve_probe_failure_surfaces_install_link_once() {
        struct FailingProbeRunner;
        impl OpRunner for FailingProbeRunner {
            fn read(&self, _reference: &str) -> anyhow::Result<String> {
                panic!("read must not be called when probe fails")
            }
            fn probe(&self) -> anyhow::Result<()> {
                anyhow::bail!(
                    "1Password CLI (\"op\") was not found on PATH — install from \
                     https://developer.1password.com/docs/cli/"
                )
            }
        }

        let mut cfg = crate::config::AppConfig::default();
        cfg.env
            .insert("A".to_string(), "op://Personal/a".to_string());
        cfg.env
            .insert("B".to_string(), "op://Personal/b".to_string());
        let err = resolve_operator_env_with(&cfg, None, None, &FailingProbeRunner, |_| {
            Err(std::env::VarError::NotPresent)
        })
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("developer.1password.com"),
            "expected install link once: {msg}"
        );
    }

    #[test]
    fn resolve_op_failure_includes_layer_and_key() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env.insert(
            "TOKEN".to_string(),
            "op://Personal/broken/token".to_string(),
        );

        let runner = TestOpRunner::new(Err(anyhow::anyhow!("item not found")));

        let err = resolve_operator_env_with(&cfg, None, None, &runner, |_| {
            Err(std::env::VarError::NotPresent)
        })
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("TOKEN"), "{msg}");
        assert!(msg.contains("op://Personal/broken/token"), "{msg}");
        assert!(msg.contains("global [env]"), "{msg}");
    }

    #[test]
    fn resolve_host_ref_success_returns_value() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env
            .insert("API_KEY".to_string(), "${MY_HOST_API_KEY}".to_string());

        let resolved =
            resolve_operator_env_with(&cfg, None, None, &TestOpRunner::forbidden(), |name| {
                if name == "MY_HOST_API_KEY" {
                    Ok("host-secret".to_string())
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            })
            .unwrap();

        assert_eq!(
            resolved.get("API_KEY").map(|v| v.as_str()),
            Some("host-secret")
        );
    }

    #[test]
    fn launch_diagnostic_normal_mode_prints_counts_only_no_values() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env
            .insert("LITERAL_KEY".to_string(), "super-secret".to_string());
        cfg.env
            .insert("HOST_KEY".to_string(), "$HOST_VAR".to_string());
        cfg.env
            .insert("OP_KEY".to_string(), "op://Personal/item/field".to_string());
        let resolved: std::collections::BTreeMap<String, String> = [
            ("LITERAL_KEY".to_string(), "super-secret".to_string()),
            ("HOST_KEY".to_string(), "host-value-secret".to_string()),
            ("OP_KEY".to_string(), "op-value-secret".to_string()),
        ]
        .into_iter()
        .collect();

        let rendered = format_launch_diagnostic_for_test(&cfg, None, None, &resolved, false);

        assert!(rendered.contains("3 resolved"), "{rendered}");
        assert!(rendered.contains("1 op://"), "{rendered}");
        assert!(rendered.contains("1 host ref"), "{rendered}");
        assert!(rendered.contains("1 literal"), "{rendered}");

        // Values must never appear under any mode.
        assert!(!rendered.contains("super-secret"), "{rendered}");
        assert!(!rendered.contains("host-value-secret"), "{rendered}");
        assert!(!rendered.contains("op-value-secret"), "{rendered}");

        // In normal mode, references are NOT emitted (only counts).
        assert!(!rendered.contains("$HOST_VAR"), "{rendered}");
        assert!(!rendered.contains("op://Personal/item/field"), "{rendered}");
    }

    #[test]
    fn launch_diagnostic_debug_mode_prints_references_but_not_values() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env
            .insert("LITERAL_KEY".to_string(), "super-secret".to_string());
        cfg.env
            .insert("OP_KEY".to_string(), "op://Personal/item/field".to_string());
        let resolved: std::collections::BTreeMap<String, String> = [
            ("LITERAL_KEY".to_string(), "super-secret".to_string()),
            ("OP_KEY".to_string(), "op-value-secret".to_string()),
        ]
        .into_iter()
        .collect();

        let rendered = format_launch_diagnostic_for_test(&cfg, None, None, &resolved, true);

        // Debug mode emits references (reference string is config,
        // not secret) and the "literal" label — never the resolved
        // value.
        assert!(rendered.contains("op://Personal/item/field"), "{rendered}");
        assert!(rendered.contains("literal"), "{rendered}");
        assert!(!rendered.contains("super-secret"), "{rendered}");
        assert!(!rendered.contains("op-value-secret"), "{rendered}");
    }

    // ---- OpStructRunner tests --------------------------------------------

    #[test]
    fn op_field_concealed_derives_from_type_or_purpose() {
        // Type CONCEALED -> concealed=true.
        let raw_concealed = RawOpField {
            id: "f1".to_string(),
            label: "password".to_string(),
            field_type: "CONCEALED".to_string(),
            purpose: String::new(),
            reference: String::new(),
        };
        assert!(OpField::from(raw_concealed).concealed);

        // Purpose PASSWORD -> concealed=true, even when type is empty.
        let raw_purpose = RawOpField {
            id: "f2".to_string(),
            label: "pw".to_string(),
            field_type: String::new(),
            purpose: "PASSWORD".to_string(),
            reference: String::new(),
        };
        assert!(OpField::from(raw_purpose).concealed);

        // Plain text field -> concealed=false.
        let raw_text = RawOpField {
            id: "f3".to_string(),
            label: "username".to_string(),
            field_type: "STRING".to_string(),
            purpose: "USERNAME".to_string(),
            reference: String::new(),
        };
        assert!(!OpField::from(raw_text).concealed);
    }

    #[cfg(unix)]
    #[test]
    fn op_struct_runner_vault_list_parses_json() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-vault-list");
        std::fs::write(
            &bin_path,
            "#!/bin/sh\nif [ \"$1\" = \"vault\" ] && [ \"$2\" = \"list\" ]; then \
             printf '%s' '[{\"id\":\"v1\",\"name\":\"Personal\"}]'; exit 0; fi\nexit 99\n",
        )
        .unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let vaults = runner.vault_list(None).unwrap();
        assert_eq!(
            vaults,
            vec![OpVault {
                id: "v1".to_string(),
                name: "Personal".to_string(),
            }]
        );
    }

    #[cfg(unix)]
    #[test]
    fn op_struct_runner_item_list_parses_json() {
        // Two items are returned: the first carries an
        // `additional_information` subtitle (the username/email 1Password
        // surfaces in its UI), the second omits it. Both must round-trip
        // — the first into a populated `subtitle`, the second into an
        // empty string via `#[serde(default)]`.
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-item-list");
        std::fs::write(
            &bin_path,
            "#!/bin/sh\nif [ \"$1\" = \"item\" ] && [ \"$2\" = \"list\" ]; then \
             printf '%s' '[{\"id\":\"i1\",\"title\":\"Google\",\"additional_information\":\"alexey@zhokhov.com\"},\
{\"id\":\"i2\",\"title\":\"API Keys\"}]'; exit 0; fi\nexit 99\n",
        )
        .unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let items = runner.item_list("v1", None).unwrap();
        assert_eq!(
            items,
            vec![
                OpItem {
                    id: "i1".to_string(),
                    name: "Google".to_string(),
                    subtitle: "alexey@zhokhov.com".to_string(),
                },
                OpItem {
                    id: "i2".to_string(),
                    name: "API Keys".to_string(),
                    subtitle: String::new(),
                },
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn op_struct_runner_item_list_handles_missing_additional_information() {
        // Items without `additional_information` (e.g., secure notes)
        // must deserialize cleanly with an empty `subtitle`. Regression
        // coverage for the `#[serde(default)]` on `RawOpItem`.
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-item-list-no-subtitle");
        std::fs::write(
            &bin_path,
            "#!/bin/sh\nif [ \"$1\" = \"item\" ] && [ \"$2\" = \"list\" ]; then \
             printf '%s' '[{\"id\":\"i1\",\"title\":\"Recovery codes\"}]'; exit 0; fi\nexit 99\n",
        )
        .unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let items = runner.item_list("v1", None).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Recovery codes");
        assert_eq!(items[0].subtitle, "");
    }

    #[cfg(unix)]
    #[test]
    fn op_struct_runner_item_get_parses_fields_no_value() {
        // The crucial safety test: even when `op item get` JSON contains
        // a `value` key on each field, our `RawOpField` struct does not
        // declare it, so serde silently drops the value during
        // deserialization. The resulting `OpField` has no value field
        // at all (the type itself doesn't have one).
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-item-get");
        let json = r#"{"id":"i1","title":"API Keys","fields":[
            {"id":"username","label":"username","type":"STRING","purpose":"USERNAME","value":"alice","reference":"op://Personal/API Keys/username"},
            {"id":"password","label":"password","type":"CONCEALED","purpose":"PASSWORD","value":"super-secret","reference":"op://Personal/API Keys/password"}
        ]}"#;
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"item\" ] && [ \"$2\" = \"get\" ]; then \
             cat <<'JSON'\n{json}\nJSON\nexit 0; fi\nexit 99\n"
        );
        std::fs::write(&bin_path, script).unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let fields = runner.item_get("i1", "v1", None).unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].label, "username");
        assert!(!fields[0].concealed);
        assert_eq!(fields[1].label, "password");
        assert!(fields[1].concealed);
        // Compile-time guarantee: OpField has no `value` field. If a
        // future refactor adds one, this struct-match will fail to
        // compile and force an explicit re-review of the trust model.
        // The destructure also names `reference` — drop it from
        // `OpField` and this fails to compile, forcing a re-review of
        // the picker's commit path (which depends on `reference`
        // being the authoritative `op://` string from the CLI rather
        // than a synthesized one).
        let OpField {
            id: _,
            label: _,
            field_type: _,
            concealed: _,
            reference: _,
        } = fields[1].clone();
    }

    /// `op item get --format json` emits a `reference` key on every
    /// field carrying the authoritative `op://...` string. The picker
    /// commits this verbatim instead of synthesizing a path from
    /// display names, so verify it round-trips into `OpField`.
    #[cfg(unix)]
    #[test]
    fn op_struct_runner_item_get_captures_reference_field() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-item-get-reference");
        // `auth/secret_key` is the sectioned shape: 4-segment reference
        // where the 3rd segment is a section name. The picker must be
        // able to commit this verbatim.
        let json = r#"{"id":"i1","title":"X","fields":[
            {"id":"f1","label":"top","type":"STRING","reference":"op://X/Y/Z"},
            {"id":"f2","label":"key","type":"CONCEALED","reference":"op://Personal/API Keys/auth/secret_key"}
        ]}"#;
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"item\" ] && [ \"$2\" = \"get\" ]; then \
             cat <<'JSON'\n{json}\nJSON\nexit 0; fi\nexit 99\n"
        );
        std::fs::write(&bin_path, script).unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let fields = runner.item_get("i1", "v1", None).unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].reference, "op://X/Y/Z");
        assert_eq!(
            fields[1].reference,
            "op://Personal/API Keys/auth/secret_key"
        );
    }

    #[cfg(unix)]
    #[test]
    fn op_struct_runner_account_list_parses_real_op_output() {
        // Captured from `op account list --format json` against op CLI v2.x.
        // The actual key is `account_uuid`, not `id` — verify our serde
        // alias makes both shapes parse.
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-accounts");
        std::fs::write(
            &bin_path,
            "#!/bin/sh\ncat <<'EOF'\n[\n  {\n    \"url\": \"example.1password.com\",\n    \"email\": \"someone@example.com\",\n    \"user_uuid\": \"USERUUIDXXXX\",\n    \"account_uuid\": \"ACCTUUIDYYYY\"\n  }\n]\nEOF\n",
        )
        .unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let accounts = runner
            .account_list()
            .expect("real op account list output must parse");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "ACCTUUIDYYYY");
        // email + url round-trip from the realistic JSON fixture so the
        // picker's Account pane has the human-readable display string.
        assert_eq!(accounts[0].email, "someone@example.com");
        assert_eq!(accounts[0].url, "example.1password.com");
    }

    #[cfg(unix)]
    #[test]
    fn op_struct_runner_threads_account_flag_to_op_cli() {
        // The fake `op` shim echoes its argv to stdout when invoked. We
        // assert that passing `Some(account_uuid)` to vault_list produces
        // an `--account ACCT123` pair in the spawned argv. JSON output
        // is the empty array so deserialization succeeds.
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-account-flag");
        std::fs::write(
            &bin_path,
            "#!/bin/sh\necho \"$@\" >&2\nprintf '%s' '[]'\nexit 0\n",
        )
        .unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        // With Some(_) → must include `--account <id>` in argv.
        let _ = runner.vault_list(Some("ACCT123")).unwrap();
        // With None → must NOT include `--account` in argv.
        let _ = runner.vault_list(None).unwrap();
        // (Argv is echoed to stderr, which run_op_json drains but does
        // not return on success. Concrete argv-ordering coverage is in
        // the picker integration test that uses an inspectable stub.
        // This test verifies both code paths return Ok without panicking
        // — i.e., the args slice is well-formed in both branches.)
    }

    #[test]
    fn op_struct_runner_signed_out_detection() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-op-signed-out");
        std::fs::write(
            &bin_path,
            "#!/bin/sh\n>&2 echo 'You are not currently signed in. Run `op signin`.'\nexit 1\n",
        )
        .unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let err = runner.vault_list(None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not signed in") || msg.contains("op signin"),
            "expected signed-out detection in error: {msg}"
        );
    }
}
