# Workspace-Level Env Resolver (Operator Env Layer) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a multi-layer operator-controlled environment variable resolver that lets `config.toml` declare env at the global, per-agent, per-workspace, and per-(workspace × agent) layers; resolve `op://...` references via the 1Password `op` CLI and `$NAME` / `${NAME}` references via the host environment; merge layers with later-wins semantics; reject conflicts with reserved runtime env names at load time; and overlay the resulting map on top of the manifest-resolved env before injecting both into `docker run -e` at launch.

**Architecture:** A new `src/operator_env.rs` module owns the schema types (`EnvValue` newtype, `WorkspaceAgentOverride`), the dispatch logic (`op://` → subprocess, `$NAME`/`${NAME}` → host env, else literal), the layer-merging rules, the reserved-name validator, and the subprocess integration with `op read` (bounded stderr, 30 s timeout, `op --version` presence probe). The resolver exposes two entry points: `resolve_operator_env` (production; wires in the default `OpCli` and reads host env via `std::env::var`) and `resolve_operator_env_with` (dependency-injected; takes an explicit `OpRunner` and a host-env callback, used by every test so nothing in the test suite mutates the process env). New `env` fields are added to `AppConfig`, `AgentSource`, `WorkspaceConfig`, and a new `WorkspaceAgentOverride` struct keyed by agent selector under `WorkspaceConfig::agents`. The reserved-name check runs inside `AppConfig::load_or_init` (before `validate_workspaces`). At launch, `src/runtime/launch.rs` calls the resolver (optionally with test-supplied seams carried on `LoadOptions`), probes `op --version` once if any layer uses `op://`, overlays the resulting map on the manifest-resolved `ResolvedEnv` (operator wins on conflicts), filters reserved names, and injects via existing `docker run -e`. A diagnostic line prints reference-count summaries (normal mode) or per-key reference strings (debug mode) — values never leave the process.

**Tech Stack:** Rust (edition 2024), `serde` + `toml` 1.x (already in `Cargo.toml`), `anyhow`, `tempfile` 3.20, `cargo-nextest`. Subprocess via `std::process::Command`; timeout via a background thread with `Receiver::recv_timeout` — no new dependencies. Hung `op` children on timeout are terminated via `std::process::Child::kill()` (documented SIGKILL on Unix), so the crate-level `unsafe_code = "forbid"` lint is preserved.

**Branch:** `feature/workspace-env-resolver` (per `BRANCHING.md` — `feature/<short-description>`).
**Commit style:** Conventional Commits with DCO `Signed-off-by` and `Co-authored-by: Claude <noreply@anthropic.com>` per `AGENTS.md`.
**Spec:** `docs/superpowers/specs/2026-04-23-workspace-env-resolver-design.md`.

---

## File Structure

| File                                                                     | Purpose                                                                 |
| ------------------------------------------------------------------------ | ----------------------------------------------------------------------- |
| `src/operator_env.rs`                                                    | New module: `EnvValue` newtype, `dispatch_value`, `merge_layers`, `resolve_operator_env`, `op_read` (subprocess), `validate_reserved_names`, diagnostic formatting. Owns all env-resolver semantics. |
| `src/lib.rs`                                                             | `pub mod operator_env;` registration.                                    |
| `src/config/mod.rs`                                                      | Add `pub env: BTreeMap<String, String>` to `AppConfig`; add `pub env: BTreeMap<String, String>` to `AgentSource`. Re-export `WorkspaceAgentOverride` from workspace. |
| `src/workspace/mod.rs`                                                   | Add `pub env: BTreeMap<String, String>` and `pub agents: BTreeMap<String, WorkspaceAgentOverride>` to `WorkspaceConfig`. Define `WorkspaceAgentOverride { env }`. |
| `src/config/persist.rs`                                                  | Call `operator_env::validate_reserved_names` from `load_or_init` before `validate_workspaces`. |
| `src/runtime/launch.rs`                                                  | Call `operator_env::resolve_operator_env` after manifest env resolution; overlay map on `resolved_env.vars` (operator wins); filter reserved names; print counts/refs diagnostic. |
| `docs/src/content/docs/guides/environment-variables.mdx`                 | New guide: operator env layers, syntax dispatch, `op` CLI integration, debugging, examples. |
| `docs/src/content/docs/reference/configuration.mdx`                      | Document `[env]`, `[agents.*.env]`, `[workspaces.*.env]`, `[workspaces.*.agents.*.env]`, and the layer order. |
| `docs/src/content/docs/guides/authentication.mdx`                        | Add cross-link to the new env-variables guide (operator secrets supplement auth forwarding). |
| `docs/src/content/docs/reference/roadmap/onepassword-integration.mdx`    | Status update: option 2 (workspace-managed references) is now implemented for env; file mounts remain deferred. |
| `docs/astro.config.ts`                                                   | Register the new `environment-variables.mdx` page in the sidebar (Guides). |
| `CHANGELOG.md`                                                           | `Added` entry for operator env layers + 1Password `op` CLI integration.  |

---

## Preflight

- [ ] **Step 0.1: Ensure clean tree on `main`**

```bash
git fetch origin
git checkout main
git pull --ff-only
git status
```

Expected: `nothing to commit, working tree clean`. If dirty, stop and investigate.

- [ ] **Step 0.2: Create the feature branch**

```bash
git checkout -b feature/workspace-env-resolver
```

- [ ] **Step 0.3: Confirm pre-commit gate is currently clean (baseline)**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: all three exit 0, zero warnings. If any fail, do NOT start this work — fix baseline first or you will chase unrelated failures.

- [ ] **Step 0.4: Confirm `tempfile` and `serde`/`toml` already present**

```bash
grep -E '^(tempfile|serde|toml) =' Cargo.toml
```

Expected: all three present. No Cargo.toml changes should be needed for this PR.

---

## Task 1: Add the `operator_env` module skeleton with the `EnvValue` newtype and dispatch

**Files:**
- Create: `src/operator_env.rs`
- Modify: `src/lib.rs` (register module)

- [ ] **Step 1.1: Write the failing test for `dispatch_value`**

Create `src/operator_env.rs` with just the tests first (no implementation — the tests will fail to compile). Paste this:

```rust
//! Operator-controlled env resolution: four config layers, three value
//! syntaxes (`op://`, `$NAME` / `${NAME}`, literal), and merging onto
//! the manifest-resolved env at launch.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_literal_value_returns_literal() {
        let out = dispatch_value("global", "FOO", "plain-literal", &TestOpRunner::forbidden(), |n| {
            panic!("host env should not be queried for literal; got {n}")
        })
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
        assert_eq!(runner.last_ref().as_deref(), Some("op://Personal/api/token"));
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
}
```

- [ ] **Step 1.2: Run the new tests — confirm they fail (compile error is the expected failure)**

```bash
cargo nextest run -p jackin operator_env::tests 2>&1 | tail -30
```

Expected: fails to build because `dispatch_value` and `OpRunner` do not exist yet. This is the "red" state.

- [ ] **Step 1.3: Register the new module**

Edit `src/lib.rs`. After the existing `pub mod env_resolver;` line, insert:

```rust
pub mod operator_env;
```

(Alphabetic position is between `env_resolver` and `instance`.)

- [ ] **Step 1.4: Implement `OpRunner` trait and `dispatch_value` — just enough to make Step 1.1 tests pass**

Insert at the top of `src/operator_env.rs`, immediately below the module docstring, before the `#[cfg(test)]` block:

