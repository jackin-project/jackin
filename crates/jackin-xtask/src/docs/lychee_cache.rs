//! Durable transport for lychee responses when the bounded Actions cache evicts them.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cmd;
use crate::docs::contract;

const CACHE_PATH: &str = ".lycheecache";
const RESTORE_DIR: &str = ".ci-lychee-cache";

#[derive(Deserialize)]
struct ArtifactsResponse {
    artifacts: Vec<Artifact>,
}

#[derive(Deserialize)]
struct Artifact {
    id: u64,
    expired: bool,
    created_at: String,
    workflow_run: Option<WorkflowRun>,
}

#[derive(Deserialize)]
struct WorkflowRun {
    head_repository_id: u64,
}

impl Artifact {
    fn reusable(&self, repository_id: u64) -> bool {
        !self.expired
            && self
                .workflow_run
                .as_ref()
                .is_some_and(|run| run.head_repository_id == repository_id)
    }
}

pub(crate) fn run() -> Result<()> {
    let repository = required_env("REPOSITORY")?;
    let repository_id = required_env("REPOSITORY_ID")?
        .parse::<u64>()
        .context("REPOSITORY_ID is not an integer")?;
    let contract = contract::lychee_contract("HEAD")?;
    let output = PathBuf::from(required_env("GITHUB_OUTPUT")?);
    let actions_cache_hit = env::var("ACTIONS_CACHE_HIT").is_ok_and(|value| value == "true");
    let name = format!("docs-lychee-responses-v1-{contract}");
    let artifact = newest_artifact(&repository, repository_id, &name)?;
    let artifact_hit = artifact.is_some();
    let mut restored = false;

    if !actions_cache_hit && let Some(artifact) = artifact {
        restore(&repository, artifact.id)?;
        restored = true;
        writeln!(
            std::io::stdout().lock(),
            "::notice::restored durable lychee response cache"
        )?;
    }

    fs::write(
        &output,
        format!("name={name}\nartifact_hit={artifact_hit}\nrestored={restored}\n"),
    )
    .with_context(|| format!("writing GitHub output at {}", output.display()))
}

fn newest_artifact(repository: &str, repository_id: u64, name: &str) -> Result<Option<Artifact>> {
    let endpoint = format!("repos/{repository}/actions/artifacts?name={name}&per_page=10");
    let response: ArtifactsResponse =
        serde_json::from_slice(&cmd::output(Command::new("gh").args(["api", &endpoint]))?)
            .context("parsing lychee cache artifact response")?;
    Ok(response
        .artifacts
        .into_iter()
        .filter(|artifact| artifact.reusable(repository_id))
        .max_by(|left, right| left.created_at.cmp(&right.created_at)))
}

fn restore(repository: &str, artifact_id: u64) -> Result<()> {
    let restore_dir = Path::new(RESTORE_DIR);
    if restore_dir.exists() {
        fs::remove_dir_all(restore_dir).context("removing stale lychee restore directory")?;
    }
    fs::create_dir_all(restore_dir).context("creating lychee restore directory")?;
    let archive = restore_dir.join("artifact.zip");
    let endpoint = format!("repos/{repository}/actions/artifacts/{artifact_id}/zip");
    cmd::run_stdout_file(Command::new("gh").args(["api", &endpoint]), &archive)
        .context("downloading durable lychee response cache")?;
    cmd::run(
        Command::new("unzip")
            .args(["-q", "-o"])
            .arg(&archive)
            .arg("-d")
            .arg(restore_dir),
    )
    .context("extracting durable lychee response cache")?;
    let restored = restore_dir.join(CACHE_PATH);
    if !restored.is_file() {
        bail!("durable lychee artifact does not contain {CACHE_PATH}");
    }
    fs::rename(&restored, CACHE_PATH).context("staging durable lychee response cache")?;
    fs::remove_dir_all(restore_dir).context("removing lychee restore directory")
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is not set"))
}

#[cfg(test)]
mod tests;
