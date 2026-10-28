// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result};
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    bin: PathBuf,
    trace: PathBuf,
    children: PathBuf,
    guard: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self> {
        let temp = tempfile::tempdir().context("creating the CI guard fixture")?;
        let root = temp.path().join("workspace with spaces");
        let bin = temp.path().join("external tools");
        fs::create_dir_all(root.join("docs/content")).context("creating fixture docs root")?;
        fs::create_dir_all(&bin).context("creating fixture tools directory")?;
        let root = fs::canonicalize(root).context("canonicalizing fixture root")?;
        let trace = temp.path().join("guard trace.log");
        let children = temp.path().join("child trace.log");
        fs::write(&trace, b"").context("initializing guard trace")?;
        fs::write(&children, b"").context("initializing child trace")?;
        let guard = bin.join("guard");
        write_executable(
            &guard,
            r#"#!/bin/sh
{ printf 'GUARD|<%s>|<%s>|argc=%s|' "$PWD" "${CI_GUARD_SENTINEL:-missing}" "$#"; printf '<%s>' "$@"; printf '\n'; } >> "$GUARD_TRACE"
[ "$1" = -- ] || exit 92
shift
[ "$1" = mise ] && [ "$2" = exec ] && [ "$3" = -- ] || exit 93
if [ "${GUARD_REJECT:-0}" = 1 ]; then printf 'guard rejected child\n' >&2; exit 23; fi
exec "$@"
"#,
        )?;
        write_executable(
            &bin.join("mise"),
            r#"#!/bin/sh
{ printf 'MISE|<%s>|<%s>|argc=%s|' "$PWD" "${CI_GUARD_SENTINEL:-missing}" "$#"; printf '<%s>' "$@"; printf '\n'; } >> "$GUARD_TRACE"
[ "$#" -ge 3 ] && [ "$1" = exec ] && [ "$2" = -- ] || exit 91
shift 2
exec "$@"
"#,
        )?;
        write_executable(
            &bin.join("cargo"),
            r#"#!/bin/sh
{ printf 'CARGO|<%s>|<%s>|argc=%s|' "$PWD" "${CI_GUARD_SENTINEL:-missing}" "$#"; printf '<%s>' "$@"; printf '\n'; } >> "$CHILD_TRACE"
if [ "$1" = check ]; then
  [ "$#" -eq 4 ] && [ "$2" = --workspace ] && [ "$3" = --all-targets ] && [ "$4" = --locked ] || exit 94
  exit 0
fi
if [ "$1" = nextest ]; then
  [ "$#" -eq 5 ] && [ "$2" = run ] && [ "$3" = --workspace ] && [ "$4" = --all-features ] && [ "$5" = --locked ] || exit 95
  exit 0
fi
if [ "$1" = run ]; then
  [ "$#" -eq 8 ] && [ "$2" = --quiet ] && [ "$3" = --locked ] && [ "$4" = --package ] && [ "$5" = jackin-xtask ] && [ "$6" = -- ] && [ "$7" = lint ] && [ "$8" = --strict ] || exit 96
  exec "$XTASK_BIN" lint --strict
fi
if [ "$1" = metadata ]; then
  [ "$#" -eq 2 ] && [ "$2" = --format-version=1 ] || exit 98
  printf '%s' '{"packages":[],"workspace_members":[]}'
  exit 0
fi
exit 99
"#,
        )?;
        write_executable(
            &bin.join("mbx"),
            r#"#!/bin/sh
{ printf 'MBX|<%s>|<%s>|argc=%s|' "$PWD" "${CI_GUARD_SENTINEL:-missing}" "$#"; printf '<%s>' "$@"; printf '\n'; } >> "$CHILD_TRACE"
[ "$#" -eq 4 ] && [ "$1" = test ] && [ "$2" = --doc ] && [ "$3" = --workspace ] && [ "$4" = --locked ] || exit 100
exit 0
"#,
        )?;
        write_executable(&bin.join("jactionlint"), "#!/bin/sh\nexit 0\n")?;
        Ok(Self {
            _temp: temp,
            root,
            bin,
            trace,
            children,
            guard,
        })
    }

    fn run_ci(&self, reject: bool) -> Result<Output> {
        self.run_ci_guard(reject, &self.guard)
    }

    fn run_ci_guard(&self, reject: bool, guard: &Path) -> Result<Output> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_jackin-xtask"));
        command
            .args(["ci", "--only", "tests"])
            .current_dir(&self.root)
            .env("PATH", &self.bin)
            .env("JACKIN_TOOLCHAIN_GUARD", guard)
            .env("GUARD_TRACE", &self.trace)
            .env("CHILD_TRACE", &self.children)
            .env("CI_GUARD_SENTINEL", "ci env value with spaces")
            .env("GUARD_REJECT", if reject { "1" } else { "0" });
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawning exported xtask CI command")?
            .wait_with_output()
            .context("waiting for exported xtask CI command")
    }

    fn run_nested_lint(&self) -> Result<Output> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .context("xtask crate path should be inside the repository")?
            .to_path_buf();
        let root = fs::canonicalize(root).context("canonicalizing workspace root")?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_jackin-xtask"));
        command
            .args(["ci", "--only", "lint"])
            .current_dir(root)
            .env("PATH", &self.bin)
            .env("JACKIN_TOOLCHAIN_GUARD", &self.guard)
            .env("GUARD_TRACE", &self.trace)
            .env("CHILD_TRACE", &self.children)
            .env("XTASK_BIN", env!("CARGO_BIN_EXE_jackin-xtask"))
            .env("CI_GUARD_SENTINEL", "ci env value with spaces")
            .env("GUARD_REJECT", "0");
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawning exported xtask lint command")?
            .wait_with_output()
            .context("waiting for exported xtask lint command")
    }

    fn trace(&self) -> Result<String> {
        fs::read_to_string(&self.trace).context("reading guard trace")
    }

    fn children(&self) -> Result<String> {
        fs::read_to_string(&self.children).context("reading child trace")
    }
}