```rust
/// Test seam for the `op` CLI subprocess.
///
/// Production code uses [`OpCli`] which shells out to `op read`; tests
/// use a mock implementation that captures inputs and returns canned
/// responses.
pub trait OpRunner {
    /// Resolve a single `op://...` reference to its secret value.
    fn read(&self, reference: &str) -> anyhow::Result<String>;
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
pub fn dispatch_value(
    layer_label: &str,
    var_name: &str,
    value: &str,
    op_runner: &impl OpRunner,
    host_env: impl FnOnce(&str) -> Result<String, std::env::VarError>,
) -> anyhow::Result<String> {
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
```

- [ ] **Step 1.5: Run the new tests — confirm green**

```bash
cargo nextest run -p jackin operator_env::tests
```

Expected: all five tests pass.

- [ ] **Step 1.6: Run the full suite for regressions**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: all three exit 0, zero warnings.

- [ ] **Step 1.7: Commit Task 1**

```bash
git add src/operator_env.rs src/lib.rs
git commit -s -m "$(cat <<'EOF'
feat(operator_env): add module with EnvValue dispatch (literal / $NAME / op://)

Introduce the operator_env module with a minimal dispatch_value that
recognizes three value syntaxes:

- op://... references (resolved via an OpRunner trait seam — production
  implementation follows in a later task; this task adds only the seam)
- $NAME and ${NAME} references (resolved via a host_env callback so
  the dispatch is unit-testable without touching the real process env)
- literals, returned verbatim

Error messages include the layer label and var name so operators can
locate the offending config line. Name parsing enforces a POSIX-ish
shape (ASCII alpha or `_` head, alphanumeric or `_` tail).

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add the real `op` CLI runner with bounded stderr and 30 s timeout

**Files:**
- Modify: `src/operator_env.rs` (add `OpCli` implementation + tests)

- [ ] **Step 2.1: Write failing tests for `OpCli` via the `OpCli::with_binary` constructor**

Append to the `#[cfg(test)] mod tests` block in `src/operator_env.rs`:

```rust
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
            msg.contains("not found") || msg.contains("No such file") || msg.contains("failed to spawn"),
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
```

Add `use` of `tempfile` where needed — tests already have implicit access via `dev-dependencies`. If you hit an unresolved-import error, add `use tempfile;` at the top of the test module (it's already a dev-dep: `tempfile = "3.20"` in the root `Cargo.toml`).

- [ ] **Step 2.2: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin operator_env::tests::op_cli 2>&1 | tail -20
```

Expected: fails to compile — `OpCli` does not exist.

- [ ] **Step 2.3: Implement `OpCli` with bounded stderr and a configurable timeout**

Insert after the `is_valid_env_name` function in `src/operator_env.rs` (before the `#[cfg(test)]` block):

```rust
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
    pub fn with_binary(binary: String) -> Self {
        Self {
            binary,
            timeout: OP_DEFAULT_TIMEOUT,
        }
    }

    /// Test constructor: point at an explicit binary path with a
    /// custom (usually shorter) timeout.
    #[cfg(test)]
    fn with_binary_and_timeout(binary: String, timeout: std::time::Duration) -> Self {
        Self { binary, timeout }
    }
}

impl Default for OpCli {
    fn default() -> Self {
        Self::new()
    }
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
        let mut stderr = child.stderr.take().expect("piped stderr");
        let timeout = self.timeout;

        let stdout_handle = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf);
            buf
        });
        let stderr_handle = std::thread::spawn(move || {
            let mut buf = Vec::new();
            // Cap stderr capture to OP_STDERR_MAX + 1 so we can detect
            // overflow cleanly.
            let mut chunk = [0u8; 1024];
            loop {
                match stderr.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&chunk[..n]);
                        if buf.len() > OP_STDERR_MAX + 1 {
                            // Drain remaining stderr into the void to
                            // let the child exit cleanly, but stop
                            // accumulating bytes here.
                            let mut sink = [0u8; 4096];
                            while matches!(stderr.read(&mut sink), Ok(n) if n > 0) {}
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            buf
        });

        // Share the Child handle across the wait thread (which consumes
        // it via `wait`) and the timeout branch (which needs `kill`).
        // `Child::kill` sends SIGKILL on Unix per its documented
        // behavior — no `unsafe` or libc dependency required.
        let child = std::sync::Arc::new(std::sync::Mutex::new(Some(child)));
        let wait_child = std::sync::Arc::clone(&child);
        std::thread::spawn(move || {
            let status = {
                let mut guard = wait_child.lock().expect("child mutex poisoned");
                match guard.as_mut() {
                    Some(c) => c.wait(),
                    None => return,
                }
            };
            let _ = tx.send(status);
        });

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
                // failure for our purposes.
                if let Some(mut c) = child.lock().expect("child mutex poisoned").take() {
                    let _ = c.kill();
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
        let stderr_trimmed = if stderr.len() > OP_STDERR_MAX {
            format!("{}… [truncated]", &stderr[..OP_STDERR_MAX])
        } else {
            stderr.into_owned()
        };
        anyhow::bail!(
            "1Password CLI exited with status {} resolving {reference:?}: {}",
            status.code().map_or_else(|| "signal".to_string(), |c| c.to_string()),
            stderr_trimmed.trim()
        )
    }
}
```

Notes on the `Child::kill()` path:

- `std::process::Child::kill` is documented to send `SIGKILL` on Unix (see https://doc.rust-lang.org/std/process/struct.Child.html#method.kill). No `unsafe` block, no `libc` dependency, and no relaxation of the crate-level `unsafe_code = "forbid"` lint is required.
- The `Child` handle is shared via `Arc<Mutex<Option<Child>>>` so the wait thread can call `wait()` (which takes `&mut self`) and the timeout branch can `take()` the child out and call `kill()` on it. The `Option` lets whichever side runs first cleanly claim ownership; if `wait` returned first, the timeout branch observes `None` and skips the kill. If the timeout branch fires first, the wait thread observes `None` and exits without sending on the channel.
- `Cargo.toml`'s `unsafe_code = "forbid"` lint is untouched by this plan.

- [ ] **Step 2.4: Extend `OpRunner` with a `probe` method and add `op --version` probe on `OpCli`**

Spec requirement: "`op` must be on `$PATH` when any `op://` reference is in scope. Check presence once per launch by shelling `op --version`." We want a single fast check that fails with the install-link error message before any per-key resolution runs, so the operator sees one clear "install `op`" error rather than one-per-key noise.

First append a failing test to the `#[cfg(test)] mod tests` block in `src/operator_env.rs`:

```rust
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
```

Now add `probe` to the `OpRunner` trait (edit the trait defined in Step 1.4):

```rust
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
```

Implement `probe` on `OpCli` (append to the `impl OpRunner for OpCli` block from Step 2.3):

```rust
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
        let stderr_trimmed = if stderr.len() > OP_STDERR_MAX {
            format!("{}… [truncated]", &stderr[..OP_STDERR_MAX])
        } else {
            stderr.into_owned()
        };
        anyhow::bail!(
            "1Password CLI probe (`{} --version`) exited with status {}: {} — \
             see https://developer.1password.com/docs/cli/",
            self.binary,
            output.status.code().map_or_else(|| "signal".to_string(), |c| c.to_string()),
            stderr_trimmed.trim()
        )
    }
```

The probe is plumbed into `resolve_operator_env_with` in Task 6 (Step 6.3), which runs it once when any raw value starts with `op://`.

- [ ] **Step 2.5: Run the new tests — confirm green**

```bash
cargo nextest run -p jackin operator_env::tests::op_cli
```

Expected: all seven `op_cli_*` tests pass (five from Step 2.1 plus the two `op_cli_probe_*` tests from Step 2.4).

Note: `op_cli_large_stderr_is_truncated` shells out to `python3`. If `python3` is not on the CI runner, swap the body of `fake-op-big-stderr` for a shell-native equivalent, e.g. `for i in $(seq 1 16384); do printf X >&2; done`. Adjust the test body in Step 2.1 before running if CI lacks Python.

- [ ] **Step 2.6: Full suite**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: zero warnings, zero failures.

- [ ] **Step 2.7: Commit Task 2**

```bash
git add src/operator_env.rs
git commit -s -m "$(cat <<'EOF'
feat(operator_env): add OpCli with bounded stderr, 30s timeout, and probe

Add the production OpRunner implementation backed by std::process::Command
invoking `op read <reference>`. Stderr is captured and truncated at 4 KiB
so subprocess failure messages remain readable; wait is bounded by a 30-
second timeout (configurable via a test-only constructor) implemented with
a channel + background thread rather than a new async dep. On timeout the
hung child is terminated via `std::process::Child::kill()`, which is
documented to send SIGKILL on Unix — no `unsafe` code, no new dependency,
and the crate-level `unsafe_code = "forbid"` lint stays in place.

Extend OpRunner with a `probe` method that shells `op --version` so
resolve_operator_env can fail fast once with an install-link error when
`op` is missing, rather than producing a separate failure per op:// key.

Tests use tempfile-backed fake `op` binaries, pointed at via the
`OpCli::with_binary` constructor (pure dependency injection — no env
var seam), to cover: success, missing binary, non-zero exit with
bounded stderr, oversized stderr truncation, hang timeout, and the
`--version` probe's success / missing-binary paths.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Schema additions — `env` fields on `AppConfig`, `AgentSource`, `WorkspaceConfig`; new `WorkspaceAgentOverride`; `agents` map on `WorkspaceConfig`

**Files:**
- Modify: `src/workspace/mod.rs:21-40` (define `WorkspaceAgentOverride`; add `env` and `agents` fields to `WorkspaceConfig`)
- Modify: `src/config/mod.rs:73-98` (add `env` to `AgentSource` and `AppConfig`; re-export `WorkspaceAgentOverride`)

- [ ] **Step 3.1: Write failing tests for the schema round-trip**

In `src/config/mod.rs`, inside the existing `#[cfg(test)] mod tests` block (append at the bottom, before the final `}`):

```rust
    #[test]
    fn deserializes_global_env_map() {
        let toml_str = r#"
[env]
OPERATOR_GLOBAL = "literal"
OPERATOR_SECRET = "op://Personal/api/token"
OPERATOR_HOST = "$HOME_VAR"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.env.get("OPERATOR_GLOBAL").unwrap(), "literal");
        assert_eq!(
            config.env.get("OPERATOR_SECRET").unwrap(),
            "op://Personal/api/token"
        );
        assert_eq!(config.env.get("OPERATOR_HOST").unwrap(), "$HOME_VAR");
    }

    #[test]
    fn deserializes_per_agent_env_map() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[agents.agent-smith.env]
AGENT_TOKEN = "op://Shared/smith/token"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let agent = config.agents.get("agent-smith").unwrap();
        assert_eq!(
            agent.env.get("AGENT_TOKEN").unwrap(),
            "op://Shared/smith/token"
        );
    }

    #[test]
    fn deserializes_per_workspace_env_map() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.env]
WORKSPACE_VAR = "literal"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let ws = config.workspaces.get("big-monorepo").unwrap();
        assert_eq!(ws.env.get("WORKSPACE_VAR").unwrap(), "literal");
    }

    #[test]
    fn deserializes_workspace_agent_override_env() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.agents.agent-smith.env]
PER_WORKSPACE_PER_AGENT = "specific"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let ws = config.workspaces.get("big-monorepo").unwrap();
        let override_ = ws.agents.get("agent-smith").unwrap();
        assert_eq!(
            override_.env.get("PER_WORKSPACE_PER_AGENT").unwrap(),
            "specific"
        );
    }

    #[test]
    fn env_maps_default_to_empty_when_omitted() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert!(config.env.is_empty());
        assert!(config.agents.get("agent-smith").unwrap().env.is_empty());
    }

    #[test]
    fn deserializes_agent_with_slash_in_name_using_quoted_keys() {
        // The spec calls out `[agents."chainargos/agent-jones".env]`
        // and `[workspaces.<ws>.agents."chainargos/agent-jones".env]`
        // as the TOML shape for third-party agent selectors that
        // include a `/`. Standard TOML quoted keys suffice — this
        // test locks in that shape so a future refactor does not
        // accidentally require un-quoted identifiers.
        let toml_str = r#"
[agents."chainargos/agent-jones"]
git = "https://github.com/chainargos/jackin-agent-jones.git"

[agents."chainargos/agent-jones".env]
DATABASE_URL = "op://Work/agent-jones/db"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.agents."chainargos/agent-jones".env]
OPENAI_API_KEY = "op://Work/big-monorepo/OpenAI"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let agent = config.agents.get("chainargos/agent-jones").unwrap();
        assert_eq!(
            agent.env.get("DATABASE_URL").unwrap(),
            "op://Work/agent-jones/db"
        );
        let ws = config.workspaces.get("big-monorepo").unwrap();
        let override_ = ws.agents.get("chainargos/agent-jones").unwrap();
        assert_eq!(
            override_.env.get("OPENAI_API_KEY").unwrap(),
            "op://Work/big-monorepo/OpenAI"
        );
    }
```

- [ ] **Step 3.2: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin config::tests::deserializes_global_env_map \
  config::tests::deserializes_per_agent_env_map \
  config::tests::deserializes_per_workspace_env_map \
  config::tests::deserializes_workspace_agent_override_env \
  config::tests::env_maps_default_to_empty_when_omitted \
  config::tests::deserializes_agent_with_slash_in_name_using_quoted_keys 2>&1 | tail -30
```

Expected: build fails — `env` field does not exist on `AppConfig`, `AgentSource`, `WorkspaceConfig`; `WorkspaceAgentOverride` does not exist.

- [ ] **Step 3.3: Add `WorkspaceAgentOverride` and extend `WorkspaceConfig`**

Edit `src/workspace/mod.rs`. Replace the existing `WorkspaceConfig` struct (lines 29–40) with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceConfig {
    pub workdir: String,
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
    #[serde(default)]
    pub allowed_agents: Vec<String>,
    #[serde(default)]
    pub default_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_agent: Option<String>,
    /// Workspace-level operator env map. Keys are env var names;
    /// values use the operator_env dispatch syntax
    /// (`op://...` | `$NAME` | `${NAME}` | literal).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: std::collections::BTreeMap<String, String>,
    /// Per-(workspace × agent) env overrides, keyed by the agent
    /// selector (e.g. `"agent-smith"` or `"chainargos/agent-brown"`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub agents: std::collections::BTreeMap<String, WorkspaceAgentOverride>,
}

/// Per-(workspace × agent) operator overrides.
///
/// Currently only `env` is supported; the struct exists as a named type
/// so future overrides (e.g. `auth_forward`) can be added without a
/// TOML schema break.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceAgentOverride {
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: std::collections::BTreeMap<String, String>,
}
```

- [ ] **Step 3.4: Re-export `WorkspaceAgentOverride` from the workspace module root**

In `src/workspace/mod.rs`, at line 7 (the existing `pub use mounts::` block), add a `pub use` for `WorkspaceAgentOverride`. Since `WorkspaceAgentOverride` is defined in `src/workspace/mod.rs` itself (not a submodule), and `WorkspaceConfig` is already accessible via `crate::workspace::WorkspaceConfig`, no `pub use` is needed for the new type. Skip this step if it's redundant — only add the re-export if `WorkspaceAgentOverride` is defined in a submodule during implementation.

- [ ] **Step 3.5: Add `env` to `AgentSource` and `AppConfig`; re-export `WorkspaceAgentOverride`**

Edit `src/config/mod.rs`. Replace the existing `AgentSource` (lines 73–80) with:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentSource {
    pub git: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub trusted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<ClaudeAgentConfig>,
    /// Agent-layer operator env map. Merged on top of the global
    /// `[env]` map when the agent is launched. Values use the
    /// operator_env dispatch syntax.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}
```

And extend `AppConfig` (lines 88–98):

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub claude: ClaudeConfig,
    /// Global operator env map — the bottom layer. Merged under
    /// per-agent, per-workspace, and per-(workspace × agent) layers.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentSource>,
    #[serde(default)]
    pub docker: DockerConfig,
    #[serde(default)]
    pub workspaces: BTreeMap<String, WorkspaceConfig>,
}
```

Also add, immediately below the existing `pub use crate::workspace::MountConfig;` at line 5:

```rust
pub use crate::workspace::WorkspaceAgentOverride;
```

Note: `AgentSource` now derives `Default` (previously it did not). This is additive and safe — all existing construction sites pass every field explicitly. If clippy complains about `Default` on a type that already has an explicit `new`-style use, leave the derive in place; the tests reference `AgentSource::default()` implicitly via the new BTreeMap default.

Note: `WorkspaceConfig` now has two new fields (`env`, `agents`). Existing construction sites in tests and non-test code set the entire struct literal (e.g. `WorkspaceConfig { workdir, mounts, allowed_agents, default_agent, last_agent }`). Those sites must be updated — see Step 3.6.

- [ ] **Step 3.6: Update every `WorkspaceConfig { ... }` struct literal in the repo to initialize the two new fields**

Run:

```bash
grep -rln "WorkspaceConfig {" src/ tests/ | sort -u
```

Expected file list (as of this plan's drafting — re-verify on the branch):

- `src/config/mod.rs`
- `src/config/workspaces.rs`
- `src/workspace/mod.rs`
- `src/workspace/resolve.rs` (if present — verify with the grep above)
- `tests/workspace_config_crud.rs`
- `tests/workspace_mount_collapse.rs`

For each file, append `env: std::collections::BTreeMap::new(),` and `agents: std::collections::BTreeMap::new(),` to the struct-literal initializer just after `last_agent`. Example — before:

```rust
WorkspaceConfig {
    workdir: "/workspace/project".into(),
    mounts: vec![ /* ... */ ],
    allowed_agents: vec![],
    default_agent: None,
    last_agent: None,
}
```

After:

```rust
WorkspaceConfig {
    workdir: "/workspace/project".into(),
    mounts: vec![ /* ... */ ],
    allowed_agents: vec![],
    default_agent: None,
    last_agent: None,
    env: std::collections::BTreeMap::new(),
    agents: std::collections::BTreeMap::new(),
}
```

This is a purely mechanical update — do not use `..Default::default()` because `WorkspaceConfig` does not derive `Default`.

- [ ] **Step 3.7: Run the schema tests — confirm green**

```bash
cargo nextest run -p jackin config::tests::deserializes_global_env_map \
  config::tests::deserializes_per_agent_env_map \
  config::tests::deserializes_per_workspace_env_map \
  config::tests::deserializes_workspace_agent_override_env \
  config::tests::env_maps_default_to_empty_when_omitted \
  config::tests::deserializes_agent_with_slash_in_name_using_quoted_keys
