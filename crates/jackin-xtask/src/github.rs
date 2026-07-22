// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde::Deserialize;

use crate::{cmd, docs};

#[cfg(test)]
mod tests;

#[derive(Subcommand, Debug)]
pub(crate) enum GithubCommand {
    /// Reuse a successful Pages deployment with identical site inputs.
    #[command(name = "docs-deployment-reuse")]
    DocsDeploymentReuse(DocsDeploymentReuseArgs),
    /// Create or update the rolling preview release without a race window.
    #[command(name = "publish-preview")]
    PublishPreview(PublishPreviewArgs),
}

#[derive(Args, Debug)]
pub(crate) struct DocsDeploymentReuseArgs {
    #[arg(long)]
    repository: String,
    #[arg(long, default_value = "github-pages")]
    environment: String,
    #[arg(long)]
    github_output: bool,
}

#[derive(Args, Debug)]
pub(crate) struct PublishPreviewArgs {
    #[arg(long)]
    repository: String,
    #[arg(long, default_value = "preview")]
    tag: String,
    #[arg(long)]
    version: String,
    #[arg(long)]
    sha: String,
    #[arg(long, default_value = "artifacts")]
    assets: PathBuf,
}

#[derive(Deserialize)]
struct Deployment {
    id: u64,
    sha: String,
}

#[derive(Deserialize)]
struct DeploymentStatus {
    state: String,
}

pub(crate) fn run(command: GithubCommand) -> Result<()> {
    match command {
        GithubCommand::DocsDeploymentReuse(args) => docs_deployment_reuse(args),
        GithubCommand::PublishPreview(args) => publish_preview(args),
    }
}

fn docs_deployment_reuse(args: DocsDeploymentReuseArgs) -> Result<()> {
    let endpoint = format!(
        "repos/{}/deployments?environment={}&per_page=10",
        args.repository, args.environment
    );
    let deployments: Vec<Deployment> = api_json(&endpoint)?;
    let current = docs::contract::site_contract("HEAD")?;
    let mut reuse = false;
    for deployment in deployments {
        if deployment.sha.is_empty() || !deployment_succeeded(&args.repository, deployment.id)? {
            continue;
        }
        if !has_commit(&deployment.sha) && !fetch_commit(&deployment.sha) {
            writeln!(
                io::stderr().lock(),
                "::warning::could not fetch deployed source {}; rebuilding Docs",
                deployment.sha
            )?;
            break;
        }
        reuse = docs::contract::site_contract(&deployment.sha)? == current;
        if reuse {
            writeln!(
                io::stderr().lock(),
                "::notice::reusing semantically identical successful Pages deployment"
            )?;
        }
        break;
    }
    if args.github_output {
        return write_output("reuse", if reuse { "true" } else { "false" });
    }
    writeln!(io::stdout().lock(), "{reuse}").context("writing deployment reuse result")
}

fn deployment_succeeded(repository: &str, id: u64) -> Result<bool> {
    let endpoint = format!("repos/{repository}/deployments/{id}/statuses?per_page=1");
    let statuses: Vec<DeploymentStatus> = api_json(&endpoint)?;
    Ok(statuses
        .first()
        .is_some_and(|status| status.state == "success"))
}

fn api_json<T: serde::de::DeserializeOwned>(endpoint: &str) -> Result<T> {
    let mut last_error = None;
    for attempt in 1..=4 {
        match cmd::output(Command::new("gh").args(["api", endpoint])).and_then(|output| {
            serde_json::from_slice(&output).context("parsing GitHub API response")
        }) {
            Ok(value) => return Ok(value),
            Err(error) => {
                last_error = Some(error);
                if attempt < 4 {
                    let delay = 1_u64 << (attempt - 1);
                    if let Err(write_error) = writeln!(
                        io::stderr().lock(),
                        "::warning::GitHub API attempt {attempt} failed; retrying in {delay}s"
                    ) {
                        last_error = Some(write_error.into());
                    }
                    thread::park_timeout(Duration::from_secs(delay));
                }
            }
        }
    }
    match last_error {
        Some(error) => {
            Err(error).with_context(|| format!("querying GitHub API endpoint {endpoint}"))
        }
        None => bail!("GitHub API retry loop made no attempts for {endpoint}"),
    }
}

fn has_commit(sha: &str) -> bool {
    cmd::output_raw(Command::new("git").args(["cat-file", "-e", &format!("{sha}^{{commit}}")]))
        .is_ok_and(|result| result.success)
}

