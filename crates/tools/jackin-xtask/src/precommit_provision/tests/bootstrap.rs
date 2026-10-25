// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, ensure};
use jackin_process::{ExecRequest, ExecResult, StdioMode, exec_sync};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

fn command_path(name: &str) -> Result<PathBuf> {
    let path = std::env::var_os("PATH").context("PATH is required for the fixture")?;
    let request = ExecRequest::new("/bin/sh", ["-c", "command -v \"$1\"", "sh", name])
        .env_clear()
        .envs([("PATH", path)])
        .stdin_mode(StdioMode::Null)
        .stdout_mode(StdioMode::Capture)
        .stderr_mode(StdioMode::Capture);
    let output = exec_sync(&request)?;
    ensure!(output.success, "cannot locate {name}");
    Ok(PathBuf::from(
        String::from_utf8(output.stdout)?.trim().to_owned(),
    ))
}

fn shell_quote(path: &Path) -> Result<String> {
    let path = path.to_str().context("fixture path is not UTF-8")?;
    Ok(format!("'{}'", path.replace('\'', "'\\''")))
}

fn write_executable(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn git(root: &Path, arguments: &[&str]) -> Result<ExecResult> {
    let path = std::env::var_os("PATH").context("PATH is required for the fixture")?;
    let request = ExecRequest::new("git", arguments)
        .cwd(root)
        .env_clear()
        .envs([
            ("PATH", path),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("GIT_CONFIG_GLOBAL", "/dev/null".into()),
        ])
        .stdin_mode(StdioMode::Null)
        .stdout_mode(StdioMode::Capture)
        .stderr_mode(StdioMode::Capture);
    exec_sync(&request)
}

fn bootstrap_case(
    temporary: &Path,
    object_format: &str,
    default_hash: &str,
    unsupported_format: bool,
) -> Result<()> {
    let root = temporary.join(format!("repo-{object_format}-{unsupported_format}"));
    let tools = temporary.join(format!("tools-{object_format}-{unsupported_format}"));
    fs::create_dir_all(root.join("scripts"))?;
    fs::create_dir(&tools)?;

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../");
    fs::copy(
        workspace.join("scripts/hooks-native-bootstrap"),
        root.join("scripts/hooks-native-bootstrap"),
    )?;
    fs::copy(
        workspace.join("scripts/hooks-install"),
        root.join("scripts/hooks-install"),
    )?;
    fs::write(
        root.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.90.0\"\n",
    )?;
    let initialized = git(
        &root,
        &[
            "init",
            "--quiet",
            &format!("--object-format={object_format}"),
        ],
    )?;
    ensure!(
        initialized.success,
        "Git cannot initialize {object_format} fixture: {}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    let staged = git(
        &root,
        &[
            "add",
            "scripts/hooks-native-bootstrap",
            "scripts/hooks-install",
            "rust-toolchain.toml",
        ],
    )?;
    ensure!(staged.success, "cannot stage bootstrap fixture");

    let actual_git = command_path("git")?;
    let observed_format = tools.join("observed-format");
    let guard = format!(
        "#!/bin/sh\n{} -C \"$PWD\" rev-parse --show-object-format > {}\nexit 7\n",
        shell_quote(&actual_git)?,
        shell_quote(&observed_format)?,
    );
    write_executable(&tools.join("rust-build-guard"), &guard)?;
    write_executable(
        &tools.join("mise"),
        "#!/bin/sh\n[ \"$1\" = exec ] || exit 81\nshift\n[ \"$1\" = -- ] || exit 82\nshift\nexec \"$@\"\n",
    )?;
    let rustc = tools.join("rustc");
    write_executable(&rustc, "#!/bin/sh\nexit 0\n")?;
    write_executable(
        &tools.join("rustup"),
        &format!(
            "#!/bin/sh\n[ \"$1\" = which ] || exit 83\nprintf '%s\\n' {}\n",
            shell_quote(&rustc)?
        ),
    )?;
    write_executable(&tools.join("jq"), "#!/bin/sh\nexit 84\n")?;
    write_executable(&tools.join("cargo"), "#!/bin/sh\nexit 85\n")?;

    if unsupported_format {
        let wrapper = format!(
            "#!/bin/sh\nif [ \"$1\" = rev-parse ] && [ \"$2\" = --show-object-format ]; then printf 'unknown-format\\n'; exit 0; fi\nexec {} \"$@\"\n",
            shell_quote(&actual_git)?,
        );
        write_executable(&tools.join("git"), &wrapper)?;
    }

    let home = temporary.join("home");
    fs::create_dir_all(&home)?;
    let inherited_path = std::env::var_os("PATH").context("PATH is required for the fixture")?;
    let path = std::env::join_paths(
        std::iter::once(tools.clone()).chain(std::env::split_paths(&inherited_path)),
    )?;
    let request = ExecRequest::new("/bin/sh", ["scripts/hooks-native-bootstrap"])
        .cwd(&root)
        .env_clear()
        .envs([
            ("PATH", path),
            ("HOME", home.into_os_string()),
            ("TMPDIR", temporary.as_os_str().to_os_string()),
            ("GIT_DEFAULT_HASH", default_hash.into()),
        ])
        .stdin_mode(StdioMode::Null)
        .stdout_mode(StdioMode::Capture)
        .stderr_mode(StdioMode::Capture);
    let output = exec_sync(&request)?;
    ensure!(
        !output.success,
        "stubbed bootstrap unexpectedly reached success"
    );
    let stderr = String::from_utf8(output.stderr)?;
    if unsupported_format {
        ensure!(
            stderr.contains("repository object format is unsupported"),
            "unsupported format was not rejected: {stderr}"
        );
        ensure!(
            !observed_format.exists(),
            "unsupported object format reached the build guard"
        );
    } else {
        let private_format = fs::read_to_string(&observed_format)?;
        ensure!(
            private_format.trim() == object_format,
            "private repository format {private_format:?} differs from source {object_format}"
        );
        ensure!(
            stderr.contains("guarded locked native owner build failed"),
            "bootstrap did not stop at the stubbed guard: {stderr}"
        );
        let evidence = stderr
            .lines()
            .find_map(|line| line.strip_prefix("error: bootstrap evidence retained at "))
            .context("failed bootstrap did not retain its evidence path")?;
        let evidence = PathBuf::from(evidence);
        ensure!(
            evidence.starts_with(temporary),
            "bootstrap evidence escaped the temporary fixture"
        );
        fs::remove_dir_all(evidence)?;
    }
    Ok(())
}

#[test]
fn bootstrap_preserves_object_format_against_environment_defaults() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let temporary_path = temporary.path().canonicalize()?;
    bootstrap_case(&temporary_path, "sha1", "sha256", false)?;
    bootstrap_case(&temporary_path, "sha256", "sha1", false)?;
    bootstrap_case(&temporary_path, "sha1", "sha256", true)?;
    Ok(())
}
