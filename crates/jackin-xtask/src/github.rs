// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde::Deserialize;

use crate::{cmd, docs};

#[derive(Subcommand, Debug)]
pub(crate) enum GithubCommand {
    /// Reuse a successful Pages deployment with identical site inputs.
    #[command(name = "docs-deployment-reuse")]
    DocsDeploymentReuse(DocsDeploymentReuseArgs),
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

fn write_output(name: &str, value: &str) -> Result<()> {
    let output = env::var_os("GITHUB_OUTPUT").context("GITHUB_OUTPUT must be set")?;
    let mut contents = fs::read(&output).unwrap_or_default();
    writeln!(contents, "{name}={value}").context("formatting GitHub Actions output")?;
    fs::write(&output, contents).context("writing GitHub Actions output")
}
