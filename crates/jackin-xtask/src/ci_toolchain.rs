use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use crate::cmd;

#[cfg(test)]
mod tests;

#[derive(Subcommand, Debug)]
pub(crate) enum CiToolchainCommand {
    /// Activate an already prepared current Rust toolchain without downloading.
    Activate(PrepareArgs),
    /// Validate, repair, and export the current pinned Rust toolchain.
    Prepare(PrepareArgs),
}

#[derive(Args, Debug)]
pub(crate) struct PrepareArgs {
    #[arg(long, default_value = "")]
    version: String,
}

pub(crate) fn run(command: CiToolchainCommand) -> Result<()> {
    match command {
        CiToolchainCommand::Activate(args) => activate(&args.version, false),
        CiToolchainCommand::Prepare(args) => activate(&args.version, true),
    }
}

fn activate(version: &str, repair: bool) -> Result<()> {
    let pinned;
    let version = if version.is_empty() {
        pinned = pinned_version()?;
        pinned.as_str()
    } else {
        version
    };
    if let Some(toolchain) = find_rustup_toolchain(version)? {
        append_github_file("GITHUB_ENV", &format!("RUSTUP_TOOLCHAIN={toolchain}"))?;
        writeln!(
            io::stdout().lock(),
            "prepared Rust toolchain {toolchain} from rustup storage"
        )?;
        return Ok(());
    }

    let install = mise_install(version)?;
    if !valid_toolchain(&install) {
        if !repair {
            bail!(
                "prepared Rust toolchain {version} is unavailable; the warmup job must repair it"
            );
        }
        if install.exists() {
            fs::remove_dir_all(&install)
                .with_context(|| format!("removing incomplete {}", install.display()))?;
        }
        let tool = format!("rust@{version}");
        let _uninstall = cmd::run_streaming(Command::new("mise").args(["uninstall", &tool]));
        cmd::run_streaming(Command::new("mise").args(["install", &tool]))
            .with_context(|| format!("installing Rust {version} with mise"))?;
    }
    if let Some(toolchain) = find_rustup_toolchain(version)? {
        append_github_file("GITHUB_ENV", &format!("RUSTUP_TOOLCHAIN={toolchain}"))?;
        writeln!(
            io::stdout().lock(),
            "prepared Rust toolchain {toolchain} from repaired rustup storage"
        )?;
        return Ok(());
    }
    if !valid_toolchain(&install) {
        bail!(
            "mise reported Rust {version} installed, but {} is incomplete",
            install.display()
        );
    }
    append_github_file("GITHUB_PATH", &install.join("bin").to_string_lossy())?;
    writeln!(
        io::stdout().lock(),
        "prepared Rust toolchain {version} from mise storage"
    )?;
    Ok(())
}

fn pinned_version() -> Result<String> {
    let source = fs::read_to_string("rust-toolchain.toml")
        .context("reading rust-toolchain.toml for the pinned Rust version")?;
    source
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("channel = \"")
                .and_then(|value| value.strip_suffix('"'))
                .map(str::to_owned)
        })
        .context("rust-toolchain.toml is missing a quoted channel")
}

fn find_rustup_toolchain(version: &str) -> Result<Option<String>> {
    let rustup_home = env::var_os("RUSTUP_HOME").map_or_else(
        || {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".rustup"))
        },
        |path| Some(PathBuf::from(path)),
    );
    let Some(root) = rustup_home.map(|home| home.join("toolchains")) else {
        return Ok(None);
    };
    if !root.is_dir() {
        return Ok(None);
    }
    let suffix = format!("-{}", host_triple()?);
    let prefix = version.to_owned();
    let mut candidates = crate::fs_util::read_dir_sorted(&root)?
        .into_iter()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            (name.starts_with(&prefix) && name.ends_with(&suffix) && valid_toolchain(&entry.path()))
                .then_some(name)
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    Ok(candidates.pop())
}

fn mise_install(version: &str) -> Result<PathBuf> {
    let data = env::var_os("MISE_DATA_DIR").context("MISE_DATA_DIR must be set")?;
    Ok(PathBuf::from(data).join("installs/rust").join(version))
}

fn valid_toolchain(path: &Path) -> bool {
    ["rustc", "cargo"].into_iter().all(|binary| {
        let Ok(metadata) = fs::metadata(path.join("bin").join(binary)) else {
            return false;
        };
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    })
}

fn host_triple() -> Result<&'static str> {
    match env::consts::ARCH {
        "x86_64" => Ok("x86_64-unknown-linux-gnu"),
        "aarch64" => Ok("aarch64-unknown-linux-gnu"),
        architecture => bail!("unsupported Rust host architecture: {architecture}"),
    }
}

fn append_github_file(variable: &str, line: &str) -> Result<()> {
    let path = env::var_os(variable).with_context(|| format!("{variable} must be set"))?;
    let path = Path::new(&path);
    let mut contents = fs::read(path).unwrap_or_default();
    writeln!(contents, "{line}").with_context(|| format!("formatting {variable}"))?;
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}