```

Expected: all six tests green.

- [ ] **Step 3.8: Run the full suite — confirm no regressions from the struct-literal migration**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: all green, zero warnings. If any tests fail because of a missed struct-literal update, grep again and fix them.

- [ ] **Step 3.9: Commit Task 3**

```bash
git add src/config/mod.rs src/workspace/mod.rs src/config/workspaces.rs tests/
git commit -s -m "$(cat <<'EOF'
feat(config): add operator env maps on AppConfig / AgentSource / WorkspaceConfig

Introduce the four-layer operator env schema:

- [env]                              → AppConfig::env
- [agents.<name>.env]                → AgentSource::env
- [workspaces.<name>.env]            → WorkspaceConfig::env
- [workspaces.<name>.agents.<agent>.env] → WorkspaceAgentOverride::env
  (via a new WorkspaceAgentOverride type and a
  WorkspaceConfig::agents: BTreeMap<String, WorkspaceAgentOverride>)

All env maps default to empty and are skipped on serialization when
empty, so existing configs keep their current on-disk shape.

WorkspaceAgentOverride is named (not inline-structural) so future
per-(workspace × agent) knobs can be added without a schema break.
The type uses deny_unknown_fields to catch typos early.

Layer-merging semantics, reserved-name enforcement, and value dispatch
are added in subsequent tasks; this commit is purely the schema.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Implement layer merging (`merge_layers`) — global → agent → workspace → workspace×agent

**Files:**
- Modify: `src/operator_env.rs` (add `merge_layers`)

- [ ] **Step 4.1: Write failing tests for `merge_layers`**

Append to the `#[cfg(test)] mod tests` block in `src/operator_env.rs`:

```rust
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
```

- [ ] **Step 4.2: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin operator_env::tests::merge
```

Expected: compile error — `merge_layers` does not exist.

- [ ] **Step 4.3: Implement `merge_layers`**

Add immediately after the `OpCli` implementation block in `src/operator_env.rs`:

```rust
/// Tracks which layer supplied the currently-winning value for a key.
///
/// Used to produce precise error messages during reserved-name
/// enforcement ("global [env] declares DOCKER_HOST which is reserved")
/// and launch diagnostics ("OPERATOR_X: provided by workspace
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
```

- [ ] **Step 4.4: Run tests — confirm green**

```bash
cargo nextest run -p jackin operator_env::tests::merge
```

Expected: all six merge tests pass.

- [ ] **Step 4.5: Commit Task 4**

```bash
git add src/operator_env.rs
git commit -s -m "$(cat <<'EOF'
feat(operator_env): add layer merging with later-wins semantics

Implement merge_layers that combines the four operator env layers —
global, agent, workspace, workspace×agent — with strict later-wins
semantics: duplicates are overwritten by later layers, unique keys
from any layer are preserved.

Also introduce EnvLayer (a Display-able enum) so later tasks can
attribute resolution failures and reserved-name conflicts to the
exact config layer that supplied the offending key.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Reserved-name validator — `validate_reserved_names`

**Files:**
- Modify: `src/operator_env.rs` (add `validate_reserved_names` + tests)
- Modify: `src/config/persist.rs` (call validator from `load_or_init`)

- [ ] **Step 5.1: Write failing tests**

Append to the `#[cfg(test)] mod tests` block in `src/operator_env.rs`:

```rust
    #[test]
    fn validate_reserved_names_rejects_global_reserved() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env.insert("DOCKER_HOST".to_string(), "whatever".to_string());

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
        agent.env.insert(
            "JACKIN_CLAUDE_ENV".to_string(),
            "whatever".to_string(),
        );
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
        ws.env.insert("DOCKER_TLS_VERIFY".to_string(), "0".to_string());
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
            msg.contains("workspace \"big-monorepo\"")
                && msg.contains("agent \"agent-smith\""),
            "{msg}"
        );
    }

    #[test]
    fn validate_reserved_names_reports_all_conflicts_in_one_error() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env.insert("DOCKER_HOST".to_string(), "x".to_string());
        cfg.env.insert("DOCKER_TLS_VERIFY".to_string(), "y".to_string());

        let err = validate_reserved_names(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("DOCKER_HOST"), "{msg}");
        assert!(msg.contains("DOCKER_TLS_VERIFY"), "{msg}");
    }

    #[test]
    fn validate_reserved_names_accepts_non_reserved() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env.insert("MY_VAR".to_string(), "value".to_string());
        cfg.env.insert("OPERATOR_TOKEN".to_string(), "op://...".to_string());

        validate_reserved_names(&cfg).unwrap();
    }
```

