use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use anyhow::{Result, ensure};

pub(super) fn private_dir() -> Result<tempfile::TempDir> {
    let parent = std::env::temp_dir().canonicalize()?;
    let directory = tempfile::Builder::new()
        .prefix("jackin-owner-install-")
        .tempdir_in(parent)?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    Ok(directory)
}

pub(super) fn git(repository: &Path, args: &[&str]) -> Result<Output> {
    let output = Command::new("/bin/sh")
        .arg("-c")
        .arg("umask 077; exec \"$@\"")
        .arg("jackin-git-fixture")
        .arg("git")
        .arg("-C")
        .arg(repository.as_os_str())
        .args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    ensure!(
        output.status.success(),
        "git fixture command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

pub(super) fn init_repository(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
    git(root, &["init", "-q"])?;
    std::fs::set_permissions(root.join(".git"), std::fs::Permissions::from_mode(0o700))?;
    git(root, &["config", "user.name", "Installer Fixture"])?;
    git(
        root,
        &["config", "user.email", "installer-fixture@example.invalid"],
    )?;
    git(root, &["config", "extensions.worktreeConfig", "true"])?;
    std::fs::write(
        root.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.99.1\"\nprofile = \"minimal\"\ncomponents = [\"clippy\", \"rustfmt\"]\n",
    )?;
    std::fs::write(root.join("README.md"), "fixture\n")?;
    git(root, &["add", "README.md", "rust-toolchain.toml"])?;
    git(root, &["commit", "-q", "-m", "fixture"])?;
    Ok(())
}

/// Give public-installer subprocesses private, deterministic MBX and Rust tools.
pub(super) fn configure_admission_environment(
    command: &mut Command,
    root: &Path,
    first_path: Option<&Path>,
) -> Result<()> {
    let bin = root.join("qualified-installer-tools");
    std::fs::create_dir_all(&bin)?;
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o700))?;
    let home = root.join("qualified-installer-home");
    std::fs::create_dir_all(&home)?;
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700))?;

    let cargo = bin.join("cargo");
    write_executable(&cargo, b"#!/bin/sh\nprintf 'cargo 1.99.1 (fixture)\\n'\n");
    let rustc = bin.join("rustc");
    let rustdoc = bin.join("rustdoc");
    write_executable(
        &rustc,
        b"#!/bin/sh\nprintf 'rustc 1.99.1 (fixture)\\nbinary: rustc\\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\\nrelease: 1.99.1\\n'\n",
    );
    write_executable(
        &rustdoc,
        b"#!/bin/sh\nprintf 'rustdoc 1.99.1 (fixture)\\nbinary: rustdoc\\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\\nrelease: 1.99.1\\n'\n",
    );
    write_executable(
        &bin.join("clippy-driver"),
        b"#!/bin/sh\nprintf 'clippy 0.1.99 (0123456789abcdef0123456789abcdef01234567 fixture)\\n'\n",
    );
    write_executable(
        &bin.join("rustfmt"),
        b"#!/bin/sh\nprintf 'rustfmt 1.99.1-stable (0123456789abcdef0123456789abcdef01234567 fixture)\\n'\n",
    );
    write_executable(
        &bin.join("rustup"),
        format!(
            "#!/bin/sh\n[ \"$1\" = which ] && [ \"$2\" = --toolchain ] && [ \"$3\" = 1.99.1 ] || exit 90\ncase \"$4\" in rustc|rustdoc|clippy-driver|rustfmt) printf '%s/%s\\n' {} \"$4\" ;; *) exit 91 ;; esac\n",
            shell_quote(&bin)
        )
        .as_bytes(),
    );
    write_executable(
        &bin.join("mise"),
        format!(
            "#!/bin/sh\n[ \"$1\" = exec ] && [ \"$2\" = --locked ] && [ \"$3\" = -- ] || exit 90\nPATH={}:/usr/bin:/bin:/usr/sbin:/sbin\nexport PATH\nshift 3\nexec \"$@\"\n",
            shell_quote(&bin)
        )
        .as_bytes(),
    );
    write_executable(&bin.join("rust-build-guard"), b"#!/bin/sh\nexit 0\n");

    let mut search_path = Vec::<PathBuf>::new();
    if let Some(first_path) = first_path {
        search_path.push(first_path.to_path_buf());
    }
    search_path.push(bin);
    search_path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", std::env::join_paths(search_path)?)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0");
    Ok(())
}

fn write_executable(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).expect("write qualified installer fixture tool");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .expect("make qualified installer fixture tool executable");
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}
