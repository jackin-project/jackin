use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::cmd;

#[cfg(test)]
mod tests;

#[derive(Subcommand, Debug)]
pub(crate) enum CiTargetCommand {
    /// Resolve one crate's exact source key from the affected-crates result.
    ResolveKey(ResolveKeyArgs),
    /// Find the newest compatible target artifact and emit step outputs.
    Find(FindArgs),
    /// Download and extract one GitHub Actions artifact.
    Download(DownloadArgs),
    /// Restore a target archive and emit whether it exactly matches the source.
    Restore(RestoreArgs),
    /// Select a complete local target or restore its reusable artifact.
    Prepare(PrepareArgs),
    /// Validate the runner-local Cargo target as a reusable seed.
    ValidateLocal(ValidateLocalArgs),
    /// Pack the reusable portion of a Cargo target into one archive.
    Pack(PackArgs),
}

#[derive(Args, Debug)]
pub(crate) struct ResolveKeyArgs {
    #[arg(long)]
    package: String,
    #[arg(long)]
    cache_keys_json: String,
    /// Append the key to `GITHUB_OUTPUT` instead of printing it.
    #[arg(long)]
    github_output: bool,
}

#[derive(Args, Debug)]
pub(crate) struct FindArgs {
    #[arg(long)]
    package: String,
    #[arg(long)]
    cache_key: String,
    #[arg(long, action = clap::ArgAction::Set)]
    all_features: bool,
    #[arg(long)]
    runner_os: String,
    #[arg(long)]
    runner_arch: String,
    #[arg(long)]
    repository: String,
    /// Pre-resolved per-package target map from the affected-crates job.
    #[arg(long, default_value = "")]
    supplied_results: String,
    /// Append fields to `GITHUB_OUTPUT` instead of printing one JSON object.
    #[arg(long)]
    github_output: bool,
}

#[derive(Args, Debug)]
pub(crate) struct DownloadArgs {
    #[arg(long)]
    artifact_id: u64,
    #[arg(long)]
    destination: PathBuf,
    #[arg(long)]
    repository: String,
}

#[derive(Args, Debug)]
pub(crate) struct RestoreArgs {
    #[arg(long)]
    archive: PathBuf,
    #[arg(long, default_value = "target")]
    target: PathBuf,
    #[arg(long)]
    cache_key: String,
    #[arg(long, action = clap::ArgAction::Set)]
    known_exact: bool,
}

#[derive(Args, Debug)]
pub(crate) struct PrepareArgs {
    #[arg(long)]
    package: String,
    #[arg(long, default_value = "target")]
    target: PathBuf,
    #[arg(long)]
    artifact_id: u64,
    #[arg(long)]
    repository: String,
    #[arg(long)]
    cache_key: String,
    #[arg(long, action = clap::ArgAction::Set)]
    known_exact: bool,
}

#[derive(Args, Debug)]
pub(crate) struct PackArgs {
    #[arg(long, default_value = "target")]
    target: PathBuf,
    #[arg(long)]
    source_key: String,
    #[arg(long, default_value = "target.tar.zst")]
    output: PathBuf,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    enabled: bool,
}

#[derive(Args, Debug)]
pub(crate) struct ValidateLocalArgs {
    #[arg(long, default_value = "target")]
    target: PathBuf,
}

#[derive(Deserialize)]
struct ArtifactsResponse {
    artifacts: Vec<Artifact>,
}

#[derive(Deserialize)]
struct Artifact {
    id: u64,
    expired: bool,
    created_at: String,
    size_in_bytes: u64,
}

const MINIMUM_REUSABLE_TARGET_BYTES: u64 = 1024 * 1024;

impl Artifact {
    fn reusable(&self) -> bool {
        !self.expired && self.size_in_bytes >= MINIMUM_REUSABLE_TARGET_BYTES
    }
}

