//! Fake TUI/CLI process harness: script-driven fake binaries.
//!
//! [`FakeProcessHarness`] materializes tiny `sh` scripts that behave like
//! version/usage/account subcommand CLIs, dispatching on `argv` to canned
//! stdout/stderr/exit codes. Every spawn appends its `argv` and environment
//! to a per-binary log, retrievable via [`FakeBinary::invocations`] for
//! assertions such as "no secrets leaked into argv/env".
//!
//! Standard library only. Fake binaries are POSIX `sh` scripts (Unix
//! executable bit); on non-Unix targets spawn them via
//! [`FakeBinary::command`], which falls back to `sh <script>`.
//!
//! # Secrets
//!
//! [`Invocation::assert_no_secrets`] panics (with a redacted message) when a
//! forbidden value appears in captured `argv` or env. Pair it with
//! [`redact_secrets`](crate::fixture_http::redact_secrets) when logging.
//!
//! # Example
//!
//! ```no_run
//! use jackin_test_support::fixture_process::{FakeProcessHarness, ProcessScript};
//!
//! let harness = FakeProcessHarness::new()?;
//! let script = ProcessScript::new()
//!     .on_exact(["--version"], "fake-tui 1.2.3\n", "", 0)
//!     .on_prefix(["account"], "accounts...\n", "", 0);
//! let fake = harness.binary("fake-tui", &script)?;
//! let out = fake.command().arg("--version").output()?;
//! assert!(String::from_utf8_lossy(&out.stdout).contains("1.2.3"));
//! for invocation in fake.invocations()? {
//!     invocation.assert_no_secrets(&["real-access-token"]);
//! }
//! # Ok::<(), std::io::Error>(())
//! ```

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// How an [`Entry`] matches a spawned command line.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MatchKind {
    /// `argv[1..]` equals these args exactly (`$#` + per-position `=`).
    Exact(Vec<String>),
    /// `argv[1..]` starts with these args (extra trailing args allowed).
    Prefix(Vec<String>),
}

/// One scripted `argv` → output mapping inside a [`ProcessScript`].
#[derive(Debug, Clone)]
struct Entry {
    kind: MatchKind,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: i32,
}

/// Script describing a fake binary's behavior.
#[derive(Debug, Clone, Default)]
pub struct ProcessScript {
    entries: Vec<Entry>,
    default_stdout: Vec<u8>,
    default_stderr: Vec<u8>,
    default_exit_code: i32,
}

impl ProcessScript {
    /// Empty script: every invocation falls through to the default
    /// (empty output, exit 0) until entries are added.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Match when `argv[1..]` equals `args` exactly.
    #[must_use]
    pub fn on_exact<const N: usize>(
        mut self,
        args: [&str; N],
        stdout: &str,
        stderr: &str,
        exit_code: i32,
    ) -> Self {
        self.entries.push(Entry {
            kind: MatchKind::Exact(args.iter().map(ToString::to_string).collect()),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            exit_code,
        });
        self
    }

    /// Match when `argv[1..]` starts with `args`
    /// (for subcommands with trailing flags, e.g. `account list --json`).
    /// An empty `args` matches every invocation.
    #[must_use]
    pub fn on_prefix<const N: usize>(
        mut self,
        args: [&str; N],
        stdout: &str,
        stderr: &str,
        exit_code: i32,
    ) -> Self {
        self.entries.push(Entry {
            kind: MatchKind::Prefix(args.iter().map(ToString::to_string).collect()),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            exit_code,
        });
        self
    }

    /// Fallback output for unmatched invocations (default: empty, exit 0).
    #[must_use]
    pub fn with_default(mut self, stdout: &str, stderr: &str, exit_code: i32) -> Self {
        self.default_stdout = stdout.as_bytes().to_vec();
        self.default_stderr = stderr.as_bytes().to_vec();
        self.default_exit_code = exit_code;
        self
    }

    /// Conventional `--version` / `version` stub emitting `name version`.
    #[must_use]
    pub fn version_stub(name: &str, version: &str) -> Self {
        let out = format!("{name} {version}\n");
        Self::new()
            .on_exact(["--version"], &out, "", 0)
            .on_exact(["version"], &out, "", 0)
            .on_exact(["-V"], &out, "", 0)
    }