- [ ] **Step 5.2: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin operator_env::tests::validate_reserved_names
```

Expected: compile error — `validate_reserved_names` does not exist.

- [ ] **Step 5.3: Implement `validate_reserved_names`**

Append to `src/operator_env.rs`, below `merge_layers`:

```rust
/// Reject operator env maps that declare any name reserved by the
/// runtime (`JACKIN_CLAUDE_ENV`, `JACKIN_DIND_HOSTNAME`, `DOCKER_HOST`,
/// `DOCKER_TLS_VERIFY`, `DOCKER_CERT_PATH`). Conflicts are collected
/// across every layer and reported as a single aggregated error so
/// operators see all problems at once.
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
```

- [ ] **Step 5.4: Run the new tests — confirm green**

```bash
cargo nextest run -p jackin operator_env::tests::validate_reserved_names
```

Expected: all six tests pass.

- [ ] **Step 5.5: Wire the validator into `AppConfig::load_or_init`**

Edit `src/config/persist.rs`. Replace the body of `load_or_init` (lines 5–20) with:

```rust
    pub fn load_or_init(paths: &JackinPaths) -> anyhow::Result<Self> {
        paths.ensure_base_dirs()?;

        let mut config = match std::fs::read_to_string(&paths.config_file) {
            Ok(contents) => toml::from_str(&contents)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.into()),
        };

        if config.sync_builtin_agents() {
            config.save(paths)?;
        }

        // Reject operator env maps that declare reserved runtime names.
        // Runs at load, before validate_workspaces, so misconfigurations
        // fail fast regardless of which subcommand is about to execute.
        crate::operator_env::validate_reserved_names(&config)?;

        config.validate_workspaces()?;
        Ok(config)
    }
```

- [ ] **Step 5.6: Add an integration test that `load_or_init` rejects a config with a reserved env name**

Append to the `#[cfg(test)] mod tests` block in `src/config/persist.rs`:

```rust
    #[test]
    fn load_or_init_rejects_reserved_env_name_in_global_layer() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[env]
DOCKER_HOST = "override-attempt"

[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#,
        )
        .unwrap();

        let err = AppConfig::load_or_init(&paths).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("DOCKER_HOST"), "{msg}");
        assert!(msg.contains("reserved"), "{msg}");
        assert!(msg.contains("global"), "{msg}");
    }
```

- [ ] **Step 5.7: Run full suite**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: all green, zero warnings.

- [ ] **Step 5.8: Commit Task 5**

```bash
git add src/operator_env.rs src/config/persist.rs
git commit -s -m "$(cat <<'EOF'
feat(operator_env): reject reserved runtime env names at config load

Add validate_reserved_names — called from AppConfig::load_or_init right
after sync_builtin_agents and before validate_workspaces. Walks all four
env layers (global, per-agent, per-workspace, per-(workspace × agent))
and collects every conflict with env_model::RESERVED_RUNTIME_ENV_VARS
(JACKIN_CLAUDE_ENV, JACKIN_DIND_HOSTNAME, DOCKER_HOST, DOCKER_TLS_VERIFY,
DOCKER_CERT_PATH) into a single aggregated error. The error message
names each offending (layer, key) pair so operators can fix the whole
config in one pass rather than relauch-diagnose-repeat.

Running at LOAD rather than at launch means the check is independent of
which CLI command is about to execute — `jackin list`, `jackin config
show`, and `jackin load` all report the same error.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: `resolve_operator_env` — walk layers and resolve every value

**Files:**
- Modify: `src/operator_env.rs` (add `resolve_operator_env` + tests)

- [ ] **Step 6.1: Write failing tests**

Append to the `#[cfg(test)] mod tests` block in `src/operator_env.rs`:

```rust
    #[test]
    fn resolve_empty_config_returns_empty_map() {
        let cfg = crate::config::AppConfig::default();
        let resolved = resolve_operator_env_with(
            &cfg,
            None,
            None,
            &TestOpRunner::forbidden(),
            |_| Err(std::env::VarError::NotPresent),
        )
        .unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn resolve_global_literal_value() {
        let mut cfg = crate::config::AppConfig::default();
        cfg.env.insert("FOO".to_string(), "bar".to_string());
        let resolved = resolve_operator_env_with(
            &cfg,
            None,
            None,
            &TestOpRunner::forbidden(),
            |_| Err(std::env::VarError::NotPresent),
        )
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
        agent_source.env.insert("X".to_string(), "agent".to_string());
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

        let err = resolve_operator_env_with(
            &cfg,
            None,
            None,
            &TestOpRunner::forbidden(),
            |_| Err(std::env::VarError::NotPresent),
        )
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
        cfg.env.insert("A".to_string(), "op://Personal/a".to_string());
        cfg.env.insert("B".to_string(), "op://Personal/b".to_string());
        let runner = ProbeCountingRunner {
            probe_calls: std::cell::Cell::new(0),
            read_calls: std::cell::Cell::new(0),
        };
        resolve_operator_env_with(
            &cfg,
            None,
            None,
            &runner,
            |_| Err(std::env::VarError::NotPresent),
        )
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
        resolve_operator_env_with(
            &cfg,
            None,
            None,
            &runner,
            |_| Err(std::env::VarError::NotPresent),
        )
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
        cfg.env.insert("A".to_string(), "op://Personal/a".to_string());
        cfg.env.insert("B".to_string(), "op://Personal/b".to_string());
        let err = resolve_operator_env_with(
            &cfg,
            None,
            None,
            &FailingProbeRunner,
            |_| Err(std::env::VarError::NotPresent),
        )
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

        let err = resolve_operator_env_with(
            &cfg,
            None,
            None,
            &runner,
            |_| Err(std::env::VarError::NotPresent),
        )
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

        let resolved = resolve_operator_env_with(
            &cfg,
            None,
            None,
            &TestOpRunner::forbidden(),
            |name| {
                if name == "MY_HOST_API_KEY" {
                    Ok("host-secret".to_string())
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            },
        )
        .unwrap();

        assert_eq!(resolved.get("API_KEY").map(|v| v.as_str()), Some("host-secret"));
    }
```

- [ ] **Step 6.2: Run tests — confirm they fail**

```bash
cargo nextest run -p jackin operator_env::tests::resolve 2>&1 | tail -20
```

Expected: compile error — `resolve_operator_env_with` does not exist.

- [ ] **Step 6.3: Implement `resolve_operator_env` (and test-only `resolve_operator_env_with`)**

Append to `src/operator_env.rs` after `validate_reserved_names`:

```rust
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
            attributed.insert(k.clone(), (EnvLayer::Agent(agent_name.to_string()), v.clone()));
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
    let uses_op = attributed
        .values()
        .any(|(_, v)| v.starts_with("op://"));
    if uses_op {
        if let Err(e) = op_runner.probe() {
            anyhow::bail!("operator env resolution aborted: {e}");
        }
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
```

Note: `dispatch_value` currently takes `impl FnOnce(&str) -> Result<String, std::env::VarError>` and `&impl OpRunner`. Since it's invoked in a loop above (and since the runner may arrive as a `&dyn OpRunner` trait object), rewrite the signature to use a `FnMut` host-env closure and relax the runner bound to `?Sized`:

```rust
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
```

…and leave the call site `host_env(host_name)` unchanged — just drop the previous `FnOnce` assumption. No other changes required.

- [ ] **Step 6.4: Run tests — confirm green**

```bash
cargo nextest run -p jackin operator_env::tests::resolve \
  operator_env::tests::dispatch
```

Expected: all resolve_* tests and the earlier dispatch tests pass.

- [ ] **Step 6.5: Commit Task 6**

```bash
git add src/operator_env.rs
git commit -s -m "$(cat <<'EOF'
feat(operator_env): add resolve_operator_env that walks all four layers

Introduce resolve_operator_env (and a test-injectable resolve_operator_env_with
that takes custom OpRunner + host_env callbacks) that walks the four env
layers for a given (agent, workspace) pair and dispatches every value:
op:// references via the OpRunner, $NAME / ${NAME} references via the
host env callback, literals returned verbatim.

Attribution: each resolved (or failing) key is tagged with the exact
layer that supplied it, so an aggregated error reports which layer to
fix for each failing key. All resolution failures across the four
layers are collected and reported in a single anyhow error — consistent
with validate_reserved_names from the previous task.

dispatch_value's host_env closure is widened from FnOnce to FnMut so
it can be reused across every key in the merged map without per-key
cloning.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Integrate operator env into `src/runtime/launch.rs`

**Files:**
- Modify: `src/runtime/launch.rs` around lines 608–614 (after manifest env resolution) and lines 447–462 (env injection into `docker run`)

- [ ] **Step 7.1: Write a failing test: the docker run command contains an operator-declared env var**

Add to the `#[cfg(test)] mod tests` block at the end of `src/runtime/launch.rs`:

```rust
    #[test]
    fn load_agent_injects_global_operator_env_literal() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        // Seed a config.toml with a global operator env map.
        std::fs::write(
            &paths.config_file,
            r#"[env]
OPERATOR_SMOKE = "smoke-literal"

[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
        )
        .unwrap();

        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(
            run_cmd.contains("-e OPERATOR_SMOKE=smoke-literal"),
            "docker run must inject operator env; got: {run_cmd}"
        );
    }

    #[test]
    fn load_agent_operator_env_overrides_manifest_env() {
        // Spec: on conflict between manifest-declared env and operator
        // env, operator wins. The manifest below declares OPERATOR_SMOKE
        // as a literal "manifest-default"; the global operator env
        // declares the same key as "operator-wins". The docker run
        // command must inject the operator value.
        //
        // The `[env.OPERATOR_SMOKE]` manifest shape below matches the
        // existing EnvEntry schema in `src/env_model.rs` — if that
        // schema has diverged (e.g. `kind`/`default` field names), the
        // implementer should update the TOML fixture to match the
        // current schema; the test's *assertions* (operator-wins /
        // manifest-default not present) are unchanged.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(
            &paths.config_file,
            r#"[env]
OPERATOR_SMOKE = "operator-wins"

[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
        )
        .unwrap();

        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[env.OPERATOR_SMOKE]
kind = "literal"
default = "manifest-default"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let workspace = repo_workspace(&repo_dir);
        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &LoadOptions::default(),
        )
        .unwrap();

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(
            run_cmd.contains("-e OPERATOR_SMOKE=operator-wins"),
            "operator env must win over manifest env on conflict; got: {run_cmd}"
        );
        assert!(
            !run_cmd.contains("-e OPERATOR_SMOKE=manifest-default"),
            "manifest value must NOT leak when operator overrides it; got: {run_cmd}"
        );
    }

    #[test]
    fn load_agent_injects_host_ref_operator_env() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        // No process-env mutation anywhere — the host env for the
        // resolver is supplied via `LoadOptions::host_env`, a plain
        // `BTreeMap<String, String>`. This keeps the test free of
        // any `std::env` write, which the crate-level
        // `unsafe_code = "forbid"` lint forbids.
        std::fs::write(
            &paths.config_file,
            r#"[env]
FROM_HOST = "$JACKIN_PR2_SMOKE_HOST_VAR"

[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
        )
        .unwrap();

        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut host_env = std::collections::BTreeMap::new();
        host_env.insert(
            "JACKIN_PR2_SMOKE_HOST_VAR".to_string(),
            "from-host-env".to_string(),
        );

        let opts = LoadOptions {
            host_env: Some(host_env),
            ..LoadOptions::default()
        };

        let workspace = repo_workspace(&repo_dir);
        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &opts,
        )
        .unwrap();

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(
            run_cmd.contains("-e FROM_HOST=from-host-env"),
            "host-ref operator env must resolve and inject; got: {run_cmd}"
        );
    }
```

- [ ] **Step 7.2: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin \
  runtime::launch::tests::load_agent_injects_global_operator_env_literal \
  runtime::launch::tests::load_agent_operator_env_overrides_manifest_env \
  runtime::launch::tests::load_agent_injects_host_ref_operator_env
```

Expected: all three fail — `docker run` does not include `OPERATOR_SMOKE` or `FROM_HOST`, and the manifest-default still wins on conflict.

- [ ] **Step 7.2b: Extend `LoadOptions` with two optional injection seams**

Before wiring the resolver into `load_agent`, add the injection seams used by the Task 7 and Task 8 tests. In `src/runtime/launch.rs`, find the existing `LoadOptions` struct and add two fields (keep the existing `debug` field and any others untouched — shown elided here):

```rust
#[derive(Default)]
pub struct LoadOptions {
    pub debug: bool,
    // ... any existing fields ...

    /// Optional test seam: inject a custom `OpRunner` for `op://`
    /// resolution. `None` (the production default) means
    /// `resolve_operator_env` picks the default `OpCli::new()`.
    pub op_runner: Option<Box<dyn crate::operator_env::OpRunner>>,

    /// Optional test seam: inject a host-env lookup map. `None` (the
    /// production default) means `resolve_operator_env` reads from
    /// `std::env::var`. When `Some(map)`, `$NAME` / `${NAME}`
    /// references are resolved by looking up `name` in `map`.
    pub host_env: Option<std::collections::BTreeMap<String, String>>,
}
```

Notes:

- Both fields default to `None` so every existing call site (including `LoadOptions::default()`) keeps working unchanged.
- The `op_runner` field is `Box<dyn OpRunner>` so `OpRunner` needs `+ Send + Sync` bounds if it will ever cross thread boundaries — for this plan it does not (the resolver runs in the launch thread), so a plain trait object suffices.
- The `host_env` map is a `BTreeMap<String, String>` rather than a closure so `LoadOptions` stays `Default` and (optionally) `Clone`-able — the resolver wraps the map in a closure at the call site.

- [ ] **Step 7.3: Resolve operator env after the manifest env resolution**

In `src/runtime/launch.rs`, find the block that resolves manifest env (currently lines 608–614):

```rust
    // Resolve env vars (interactive prompts happen here, before build)
    let resolved_env = if validated_repo.manifest.env.is_empty() {
        crate::env_resolver::ResolvedEnv { vars: vec![] }
    } else {
        let prompter = crate::terminal_prompter::TerminalPrompter;
        crate::env_resolver::resolve_env(&validated_repo.manifest.env, &prompter)?
    };
```

Replace it with:

```rust
    // Resolve env vars (interactive prompts happen here, before build)
    let manifest_resolved = if validated_repo.manifest.env.is_empty() {
        crate::env_resolver::ResolvedEnv { vars: vec![] }
    } else {
        let prompter = crate::terminal_prompter::TerminalPrompter;
        crate::env_resolver::resolve_env(&validated_repo.manifest.env, &prompter)?
    };

    // Resolve operator env layers (global / agent / workspace /
    // workspace × agent). op:// refs shell out to `op`; $NAME refs
    // read the host env. Failures are aggregated into a single error.
    //
    // Workspace name: the launch pipeline does not currently pass a
    // workspace *name* down into load_agent — only a ResolvedWorkspace
    // (mounts + workdir). Look up the name by scanning config.workspaces
    // for the entry whose workdir matches; this matches the same
    // identification rule used by `jackin workspace show`.
    let workspace_name = config
        .workspaces
        .iter()
        .find(|(_, w)| w.workdir == workspace.workdir)
        .map(|(name, _)| name.clone());

    // The operator env resolver takes two injection seams:
    //   * `op_runner`  — resolves `op://...` references (production:
    //     `OpCli::new()`; tests: a mock `OpRunner` constructed directly).
    //   * `host_env`   — resolves `$NAME` / `${NAME}` references
    //     (production: `|name| std::env::var(name).ok()`; tests: a
    //     closure over a `BTreeMap` seeded by the test).
    //
    // Both seams are carried on `LoadOptions` as optional fields. When
    // unset (the production default), `resolve_operator_env` is called,
    // which wires in the real `OpCli` and the real host env. When set
    // (tests only), `resolve_operator_env_with` is called with the
    // supplied seams, so tests never need to mutate `std::env` and the
    // crate-level `unsafe_code = "forbid"` lint stays intact.
    let operator_env = match (&opts.op_runner, &opts.host_env) {
        (None, None) => crate::operator_env::resolve_operator_env(
            config,
            Some(&selector.key()),
            workspace_name.as_deref(),
        )?,
        _ => {
            let default_runner = crate::operator_env::OpCli::new();
            let runner: &dyn crate::operator_env::OpRunner =
                opts.op_runner.as_deref().unwrap_or(&default_runner);
            let host_env_fn = |name: &str| -> Result<String, std::env::VarError> {
                match &opts.host_env {
                    Some(map) => map
                        .get(name)
                        .cloned()
                        .ok_or(std::env::VarError::NotPresent),
                    None => std::env::var(name),
                }
            };
            crate::operator_env::resolve_operator_env_with(
                config,
                Some(&selector.key()),
                workspace_name.as_deref(),
                runner,
                host_env_fn,
            )?
        }
    };

    // Overlay the operator env map on top of the manifest env: operator
    // wins on conflicts (so a workspace-scoped `OPERATOR_TOKEN` overrides
    // a manifest default, which is the whole point of letting operators
    // supply env at launch time). Reserved names are filtered out in
    // the docker-run construction below.
    let mut merged_vars: Vec<(String, String)> = manifest_resolved.vars.clone();
    for (k, v) in &operator_env {
        if let Some(slot) = merged_vars.iter_mut().find(|(mk, _)| mk == k) {
            slot.1 = v.clone();
        } else {
            merged_vars.push((k.clone(), v.clone()));
        }
    }
    let resolved_env = crate::env_resolver::ResolvedEnv { vars: merged_vars };

    // Launch-time diagnostic: emit a single compact line summarising
    // the operator env that will be injected. In normal mode we show
    // counts only ("3 refs resolved"); in --debug mode we show each
    // key → layer/reference kind ("OPERATOR_TOKEN: op://Personal/...
    // from workspace \"big-monorepo\"") — never values.
    if !operator_env.is_empty() {
        crate::operator_env::print_launch_diagnostic(
            config,
            Some(&selector.key()),
            workspace_name.as_deref(),
            &operator_env,
            opts.debug,
        );
    }