pub(crate) fn run(command: CiTargetCommand) -> Result<()> {
    match command {
        CiTargetCommand::ResolveKey(args) => resolve_key(args),
        CiTargetCommand::Find(args) => find(args),
        CiTargetCommand::Download(args) => download(args),
        CiTargetCommand::Restore(args) => restore(args),
        CiTargetCommand::Prepare(args) => prepare(args),
        CiTargetCommand::ValidateLocal(args) => validate_local(args),
        CiTargetCommand::Pack(args) => pack(args),
    }
}

fn validate_local(args: ValidateLocalArgs) -> Result<()> {
    let hit = has_reusable_local_target(&args.target, "")?;
    if hit {
        writeln!(
            io::stdout().lock(),
            "::notice::using runner-local current Cargo target as a validated seed"
        )?;
    }
    write_output("hit", if hit { "true" } else { "false" })
}

fn has_reusable_local_target(target: &Path, package: &str) -> Result<bool> {
    if !target.join(".rustc_info.json").is_file() {
        return Ok(false);
    }
    let dependencies = target.join("debug/deps");
    if !dependencies.is_dir() {
        return Ok(false);
    }
    let has_library = crate::fs_util::read_dir_sorted(&dependencies)?
        .into_iter()
        .any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "rlib")
        });
    if !has_library {
        return Ok(false);
    }
    let Some(fuzz_targets) = crate::ci_fuzz::target_names(package) else {
        return Ok(true);
    };
    Ok(fuzz_targets.iter().all(|binary| {
        target
            .join("x86_64-unknown-linux-gnu/release")
            .join(binary)
            .is_file()
    }))
}

fn prepare(args: PrepareArgs) -> Result<()> {
    if has_reusable_local_target(&args.target, &args.package)? {
        writeln!(
            io::stdout().lock(),
            "::notice::using complete runner-local Cargo target"
        )?;
        write_output("local-hit", "true")?;
        return write_output("canonical-hit", "false");
    }
    write_output("local-hit", "false")?;
    let destination = args.target.join(".ci-restore");
    download(DownloadArgs {
        artifact_id: args.artifact_id,
        destination: destination.clone(),
        repository: args.repository,
    })?;
    restore(RestoreArgs {
        archive: destination.join("target.tar.zst"),
        target: args.target,
        cache_key: args.cache_key,
        known_exact: args.known_exact,
    })
}

fn resolve_key(args: ResolveKeyArgs) -> Result<()> {
    let key = key_for_package(&args.cache_keys_json, &args.package)?;
    if args.github_output {
        return write_output("key", &key);
    }
    writeln!(io::stdout().lock(), "{key}").context("writing crate source key")
}

fn key_for_package(cache_keys_json: &str, package: &str) -> Result<String> {
    let keys: JsonValue =
        serde_json::from_str(cache_keys_json).context("parsing per-package cache keys")?;
    keys.get(package)
        .and_then(JsonValue::as_str)
        .filter(|key| !key.is_empty())
        .map(ToOwned::to_owned)
        .with_context(|| format!("cache key is missing for crate `{package}`"))
}

fn find(args: FindArgs) -> Result<()> {
    if !args.supplied_results.is_empty() {
        let supplied: JsonValue = serde_json::from_str(&args.supplied_results)
            .context("parsing supplied per-package target results")?;
        if let Some(result) = supplied.get(&args.package) {
            return emit_find_result(
                &args,
                result["name"]
                    .as_str()
                    .context("supplied target result is missing name")?,
                result["hit"].as_bool().unwrap_or(false),
                result["known_exact"].as_bool().unwrap_or(false),
                result["artifact_id"].as_u64(),
            );
        }
    }
    let version = if args.all_features {
        "v9"
    } else {
        "default-v9"
    };
    let name = format!(
        "ci-crate-target-{version}-{}-{}-{}",
        args.runner_os, args.runner_arch, args.package
    );
    let current = newest_artifact(&args.repository, &name)?;
    let warmup = if current.is_none() {
        newest_artifact(&args.repository, &format!("{name}-warmup"))?
    } else {
        None
    };
    let artifact = current.as_ref().or(warmup.as_ref());
    emit_find_result(
        &args,
        &name,
        artifact.is_some(),
        false,
        artifact.map(|artifact| artifact.id),
    )
}