fn fetch_commit(sha: &str) -> bool {
    cmd::output_raw(Command::new("git").args(["fetch", "--no-tags", "--depth=1", "origin", sha]))
        .is_ok_and(|result| result.success)
}

fn publish_preview(args: PublishPreviewArgs) -> Result<()> {
    let assets = release_assets(&args.assets)?;
    if assets.is_empty() {
        bail!(
            "no preview release assets found in {}",
            args.assets.display()
        );
    }
    let notes = format!(
        "Preview build from [{}](https://github.com/{}/commit/{}).",
        args.sha.chars().take(7).collect::<String>(),
        args.repository,
        args.sha
    );
    let view = release_view(&args.repository, &args.tag)?;
    if view {
        return update_release(&args, &notes, &assets);
    }
    let create = release_command("create", &args, &notes, &assets);
    let result = cmd::output_raw(&mut create.to_command())?;
    if result.success {
        return Ok(());
    }
    let failure = format!(
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    if release_already_exists(&failure) {
        writeln!(
            io::stderr().lock(),
            "::notice::preview release appeared concurrently; updating it"
        )?;
        return update_release(&args, &notes, &assets);
    }
    bail!("creating preview release failed: {}", failure.trim())
}

fn release_view(repository: &str, tag: &str) -> Result<bool> {
    let result = cmd::output_raw(Command::new("gh").args([
        "release", "view", tag, "--repo", repository, "--json", "tagName",
    ]))?;
    if result.success {
        return Ok(true);
    }
    let failure = String::from_utf8_lossy(&result.stderr);
    if release_missing(&failure) {
        return Ok(false);
    }
    bail!("querying preview release failed: {}", failure.trim())
}

fn update_release(args: &PublishPreviewArgs, notes: &str, assets: &[PathBuf]) -> Result<()> {
    cmd::run(Command::new("gh").args([
        "release",
        "edit",
        &args.tag,
        "--repo",
        &args.repository,
        "--prerelease",
        "--target",
        &args.sha,
        "--title",
        &format!("Preview {}", args.version),
        "--notes",
        notes,
    ]))?;
    let mut upload = Command::new("gh");
    upload.args([
        "release",
        "upload",
        &args.tag,
        "--repo",
        &args.repository,
        "--clobber",
    ]);
    upload.args(assets);
    cmd::run(&mut upload)
}

struct ReleaseCommand {
    args: Vec<OsString>,
}

impl ReleaseCommand {
    fn to_command(&self) -> Command {
        let mut command = Command::new("gh");
        command.args(&self.args);
        command
    }
}

fn release_command(
    operation: &str,
    args: &PublishPreviewArgs,
    notes: &str,
    assets: &[PathBuf],
) -> ReleaseCommand {
    let mut command = vec![
        "release".into(),
        operation.into(),
        args.tag.clone().into(),
        "--repo".into(),
        args.repository.clone().into(),
        "--prerelease".into(),
        "--target".into(),
        args.sha.clone().into(),
        "--title".into(),
        format!("Preview {}", args.version).into(),
        "--notes".into(),
        notes.into(),
    ];
    command.extend(assets.iter().map(|path| OsString::from(path.as_os_str())));
    ReleaseCommand { args: command }
}

fn release_assets(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut assets = crate::fs_util::read_dir_sorted(directory)
        .with_context(|| format!("reading preview assets from {}", directory.display()))?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_release_asset(path))
        .collect::<Vec<_>>();
    assets.sort_unstable();
    Ok(assets)
}

fn is_release_asset(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    (name.starts_with("jackin-")
        && [
            ".tar.gz",
            ".tar.gz.sha256",
            ".tar.gz.bundle",
            ".tar.gz.sbom.json",
        ]
        .iter()
        .any(|suffix| name.ends_with(suffix)))
        || matches!(
            name,
            "capsule-manifest.json" | "capsule-manifest.json.bundle"
        )
}

fn release_missing(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("release not found")
        || message.contains("release does not exist")
        || message.contains("http 404")
}

fn release_already_exists(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("same tag name already exists") || message.contains("already_exists")
}

fn write_output(name: &str, value: &str) -> Result<()> {
    let output = env::var_os("GITHUB_OUTPUT").context("GITHUB_OUTPUT must be set")?;
    let mut contents = fs::read(&output).unwrap_or_default();
    writeln!(contents, "{name}={value}").context("formatting GitHub Actions output")?;
    fs::write(&output, contents).context("writing GitHub Actions output")
}