```

- [ ] **Step 7.4: Add the `print_launch_diagnostic` helper to `operator_env`**

Append to `src/operator_env.rs`, after `resolve_operator_env_with`:

```rust
/// Emit a single-line (normal mode) or multi-line (debug mode) launch
/// diagnostic summarising the operator env that was just resolved.
/// Values are NEVER printed — only counts (normal) or reference strings
/// (debug) and the layer that supplied each key.
///
/// Normal mode format:
///
/// ```text
/// [jackin] operator env: 3 resolved (2 op://, 1 host ref, 0 literal)
/// ```
///
/// Debug mode format:
///
/// ```text
/// [jackin] operator env:
///   OPERATOR_TOKEN        op://Personal/api/token   (workspace "big-monorepo" → agent "agent-smith" [env])
///   CI_CACHE_DIR          ${HOME_CACHE}             (global [env])
///   AGENT_VERSION         literal                   (agent "agent-smith" [env])
/// ```
pub fn print_launch_diagnostic(
    config: &crate::config::AppConfig,
    agent_selector: Option<&str>,
    workspace_name: Option<&str>,
    resolved: &std::collections::BTreeMap<String, String>,
    debug: bool,
) {
    // Rebuild the (key → (layer, raw_value)) attribution using the same
    // precedence rule as resolve_operator_env_with.
    let empty = std::collections::BTreeMap::new();
    let ws_opt = workspace_name.and_then(|w| config.workspaces.get(w));

    let mut attributed: std::collections::BTreeMap<String, (EnvLayer, String)> =
        std::collections::BTreeMap::new();

    for (k, v) in &config.env {
        attributed.insert(k.clone(), (EnvLayer::Global, v.clone()));
    }
    if let Some(agent_name) = agent_selector
        && let Some(a) = config.agents.get(agent_name)
    {
        for (k, v) in &a.env {
            attributed.insert(k.clone(), (EnvLayer::Agent(agent_name.to_string()), v.clone()));
        }
    }
    if let (Some(ws_name), Some(ws)) = (workspace_name, ws_opt) {
        for (k, v) in &ws.env {
            attributed.insert(
                k.clone(),
                (EnvLayer::Workspace(ws_name.to_string()), v.clone()),
            );
        }
        if let Some(agent_name) = agent_selector
            && let Some(ov) = ws.agents.get(agent_name)
        {
            for (k, v) in &ov.env {
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
        // Workspace fields used via &ws only above — drop
        let _ = ws_opt;
    }

    // Restrict to keys actually in `resolved` (they were successfully
    // dispatched); a key missing from `resolved` indicates a prior
    // error path and should not show up here.
    attributed.retain(|k, _| resolved.contains_key(k));

    if debug {
        eprintln!("[jackin] operator env:");
        // Compute a column width for nice alignment.
        let key_width = attributed.keys().map(|k| k.len()).max().unwrap_or(0).min(40);
        let raw_width = attributed
            .values()
            .map(|(_, v)| classify_value(v).len())
            .max()
            .unwrap_or(0)
            .min(40);
        for (key, (layer, raw_value)) in &attributed {
            let kind = classify_value(raw_value);
            eprintln!(
                "  {:kw$}  {:rw$}  ({})",
                key,
                kind,
                layer,
                kw = key_width,
                rw = raw_width
            );
        }
        return;
    }

    let (mut op_count, mut host_count, mut literal_count) = (0u32, 0u32, 0u32);
    for (_, raw) in attributed.values() {
        match ValueKind::of(raw) {
            ValueKind::Op => op_count += 1,
            ValueKind::Host => host_count += 1,
            ValueKind::Literal => literal_count += 1,
        }
    }
    eprintln!(
        "[jackin] operator env: {} resolved ({} op://, {} host ref, {} literal)",
        attributed.len(),
        op_count,
        host_count,
        literal_count
    );
}

#[derive(Debug, Clone, Copy)]
enum ValueKind {
    Op,
    Host,
    Literal,
}

impl ValueKind {
    fn of(raw: &str) -> Self {
        if raw.starts_with("op://") {
            Self::Op
        } else if parse_host_ref(raw).is_some() {
            Self::Host
        } else {
            Self::Literal
        }
    }
}

/// Return a short, value-free label for a raw operator env entry:
/// `op://...` references are returned verbatim (the reference string
/// is not secret; only the resolved value is); `$NAME` / `${NAME}`
/// references are returned verbatim; literals are labelled `"literal"`
/// so the resolved value is never printed.
fn classify_value(raw: &str) -> String {
    match ValueKind::of(raw) {
        ValueKind::Op | ValueKind::Host => raw.to_string(),
        ValueKind::Literal => "literal".to_string(),
    }
}
```

- [ ] **Step 7.5: Refactor `print_launch_diagnostic` to a writer-based formatter and add value-leak tests**

The diagnostic currently uses `eprintln!` directly, which is hard to assert on. Refactor to a writer-based formatter with a test-only String wrapper; then add unit tests that verify values never appear in the output under either mode.

First, replace the body of `print_launch_diagnostic` from Step 7.4 with:

```rust
pub fn print_launch_diagnostic(
    config: &crate::config::AppConfig,
    agent_selector: Option<&str>,
    workspace_name: Option<&str>,
    resolved: &std::collections::BTreeMap<String, String>,
    debug: bool,
) {
    let mut out = Vec::new();
    // write_launch_diagnostic writes into an in-memory buffer and
    // cannot fail with an I/O error; unwrap is safe here.
    write_launch_diagnostic(
        &mut out,
        config,
        agent_selector,
        workspace_name,
        resolved,
        debug,
    )
    .expect("writing to Vec<u8> is infallible");
    use std::io::Write;
    let _ = std::io::stderr().write_all(&out);
}

/// Test-visible entry point that returns the diagnostic as a String.
/// Production code uses [`print_launch_diagnostic`], which writes the
/// same bytes to process stderr.
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
    // Lift the body of print_launch_diagnostic (Step 7.4) verbatim
    // here, replacing each `eprintln!(...)` with `writeln!(w, ...)?`.
    // Keys, counts, layer attribution, and column-alignment logic
    // are all unchanged — only the output sink is routed through
    // the supplied writer.
    //
    // The mechanical diff, applied to the existing body:
    //
    //   -    eprintln!("[jackin] operator env:");
    //   +    writeln!(w, "[jackin] operator env:")?;
    //
    //   -    eprintln!(
    //   -        "  {:kw$}  {:rw$}  ({})",
    //   -        key, kind, layer, kw = key_width, rw = raw_width
    //   -    );
    //   +    writeln!(
    //   +        w,
    //   +        "  {:kw$}  {:rw$}  ({})",
    //   +        key, kind, layer, kw = key_width, rw = raw_width
    //   +    )?;
    //
    //   -    eprintln!(
    //   -        "[jackin] operator env: {} resolved (...)",
    //   -        attributed.len(), op_count, host_count, literal_count
    //   -    );
    //   +    writeln!(
    //   +        w,
    //   +        "[jackin] operator env: {} resolved (...)",
    //   +        attributed.len(), op_count, host_count, literal_count
    //   +    )?;
    //
    // The function returns `Ok(())` at the end.
    let _ = (w, config, agent_selector, workspace_name, resolved, debug);
    Ok(())
}
```

Then append the following tests to the `#[cfg(test)] mod tests` block in `src/operator_env.rs`:

```rust
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

        let rendered =
            format_launch_diagnostic_for_test(&cfg, None, None, &resolved, false);

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

        let rendered =
            format_launch_diagnostic_for_test(&cfg, None, None, &resolved, true);

        // Debug mode emits references (reference string is config,
        // not secret) and the "literal" label — never the resolved
        // value.
        assert!(rendered.contains("op://Personal/item/field"), "{rendered}");
        assert!(rendered.contains("literal"), "{rendered}");
        assert!(!rendered.contains("super-secret"), "{rendered}");
        assert!(!rendered.contains("op-value-secret"), "{rendered}");
    }
```

- [ ] **Step 7.6: Keep the `docker run -e` env injection block untouched**

The existing block at lines 447–462 (`let mut env_strings: Vec<String> = Vec::new(); ...`) already iterates over `resolved_env.vars` and skips names that match `is_reserved`. Because we've merged the operator map into `resolved_env.vars` (Step 7.3), no changes are needed here. Double-check by re-reading the block.

- [ ] **Step 7.7: Run the Task 7 tests — confirm green**

```bash
cargo nextest run -p jackin \
  runtime::launch::tests::load_agent_injects_global_operator_env_literal \
  runtime::launch::tests::load_agent_operator_env_overrides_manifest_env \
  runtime::launch::tests::load_agent_injects_host_ref_operator_env \
  operator_env::tests::launch_diagnostic_normal_mode_prints_counts_only_no_values \
  operator_env::tests::launch_diagnostic_debug_mode_prints_references_but_not_values
```

Expected: all five tests pass.

- [ ] **Step 7.8: Full suite**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: all green, zero warnings.

- [ ] **Step 7.9: Commit Task 7**

```bash
git add src/runtime/launch.rs src/operator_env.rs
git commit -s -m "$(cat <<'EOF'
feat(runtime): inject resolved operator env into docker run; print diagnostic

Integrate operator_env into the agent launch path. After manifest env
resolution, walk the four operator env layers, overlay the result on
top of the manifest-resolved vars (operator wins on conflicts), and let
the existing docker run -e construction loop inject everything. Reserved
names are already filtered there via env_model::is_reserved, so no
change to that block.

Emit a single-line (normal) / multi-line (debug) launch diagnostic that
reports key counts by reference kind (or per-key reference strings and
layer attribution in debug mode). Values are never printed — only
op://... and $NAME reference strings, which are not secret themselves.

Workspace name is inferred from the ResolvedWorkspace by finding the
config entry whose workdir matches. If no workspace entry matches, the
workspace layer and workspace×agent layer contribute nothing.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: End-to-end integration test with the `op` CLI mocked via `LoadOptions::op_runner`

**Files:**
- Modify: `src/runtime/launch.rs` (add one more test that exercises `op://` through the mocked CLI)

- [ ] **Step 8.1: Write a failing test that uses a tempfile fake `op` binary injected via `LoadOptions::op_runner`**

Add to `src/runtime/launch.rs` `#[cfg(test)] mod tests`:

```rust
    #[cfg(unix)]
    #[test]
    fn load_agent_injects_op_cli_resolved_value() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        let bin_dir = temp.path().join("fake-bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin_path = bin_dir.join("op");
        std::fs::write(
            &bin_path,
            "#!/bin/sh\nif [ \"$1\" = \"read\" ] && [ \"$2\" = \"op://Personal/api/token\" ]; then echo -n 'resolved-op-token'; exit 0; fi\nexit 99\n",
        )
        .unwrap();
        let mut perms = std::fs::metadata(&bin_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&bin_path, perms).unwrap();

        std::fs::write(
            &paths.config_file,
            r#"[env]
OPERATOR_TOKEN = "op://Personal/api/token"

[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
        )
        .unwrap();

        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::for_load_agent([
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        // Inject the fake `op` binary path via `LoadOptions::op_runner`.
        // No process env mutation — `OpCli::with_binary` takes the path
        // as a direct argument, so the `unsafe_code = "forbid"`
        // crate-level lint stays intact and sibling tests running in
        // parallel via cargo-nextest cannot race on any shared env var.
        let op_runner: Box<dyn crate::operator_env::OpRunner> = Box::new(
            crate::operator_env::OpCli::with_binary(bin_path.to_string_lossy().to_string()),
        );
        let opts = LoadOptions {
            op_runner: Some(op_runner),
            ..LoadOptions::default()
        };

        let workspace = repo_workspace(&repo_dir);
        load_agent(
            &paths,
            &mut config,
            &selector,
            &workspace,
            &mut runner,
            &opts,
        )
        .unwrap();

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -d -it"))
            .unwrap();
        assert!(
            run_cmd.contains("-e OPERATOR_TOKEN=resolved-op-token"),
            "op:// ref must resolve via the injected OpCli and inject; got: {run_cmd}"
        );
    }
```

- [ ] **Step 8.2: Run the new test — confirm it already passes (Task 7 wired up the integration)**

```bash
cargo nextest run -p jackin runtime::launch::tests::load_agent_injects_op_cli_resolved_value
```

Expected: passes. If it does not, verify that `LoadOptions::op_runner` is plumbed into `resolve_operator_env_with` at the `load_agent` call site (Step 7.3). Because the injected `OpRunner` is carried on the per-call `LoadOptions` (no shared state, no process env mutation), this test is race-free against sibling tests that also exercise the launch path in parallel under `cargo-nextest`.

```bash
cargo nextest run -p jackin \
  runtime::launch::tests::load_agent_injects_op_cli_resolved_value \
  runtime::launch::tests::load_agent_injects_host_ref_operator_env \
  runtime::launch::tests::load_agent_injects_global_operator_env_literal
```

Expected: all three pass.

- [ ] **Step 8.3: Commit Task 8**

```bash
git add src/runtime/launch.rs
git commit -s -m "$(cat <<'EOF'
test(runtime): end-to-end op:// resolution via injected OpCli runner

Cover the full launch path with a tempfile-backed fake `op` binary
injected into `load_agent` via `LoadOptions::op_runner` (a
`Box<dyn OpRunner>`). Verifies that op://... values in the global
operator env are resolved via the injected CLI and injected into the
docker run command with the resolved secret.

The runner is passed as a direct dependency on `LoadOptions` rather
than through a `std::env` side channel, so the test is free of any
process-env mutation, the crate-level `unsafe_code = "forbid"` lint
stays intact, and parallel test runs under cargo-nextest cannot race.

This is the full integration complement to the unit-level tests of
OpCli::read in Task 2.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Documentation — new environment-variables guide + cross-links + config reference

**Files:**
- Create: `docs/src/content/docs/guides/environment-variables.mdx`
- Modify: `docs/src/content/docs/reference/configuration.mdx`
- Modify: `docs/src/content/docs/guides/authentication.mdx`
- Modify: `docs/src/content/docs/reference/roadmap/onepassword-integration.mdx`
- Modify: `docs/astro.config.ts`

- [ ] **Step 9.1: Create `docs/src/content/docs/guides/environment-variables.mdx`**

```mdx
---
title: Environment Variables
description: Operator-controlled env layers, 1Password integration, and host-env forwarding for agent containers
---
import { Aside } from '@astrojs/starlight/components'

jackin' lets operators declare environment variables in four config layers that are merged at launch and injected into the agent container via `docker run -e`. Values can be literal strings, references to host env vars, or references to 1Password items resolved through the `op` CLI.

## Why operator env

Agent manifests (`jackin.agent.toml`) declare the env *shape* — what keys an agent expects, what's interactive, what has a default. Operator env layers declare the *values* for a specific operator on a specific workspace with a specific agent. Keeping shape and values in different places means:

- Third-party agents never see your secrets in their git history.
- The same agent can run with different credentials in different workspaces (personal laptop vs company monorepo).
- You can override a manifest default from `config.toml` without forking the agent.

## Layers

Four layers are merged with later-wins semantics:

1. **Global** — `[env]` at the top of `~/.config/jackin/config.toml`.
2. **Agent** — `[agents.<selector>.env]`.
3. **Workspace** — `[workspaces.<name>.env]`.
4. **Workspace × Agent** — `[workspaces.<name>.agents.<selector>.env]`.

Keys present in multiple layers take the value from the highest-priority layer (layer 4 > layer 3 > layer 2 > layer 1). Keys unique to any layer are preserved.

## Value syntax

Each value in an env map is one of:

| Syntax | Resolution |
|---|---|
| `op://VAULT/ITEM/FIELD` | Resolved via the [1Password CLI](https://developer.1password.com/docs/cli/) (`op read "<ref>"`). Requires `op` on `PATH` and an authenticated session. |
| `$NAME` or `${NAME}` | Read from the host's environment at launch time (`std::env::var(NAME)`). Errors if the host var is unset. |
| Anything else | Literal string. |

## Example

```toml
# ~/.config/jackin/config.toml

# Global — applies to every agent launch
[env]
OPERATOR_ORG = "acme-corp"

# Agent layer — only when loading agent-smith
[agents.agent-smith.env]
API_TOKEN = "op://Personal/acme-api/token"

# Workspace layer — only when launching in "big-monorepo"
[workspaces.big-monorepo.env]
ARTIFACT_REGISTRY = "${COMPANY_REGISTRY_URL}"

# Workspace × Agent — most specific, wins on conflict
[workspaces.big-monorepo.agents.agent-smith.env]
API_TOKEN = "op://Work/shared-smith/token"
```

When `jackin load agent-smith` is invoked in `big-monorepo`:

- `OPERATOR_ORG = "acme-corp"` (literal; from global)
- `API_TOKEN = <resolved via op read "op://Work/shared-smith/token">` (workspace × agent wins over agent layer)
- `ARTIFACT_REGISTRY = <value of $COMPANY_REGISTRY_URL on host>` (from workspace layer)

## Reserved names

Five runtime-owned names are reserved and rejected at config load. Declaring them at any layer is a hard error:

- `JACKIN_CLAUDE_ENV`
- `JACKIN_DIND_HOSTNAME`
- `DOCKER_HOST`
- `DOCKER_TLS_VERIFY`
- `DOCKER_CERT_PATH`

<Aside type="note">
Reserved-name enforcement runs at config LOAD, not at launch. If your `config.toml` declares a reserved name, any `jackin` subcommand (including `config show`) will fail with a message pointing at the offending layer.
</Aside>

## 1Password CLI setup

1. Install the [1Password CLI](https://developer.1password.com/docs/cli/get-started/).
2. Sign in: `eval $(op signin)` (or unlock the desktop-app integration).
3. Declare `op://...` references in your config.

Each `jackin load` that touches `op://...` values shells out to `op read <ref>` per key. `op` failures (missing item, expired session, binary not on PATH) surface as aggregated resolution errors — all failing keys are reported in a single message. `op` subprocesses have a 30-second timeout per call, and stderr is truncated at 4 KiB in error messages.

## Launch diagnostic

jackin' prints a compact diagnostic line on launch when operator env is non-empty:

```
[jackin] operator env: 3 resolved (2 op://, 1 host ref, 0 literal)
```

In `--debug` mode the diagnostic expands to per-key reference strings and layer attribution:

```
[jackin] operator env:
  API_TOKEN      op://Work/shared-smith/token  (workspace "big-monorepo" → agent "agent-smith" [env])
  OPERATOR_ORG   literal                        (global [env])
  REGISTRY       ${COMPANY_REGISTRY_URL}        (workspace "big-monorepo" [env])
```

Resolved values are never printed — only `op://` references (which are not secret themselves), host-var references, or the label `"literal"`.

## Interaction with manifest env

Manifest-declared env (`jackin.agent.toml`'s `[env]` table) is resolved first (with interactive prompts as needed). Operator env is resolved second and overlaid on top: **operator wins on key conflict**. This lets operators pin a value that the manifest would otherwise prompt for, or swap a manifest default for a workspace-specific secret.

See [Authentication Forwarding](/guides/authentication) for the companion mechanism that forwards Claude Code credentials from host to container. Env layers and auth forwarding are orthogonal — use both.
```

- [ ] **Step 9.2: Register the new page in the sidebar**

Open `docs/astro.config.ts` and find the `sidebar:` array. Under the `Guides` section (which already contains `agent-repos`, `authentication`, `mounts`, etc.), insert a new entry for the environment-variables guide alphabetically:

```ts
{ slug: 'guides/environment-variables' },
```

If the sidebar uses explicit `label` entries, match the existing style (e.g. `{ label: 'Environment Variables', slug: 'guides/environment-variables' }`). Adapt to the file's actual shape.

- [ ] **Step 9.3: Update `configuration.mdx` to document the env layers**

Open `docs/src/content/docs/reference/configuration.mdx`. After the `### Claude Code settings` section (ending around line 36), insert a new section:

```mdx
### Operator env layers

Four layers contribute env vars to the agent container, merged with later-wins semantics:

```toml
# Layer 1 — global
[env]
OPERATOR_ORG = "acme-corp"

# Layer 2 — per agent
[agents.agent-smith.env]
API_TOKEN = "op://Personal/api/token"

# Layer 3 — per workspace
[workspaces.big-monorepo.env]
REGISTRY = "${COMPANY_REGISTRY_URL}"

# Layer 4 — per (workspace, agent)
[workspaces.big-monorepo.agents.agent-smith.env]
API_TOKEN = "op://Work/shared-smith/token"
```

Values can be:

- `op://VAULT/ITEM/FIELD` — resolved via the 1Password CLI (`op read`)
- `$NAME` or `${NAME}` — read from the host env
- Anything else — literal

Reserved runtime names (`JACKIN_CLAUDE_ENV`, `JACKIN_DIND_HOSTNAME`, `DOCKER_HOST`, `DOCKER_TLS_VERIFY`, `DOCKER_CERT_PATH`) are rejected at config load.

See [Environment Variables](/guides/environment-variables) for the full guide.
```

- [ ] **Step 9.4: Add a cross-link from `authentication.mdx`**

Open `docs/src/content/docs/guides/authentication.mdx`. At the very end of the file (after the Troubleshooting section), append:

```mdx

## See also

- [Environment Variables](/guides/environment-variables) — forward operator-owned secrets (API tokens, host env vars, 1Password items) into the agent container. Complements (but does not replace) Claude Code auth forwarding.
```

- [ ] **Step 9.5: Update the 1Password roadmap status**

Open `docs/src/content/docs/reference/roadmap/onepassword-integration.mdx`. Replace the `**Status**: Deferred — needs design work` line with:

```mdx
**Status**: Partially implemented — option 2 (workspace-managed env references) shipped; file-mount generation (option 4) still deferred
```

After the `## Options` section, insert a new `## Current Status` section:

```mdx
## Current Status

Option 2 — workspace-managed secret references — is implemented for environment variables. Operators can declare `op://VAULT/ITEM/FIELD` references in `config.toml` at four layers (global, per-agent, per-workspace, per-(workspace × agent)), and jackin resolves them via the `op` CLI at launch and injects them via `docker run -e`. See the [Environment Variables](/guides/environment-variables) guide for the full operator-facing documentation.

Option 4 — read-only secret mount generation — remains deferred. Agents that need secrets as files (e.g. SSH keys, kubeconfigs) still require ad-hoc mounts. A future iteration may add `[workspaces.*.op_files]` or similar.
```

- [ ] **Step 9.6: Verify the docs site builds**

```bash
cd docs
bun install --frozen-lockfile
bun run build 2>&1 | tail -20
cd ..
```

Expected: build succeeds. If `bun install` is slow on first run, node_modules cache will speed up subsequent builds.

- [ ] **Step 9.7: Commit Task 9**

```bash
git add docs/src/content/docs/guides/environment-variables.mdx \
  docs/src/content/docs/reference/configuration.mdx \
  docs/src/content/docs/guides/authentication.mdx \
  docs/src/content/docs/reference/roadmap/onepassword-integration.mdx \
  docs/astro.config.ts
git commit -s -m "$(cat <<'EOF'
docs(env): add Environment Variables guide; document four-layer operator env

New /guides/environment-variables page covers the four operator env
layers, the three value syntaxes (op://, $NAME / ${NAME}, literal),
reserved-name enforcement, the 1Password CLI setup, and the normal /
debug launch diagnostic.

Configuration reference gains an "Operator env layers" section that
links to the new guide. Authentication guide gains a See also cross-
link — env forwarding complements but does not replace Claude Code
auth forwarding. 1Password roadmap status flips to "partially
implemented" (option 2 shipped; option 4 file mounts still deferred).

Sidebar registration added in astro.config.ts.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: CHANGELOG entry

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 10.1: Read the existing CHANGELOG to match style**

```bash
head -30 CHANGELOG.md
```

- [ ] **Step 10.2: Add an `Added` entry under `## [Unreleased]`**

Append after the `## [Unreleased]` header:

```markdown
### Added

- Operator-controlled environment variables in `config.toml`. Four layers are merged with later-wins semantics at launch: global `[env]`, per-agent `[agents.<selector>.env]`, per-workspace `[workspaces.<name>.env]`, and per-(workspace × agent) `[workspaces.<name>.agents.<selector>.env]`. Values are either literal strings, host env references (`$NAME` / `${NAME}`), or 1Password references (`op://VAULT/ITEM/FIELD`) resolved via the `op` CLI. Reserved runtime names (`JACKIN_CLAUDE_ENV`, `JACKIN_DIND_HOSTNAME`, `DOCKER_HOST`, `DOCKER_TLS_VERIFY`, `DOCKER_CERT_PATH`) are rejected at config load. Resolved values are injected via `docker run -e` on top of manifest-declared env (operator wins on conflicts). See the new [Environment Variables](guides/environment-variables) guide. [#<pr-number>]
```

Leave `<pr-number>` as a literal placeholder; it will be filled in after the PR is opened.

- [ ] **Step 10.3: Commit Task 10**

```bash
git add CHANGELOG.md
git commit -s -m "$(cat <<'EOF'
docs(changelog): record operator env layers + 1Password op:// integration

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: Final verification

- [ ] **Step 11.1: Full pre-commit gate**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: all three exit 0, zero warnings, zero failures. If clippy flags anything, fix it in a separate `style:` commit on this branch (do not amend prior task commits).

- [ ] **Step 11.2: Manual smoke — reserved-name rejection**

```bash
mkdir -p /tmp/jackin-env-test
cat > /tmp/jackin-env-test/config.toml <<'EOF'
[env]
DOCKER_HOST = "whatever"
EOF
JACKIN_CONFIG=/tmp/jackin-env-test/config.toml cargo run -- config show 2>&1 | head -20
```

Expected: fails at load with an error mentioning `DOCKER_HOST` and `global [env]` and `reserved`. If `JACKIN_CONFIG` is not supported, back up `~/.config/jackin/config.toml`, place the offending config there, run `cargo run -- config show`, and restore the backup.

- [ ] **Step 11.3: Manual smoke — literal env injected**

Clean the prior config and replace with a literal-only entry:

```bash
cat > /tmp/jackin-env-test/config.toml <<'EOF'
[env]
OPERATOR_SMOKE = "smoke-value"
EOF
JACKIN_CONFIG=/tmp/jackin-env-test/config.toml cargo run -- config show 2>&1 | grep -i operator_smoke
```

Expected: the config-show output includes `OPERATOR_SMOKE`.

- [ ] **Step 11.4: Manual smoke — `op://` ref with the real `op` CLI (skip if not installed locally)**

```bash
which op || echo "op CLI not installed — skipping live op:// smoke"
```

If `op` is installed and an authenticated session exists, create a temporary item and test that `jackin load` resolves it. If not installed, this step is a no-op and the Task 8 integration test (which injects a tempfile-backed fake `op` via `LoadOptions::op_runner`) is the definitive coverage.

- [ ] **Step 11.5: Verify commit log is clean and DCO-signed**

```bash
git log main..HEAD --oneline
git log main..HEAD --format="%B" | grep -c "Signed-off-by"
git log main..HEAD --format="%B" | grep -c "Co-authored-by: Claude"
```

Expected: 10 commits (one per task from 1–10). `Signed-off-by` count = 10. `Co-authored-by: Claude` count = 10. If a style-fix commit was needed in Step 11.1, expected count becomes 11.

- [ ] **Step 11.6: Push and open the PR**

```bash
git push -u origin feature/workspace-env-resolver
gh pr create --title "feat(config): operator env layers with op:// and host-env resolution" --body "$(cat <<'BODY'
## Summary

Adds operator-controlled environment variables to `config.toml` as four merged layers (global → per-agent → per-workspace → per-(workspace × agent)), resolved via three value syntaxes (`op://...`, `$NAME` / `${NAME}`, literal), and injected into agent containers via `docker run -e` on top of manifest-declared env. Reserved runtime names are rejected at config load so misconfigurations fail fast.

- New `src/operator_env.rs` module: schema, dispatch, layer merging, `OpCli` runner with bounded stderr + 30s timeout, reserved-name enforcement.
- Four new schema fields: `AppConfig::env`, `AgentSource::env`, `WorkspaceConfig::env`, and `WorkspaceConfig::agents: BTreeMap<String, WorkspaceAgentOverride>` (new named type for future extensibility).
- Integration into `src/runtime/launch.rs`: manifest env + operator env merged with operator-wins semantics; single-line (normal) / multi-line (debug) launch diagnostic — values never printed.
- Test seam: integration tests inject a custom `OpRunner` (constructed via `OpCli::with_binary` against a tempfile-backed fake binary) through `LoadOptions::op_runner`, and inject host env via `LoadOptions::host_env` — no process-env mutation, so the crate-level `unsafe_code = "forbid"` lint is preserved.
- Docs: new `/guides/environment-variables` page, updates to `configuration.mdx` and `authentication.mdx`, status update on the 1Password roadmap (option 2 shipped).
- Subprocess timeout handling uses `std::process::Child::kill()` (documented to send `SIGKILL` on Unix), so no `unsafe` code, no `libc` dependency, and the crate-level `unsafe_code = "forbid"` lint is preserved.

Delivers PR 2 of the three-PR workspace-env-resolver series. Spec: `docs/superpowers/specs/2026-04-23-workspace-env-resolver-design.md`. Plan: `docs/superpowers/plans/2026-04-23-workspace-env-resolver.md`.

## Test plan

- [x] `cargo fmt -- --check && cargo clippy && cargo nextest run` — all green, zero warnings.
- [x] Unit tests: `operator_env::tests::dispatch_*`, `operator_env::tests::op_cli_*`, `operator_env::tests::merge_*`, `operator_env::tests::validate_reserved_names_*`, `operator_env::tests::resolve_*`.
- [x] Integration tests: `runtime::launch::tests::load_agent_injects_global_operator_env_literal`, `..._host_ref_operator_env`, `..._op_cli_resolved_value` (injects a tempfile-backed fake `op` via `LoadOptions::op_runner`; host env via `LoadOptions::host_env`).
- [x] Manual smoke: `DOCKER_HOST` in `[env]` rejected at load with layer-attributed error.
- [x] Manual smoke: literal `OPERATOR_SMOKE` surfaces through to docker run.
- [ ] Reviewer confirms the operator-facing guide is accurate (`/guides/environment-variables`).
- [ ] Reviewer confirms the diagnostic output is value-free.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
BODY
)"
```

Return the PR URL in full; **do not merge** — per `AGENTS.md`, agents must never merge a PR without explicit per-PR operator confirmation.

---

## Self-Review Checklist (for the implementer)

Before marking this plan complete:

- [ ] No remaining `TODO` / `unimplemented!()` in `src/operator_env.rs`
- [ ] All four env layers are writable and readable via `config.toml` round-trip (grep: `rg 'env:' src/config/mod.rs src/workspace/mod.rs`)
- [ ] No process-env writes or `unsafe` blocks anywhere in `src/` or `tests/` (grep: `rg -e 'set[_]var' -e 'unsafe[[:space:]]*\{' src/ tests/` returns zero hits); the operator-env test seams live on `LoadOptions::op_runner` and `LoadOptions::host_env`, both injected per call
- [ ] Launch diagnostic never prints a resolved value — only `op://` references, `$NAME` references, or the label `"literal"` (grep the `print_launch_diagnostic` body against `resolved.get`)
- [ ] `CHANGELOG.md` PR-number placeholder is replaced with the actual PR number after the PR is opened
- [ ] All 10 commits carry both `Signed-off-by:` and `Co-authored-by: Claude <noreply@anthropic.com>`
- [ ] Pre-commit gate is clean after every task's final step, not just the final verification task
- [ ] Manual smokes from Step 11.2 and 11.3 succeeded on the implementer's machine
- [ ] `Cargo.toml`'s `unsafe_code = "forbid"` lint remains unchanged — subprocess timeout uses `Child::kill()`, not a `libc` extern (`grep -n 'unsafe_code' Cargo.toml` returns `unsafe_code = "forbid"` and nothing else)
- [ ] `WorkspaceAgentOverride` uses `deny_unknown_fields` so future schema additions surface typos instead of silently ignoring them
- [ ] The reserved-name validator runs on `jackin config show`, not just `jackin load`, because it fires at `AppConfig::load_or_init` — verified by Step 11.2
