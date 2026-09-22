// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Homebrew preview source-change classification.
#![allow(
    dead_code,
    reason = "exercised by unit tests; workflow template will call via xtask next"
)]
#![expect(
    clippy::disallowed_methods,
    clippy::print_stdout,
    reason = "preview migration is host-side release CLI work"
)]

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::release_verify::expected_package_file_names;

#[cfg(test)]
mod tests;

#[derive(Debug, Subcommand)]
pub(crate) enum PreviewCommand {
    /// Archive and retire the one known invalid rolling preview release.
    #[command(name = "migrate-legacy")]
    MigrateLegacy(MigrateLegacyArgs),
}

#[derive(Debug, Args)]
pub(crate) struct MigrateLegacyArgs {}

pub(crate) fn run(command: PreviewCommand) -> Result<()> {
    match command {
        PreviewCommand::MigrateLegacy(_) => migrate_legacy_preview(),
    }
}

const LEGACY_ARCHIVE_SCHEMA: &str = "jackin.preview-legacy-archive.v2";
const LEGACY_SOURCE_REPOSITORY: &str = "jackin-project/jackin";
const LEGACY_TAG: &str = "preview";
const LEGACY_ARCHIVE_TAG: &str = "preview-legacy-archive-a506eee";
const LEGACY_ARCHIVE_RELEASE_NAME: &str = "Legacy Preview Archive a506eee";
const LEGACY_ARCHIVE_MANIFEST: &str = "metadata.json";
const LEGACY_ARCHIVE_PHASE: &str = "archive-verified";
const LEGACY_RELEASE_ID: u64 = 328_385_904;
const LEGACY_RELEASE_NAME: &str = "Preview 0.6.4-preview.1181+a506eee";
const LEGACY_RELEASE_BODY: &str = "Preview build from [a506eee](https://github.com/jackin-project/jackin/commit/a506eee0581ef7add4f615281dbb90d828d5a657).";
const LEGACY_TAG_TARGET: &str = "1c623d10e9f7e9072db4bd8ef027d4c24cd95ab3";
const EXPECTED_SOURCE_REF: &str = "refs/heads/main";
const GITHUB_ENV: &str = "GITHUB_ENV";
const RETAIN_MARKER: &str = "VELNOR_PUBLICATION_LOCK_RETAIN";
const PREPUBLISH_MARKER: &str = "VELNOR_PREPUBLISH_COMPLETED";

const LEGACY_ASSETS: &[(&str, &str)] = &[
    (
        "capsule-manifest.json",
        "f807bfd2b5830f46f23280f06852eebe561cdf31287119c6c74eba7d669def24",
    ),
    (
        "capsule-manifest.json.bundle",
        "93c20ee290e175f343e5cac1f88b4fa72fa0ca38fbd6ca982f56f6c9ddb7f789",
    ),
    (
        "jackin-aarch64-apple-darwin.tar.gz",
        "8b1bcecdd9798a84cbe0d9358374b2bf3425984e84dbda81279a7050c9854b7d",
    ),
    (
        "jackin-aarch64-apple-darwin.tar.gz.bundle",
        "60a0aeae20b309f46bbce30e68ab60da6e4aebfc049fcb9bb41b30a233f27d4a",
    ),
    (
        "jackin-aarch64-apple-darwin.tar.gz.sbom.json",
        "716a34b055af75007cfc1d1b2fa90c70cf25766f2bd328f8e386c6dba91dbc5a",
    ),
    (
        "jackin-aarch64-apple-darwin.tar.gz.sha256",
        "7b41ba338520be08cae0a90e6a338ef13378a38b773db89cc0af0351cf1ab1d1",
    ),
    (
        "jackin-aarch64-unknown-linux-gnu.tar.gz",
        "3d276461105b3297f77b82ffc0f8f39c01ec3b68a6670648f06426759b28ca21",
    ),
    (
        "jackin-aarch64-unknown-linux-gnu.tar.gz.bundle",
        "9a5f0d9e3b36dbb22ee349ba4febf8c937c641d41a484a9ee2e72c5134428ddf",
    ),
    (
        "jackin-aarch64-unknown-linux-gnu.tar.gz.sbom.json",
        "7b719eac1a6f2f6572d716724beebd2fc66ca015e101435b0f14298d953526f8",
    ),
    (
        "jackin-aarch64-unknown-linux-gnu.tar.gz.sha256",
        "4ededbc4ec2fc16bd17a2b6d3028c1837cd4373618ca0532db153633ddbea383",
    ),
    (
        "jackin-capsule-aarch64-unknown-linux-gnu.tar.gz",
        "6c24b131f74bb9574ab5e0670150da6d7ec17b006210fd5132dce148d40e606b",
    ),
    (
        "jackin-capsule-aarch64-unknown-linux-gnu.tar.gz.bundle",
        "701c9787290a067c70324ee686b84ff376758002feb157e1751896b30c7d858a",
    ),
    (
        "jackin-capsule-aarch64-unknown-linux-gnu.tar.gz.sbom.json",
        "aa607e0e06ae6779479cb2f5188d03d5c6b2049e6f079fe659f23baf366fa09e",
    ),
    (
        "jackin-capsule-aarch64-unknown-linux-gnu.tar.gz.sha256",
        "e36a7a2518f3de73fa73e97321dd821cc74759c0fc55da56190ce00a6eca3611",
    ),
    (
        "jackin-capsule-x86_64-unknown-linux-gnu.tar.gz",
        "7cf2f29c55799ddbc359ab0b2b39f4b8a2307f4e96b9c1c601e7e8f780fb6b89",
    ),
    (
        "jackin-capsule-x86_64-unknown-linux-gnu.tar.gz.bundle",
        "8c3e50a94358d5f5da9134e03036163da25280f76dfd4c035398fb8dffb847e3",
    ),
    (
        "jackin-capsule-x86_64-unknown-linux-gnu.tar.gz.sbom.json",
        "12000c3458ca933ff504b148769c19b9054f60a50fedaf0c898d1d7db8ee1bb9",
    ),
    (
        "jackin-capsule-x86_64-unknown-linux-gnu.tar.gz.sha256",
        "111ae2b270bf77b17466a255297b6248addee0276ff19d7fbdc180aa97db31db",
    ),
    (
        "jackin-x86_64-apple-darwin.tar.gz",
        "48b4d07c3309266c9877ea31aa865b68acfa922448df25e3fefd8b1505acecbc",
    ),
    (
        "jackin-x86_64-apple-darwin.tar.gz.bundle",
        "a4af65b3729826f6f9b5bb0add4fff9c008eede196a8d9f32ca3a919d7585e7c",
    ),
    (
        "jackin-x86_64-apple-darwin.tar.gz.sbom.json",
        "2261c438c4611d7a4d5076dc3d8476dc0411e652efa2916171aa6c7a31e46a21",
    ),
    (
        "jackin-x86_64-apple-darwin.tar.gz.sha256",
        "c0ce584361b96a5a5b06b99075192fe9f525b4c88b1ab1823d575d904ab8ffe4",
    ),
    (
        "jackin-x86_64-unknown-linux-gnu.tar.gz",
        "e3fe87061bad25b2850089dd9de558a317714bf7711c25ce9328a2e63381f9c7",
    ),
    (
        "jackin-x86_64-unknown-linux-gnu.tar.gz.bundle",
        "cd768b8beed021e6b9ed6041dd1833de23bd98246d3361155070831fd8784981",
    ),
    (
        "jackin-x86_64-unknown-linux-gnu.tar.gz.sbom.json",
        "607766b55f16de5e31d137b28032dbaab9f2c38eca817c53677532f1506b76cb",
    ),
    (
        "jackin-x86_64-unknown-linux-gnu.tar.gz.sha256",
        "1df597c89046a0beed61ae2a5edcdb6a80106992b893072f24f30ecf04ec80f1",
    ),
];

