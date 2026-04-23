//! Operator-controlled env resolution: four config layers, three value
//! syntaxes (`op://`, `$NAME` / `${NAME}`, literal), and merging onto
//! the manifest-resolved env at launch.

/// Test seam for the `op` CLI subprocess.
///
/// Production code uses [`OpCli`] which shells out to `op read`; tests
/// use a mock implementation that captures inputs and returns canned
/// responses.
pub trait OpRunner {
    /// Resolve a single `op://...` reference to its secret value.
    fn read(&self, reference: &str) -> anyhow::Result<String>;

    /// Verify the 1Password CLI is available on this host. Called
    /// once per launch before any `op://` reference is resolved so
    /// the operator sees a single, clear "install op" error rather
    /// than one-per-key noise. Default is a no-op so mock runners
    /// used in unit tests do not need to implement it.
    fn probe(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Dispatch a single env value string to the appropriate resolver.
///
/// * `op://...`              → `op_runner.read(value)`
/// * `$NAME` or `${NAME}`    → `host_env(name)`
/// * anything else           → returned verbatim as a literal
///
/// `layer_label` and `var_name` are used only for error messages so
/// operators can locate the offending config line (e.g. `"workspace
/// \"big-monorepo\" env var \"API_TOKEN\""`).
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
    if value.starts_with("op://") {
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

/// Parse `$NAME` or `${NAME}` and return the name. Returns `None` for
/// any other string (including bare `$`, `${}`, partially braced like
/// `${NAME`, and anything containing whitespace or non-identifier
/// characters after the sigil).
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

/// A valid POSIX-ish env name: ASCII letter or `_`, followed by ASCII
/// alphanumeric or `_`. Empty names are rejected.
fn is_valid_env_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Default production path for the 1Password CLI binary.
const OP_DEFAULT_BIN: &str = "op";

/// Default timeout for a single `op read` subprocess.
const OP_DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Maximum bytes of subprocess stderr captured in error output.
/// Larger outputs are truncated with a visible marker.
const OP_STDERR_MAX: usize = 4 * 1024;

/// Production `OpRunner` that shells out to the 1Password CLI.
///
/// Tests replace this with a mock by constructing a different
/// `OpRunner` implementation directly (e.g. `TestOpRunner`) or by
/// pointing `OpCli` at an explicit binary path via `OpCli::with_binary`.
/// No env-var-based test seam is used — the runner is always injected
/// as a dependency, which keeps tests free of any process-env mutation
/// and keeps the crate-level `unsafe_code = "forbid"` lint intact.
pub struct OpCli {
    binary: String,
    timeout: std::time::Duration,
}

impl OpCli {
    /// Construct a runner that invokes the default `op` binary on `$PATH`.
    /// Production code uses this via `OpCli::default()` inside
    /// `resolve_operator_env`; tests construct a different runner
    /// directly and pass it into `resolve_operator_env_with`.
    pub fn new() -> Self {
        Self {
            binary: OP_DEFAULT_BIN.to_string(),
            timeout: OP_DEFAULT_TIMEOUT,
        }
    }

    /// Construct a runner that invokes an explicit binary path. Used
    /// by integration tests to point `OpCli` at a tempfile-backed fake
    /// `op` binary without touching the process env.
    pub const fn with_binary(binary: String) -> Self {
        Self {
            binary,
            timeout: OP_DEFAULT_TIMEOUT,
        }
    }

    /// Test constructor: point at an explicit binary path with a
    /// custom (usually shorter) timeout.
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

/// Format a subprocess exit status for inclusion in an error message,
/// falling back to `"signal"` if the process was terminated by a signal.
fn format_exit_status(status: std::process::ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| "signal".to_string(), |c| c.to_string())
}

/// Truncate a stderr string to `OP_STDERR_MAX` bytes with a visible
/// marker. Returns an owned `String` in either branch.
fn truncate_stderr(stderr: &str) -> String {
    if stderr.len() > OP_STDERR_MAX {
        format!("{}… [truncated]", &stderr[..OP_STDERR_MAX])
    } else {
        stderr.to_owned()
    }
}

/// Drain a child's stderr into a buffer capped at `OP_STDERR_MAX + 1`
/// bytes. The extra byte lets the caller detect overflow; any further
/// stderr output is drained into a sink so the child exits cleanly.
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

/// Spawn a background thread that polls `try_wait` on the shared child
/// and forwards the exit status through `tx` when the child exits.
///
/// The poll loop releases the mutex between attempts so a concurrent
/// timeout branch can `take` the child and call `kill` without waiting
/// on a blocking `wait()`.
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

        // Wait with timeout using a channel-and-thread pattern so we
        // don't pull in a new async dep.
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

        // Share the Child handle across the wait thread (which polls
        // `try_wait` and consumes the child on completion) and the
        // timeout branch (which `take`s the child and calls `kill`).
        // `Child::kill` sends SIGKILL on Unix per its documented
        // behavior — no `unsafe` or libc dependency required.
        //
        // The wait thread must not hold the mutex across a blocking
        // `wait()` call — that would deadlock the timeout branch,
        // which needs the lock to perform the kill. Instead we poll
        // `try_wait` on a short cadence and release the lock between
        // polls so the timeout branch can take ownership the moment
        // it needs to.
        let child = std::sync::Arc::new(std::sync::Mutex::new(Some(child)));
        spawn_wait_thread(std::sync::Arc::clone(&child), tx);

        let status = match rx.recv_timeout(timeout) {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                anyhow::bail!("1Password CLI wait failed for {reference:?}: {e}");
            }
            Err(_) => {
                // Timeout: SIGKILL the child via the documented std API.
                // `Child::kill` returns `io::Result<()>`; we ignore the
                // result because the child may already have exited
                // between `recv_timeout` expiring and us reaching here,
                // which yields `Err(InvalidInput)` and is not a real
                // failure for our purposes. Take the child out of the
                // mutex in a short scope so no guard is held across the
                // blocking `wait()` below.
                let killed = child.lock().expect("child mutex poisoned").take();
                if let Some(mut c) = killed {
                    let _ = c.kill();
                    // Reap the killed child so its pipes close and the
                    // stdout/stderr reader threads can exit.
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
            let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
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
        use std::process::{Command, Stdio};

        let output = Command::new(&self.binary)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                anyhow::anyhow!(
                    "1Password CLI ({:?}) was not found on PATH: {e} — \
                     install from https://developer.1password.com/docs/cli/",
                    self.binary
                )
            })?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_trimmed = truncate_stderr(&stderr);
        anyhow::bail!(
            "1Password CLI probe (`{} --version`) exited with status {}: {} — \
             see https://developer.1password.com/docs/cli/",
            self.binary,
            format_exit_status(output.status),
            stderr_trimmed.trim()
        )
    }
}

/// Tracks which layer supplied the currently-winning value for a key.
///
/// Used to produce precise error messages during reserved-name
/// enforcement ("global [env] declares `DOCKER_HOST` which is reserved")
/// and launch diagnostics ("`OPERATOR_X`: provided by workspace
/// \"big-monorepo\" [agent override]").
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

/// Merge four env layers with later-wins semantics. Keys present in a
/// later layer overwrite values from earlier layers. Keys unique to any
/// layer are preserved.
///
/// Order, low → high priority:
///   1. `global`          — `[env]`
///   2. `agent`           — `[agents.<agent>.env]`
///   3. `workspace`       — `[workspaces.<ws>.env]`
///   4. `workspace_agent` — `[workspaces.<ws>.agents.<agent>.env]`
pub fn merge_layers(
    global: &std::collections::BTreeMap<String, String>,
    agent: &std::collections::BTreeMap<String, String>,
    workspace: &std::collections::BTreeMap<String, String>,
    workspace_agent: &std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    let mut merged = std::collections::BTreeMap::new();
    for (k, v) in global {
        merged.insert(k.clone(), v.clone());
    }
    for (k, v) in agent {
        merged.insert(k.clone(), v.clone());
    }
    for (k, v) in workspace {
        merged.insert(k.clone(), v.clone());
    }
    for (k, v) in workspace_agent {
        merged.insert(k.clone(), v.clone());
    }
    merged
}

/// Reject operator env maps that declare any reserved runtime name.
///
/// The reserved names (`JACKIN_CLAUDE_ENV`, `JACKIN_DIND_HOSTNAME`,
/// `DOCKER_HOST`, `DOCKER_TLS_VERIFY`, `DOCKER_CERT_PATH`) are fixed
/// by jackin and cannot be overridden. Conflicts are collected across
/// every layer and reported as a single aggregated error so operators
/// see all problems at once.
///
/// This runs at config LOAD time (in `AppConfig::load_or_init`),
/// before any launch path — so misconfigurations fail fast and the
/// runtime never sees a resolved map with a reserved key.
pub fn validate_reserved_names(config: &crate::config::AppConfig) -> anyhow::Result<()> {
    let mut offenses: Vec<String> = Vec::new();

    for key in config.env.keys() {
        if crate::env_model::is_reserved(key) {
            offenses.push(format!(
                "  - {key:?} is reserved by the jackin runtime; declared in {}",
                EnvLayer::Global
            ));
        }
    }

    for (agent_name, agent_source) in &config.agents {
        for key in agent_source.env.keys() {
            if crate::env_model::is_reserved(key) {
                offenses.push(format!(
                    "  - {key:?} is reserved by the jackin runtime; declared in {}",
                    EnvLayer::Agent(agent_name.clone())
                ));
            }
        }
    }

    for (ws_name, ws) in &config.workspaces {
        for key in ws.env.keys() {
            if crate::env_model::is_reserved(key) {
                offenses.push(format!(
                    "  - {key:?} is reserved by the jackin runtime; declared in {}",
                    EnvLayer::Workspace(ws_name.clone())
                ));
            }
        }
        for (agent_name, override_) in &ws.agents {
            for key in override_.env.keys() {
                if crate::env_model::is_reserved(key) {
                    offenses.push(format!(
                        "  - {key:?} is reserved by the jackin runtime; declared in {}",
                        EnvLayer::WorkspaceAgent {
                            workspace: ws_name.clone(),
                            agent: agent_name.clone()
                        }
                    ));
                }
            }
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

/// Walk the four env layers for a given `(agent, workspace)` pair and
/// resolve every value. Returns a map of resolved `(key → value)`.
///
/// Resolution failures from every layer are collected and reported in
/// a single aggregated error so operators see all problems at once
/// (matching the policy of `validate_reserved_names`).
///
/// The `agent` and `workspace` selectors are optional. When they are
/// `None`, only the global layer contributes; when only `agent` is set,
/// the agent layer joins; when only `workspace` is set, the workspace
/// layer joins; when both are set, all four layers are consulted.
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

/// Test-injectable version of [`resolve_operator_env`].
///
/// `R: OpRunner + ?Sized` so callers can pass either a concrete runner
/// (`&OpCli`, `&TestOpRunner`) or a trait object (`&dyn OpRunner`) —
/// the latter is how `LoadOptions::op_runner` flows through
/// `src/runtime/launch.rs`.
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
    let empty = std::collections::BTreeMap::new();

    let global = &config.env;
    let agent = agent_selector
        .and_then(|a| config.agents.get(a))
        .map_or(&empty, |a| &a.env);
    let ws_opt = workspace_name.and_then(|w| config.workspaces.get(w));
    let workspace = ws_opt.map_or(&empty, |w| &w.env);
    let workspace_agent = ws_opt
        .zip(agent_selector)
        .and_then(|(w, a)| w.agents.get(a))
        .map_or(&empty, |o| &o.env);

    // Produce a (key → (layer, raw_value)) map so resolution errors can
    // attribute which layer supplied each value.
    let mut attributed: std::collections::BTreeMap<String, (EnvLayer, String)> =
        std::collections::BTreeMap::new();

    for (k, v) in global {
        attributed.insert(k.clone(), (EnvLayer::Global, v.clone()));
    }
    if let Some(agent_name) = agent_selector {
        for (k, v) in agent {
            attributed.insert(
                k.clone(),
                (EnvLayer::Agent(agent_name.to_string()), v.clone()),
            );
        }
    }
    if let Some(ws_name) = workspace_name {
        for (k, v) in workspace {
            attributed.insert(
                k.clone(),
                (EnvLayer::Workspace(ws_name.to_string()), v.clone()),
            );
        }
    }
    if let (Some(ws_name), Some(agent_name)) = (workspace_name, agent_selector) {
        for (k, v) in workspace_agent {
            attributed.insert(
                k.clone(),
                (
                    EnvLayer::WorkspaceAgent {
                        workspace: ws_name.to_string(),
                        agent: agent_name.to_string(),
                    },
                    v.clone(),
                ),
            );
        }
    }

    let mut resolved = std::collections::BTreeMap::new();
    let mut errors: Vec<String> = Vec::new();

    // If ANY value uses the op:// scheme, probe the op CLI once up
    // front. This turns "op is not installed" from an N-failures
    // aggregate into a single clear install-link error, which is the
    // failure mode documented in the spec.
    let uses_op = attributed.values().any(|(_, v)| v.starts_with("op://"));
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn dispatch_host_ref_empty_string_passes_through() {
        // Spec: empty string host-env result is "set but empty" and
        // passes through unchanged (Unix semantics). Differentiates
        // from VarError::NotPresent, which is a hard error.
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

    /// Test seam: an `OpRunner` that captures the last `op read` argument.
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
            "#!/bin/sh\nif [ \"$1\" = \"read\" ] && [ \"$2\" = \"op://Personal/api/token\" ]; then echo -n 'tok-123'; exit 0; fi\nexit 99\n",
        )
        .unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let out = runner.read("op://Personal/api/token").unwrap();
        assert_eq!(out, "tok-123");
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
        // Emit ~16 KiB of stderr then fail. The runner must cap the
        // captured bytes so operator error output stays readable.
        std::fs::write(
            &bin_path,
            "#!/bin/sh\npython3 -c \"import sys; sys.stderr.write('X' * 16384)\" 2>&1 1>&2\nexit 1\n",
        )
        .unwrap();
        make_executable(&bin_path);

        let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
        let err = runner.read("op://Foo/bar").unwrap_err();
        let msg = err.to_string();
        // OP_STDERR_MAX is 4 KiB; the error should be bounded to that plus a
        // short truncation marker and the exit code framing.
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

        // Shorten the timeout for the test via the test-only constructor.
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
    fn make_executable(_path: &std::path::Path) {
        // Tests that require fake binaries are cfg-gated to unix; on
        // other platforms they are no-ops because the launch path
        // itself is unix-only in this codebase.
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
        // Spec: check op presence once per launch by shelling
        // `op --version`. Here we verify the probe fires for configs
        // that use op://... and is skipped for configs that do not.
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
}