fn emit_find_result(
    args: &FindArgs,
    name: &str,
    hit: bool,
    known_exact: bool,
    artifact_id: Option<u64>,
) -> Result<()> {
    if args.github_output {
        write_output("name", name)?;
        write_output("hit", if hit { "true" } else { "false" })?;
        write_output("known-exact", if known_exact { "true" } else { "false" })?;
        return write_output(
            "artifact-id",
            &artifact_id.map_or_else(String::new, |id| id.to_string()),
        );
    }
    let result = serde_json::json!({
        "name": name,
        "source_key": args.cache_key,
        "hit": hit,
        "known_exact": known_exact,
        "artifact_id": artifact_id,
    });
    writeln!(io::stdout().lock(), "{result}").context("writing target result JSON")
}

fn newest_artifact(repository: &str, name: &str) -> Result<Option<Artifact>> {
    let endpoint = format!("repos/{repository}/actions/artifacts?name={name}&per_page=10");
    let output = cmd::output(Command::new("gh").args(["api", &endpoint]))
        .with_context(|| format!("querying GitHub Actions artifact `{name}`"))?;
    let mut response: ArtifactsResponse =
        serde_json::from_slice(&output).context("parsing GitHub artifact response")?;
    response.artifacts.retain(Artifact::reusable);
    response
        .artifacts
        .sort_unstable_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(response.artifacts.into_iter().next())
}

fn download(args: DownloadArgs) -> Result<()> {
    if args.destination.exists() {
        fs::remove_dir_all(&args.destination).with_context(|| {
            format!("removing stale destination {}", args.destination.display())
        })?;
    }
    fs::create_dir_all(&args.destination)
        .with_context(|| format!("creating {}", args.destination.display()))?;
    let archive = args.destination.join("artifact.zip");
    let endpoint = format!(
        "repos/{}/actions/artifacts/{}/zip",
        args.repository, args.artifact_id
    );
    cmd::run_stdout_file(Command::new("gh").args(["api", &endpoint]), &archive)
        .context("downloading GitHub Actions artifact")?;
    checked(
        Command::new("unzip")
            .arg("-q")
            .arg(&archive)
            .arg("-d")
            .arg(&args.destination),
        "extracting GitHub Actions artifact",
    )?;
    fs::remove_file(&archive).with_context(|| format!("removing {}", archive.display()))
}

fn restore(args: RestoreArgs) -> Result<()> {
    fs::create_dir_all(&args.target)
        .with_context(|| format!("creating {}", args.target.display()))?;
    checked(
        Command::new("tar")
            .args(["--zstd", "--strip-components=1", "-C"])
            .arg(&args.target)
            .arg("-xf")
            .arg(&args.archive),
        "restoring reusable Cargo target",
    )?;
    let source_key_path = args.target.join(".ci-source-key");
    let source_key = fs::read_to_string(&source_key_path).unwrap_or_default();
    let canonical_hit = args.known_exact || source_key.trim() == args.cache_key;
    if source_key_path.exists() {
        fs::remove_file(&source_key_path)
            .with_context(|| format!("removing {}", source_key_path.display()))?;
    }
    if let Some(restore_dir) = args.archive.parent()
        && restore_dir
            .file_name()
            .is_some_and(|name| name == ".ci-restore")
    {
        fs::remove_dir_all(restore_dir)
            .with_context(|| format!("removing {}", restore_dir.display()))?;
    }
    if canonical_hit {
        normalize_checkout_timestamps()?;
    }
    writeln!(
        io::stdout().lock(),
        "restored Cargo target canonical-hit={canonical_hit}"
    )?;
    write_output(
        "canonical-hit",
        if canonical_hit { "true" } else { "false" },
    )
}

fn normalize_checkout_timestamps() -> Result<()> {
    let output = cmd::output(Command::new("git").args(["ls-files", "-z"]))
        .context("listing tracked files for canonical target restore")?;
    let paths = output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    for chunk in paths.chunks(100) {
        let mut command = Command::new("touch");
        command.args(["--date=2000-01-01T00:00:00Z", "--"]);
        for path in chunk {
            command.arg(std::ffi::OsString::from_vec(path.to_vec()));
        }
        checked(&mut command, "normalizing checkout timestamps")?;
    }
    Ok(())
}

