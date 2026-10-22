// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{API, RECEIPT_LIMIT, Transport, read};
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::collections::BTreeSet;
use velnor_actions_contract::package_release::PackageReleaseReceipt;

#[derive(Deserialize)]
struct Owner {
    id: u64,
}
#[derive(Deserialize)]
struct Repository {
    id: u64,
    full_name: String,
    private: bool,
    default_branch: String,
    owner: Owner,
}
#[derive(Deserialize)]
struct RepositoryId {
    id: u64,
}
#[derive(Deserialize)]
struct Run {
    id: u64,
    run_attempt: u32,
    head_sha: String,
    head_branch: Option<String>,
    path: String,
    event: String,
    status: String,
    conclusion: Option<String>,
    repository: RepositoryId,
    head_repository: RepositoryId,
}
#[derive(Deserialize)]
struct Jobs {
    total_count: usize,
    jobs: Vec<Job>,
}
#[derive(Deserialize)]
struct Job {
    id: u64,
    run_id: u64,
    head_sha: String,
    name: String,
    status: String,
    conclusion: Option<String>,
}

pub(super) async fn repository(transport: &impl Transport) -> Result<()> {
    let repository: Repository = serde_json::from_slice(&transport.get(API, RECEIPT_LIMIT).await?)?;
    ensure!(
        repository.id == 1_197_700_841
            && repository.full_name == "jackin-project/jackin"
            && !repository.private
            && repository.default_branch == "main"
            && repository.owner.id == 273_541_039,
        "release repository identity changed"
    );
    Ok(())
}

#[derive(Deserialize)]
struct Commit {
    sha: String,
}
#[derive(Deserialize)]
struct Comparison {
    status: String,
    ahead_by: u64,
    behind_by: u64,
    base_commit: Commit,
    merge_base_commit: Commit,
}

fn identity(
    run: &Run,
    id: u64,
    attempt: u32,
    path: &str,
    event: &str,
    expected_sha: &str,
    receipt: &PackageReleaseReceipt,
) -> Result<()> {
    ensure!(
        run.id == id
            && run.run_attempt == attempt
            && run.head_sha == expected_sha
            && run.head_branch.as_deref() == Some("main")
            && run.path == path
            && run.event == event
            && run.repository.id == receipt.repository.id
            && run.head_repository.id == receipt.repository.id,
        "release workflow run/source identity mismatch"
    );
    Ok(())
}

fn original_build_jobs(jobs: Jobs, receipt: &PackageReleaseReceipt) -> Result<()> {
    ensure!(
        jobs.jobs.len() == jobs.total_count && jobs.total_count <= 100,
        "incomplete release job inventory"
    );
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for job in &jobs.jobs {
        ensure!(
            job.id > 0
                && ids.insert(job.id)
                && names.insert(&job.name)
                && job.run_id == receipt.workflow.run_id
                && job.head_sha == receipt.workflow.authority_sha,
            "duplicate or mismatched release job identity"
        );
    }
    let mut required: Vec<String> = [
        receipt.channel.assemble_job_id(),
        receipt.channel.signer_job_id(),
    ]
    .map(str::to_owned)
    .to_vec();
    required.extend(receipt.products.iter().flat_map(|product| {
        product
            .payloads
            .iter()
            .map(|payload| format!("Build {} / {}", product.id, payload.target))
    }));
    for required in required {
        let job = jobs
            .jobs
            .iter()
            .find(|job| job.name == required)
            .ok_or_else(|| anyhow::anyhow!("missing authenticated release job {required}"))?;
        ensure!(
            job.status == "completed" && job.conclusion.as_deref() == Some("success"),
            "release producer or signer did not succeed"
        );
    }
    Ok(())
}

pub(super) async fn validate(
    transport: &impl Transport,
    receipt: &PackageReleaseReceipt,
) -> Result<()> {
    let run: Run = read(
        transport,
        &format!(
            "actions/runs/{}/attempts/{}",
            receipt.workflow.run_id, receipt.workflow.run_attempt
        ),
    )
    .await?;
    identity(
        &run,
        receipt.workflow.run_id,
        receipt.workflow.run_attempt,
        receipt.channel.workflow_path(),
        receipt.channel.event().as_str(),
        &receipt.workflow.authority_sha,
        receipt,
    )?;
    ensure!(
        matches!(run.status.as_str(), "in_progress" | "completed"),
        "release run has not started"
    );
    // Admit the original successful producers, assembler and signer independently
    // of the original publisher's terminal result. A downstream Tap failure or
    // still-running promotion does not unpublish the complete signed generation.
    // Recovery retains the original signed receipt even when its publisher failed.
    // Current publication authority comes from the complete authenticated release
    // inventory, immutable lifecycle and signed source-bound tag, checked separately.
    let jobs: Jobs = read(
        transport,
        &format!(
            "actions/runs/{}/attempts/{}/jobs?per_page=100&page=1",
            receipt.workflow.run_id, receipt.workflow.run_attempt
        ),
    )
    .await?;
    original_build_jobs(jobs, receipt)?;
    let ci: Run = read(
        transport,
        &format!(
            "actions/runs/{}/attempts/{}",
            receipt.source.ci_run_id, receipt.source.ci_run_attempt
        ),
    )
    .await?;
    identity(
        &ci,
        receipt.source.ci_run_id,
        receipt.source.ci_run_attempt,
        ".github/workflows/ci.yml",
        "push",
        &receipt.source.commit_sha,
        receipt,
    )?;
    ensure!(
        ci.status == "completed" && ci.conclusion.as_deref() == Some("success"),
        "signed source CI is not successful"
    );
    // Source ancestry is immutable signing context and is admitted once per pin.
    // Rechecks observe mutable publication state without repeating this history query.
    ancestry(
        transport,
        &receipt.source.commit_sha,
        &receipt.workflow.authority_sha,
    )
    .await?;
    ancestry(transport, &receipt.workflow.authority_sha, "main").await
}

/// Require `base` to be `head` or one of its ancestors (`A <= B <= main`).
async fn ancestry(transport: &impl Transport, base: &str, head: &str) -> Result<()> {
    if base == head {
        return Ok(());
    }
    // Page 2 keeps comparison metadata without the potentially large file list.
    let comparison: Comparison = read(
        transport,
        &format!("compare/{base}...{head}?per_page=1&page=2"),
    )
    .await?;
    ensure!(
        comparison.base_commit.sha == base
            && comparison.merge_base_commit.sha == base
            && comparison.behind_by == 0
            && match comparison.status.as_str() {
                "identical" => comparison.ahead_by == 0,
                "ahead" => comparison.ahead_by > 0,
                _ => false,
            },
        "release source is not an ancestor of its workflow authority on protected main"
    );
    Ok(())
}