    /// Conventional `--help` / `help` stub emitting `usage`.
    #[must_use]
    pub fn help_stub(usage: &str) -> Self {
        Self::new()
            .on_exact(["--help"], usage, "", 0)
            .on_exact(["help"], usage, "", 0)
            .on_exact(["-h"], usage, "", 0)
    }
}

/// One captured spawn: `argv` (including `argv[0]`) plus environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// Full `argv`, `argv[0]` first (the script path as spawned).
    pub argv: Vec<String>,
    /// Environment as `(KEY, VALUE)` pairs captured via `env`.
    pub env: Vec<(String, String)>,
}

impl Invocation {
    /// `argv` without `argv[0]`.
    #[must_use]
    pub fn args(&self) -> &[String] {
        self.argv.get(1..).unwrap_or_default()
    }

    /// Value of environment variable `key`, if captured.
    #[must_use]
    pub fn env_get(&self, key: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// Panic if any `forbidden` value appears in `argv` or env.
    ///
    /// The panic message names the match location and index, never the
    /// secret itself.
    ///
    /// # Panics
    ///
    /// Panics when a non-empty `forbidden` value is found in `argv` or env.
    pub fn assert_no_secrets(&self, forbidden: &[&str]) {
        for (index, secret) in forbidden.iter().enumerate() {
            if secret.is_empty() {
                continue;
            }
            for arg in &self.argv {
                assert!(
                    !arg.contains(secret),
                    "spawned argv leaked forbidden value #{index} (redacted)"
                );
            }
            for (name, value) in &self.env {
                assert!(
                    !value.contains(secret),
                    "spawned env leaked forbidden value #{index} in {name} (redacted)"
                );
            }
        }
    }
}

/// Owns the temp directory holding fake binaries and their logs.
///
/// The directory is removed on drop (best effort).
#[derive(Debug)]
pub struct FakeProcessHarness {
    dir: PathBuf,
}

impl FakeProcessHarness {
    /// Create a harness rooted at a fresh unique temp directory.
    ///
    /// # Errors
    ///
    /// Returns the filesystem error when the temp directory cannot be created.
    pub fn new() -> std::io::Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("jackin-fakeproc-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Directory holding generated scripts and logs.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Materialize an executable fake binary named `name` running `script`.
    ///
    /// `name` must be a plain file name (no path separators).
    ///
    /// # Errors
    ///
    /// Returns the filesystem error when the script cannot be written or
    /// made executable.
    ///
    /// # Panics
    ///
    /// Panics when `name` is empty or contains a path separator.
    pub fn binary(&self, name: &str, script: &ProcessScript) -> std::io::Result<FakeBinary> {
        assert!(
            !name.is_empty() && !name.contains('/') && !name.contains('\\'),
            "fake binary name must be a plain file name: {name}"
        );
        let path = self.dir.join(format!("{name}.sh"));
        let log = self.dir.join(format!("{name}.log"));
        // Fresh log per binary so `invocations` only sees this script's spawns.
        drop(std::fs::remove_file(&log));
        let source = render_script(&log, script);
        std::fs::write(&path, source)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms)?;
        }
        Ok(FakeBinary { path, log })
    }
}

impl Drop for FakeProcessHarness {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.dir));
    }
}

/// A materialized fake binary: spawn it, then inspect [`Invocation`]s.
///
/// Borrows nothing, but the owning [`FakeProcessHarness`] must stay alive:
/// dropping the harness removes the temp dir holding the script and log.
#[derive(Debug)]
pub struct FakeBinary {
    path: PathBuf,
    log: PathBuf,
}

impl FakeBinary {
    /// Path of the generated script.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `Command` that spawns this fake (`sh <script>` on non-Unix).
    #[must_use]
    pub fn command(&self) -> Command {
        if cfg!(unix) {
            Command::new(&self.path)
        } else {
            let mut cmd = Command::new("sh");
            cmd.arg(&self.path);
            cmd
        }
    }

