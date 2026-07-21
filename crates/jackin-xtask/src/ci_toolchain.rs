use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde::Deserialize;

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

#[derive(Deserialize)]
struct ToolchainFile {
    toolchain: ToolchainConfig,
}

#[derive(Deserialize)]
struct ToolchainConfig {
    channel: String,
    #[serde(default)]
    components: Vec<String>,
    #[serde(default)]
    targets: Vec<String>,
}

pub(crate) fn run(command: CiToolchainCommand) -> Result<()> {
    match command {
        CiToolchainCommand::Activate(args) => activate(&args.version, false),
        CiToolchainCommand::Prepare(args) => activate(&args.version, true),
    }
}

fn activate(version: &str, repair: bool) -> Result<()> {
    let config = pinned_config()?;
    let version = if version.is_empty() {
        config.channel.as_str()
    } else {
        version
    };
    if let Some(toolchain) = find_rustup_toolchain(version, &config)? {
        append_github_file("GITHUB_ENV", &format!("RUSTUP_TOOLCHAIN={toolchain}"))?;
        writeln!(
            io::stdout().lock(),
            "prepared Rust toolchain {toolchain} from rustup storage"
        )?;
        return Ok(());
    }
    if !repair && let Some(toolchain) = find_mise_toolchain(version, &config) {
        append_github_file("GITHUB_PATH", &toolchain.join("bin").display().to_string())?;
        append_github_file(
            "GITHUB_ENV",
            &format!("RUSTC={}", toolchain.join("bin/rustc").display()),
        )?;
        append_github_file(
            "GITHUB_ENV",
            &format!("RUSTDOC={}", toolchain.join("bin/rustdoc").display()),
        )?;
        writeln!(
            io::stdout().lock(),
            "prepared Rust toolchain {} from mise storage",
            toolchain.display()
        )?;
        return Ok(());
    }

    if !repair {
        bail!("prepared Rust toolchain {version} is unavailable; the warmup job must repair it");
    }
    remove_incomplete_rustup_toolchains(version, &config)?;
    let mut install = Command::new("rustup");
    install.args(["toolchain", "install", version, "--profile", "minimal"]);
    if !config.components.is_empty() {
        install.args(["--component", &config.components.join(",")]);
    }
    if !config.targets.is_empty() {
        install.args(["--target", &config.targets.join(",")]);
    }
    cmd::run_streaming(&mut install)
        .with_context(|| format!("installing Rust {version} with rustup"))?;
    if let Some(toolchain) = find_rustup_toolchain(version, &config)? {
        append_github_file("GITHUB_ENV", &format!("RUSTUP_TOOLCHAIN={toolchain}"))?;
        writeln!(
            io::stdout().lock(),
            "prepared Rust toolchain {toolchain} from repaired rustup storage"
        )?;
        return Ok(());
    }
    bail!("rustup reported Rust {version} installed, but its toolchain is incomplete")
}

fn find_mise_toolchain(version: &str, config: &ToolchainConfig) -> Option<PathBuf> {
    let root = env::var_os("MISE_DATA_DIR").map(PathBuf::from)?;
    mise_toolchain_at(&root, version, config)
}

fn mise_toolchain_at(root: &Path, version: &str, config: &ToolchainConfig) -> Option<PathBuf> {
    let candidate = root.join("installs/rust").join(version);
    valid_toolchain(&candidate, config).then_some(candidate)
}

fn pinned_config() -> Result<ToolchainConfig> {
    let source = fs::read_to_string("rust-toolchain.toml")
        .context("reading rust-toolchain.toml for the pinned Rust version")?;
    toml::from_str::<ToolchainFile>(&source)
        .context("parsing rust-toolchain.toml")
        .map(|file| file.toolchain)
}

fn find_rustup_toolchain(version: &str, config: &ToolchainConfig) -> Result<Option<String>> {
    let mut candidates = rustup_toolchain_candidates(version)?;
    candidates.retain(|(_, path)| valid_toolchain(path, config));
    Ok(candidates.pop().map(|(name, _)| name))
}

fn rustup_toolchain_candidates(version: &str) -> Result<Vec<(String, PathBuf)>> {
    let rustup_home = env::var_os("RUSTUP_HOME").map_or_else(
        || {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".rustup"))
        },
        |path| Some(PathBuf::from(path)),
    );
    let Some(root) = rustup_home.map(|home| home.join("toolchains")) else {
        return Ok(Vec::new());
    };
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let suffix = format!("-{}", host_triple()?);
    let prefix = version.to_owned();
    let mut candidates = crate::fs_util::read_dir_sorted(&root)?
        .into_iter()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            (name.starts_with(&prefix) && name.ends_with(&suffix)).then(|| (name, entry.path()))
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    Ok(candidates)
}

fn remove_incomplete_rustup_toolchains(version: &str, config: &ToolchainConfig) -> Result<()> {
    for (name, path) in rustup_toolchain_candidates(version)? {
        if valid_toolchain(&path, config) {
            continue;
        }
        let mut uninstall = Command::new("rustup");
        uninstall.args(["toolchain", "uninstall", &name]);
        cmd::run_streaming(&mut uninstall)
            .with_context(|| format!("removing incomplete Rust toolchain {name}"))?;
    }
    Ok(())
}

fn valid_toolchain(path: &Path, config: &ToolchainConfig) -> bool {
    let binaries = ["rustc", "cargo"]
        .into_iter()
        .chain(
            config
                .components
                .iter()
                .filter_map(|component| match component.as_str() {
                    "rustfmt" => Some("rustfmt"),
                    "clippy" => Some("clippy-driver"),
                    _ => None,
                }),
        );
    binaries.into_iter().all(|binary| {
        let Ok(metadata) = fs::metadata(path.join("bin").join(binary)) else {
            return false;
        };
        metadata.is_file()
            && metadata.permissions().mode() & 0o111 != 0
            && Command::new(path.join("bin").join(binary))
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
    }) && config
        .targets
        .iter()
        .all(|target| path.join("lib/rustlib").join(target).join("lib").is_dir())
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