fn pack(args: PackArgs) -> Result<()> {
    if !args.enabled || !has_reusable_local_target(&args.target, "")? {
        if args.output.exists() {
            fs::remove_file(&args.output)
                .with_context(|| format!("removing stale {}", args.output.display()))?;
        }
        writeln!(
            io::stdout().lock(),
            "Cargo target publication disabled or empty; no reusable archive was produced"
        )?;
        return Ok(());
    }
    let current_dir = env::current_dir().context("resolving current directory")?;
    let target_parent = args
        .target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let source_key_path = args.target.join(".ci-source-key");
    fs::write(&source_key_path, format!("{}\n", args.source_key))
        .with_context(|| format!("writing {}", source_key_path.display()))?;
    let list_path = args.target.join(".ci-pack-files");
    let mut paths = reusable_paths(&args.target)?;
    paths.sort_unstable();
    let mut list =
        File::create(&list_path).with_context(|| format!("creating {}", list_path.display()))?;
    for path in paths {
        let archive_path = path.strip_prefix(target_parent).unwrap_or(&path);
        list.write_all(archive_path.as_os_str().as_encoded_bytes())?;
        list.write_all(&[0])?;
    }
    drop(list);
    let list_path = fs::canonicalize(&list_path)
        .with_context(|| format!("canonicalizing {}", list_path.display()))?;
    let output = if args.output.is_absolute() {
        args.output.clone()
    } else {
        current_dir.join(&args.output)
    };
    let target_name = args
        .target
        .file_name()
        .and_then(|name| name.to_str())
        .context("Cargo target path must have a UTF-8 final component")?;
    checked(
        Command::new("tar")
            .current_dir(target_parent)
            .env("ZSTD_CLEVEL", "10")
            .args(["--zstd", "--null", "--no-recursion"])
            .arg(format!("--transform=s|^{target_name}/|target/|"))
            .arg("--files-from")
            .arg(&list_path)
            .arg("-cf")
            .arg(&output),
        "packing reusable Cargo target",
    )?;
    fs::remove_file(&list_path).with_context(|| format!("removing {}", list_path.display()))?;
    fs::remove_file(&source_key_path)
        .with_context(|| format!("removing {}", source_key_path.display()))?;
    Ok(())
}

fn reusable_paths(target: &Path) -> Result<Vec<PathBuf>> {
    let mut pending = vec![target.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in crate::fs_util::read_dir_sorted(&directory)? {
            let path = entry.path();
            let relative = path.strip_prefix(target).unwrap_or(&path);
            if excluded(relative) {
                continue;
            }
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                pending.push(path);
            } else if (file_type.is_file() || file_type.is_symlink()) && !excluded_file(relative) {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn excluded_file(path: &Path) -> bool {
    let parent = path.parent();
    let generated_binary = parent == Some(Path::new("debug"))
        || parent == Some(Path::new("debug/deps"))
        || parent == Some(Path::new("debug/examples"));
    let hidden = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'));
    generated_binary && !hidden && path.extension().is_none()
}

fn excluded(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text == "nextest"
        || text.starts_with("nextest/")
        || text.starts_with("telemetry-")
        || text == "debug/incremental"
        || text.starts_with("debug/incremental/")
        || text == ".ci-restore"
        || text.starts_with(".ci-restore/")
        || text == ".ci-pack-files"
}

fn write_output(name: &str, value: &str) -> Result<()> {
    let output = env::var_os("GITHUB_OUTPUT").context("GITHUB_OUTPUT must be set")?;
    let path = Path::new(&output);
    let mut contents = fs::read(path).unwrap_or_default();
    writeln!(contents, "{name}={value}").context("formatting GitHub Actions output")?;
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

fn checked(command: &mut Command, context: &str) -> Result<()> {
    cmd::run(command).with_context(|| context.to_owned())
}