/// Snapshot of a public rolling release before any migration mutation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct RollingReleaseSnapshot {
    pub(crate) source_repository: String,
    pub(crate) tag_name: String,
    pub(crate) release_id: u64,
    pub(crate) release_name: String,
    pub(crate) release_body: String,
    pub(crate) tag_target: String,
    pub(crate) draft: bool,
    pub(crate) prerelease: bool,
    pub(crate) assets: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    id: u64,
    tag_name: String,
    name: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    digest: Option<String>,
}

impl GithubRelease {
    fn snapshot(
        &self,
        source_repository: &str,
        tag_target: String,
    ) -> Result<RollingReleaseSnapshot> {
        let mut assets = BTreeMap::new();
        for asset in &self.assets {
            let digest = asset
                .digest
                .as_deref()
                .with_context(|| format!("rolling preview asset has no digest: {}", asset.name))?;
            let digest = digest.strip_prefix("sha256:").with_context(|| {
                format!("rolling preview asset digest is not SHA256: {}", asset.name)
            })?;
            ensure!(
                digest.len() == 64 && is_lower_hex(digest),
                "rolling preview asset digest is invalid: {}",
                asset.name
            );
            ensure!(
                assets
                    .insert(asset.name.clone(), digest.to_owned())
                    .is_none(),
                "rolling preview release contains a duplicate asset: {}",
                asset.name
            );
        }
        Ok(RollingReleaseSnapshot {
            source_repository: source_repository.to_owned(),
            tag_name: self.tag_name.clone(),
            release_id: self.id,
            release_name: self.name.clone(),
            release_body: self.body.clone().unwrap_or_default(),
            tag_target,
            draft: self.draft,
            prerelease: self.prerelease,
            assets,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct LegacyArchiveMetadata {
    schema: String,
    verification_status: String,
    phase: String,
    archive_tag: String,
    release: RollingReleaseSnapshot,
    assets: BTreeMap<String, ArchivedLegacyAsset>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ArchivedLegacyAsset {
    archived_path: String,
    expected_sha256: String,
    observed_sha256: String,
    matches_expected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LegacyReleaseState {
    Absent,
    CurrentContract,
    KnownLegacy,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LegacyMigrationPhase {
    Noop,
    Archive,
    DeleteRelease,
    AdvanceTag,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LegacyTagState {
    Absent,
    Legacy,
    Candidate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DurableArchiveState {
    Missing,
    Incomplete,
    Verified,
}

/// Return the exact public asset fingerprint observed for the one-time migration.
fn known_legacy_assets() -> BTreeMap<String, String> {
    LEGACY_ASSETS
        .iter()
        .map(|(name, digest)| ((*name).to_owned(), (*digest).to_owned()))
        .collect()
}

pub(crate) fn plan_legacy_migration(
    release: LegacyReleaseState,
    tag: LegacyTagState,
    archive_verified: bool,
) -> Result<LegacyMigrationPhase> {
    match (release, tag, archive_verified) {
        (LegacyReleaseState::Absent, LegacyTagState::Absent, _) => Ok(LegacyMigrationPhase::Noop),
        (LegacyReleaseState::Absent, LegacyTagState::Candidate, _) => {
            Ok(LegacyMigrationPhase::Noop)
        }
        (LegacyReleaseState::Absent, LegacyTagState::Legacy, true) => {
            Ok(LegacyMigrationPhase::AdvanceTag)
        }
        (LegacyReleaseState::Absent, LegacyTagState::Legacy, false) => {
            bail!("rolling preview tag remains but its durable legacy archive is not verified")
        }
        (LegacyReleaseState::CurrentContract, LegacyTagState::Absent, _) => {
            bail!("rolling preview release exists without its tag; refusing mutation")
        }
        (
            LegacyReleaseState::CurrentContract,
            LegacyTagState::Legacy | LegacyTagState::Candidate,
            _,
        ) => Ok(LegacyMigrationPhase::Noop),
        (LegacyReleaseState::KnownLegacy, LegacyTagState::Absent, _) => {
            bail!("known legacy rolling release exists without its tag; refusing mutation")
        }
        (LegacyReleaseState::KnownLegacy, LegacyTagState::Legacy, true) => {
            Ok(LegacyMigrationPhase::DeleteRelease)
        }
        (LegacyReleaseState::KnownLegacy, LegacyTagState::Legacy, false) => {
            Ok(LegacyMigrationPhase::Archive)
        }
        (LegacyReleaseState::KnownLegacy, LegacyTagState::Candidate, _) => {
            bail!("known legacy rolling release tag points at the candidate; refusing mutation")
        }
        (LegacyReleaseState::Unknown, _, _) => {
            bail!("refusing to migrate unknown or changed rolling preview release")
        }
    }
}

fn classify_tag_target(
    tag_target: Option<&str>,
    candidate_source_commit: &str,
) -> Result<LegacyTagState> {
    match tag_target {
        None => Ok(LegacyTagState::Absent),
        Some(target) if target == candidate_source_commit => Ok(LegacyTagState::Candidate),
        Some(LEGACY_TAG_TARGET) => Ok(LegacyTagState::Legacy),
        Some(_) => bail!("rolling preview tag target changed; refusing mutation"),
    }
}

fn required_env(name: &str) -> Result<String> {
    let value =
        env::var(name).with_context(|| format!("{name} is required for preview migration"))?;
    ensure!(!value.trim().is_empty(), "{name} is empty");
    Ok(value)
}

#[derive(Debug)]
struct LegacyMigrationContext {
    repository: String,
    token: String,
    source_checkout: PathBuf,
    expected_source_commit: String,
}

fn legacy_migration_context() -> Result<LegacyMigrationContext> {
    let repository = required_env("GITHUB_REPOSITORY")?;
    ensure!(
        repository == LEGACY_SOURCE_REPOSITORY,
        "preview migration must run in {LEGACY_SOURCE_REPOSITORY}, got {repository}"
    );
    let token = required_env("GH_TOKEN")?;
    let source_checkout = PathBuf::from(required_env("VELNOR_SOURCE_CHECKOUT_DIR")?);
    let expected_source_ref = required_env("EXPECTED_SOURCE_REF")?;
    let expected_source_commit = required_env("EXPECTED_SOURCE_COMMIT")?;
    ensure!(
        expected_source_ref == EXPECTED_SOURCE_REF,
        "preview migration source ref must be {EXPECTED_SOURCE_REF}, got {expected_source_ref}"
    );
    ensure!(
        expected_source_commit.len() == 40 && is_lower_hex(&expected_source_commit),
        "preview migration source commit is not a lowercase commit SHA"
    );
    validate_source_checkout(
        &source_checkout,
        &repository,
        &token,
        &expected_source_ref,
        &expected_source_commit,
    )?;
    Ok(LegacyMigrationContext {
        repository,
        token,
        source_checkout,
        expected_source_commit,
    })
}

fn migrate_legacy_preview() -> Result<()> {
    let LegacyMigrationContext {
        repository,
        token,
        source_checkout,
        expected_source_commit,
    } = legacy_migration_context()?;

    let mut publication_lock_retained = false;
    loop {
        let release = github_release_by_tag(&repository, LEGACY_TAG, &token)?;
        let tag_target = git_tag_target(&source_checkout, &token, LEGACY_TAG)?;
        let release_state = classify_release(release.as_ref(), &repository, tag_target.as_deref())?;
        let tag_state = classify_tag_target(tag_target.as_deref(), &expected_source_commit)?;

        if release_state == LegacyReleaseState::Absent && tag_state == LegacyTagState::Legacy {
            ensure_candidate_contains_tag(
                &source_checkout,
                LEGACY_TAG_TARGET,
                &expected_source_commit,
            )?;
        }

        let archive_state = if matches!(release_state, LegacyReleaseState::KnownLegacy)
            || (release_state == LegacyReleaseState::Absent && tag_state == LegacyTagState::Legacy)
        {
            durable_archive_state(&repository, &source_checkout, &token, &legacy_snapshot())?
        } else {
            DurableArchiveState::Missing
        };
        let phase = plan_legacy_migration(
            release_state,
            tag_state,
            archive_state == DurableArchiveState::Verified,
        )?;

        match phase {
            LegacyMigrationPhase::Noop => {
                append_github_env_marker(PREPUBLISH_MARKER)?;
                if release_state == LegacyReleaseState::CurrentContract {
                    println!(
                        "preview migration: rolling release already uses the current contract"
                    );
                } else if tag_state == LegacyTagState::Candidate {
                    println!(
                        "preview migration: rolling preview tag already points at the candidate source"
                    );
                } else {
                    println!("preview migration: no rolling release or tag is present");
                }
                return Ok(());
            }
            LegacyMigrationPhase::Archive => {
                let release = release
                    .as_ref()
                    .context("known legacy rolling release disappeared before archival")?;
                let tag_target = tag_target
                    .as_deref()
                    .context("known legacy rolling release tag is missing")?;
                let snapshot = release.snapshot(&repository, tag_target.to_owned())?;
                ensure_known_legacy_rolling_release(&snapshot)?;
                ensure_candidate_contains_tag(
                    &source_checkout,
                    tag_target,
                    &expected_source_commit,
                )?;

                let runner_temp =
                    env::var_os("RUNNER_TEMP").map_or_else(env::temp_dir, PathBuf::from);
                fs::create_dir_all(&runner_temp).with_context(|| {
                    format!("creating runner temp directory {}", runner_temp.display())
                })?;
                let transaction = tempfile::Builder::new()
                    .prefix("jackin-preview-legacy-migration-")
                    .tempdir_in(&runner_temp)
                    .context("creating preview migration transaction")?;
                let downloaded = transaction.path().join("downloaded-assets");
                fs::create_dir(&downloaded)
                    .context("creating preview migration download directory")?;
                download_release(&repository, LEGACY_TAG, &token, &downloaded)?;
                let archive = archive_known_legacy_rolling_release(
                    &snapshot,
                    &downloaded,
                    transaction.path(),
                )?;
                ensure_verified_legacy_archive(&snapshot, &archive)?;
                ensure_durable_archive(&repository, &source_checkout, &token, &snapshot, &archive)?;
                println!(
                    "::notice::durably archived known invalid rolling preview in draft release {LEGACY_ARCHIVE_TAG}"
                );
            }
            LegacyMigrationPhase::DeleteRelease => {
                let release = release
                    .as_ref()
                    .context("known legacy rolling release disappeared before deletion")?;
                let tag_target = tag_target
                    .as_deref()
                    .context("known legacy rolling release tag disappeared before deletion")?;
                let snapshot = release.snapshot(&repository, tag_target.to_owned())?;
                ensure_known_legacy_rolling_release(&snapshot)?;
                ensure_candidate_contains_tag(
                    &source_checkout,
                    tag_target,
                    &expected_source_commit,
                )?;
                revalidate_legacy_snapshot(&repository, &source_checkout, &token, &snapshot)?;
                ensure!(
                    durable_archive_state(&repository, &source_checkout, &token, &snapshot,)?
                        == DurableArchiveState::Verified,
                    "durable legacy archive is no longer verified; refusing release deletion"
                );
                retain_publication_lock(&mut publication_lock_retained)?;
                delete_release_and_reconcile(
                    &repository,
                    &source_checkout,
                    &token,
                    snapshot.release_id,
                )?;
            }
            LegacyMigrationPhase::AdvanceTag => {
                let tag_target = tag_target
                    .as_deref()
                    .context("rolling preview tag disappeared before advancement")?;
                ensure!(
                    tag_target == LEGACY_TAG_TARGET,
                    "rolling preview tag changed before advancement; refusing mutation"
                );
                let snapshot = legacy_snapshot();
                ensure!(
                    durable_archive_state(&repository, &source_checkout, &token, &snapshot,)?
                        == DurableArchiveState::Verified,
                    "durable legacy archive is no longer verified; refusing tag advancement"
                );
                ensure_candidate_contains_tag(
                    &source_checkout,
                    tag_target,
                    &expected_source_commit,
                )?;
                retain_publication_lock(&mut publication_lock_retained)?;
                advance_tag_and_reconcile(
                    &repository,
                    &source_checkout,
                    &token,
                    &expected_source_commit,
                )?;
            }
        }
    }
}

fn classify_release(
    release: Option<&GithubRelease>,
    repository: &str,
    tag_target: Option<&str>,
) -> Result<LegacyReleaseState> {
    let Some(release) = release else {
        return Ok(LegacyReleaseState::Absent);
    };
    let names = release_asset_names(release)?;
    if names == expected_package_file_names()
        && release
            .snapshot(repository, tag_target.unwrap_or_default().to_owned())
            .is_ok()
    {
        return Ok(LegacyReleaseState::CurrentContract);
    }
    let Some(tag_target) = tag_target else {
        return Ok(LegacyReleaseState::Unknown);
    };
    let Ok(snapshot) = release.snapshot(repository, tag_target.to_owned()) else {
        return Ok(LegacyReleaseState::Unknown);
    };
    if ensure_known_legacy_rolling_release(&snapshot).is_ok() {
        Ok(LegacyReleaseState::KnownLegacy)
    } else {
        Ok(LegacyReleaseState::Unknown)
    }
}

fn release_asset_names(release: &GithubRelease) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for asset in &release.assets {
        ensure!(
            names.insert(asset.name.clone()),
            "rolling preview release contains a duplicate asset: {}",
            asset.name
        );
    }
    Ok(names)
}

fn github_release_by_tag(
    repository: &str,
    tag: &str,
    token: &str,
) -> Result<Option<GithubRelease>> {
    let endpoint = format!("repos/{repository}/releases/tags/{tag}");
    let Some(value) = github_api_json(repository, &endpoint, token)? else {
        return Ok(None);
    };
    serde_json::from_value(value)
        .with_context(|| format!("parsing the GitHub release for tag {tag}"))
        .map(Some)
}

fn github_api_json(repository: &str, endpoint: &str, token: &str) -> Result<Option<Value>> {
    let mut command = crate::cmd::command("gh");
    command
        .args(["api", "--repo", repository, "-i", endpoint])
        .env("GH_TOKEN", token);
    let response = crate::cmd::output_raw(&mut command).context("querying GitHub API")?;
    let text = String::from_utf8(response.stdout).context("GitHub API response is not UTF-8")?;
    let status_line = text
        .lines()
        .rev()
        .find(|line| line.starts_with("HTTP/"))
        .context("GitHub API response omitted an HTTP status")?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .context("GitHub API status is missing")?
        .parse::<u16>()
        .context("GitHub API status is invalid")?;
    let body = text
        .rsplit_once("\r\n\r\n")
        .or_else(|| text.rsplit_once("\n\n"))
        .map_or(text.as_str(), |(_, body)| body);
    if status == 404 {
        return Ok(None);
    }
    ensure!(
        response.success && (200..300).contains(&status),
        "GitHub API request failed with HTTP {status}"
    );
    serde_json::from_str(body)
        .context("parsing GitHub API JSON response")
        .map(Some)
}

fn git_tag_target(source_checkout: &Path, token: &str, tag: &str) -> Result<Option<String>> {
    for refspec in [format!("refs/tags/{tag}^{{}}"), format!("refs/tags/{tag}")] {
        if let Some(target) = git_remote_ref_target(source_checkout, token, &refspec)? {
            return Ok(Some(target));
        }
    }
    Ok(None)
}

fn git_remote_ref_target(
    source_checkout: &Path,
    token: &str,
    refspec: &str,
) -> Result<Option<String>> {
    let mut command = git_checkout_command(source_checkout);
    command
        .args(["ls-remote", "origin", refspec])
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "http.extraheader")
        .env(
            "GIT_CONFIG_VALUE_0",
            format!("AUTHORIZATION: bearer {token}"),
        );
    let output = crate::cmd::output(&mut command).context("reading GitHub source ref")?;
    let Some(line) = String::from_utf8(output)
        .context("GitHub source ref response is not UTF-8")?
        .lines()
        .next()
        .map(str::to_owned)
    else {
        return Ok(None);
    };
    let sha = line
        .split_whitespace()
        .next()
        .context("GitHub source ref response omitted its object")?;
    ensure!(
        sha.len() == 40 && is_lower_hex(sha),
        "GitHub source ref object is not a commit SHA"
    );
    Ok(Some(sha.to_owned()))
}

fn download_release(repository: &str, tag: &str, token: &str, destination: &Path) -> Result<()> {
    let mut command = crate::cmd::command("gh");
    command
        .args(["release", "download", tag, "--repo", repository, "--dir"])
        .arg(destination)
        .args(["--clobber"])
        .env("GH_TOKEN", token);
    crate::cmd::run(&mut command).with_context(|| format!("downloading release assets for {tag}"))
}

fn delete_release(repository: &str, token: &str, release_id: u64) -> Result<()> {
    let endpoint = format!("repos/{repository}/releases/{release_id}");
    let mut command = crate::cmd::command("gh");
    command
        .args(["api", "--method", "DELETE", "--repo", repository, &endpoint])
        .env("GH_TOKEN", token);
    crate::cmd::run(&mut command).context("deleting known legacy preview release")
}

fn preview_tag_patch_api_args(repository: &str, expected_source_commit: &str) -> Vec<String> {
    let endpoint = format!("repos/{repository}/git/refs/tags/{LEGACY_TAG}");
    vec![
        "api".to_owned(),
        "--method".to_owned(),
        "PATCH".to_owned(),
        "--repo".to_owned(),
        repository.to_owned(),
        endpoint,
        "--raw-field".to_owned(),
        format!("sha={expected_source_commit}"),
        "--field".to_owned(),
        "force=false".to_owned(),
    ]
}

fn wait_for_release_absent(repository: &str, token: &str) -> Result<()> {
    for attempt in 0..5 {
        if github_release_by_tag(repository, LEGACY_TAG, token)?.is_none() {
            return Ok(());
        }
        if attempt < 4 {
            thread::sleep(Duration::from_secs(1));
        }
    }
    bail!("known legacy preview release is still visible after deletion")
}

fn wait_for_tag_target(
    source_checkout: &Path,
    token: &str,
    expected_source_commit: &str,
) -> Result<()> {
    for attempt in 0..5 {
        match git_tag_target(source_checkout, token, LEGACY_TAG)? {
            Some(target) if target == expected_source_commit => return Ok(()),
            Some(target) if target == LEGACY_TAG_TARGET => {}
            Some(_) => bail!("rolling preview tag changed during advancement; refusing mutation"),
            None => bail!("rolling preview tag disappeared during advancement"),
        }
        if attempt < 4 {
            thread::sleep(Duration::from_secs(1));
        }
    }
    bail!("rolling preview tag still points at the legacy commit after advancement")
}

fn legacy_snapshot() -> RollingReleaseSnapshot {
    RollingReleaseSnapshot {
        source_repository: LEGACY_SOURCE_REPOSITORY.to_owned(),
        tag_name: LEGACY_TAG.to_owned(),
        release_id: LEGACY_RELEASE_ID,
        release_name: LEGACY_RELEASE_NAME.to_owned(),
        release_body: LEGACY_RELEASE_BODY.to_owned(),
        tag_target: LEGACY_TAG_TARGET.to_owned(),
        draft: false,
        prerelease: true,
        assets: known_legacy_assets(),
    }
}

fn git_checkout_command(source_checkout: &Path) -> Command {
    let mut command = crate::cmd::command("git");
    command.arg("-C").arg(source_checkout);
    command
}

fn git_checkout_output(source_checkout: &Path, args: &[&str]) -> Result<String> {
    let mut command = git_checkout_command(source_checkout);
    command.args(args);
    String::from_utf8(crate::cmd::output(&mut command)?)
        .context("git checkout output is not UTF-8")
        .map(|output| output.trim().to_owned())
}

fn github_repository_from_remote(remote: &str) -> Option<String> {
    let remote = remote.trim().trim_end_matches('/');
    let path = remote
        .strip_prefix("https://github.com/")
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))
        .or_else(|| remote.strip_prefix("git@github.com:"))?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    (!path.is_empty() && path.split('/').count() == 2).then(|| path.to_owned())
}

fn validate_source_checkout(
    source_checkout: &Path,
    repository: &str,
    token: &str,
    expected_source_ref: &str,
    expected_source_commit: &str,
) -> Result<()> {
    ensure!(
        source_checkout.is_dir(),
        "VELNOR_SOURCE_CHECKOUT_DIR is not a directory: {}",
        source_checkout.display()
    );
    ensure!(
        git_checkout_output(source_checkout, &["rev-parse", "--is-inside-work-tree"])? == "true",
        "source checkout is not a Git worktree: {}",
        source_checkout.display()
    );
    let remote = git_checkout_output(source_checkout, &["remote", "get-url", "origin"])
        .context("source checkout origin is required")?;
    ensure!(
        github_repository_from_remote(&remote).as_deref() == Some(repository),
        "source checkout origin does not match {repository}: {remote}"
    );
    let head = git_checkout_output(source_checkout, &["rev-parse", "HEAD"])?;
    ensure!(
        head == expected_source_commit,
        "source checkout HEAD does not match the candidate source commit"
    );
    let candidate_object = format!("{expected_source_commit}^{{commit}}");
    let mut candidate_check = git_checkout_command(source_checkout);
    candidate_check.args(["cat-file", "-e", &candidate_object]);
    crate::cmd::run(&mut candidate_check)
        .context("candidate source commit is not available in the source checkout")?;

    let remote_commit = git_remote_ref_target(source_checkout, token, expected_source_ref)?
        .with_context(|| {
            format!("candidate source ref is missing from origin: {expected_source_ref}")
        })?;
    ensure!(
        remote_commit == expected_source_commit,
        "origin {expected_source_ref} does not resolve to the candidate source commit"
    );
    Ok(())
}

fn ensure_candidate_contains_tag(
    source_checkout: &Path,
    tag_target: &str,
    candidate_source_commit: &str,
) -> Result<()> {
    let tag_object = format!("{tag_target}^{{commit}}");
    let mut tag_check = git_checkout_command(source_checkout);
    tag_check.args(["cat-file", "-e", &tag_object]);
    crate::cmd::run(&mut tag_check)
        .context("known legacy tag target is not available in the source checkout")?;
    let mut ancestry = git_checkout_command(source_checkout);
    ancestry.args([
        "merge-base",
        "--is-ancestor",
        tag_target,
        candidate_source_commit,
    ]);
    crate::cmd::run(&mut ancestry)
        .context("candidate source commit is not a descendant of the known legacy tag")?;
    Ok(())
}

fn append_github_env_marker(name: &str) -> Result<()> {
    let Some(path) = env::var_os(GITHUB_ENV) else {
        return Ok(());
    };
    let path = PathBuf::from(path);
    ensure!(
        path.is_file(),
        "{GITHUB_ENV} points to a missing or non-file path: {}",
        path.display()
    );
    append_env_marker(&path, name, "1")
}

pub(crate) fn append_env_marker(path: &Path, name: &str, value: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte == b'_'),
        "invalid GitHub environment marker name: {name:?}"
    );
    ensure!(
        !value.contains(['\r', '\n']),
        "GitHub environment marker contains a newline"
    );
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .with_context(|| format!("opening {GITHUB_ENV} {}", path.display()))?;
    writeln!(file, "{name}={value}").with_context(|| format!("writing {name} to {GITHUB_ENV}"))?;
    file.sync_all()
        .with_context(|| format!("syncing {GITHUB_ENV} {}", path.display()))
}

fn retain_publication_lock(retained: &mut bool) -> Result<()> {
    if !*retained {
        append_github_env_marker(RETAIN_MARKER)?;
        *retained = true;
    }
    Ok(())
}

fn revalidate_legacy_snapshot(
    repository: &str,
    source_checkout: &Path,
    token: &str,
    expected: &RollingReleaseSnapshot,
) -> Result<()> {
    let current_release = github_release_by_tag(repository, LEGACY_TAG, token)?
        .context("rolling preview disappeared before migration mutation")?;
    let current_target = git_tag_target(source_checkout, token, LEGACY_TAG)?
        .context("rolling preview tag disappeared before migration mutation")?;
    ensure!(
        current_release.snapshot(repository, current_target)? == *expected,
        "rolling preview changed during migration; refusing mutation"
    );
    Ok(())
}

fn delete_release_and_reconcile(
    repository: &str,
    source_checkout: &Path,
    token: &str,
    release_id: u64,
) -> Result<()> {
    if let Err(error) = delete_release(repository, token, release_id) {
        if github_release_by_tag(repository, LEGACY_TAG, token)?.is_some() {
            return Err(error);
        }
        println!("preview migration: release delete reported an error after remote disappearance");
    }
    wait_for_release_absent(repository, token)?;
    let tag_target = git_tag_target(source_checkout, token, LEGACY_TAG)?
        .context("rolling preview tag disappeared after release deletion")?;
    ensure!(
        tag_target == LEGACY_TAG_TARGET,
        "rolling preview tag changed after release deletion; refusing mutation"
    );
    Ok(())
}

fn advance_tag_and_reconcile(
    repository: &str,
    source_checkout: &Path,
    token: &str,
    expected_source_commit: &str,
) -> Result<()> {
    let current_target = git_tag_target(source_checkout, token, LEGACY_TAG)?
        .context("rolling preview tag disappeared before advancement")?;
    ensure!(
        current_target == LEGACY_TAG_TARGET,
        "rolling preview tag changed before advancement; refusing mutation"
    );

    let mut command = crate::cmd::command("gh");
    command
        .args(preview_tag_patch_api_args(
            repository,
            expected_source_commit,
        ))
        .env("GH_TOKEN", token);
    if let Err(error) = crate::cmd::run(&mut command) {
        if git_tag_target(source_checkout, token, LEGACY_TAG)?.as_deref()
            != Some(expected_source_commit)
        {
            return Err(error).context("advancing known legacy preview tag");
        }
        println!("preview migration: tag advancement reported an error after remote update");
    }
    wait_for_tag_target(source_checkout, token, expected_source_commit)
}

fn archive_expected_names(snapshot: &RollingReleaseSnapshot) -> BTreeSet<String> {
    let mut names = snapshot.assets.keys().cloned().collect::<BTreeSet<_>>();
    names.insert(LEGACY_ARCHIVE_MANIFEST.to_owned());
    names
}

fn ensure_archive_release_identity(release: &GithubRelease) -> Result<()> {
    ensure!(
        release.tag_name == LEGACY_ARCHIVE_TAG,
        "legacy archive release has an unexpected tag"
    );
    ensure!(
        release.name == LEGACY_ARCHIVE_RELEASE_NAME,
        "legacy archive release has an unexpected name"
    );
    ensure!(release.draft, "legacy archive release must remain a draft");
    Ok(())
}

fn durable_archive_state(
    repository: &str,
    source_checkout: &Path,
    token: &str,
    snapshot: &RollingReleaseSnapshot,
) -> Result<DurableArchiveState> {
    let release = github_release_by_tag(repository, LEGACY_ARCHIVE_TAG, token)?;
    let tag_target = git_tag_target(source_checkout, token, LEGACY_ARCHIVE_TAG)?;
    match (release, tag_target) {
        (None, None) => Ok(DurableArchiveState::Missing),
        (Some(_), None) | (None, Some(_)) => {
            bail!("legacy archive release and tag are only partially present; refusing mutation")
        }
        (Some(release), Some(tag_target)) => {
            ensure_archive_release_identity(&release)?;
            ensure!(
                tag_target == snapshot.tag_target,
                "legacy archive tag resolves to an unexpected source commit"
            );
            let expected_names = archive_expected_names(snapshot);
            let actual_names = release_asset_names(&release)?;
            ensure!(
                actual_names.is_subset(&expected_names),
                "legacy archive release contains an undeclared asset"
            );
            if actual_names != expected_names {
                ensure_archive_asset_digests(&release, snapshot, None)?;
                return Ok(DurableArchiveState::Incomplete);
            }
            verify_remote_archive(repository, token, &release, snapshot)?;
            Ok(DurableArchiveState::Verified)
        }
    }
}

fn ensure_durable_archive(
    repository: &str,
    source_checkout: &Path,
    token: &str,
    snapshot: &RollingReleaseSnapshot,
    local_archive: &Path,
) -> Result<()> {
    ensure_verified_legacy_archive(snapshot, local_archive)?;
    let mut state = durable_archive_state(repository, source_checkout, token, snapshot)?;
    if state == DurableArchiveState::Missing {
        let mut command = crate::cmd::command("gh");
        command
            .args([
                "release",
                "create",
                LEGACY_ARCHIVE_TAG,
                "--repo",
                repository,
                "--target",
                &snapshot.tag_target,
                "--draft",
                "--title",
                LEGACY_ARCHIVE_RELEASE_NAME,
                "--notes",
                "Verified archive of the retired legacy preview release. The archived bytes are evidence only.",
            ])
            .env("GH_TOKEN", token);
        crate::cmd::run(&mut command).context("creating the durable legacy preview archive")?;
        state = DurableArchiveState::Incomplete;
    }
    ensure!(
        state == DurableArchiveState::Incomplete,
        "durable legacy archive is not resumable"
    );

    let release = github_release_by_tag(repository, LEGACY_ARCHIVE_TAG, token)?
        .context("durable legacy archive release disappeared during upload")?;
    ensure_archive_release_identity(&release)?;
    let tag_target = git_tag_target(source_checkout, token, LEGACY_ARCHIVE_TAG)?
        .context("durable legacy archive tag is missing during upload")?;
    ensure!(
        tag_target == snapshot.tag_target,
        "legacy archive tag changed during upload"
    );
    ensure_archive_asset_digests(&release, snapshot, Some(local_archive))?;
    let existing = release_asset_names(&release)?;
    for path in archive_upload_paths(snapshot, local_archive)? {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("legacy archive asset name is not UTF-8")?;
        if existing.contains(name) {
            continue;
        }
        let mut command = crate::cmd::command("gh");
        command
            .args([
                "release",
                "upload",
                LEGACY_ARCHIVE_TAG,
                "--repo",
                repository,
            ])
            .arg(&path)
            .env("GH_TOKEN", token);
        crate::cmd::run(&mut command)
            .with_context(|| format!("uploading durable legacy archive asset {name}"))?;
    }
    ensure!(
        durable_archive_state(repository, source_checkout, token, snapshot)?
            == DurableArchiveState::Verified,
        "durable legacy archive failed byte verification after upload"
    );
    Ok(())
}

fn archive_upload_paths(
    snapshot: &RollingReleaseSnapshot,
    archive_root: &Path,
) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::with_capacity(snapshot.assets.len() + 1);
    paths.push(archive_root.join(LEGACY_ARCHIVE_MANIFEST));
    for name in snapshot.assets.keys() {
        ensure_safe_asset_name(name)?;
        paths.push(archive_root.join("assets").join(name));
    }
    for path in &paths {
        ensure!(
            path.is_file(),
            "durable legacy archive asset is missing: {}",
            path.display()
        );
    }
    Ok(paths)
}

fn ensure_archive_asset_digests(
    release: &GithubRelease,
    snapshot: &RollingReleaseSnapshot,
    local_archive: Option<&Path>,
) -> Result<()> {
    for asset in &release.assets {
        let expected = if let Some(expected) = snapshot.assets.get(&asset.name) {
            expected.clone()
        } else if asset.name == LEGACY_ARCHIVE_MANIFEST {
            let Some(archive) = local_archive else {
                continue;
            };
            file_sha256(&archive.join(LEGACY_ARCHIVE_MANIFEST))?
        } else {
            bail!(
                "legacy archive release contains an undeclared asset: {}",
                asset.name
            );
        };
        let expected_digest = format!("sha256:{expected}");
        ensure!(
            asset.digest.as_deref() == Some(expected_digest.as_str()),
            "legacy archive asset digest does not match its verified bytes: {}",
            asset.name
        );
    }
    Ok(())
}

fn verify_remote_archive(
    repository: &str,
    token: &str,
    release: &GithubRelease,
    snapshot: &RollingReleaseSnapshot,
) -> Result<()> {
    let runner_temp = env::var_os("RUNNER_TEMP").map_or_else(env::temp_dir, PathBuf::from);
    fs::create_dir_all(&runner_temp).with_context(|| {
        format!(
            "creating archive verification directory {}",
            runner_temp.display()
        )
    })?;
    let transaction = tempfile::tempdir_in(&runner_temp)
        .context("creating durable archive verification transaction")?;
    download_release(repository, LEGACY_ARCHIVE_TAG, token, transaction.path())?;
    ensure_remote_archive_contents(snapshot, transaction.path())?;
    ensure_archive_asset_digests(release, snapshot, Some(transaction.path()))?;
    Ok(())
}

fn ensure_remote_archive_contents(snapshot: &RollingReleaseSnapshot, root: &Path) -> Result<()> {
    ensure!(
        root.is_dir(),
        "downloaded durable archive is not a directory"
    );
    let expected_names = archive_expected_names(snapshot);
    let mut actual_names = BTreeSet::new();
    for entry in fs::read_dir(root).context("reading downloaded durable archive")? {
        let entry = entry.context("reading downloaded durable archive entry")?;
        ensure!(
            entry.file_type()?.is_file(),
            "downloaded durable archive contains a non-file entry"
        );
        let name = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .context("downloaded durable archive asset name is not UTF-8")?;
        ensure!(
            actual_names.insert(name),
            "downloaded durable archive has duplicate assets"
        );
    }
    ensure!(
        actual_names == expected_names,
        "downloaded durable archive asset set is not exact"
    );
    ensure_archive_files(snapshot, &root.join(LEGACY_ARCHIVE_MANIFEST), root)
}

fn ensure_verified_legacy_archive(
    snapshot: &RollingReleaseSnapshot,
    archive_root: &Path,
) -> Result<()> {
    ensure!(
        archive_root.is_dir(),
        "legacy archive root is not a directory: {}",
        archive_root.display()
    );
    let assets = archive_root.join("assets");
    ensure!(
        assets.is_dir(),
        "legacy archive assets directory is missing"
    );
    let entries = fs::read_dir(archive_root).context("reading legacy archive root")?;
    let mut names = BTreeSet::new();
    for entry in entries {
        let entry = entry.context("reading legacy archive root entry")?;
        let name = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .context("legacy archive entry name is not UTF-8")?;
        ensure!(names.insert(name), "legacy archive has duplicate entries");
    }
    ensure!(
        names == BTreeSet::from(["assets".to_owned(), LEGACY_ARCHIVE_MANIFEST.to_owned()]),
        "legacy archive root contains an undeclared entry"
    );
    ensure_archive_files(
        snapshot,
        &archive_root.join(LEGACY_ARCHIVE_MANIFEST),
        &assets,
    )
}

fn ensure_archive_files(
    snapshot: &RollingReleaseSnapshot,
    manifest_path: &Path,
    assets_dir: &Path,
) -> Result<()> {
    let metadata: LegacyArchiveMetadata =
        serde_json::from_slice(&fs::read(manifest_path).with_context(|| {
            format!(
                "reading legacy archive manifest {}",
                manifest_path.display()
            )
        })?)
        .context("parsing legacy archive manifest")?;
    ensure!(
        metadata.schema == LEGACY_ARCHIVE_SCHEMA
            && metadata.verification_status == "verified-legacy-bytes"
            && metadata.phase == LEGACY_ARCHIVE_PHASE
            && metadata.archive_tag == LEGACY_ARCHIVE_TAG
            && metadata.release == *snapshot
            && metadata.assets.len() == snapshot.assets.len(),
        "legacy archive manifest identity is not verified"
    );
    let expected_names = snapshot.assets.keys().cloned().collect::<BTreeSet<_>>();
    let actual_names = fs::read_dir(assets_dir)
        .with_context(|| format!("reading legacy archive assets {}", assets_dir.display()))?
        .map(|entry| {
            let entry = entry.context("reading legacy archive asset entry")?;
            ensure!(
                entry.file_type()?.is_file(),
                "legacy archive asset is not a file"
            );
            entry
                .file_name()
                .to_str()
                .map(str::to_owned)
                .context("legacy archive asset name is not UTF-8")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        actual_names == expected_names,
        "legacy archive asset set is not exact"
    );
    for (name, expected_sha256) in &snapshot.assets {
        let record = metadata
            .assets
            .get(name)
            .with_context(|| format!("legacy archive manifest omits {name}"))?;
        ensure!(
            record.archived_path == format!("assets/{name}")
                && record.expected_sha256 == *expected_sha256
                && record.observed_sha256 == *expected_sha256
                && record.matches_expected,
            "legacy archive manifest does not bind {name} to verified bytes"
        );
        let observed = file_sha256(&assets_dir.join(name))?;
        ensure!(
            observed == *expected_sha256,
            "legacy archive bytes do not match the known digest: {name}"
        );
    }
    Ok(())
}

/// Refuse every invalid rolling release except the one known legacy fingerprint.
///
/// The caller must run the normal package verifier for any release that is not
/// passed to this function. A successful result authorizes archival only; it
/// never authorizes treating the archived bytes as a verified package.
pub(crate) fn ensure_known_legacy_rolling_release(snapshot: &RollingReleaseSnapshot) -> Result<()> {
    ensure!(
        snapshot.source_repository == LEGACY_SOURCE_REPOSITORY
            && snapshot.tag_name == LEGACY_TAG
            && snapshot.release_id == LEGACY_RELEASE_ID
            && snapshot.release_name == LEGACY_RELEASE_NAME
            && snapshot.release_body == LEGACY_RELEASE_BODY
            && snapshot.tag_target == LEGACY_TAG_TARGET
            && !snapshot.draft
            && snapshot.prerelease
            && snapshot.assets == known_legacy_assets(),
        "refusing to migrate unknown or changed rolling preview release fingerprint"
    );
    Ok(())
}

/// Archive the known legacy release as unverified migration evidence.
///
/// This copies the downloaded asset bytes into transaction storage and records
/// both the API digest and the digest of each downloaded byte stream. A digest
/// mismatch is retained as evidence but never upgrades the bytes to trusted
/// release material. The caller must stage and verify a new package separately.
pub(crate) fn archive_known_legacy_rolling_release(
    snapshot: &RollingReleaseSnapshot,
    downloaded_assets: &Path,
    transaction_root: &Path,
) -> Result<PathBuf> {
    ensure_known_legacy_rolling_release(snapshot)?;
    ensure!(
        downloaded_assets.is_dir(),
        "legacy preview asset download is not a directory: {}",
        downloaded_assets.display()
    );

    fs::create_dir_all(transaction_root).with_context(|| {
        format!(
            "creating transaction storage {}",
            transaction_root.display()
        )
    })?;
    let archive_root = transaction_root.join("legacy-preview");
    ensure!(
        !archive_root.exists(),
        "legacy preview transaction archive already exists: {}",
        archive_root.display()
    );

    for entry in fs::read_dir(downloaded_assets).with_context(|| {
        format!(
            "reading legacy preview assets {}",
            downloaded_assets.display()
        )
    })? {
        let entry = entry.context("reading legacy preview asset entry")?;
        let file_type = entry
            .file_type()
            .context("reading legacy preview asset file type")?;
        ensure!(
            file_type.is_file(),
            "legacy preview asset directory contains a non-file entry: {}",
            entry.path().display()
        );
        let name = entry.file_name();
        let name = name
            .to_str()
            .context("legacy preview asset name is not UTF-8")?;
        ensure_safe_asset_name(name)?;
        ensure!(
            snapshot.assets.contains_key(name),
            "legacy preview asset directory contains unexpected asset: {name}"
        );
    }

    let staging = tempfile::tempdir_in(transaction_root)
        .context("creating legacy preview transaction staging directory")?;
    let staged_assets = staging.path().join("assets");
    fs::create_dir(&staged_assets).context("creating archived legacy asset directory")?;
    let mut archived_assets = BTreeMap::new();

    for (name, expected_sha256) in &snapshot.assets {
        ensure_safe_asset_name(name)?;
        let source = downloaded_assets.join(name);
        ensure!(
            source.is_file(),
            "legacy preview asset download is missing {name}"
        );
        let observed_sha256 = file_sha256(&source)?;
        let destination = staged_assets.join(name);
        fs::copy(&source, &destination)
            .with_context(|| format!("archiving legacy preview asset {name}"))?;
        sync_file(&destination)?;
        archived_assets.insert(
            name.clone(),
            ArchivedLegacyAsset {
                archived_path: format!("assets/{name}"),
                matches_expected: observed_sha256 == *expected_sha256,
                expected_sha256: expected_sha256.clone(),
                observed_sha256,
            },
        );
    }

    let verification_status = if archived_assets.values().all(|asset| asset.matches_expected) {
        "verified-legacy-bytes"
    } else {
        "unverified-legacy-bytes"
    };
    let metadata = LegacyArchiveMetadata {
        schema: LEGACY_ARCHIVE_SCHEMA.to_owned(),
        verification_status: verification_status.to_owned(),
        phase: LEGACY_ARCHIVE_PHASE.to_owned(),
        archive_tag: LEGACY_ARCHIVE_TAG.to_owned(),
        release: snapshot.clone(),
        assets: archived_assets,
    };
    let metadata_path = staging.path().join("metadata.json");
    fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&metadata).context("serializing legacy preview archive")?,
    )
    .context("writing legacy preview archive metadata")?;
    sync_file(&metadata_path)?;

    fs::rename(staging.path(), &archive_root).with_context(|| {
        format!(
            "publishing legacy preview transaction archive {}",
            archive_root.display()
        )
    })?;
    sync_file(transaction_root)?;
    Ok(archive_root)
}

fn ensure_safe_asset_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains('/')
            && !name.contains('\\'),
        "unsafe legacy preview asset name: {name:?}"
    );
    Ok(())
}