fn write_executable(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).context("writing fixture executable")?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .context("making fixture executable")
}

#[test]
fn ci_test_partition_sends_cargo_and_native_doctest_through_configured_guard() -> Result<()> {
    let fixture = Fixture::new()?;

    let output = fixture.run_ci(false)?;

    assert!(
        output.status.success(),
        "xtask failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let trace = fixture.trace()?;
    assert!(trace.contains("GUARD|"), "guard was not invoked: {trace}");
    assert!(
        trace.contains(&format!(
            "GUARD|<{}>|<ci env value with spaces>|argc=9|<--><mise><exec><--><cargo><check><--workspace><--all-targets><--locked>",
            fixture.root.display()
        )),
        "cargo check argv/env/cwd changed: {trace}"
    );
    assert!(
        trace.contains(&format!(
            "GUARD|<{}>|<ci env value with spaces>|argc=10|<--><mise><exec><--><cargo><nextest><run><--workspace><--all-features><--locked>",
            fixture.root.display()
        )),
        "nextest argv/env/cwd changed: {trace}"
    );
    assert!(
        trace.contains(&format!(
            "GUARD|<{}>|<ci env value with spaces>|argc=9|<--><mise><exec><--><mbx><test><--doc><--workspace><--locked>",
            fixture.root.display()
        )),
        "doctest argv/env/cwd changed: {trace}"
    );
    let children = fixture.children()?;
    assert!(
        children.contains(&format!(
            "CARGO|<{}>|<ci env value with spaces>|argc=4|<check><--workspace><--all-targets><--locked>",
            fixture.root.display()
        )),
        "Cargo check argv/env/cwd changed: {children}"
    );
    assert!(
        children.contains(&format!(
            "MBX|<{}>|<ci env value with spaces>|argc=4|<test><--doc><--workspace><--locked>",
            fixture.root.display()
        )),
        "mbx doctest argv/env/cwd changed: {children}"
    );
    Ok(())
}

#[test]
fn guard_rejection_blocks_external_children_and_reports_failure() -> Result<()> {
    let fixture = Fixture::new()?;

    let output = fixture.run_ci(true)?;

    assert!(
        !output.status.success(),
        "rejected commands returned success"
    );
    assert!(
        fixture.children()?.is_empty(),
        "a child ran after guard rejection"
    );
    assert!(fixture.trace()?.contains("GUARD|"), "guard was not invoked");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("guard rejected child"),
        "guard output was lost: {stderr}"
    );
    assert!(
        stderr.contains("Some(23)"),
        "guard exit status was lost: {stderr}"
    );
    let relative = Fixture::new()?;
    let output = relative.run_ci_guard(false, Path::new("relative/guard"))?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("JACKIN_TOOLCHAIN_GUARD must be an absolute executable path")
    );
    assert!(relative.trace()?.is_empty());
    assert!(relative.children()?.is_empty());
    Ok(())
}

#[test]
fn nested_lint_metadata_query_uses_the_configured_guard() -> Result<()> {
    let fixture = Fixture::new()?;

    let _output = fixture.run_nested_lint()?;

    let trace = fixture.trace()?;
    let root = fs::canonicalize(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .context("xtask crate path should be inside the repository")?,
    )
    .context("canonicalizing workspace root")?;
    assert!(
        trace.contains(&format!(
            "GUARD|<{}>|<ci env value with spaces>|argc=13|<--><mise><exec><--><cargo><run><--quiet><--locked><--package><jackin-xtask><--><lint><--strict>",
            root.display()
        )),
        "outer lint command was not guarded: {trace}"
    );
    assert!(
        trace.contains(&format!(
            "GUARD|<{}>|<ci env value with spaces>|argc=7|<--><mise><exec><--><cargo><metadata><--format-version=1>",
            root.display()
        )),
        "nested metadata query was not guarded: {trace}"
    );
    assert!(
        fixture.children()?.contains(&format!(
            "CARGO|<{}>|<ci env value with spaces>|argc=2|<metadata><--format-version=1>",
            root.display()
        )),
        "metadata argv/env/cwd changed: {}",
        fixture.children()?
    );
    Ok(())
}