    /// All invocations logged so far, in spawn order.
    ///
    /// # Errors
    ///
    /// Returns the filesystem error when the log cannot be read (a missing
    /// log — no spawns yet — yields an empty vec, not an error).
    #[expect(
        clippy::disallowed_methods,
        reason = "test-only log read on the calling test thread; never a render/runtime path"
    )]
    pub fn invocations(&self) -> std::io::Result<Vec<Invocation>> {
        let file = match std::fs::File::open(&self.log) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };
        parse_log(BufReader::new(file))
    }
}

/// Quote `text` as a POSIX shell single-quoted word.
fn shell_quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for chunk in text.split('\'') {
        // `split` yields the pieces around each `'`; rejoin with '\''.
        if out.len() > 1 {
            out.push_str("'\\''");
        }
        out.push_str(chunk);
    }
    out.push('\'');
    out
}

/// Bytes emitted via `printf %s` so embedded quotes/newlines/`$` are safe.
fn emit(bytes: &[u8], stream: &str) -> String {
    // Script bodies are `&str` at the API boundary, so Lossy == identity.
    let body = String::from_utf8_lossy(bytes).into_owned();
    let redirect = if stream == "stdout" { "" } else { " >&2" };
    format!("printf '%s' {}{redirect}", shell_quote(&body))
}

/// Shell condition matching one [`Entry`]: `$#` arity plus per-position
/// literal `[ = ]` comparisons. Deliberately not `case` glob patterns:
/// literal comparison needs only single-quote escaping, so args containing
/// glob characters, backslashes, or whitespace match exactly.
fn condition(kind: &MatchKind) -> String {
    let (args, op) = match kind {
        MatchKind::Exact(args) => (args, "-eq"),
        MatchKind::Prefix(args) => (args, "-ge"),
    };
    let mut out = format!("[ \"$#\" {op} {} ]", args.len());
    for (index, arg) in args.iter().enumerate() {
        // `${N}` form: `$10` would parse as `${1}0` on vintage shells.
        out.push_str(&format!(
            " && [ \"${{{}}}\" = {} ]",
            index + 1,
            shell_quote(arg)
        ));
    }
    out
}

fn render_script(log: &Path, script: &ProcessScript) -> String {
    let mut out = String::from("#!/bin/sh\n");
    out.push_str(&format!(
        "_JACKIN_LOG={}\n",
        shell_quote(&log.to_string_lossy())
    ));
    out.push_str(
        "{\necho \"A:$0\"\nfor _a in \"$@\"; do echo \"A:$_a\"; done\n\
         env | sed 's/^/E:/'\necho '---'\n} >> \"$_JACKIN_LOG\"\n",
    );
    for (index, entry) in script.entries.iter().enumerate() {
        let keyword = if index == 0 { "if" } else { "elif" };
        out.push_str(&format!(
            "{keyword} {}; then {}; {}; exit {};\n",
            condition(&entry.kind),
            emit(&entry.stdout, "stdout"),
            emit(&entry.stderr, "stderr"),
            entry.exit_code
        ));
    }
    if script.entries.is_empty() {
        // No dispatch: the default is the whole program.
        out.push_str(&format!(
            "{}; {}; exit {};\n",
            emit(&script.default_stdout, "stdout"),
            emit(&script.default_stderr, "stderr"),
            script.default_exit_code
        ));
    } else {
        out.push_str(&format!(
            "else {}; {}; exit {};\nfi\n",
            emit(&script.default_stdout, "stdout"),
            emit(&script.default_stderr, "stderr"),
            script.default_exit_code
        ));
    }
    out
}

fn parse_log(reader: BufReader<std::fs::File>) -> std::io::Result<Vec<Invocation>> {
    let mut invocations = Vec::new();
    let mut argv = Vec::new();
    let mut env = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line == "---" {
            invocations.push(Invocation {
                argv: std::mem::take(&mut argv),
                env: std::mem::take(&mut env),
            });
        } else if let Some(arg) = line.strip_prefix("A:") {
            argv.push(arg.to_owned());
        } else if let Some((key, value)) = line
            .strip_prefix("E:")
            .and_then(|pair| pair.split_once('='))
        {
            env.push((key.to_owned(), value.to_owned()));
        }
    }
    Ok(invocations)
}

#[cfg(test)]
mod tests;