#[expect(
    clippy::disallowed_methods,
    reason = "release migration archival is host-side xtask filesystem work"
)]
fn file_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("reading {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[expect(
    clippy::disallowed_methods,
    reason = "release migration archival is host-side xtask filesystem work"
)]
fn sync_file(path: &Path) -> Result<()> {
    fs::File::open(path)
        .with_context(|| format!("opening {} for sync", path.display()))?
        .sync_all()
        .with_context(|| format!("syncing {}", path.display()))
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Whether a changed path can alter the shipped Homebrew preview binaries.
pub(crate) fn path_affects_preview(path: &str) -> bool {
    match path {
        ".github/workflows/preview.yml"
        | "Cargo.toml"
        | "Cargo.lock"
        | "rust-toolchain.toml"
        | "build.rs" => true,
        path if path.starts_with("src/") => true,
        path if path.starts_with("docker/runtime/") => true,
        path if path.starts_with("crates/") => true,
        "mise.toml" => false,
        _ => false,
    }
}

/// Whether a mise.toml diff affects release-relevant tool pins.
pub(crate) fn mise_release_tools_changed(base: &str, head: &str) -> bool {
    extract_release_mise(base) != extract_release_mise(head)
}

/// Classify whether any changed path requires a preview republish.
pub(crate) fn classify_preview_source(changed_paths: &[&str], mise_changed: bool) -> bool {
    changed_paths
        .iter()
        .any(|path| path_affects_preview(path) || (*path == "mise.toml" && mise_changed))
}

/// Whether the consumer updater produced changes that must be committed.
///
/// `git status --porcelain --untracked-files=all` is empty only when the
/// updater is a no-op. The dirty branch must commit and push; the clean branch
/// must report that the consumer already references the release.
pub(crate) fn consumer_update_requires_commit(status: &str) -> bool {
    !status.trim().is_empty()
}

fn extract_release_mise(source: &str) -> Vec<String> {
    let mut section = "";
    let mut lines = Vec::new();
    for line in source.lines() {
        if line.trim_start().starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if line == "[tools]" {
            section = "tools";
            continue;
        }
        if line == "[tool_alias]" {
            section = "tool_alias";
            continue;
        }
        if let Some(rest) = line.strip_prefix("[tools.") {
            section = rest.trim_end_matches(']');
            continue;
        }
        if line.starts_with('[') {
            section = "";
            continue;
        }
        let raw_key = line
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"');
        let key = raw_key.strip_prefix("cargo:").unwrap_or(raw_key);
        if keep_release_mise_key(section, key) {
            lines.push(format!("{section}.{key}={line}"));
        }
    }
    lines.sort_unstable();
    lines
}

fn keep_release_mise_key(section: &str, key: &str) -> bool {
    matches!(
        key,
        "zig" | "cosign" | "syft" | "cargo-zigbuild" | "sccache"
    ) && (section == "tools" || section == "tool_alias" || !section.is_empty())
}

/// Parse the canonical source commit from a preview release body.
pub(crate) fn preview_commit_from_body(body: &str) -> Option<String> {
    for line in body.lines() {
        let Some((_, url)) = line.split_once("](") else {
            continue;
        };
        let url = url.trim_end_matches(|ch: char| !ch.is_ascii_hexdigit());
        let sha = url.rsplit('/').next()?;
        if sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Some(sha.to_ascii_lowercase());
        }
    }
    None
}
