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
    collections::BTreeMap,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

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

const LEGACY_ARCHIVE_SCHEMA: &str = "jackin.preview-legacy-archive.v1";
const LEGACY_SOURCE_REPOSITORY: &str = "jackin-project/jackin";
const LEGACY_TAG: &str = "preview";
const LEGACY_RELEASE_ID: u64 = 328_385_904;
const LEGACY_RELEASE_NAME: &str = "Preview 0.6.4-preview.1181+a506eee";
const LEGACY_RELEASE_BODY: &str = "Preview build from [a506eee](https://github.com/jackin-project/jackin/commit/a506eee0581ef7add4f615281dbb90d828d5a657).";
const LEGACY_TAG_TARGET: &str = "1c623d10e9f7e9072db4bd8ef027d4c24cd95ab3";

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
    fn snapshot(&self, tag_target: String) -> Result<RollingReleaseSnapshot> {
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
            source_repository: LEGACY_SOURCE_REPOSITORY.to_owned(),
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

#[derive(Debug, Serialize)]
struct LegacyArchiveMetadata {
    schema: String,
    verification_status: String,
    release: RollingReleaseSnapshot,
    assets: BTreeMap<String, ArchivedLegacyAsset>,
}

#[derive(Debug, Serialize)]
struct ArchivedLegacyAsset {
    archived_path: String,
    expected_sha256: String,
    observed_sha256: String,
    matches_expected: bool,
}

/// Return the exact public asset fingerprint observed for the one-time migration.
fn known_legacy_assets() -> BTreeMap<String, String> {
    LEGACY_ASSETS
        .iter()
        .map(|(name, digest)| ((*name).to_owned(), (*digest).to_owned()))
        .collect()
}

fn migrate_legacy_preview() -> Result<()> {
    let repository =
        env::var("GITHUB_REPOSITORY").unwrap_or_else(|_| LEGACY_SOURCE_REPOSITORY.to_owned());
    ensure!(
        repository == LEGACY_SOURCE_REPOSITORY,
        "preview migration must run in {LEGACY_SOURCE_REPOSITORY}, got {repository}"
    );
    let token = env::var("GH_TOKEN").context("GH_TOKEN is required for preview migration")?;
    ensure!(!token.is_empty(), "preview migration token is empty");
    let source_checkout = match env::var_os("VELNOR_SOURCE_CHECKOUT_DIR") {
        Some(path) => PathBuf::from(path),
        None => env::current_dir().context("locating preview migration checkout")?,
    };

    let Some(release) = github_release_by_tag(&token)? else {
        ensure!(
            git_tag_target(&source_checkout, &token, LEGACY_TAG)?.is_none(),
            "rolling preview tag exists without a release; refusing unverified mutation"
        );
        println!("preview migration: no rolling release or tag is present");
        return Ok(());
    };

    // A current-contract release is handled by the generic typed publisher.
    // Only the exact, known invalid fingerprint enters this destructive path.
    let names = release_asset_names(&release)?;
    if names.contains("release-manifest.json") && names.contains("identity.json") {
        println!("preview migration: rolling release already uses the current contract");
        return Ok(());
    }

    let tag_target = git_tag_target(&source_checkout, &token, LEGACY_TAG)?
        .context("known legacy rolling release tag is missing")?;
    let snapshot = release.snapshot(tag_target.clone())?;
    ensure_known_legacy_rolling_release(&snapshot)?;

    let runner_temp = env::var_os("RUNNER_TEMP").map_or_else(env::temp_dir, PathBuf::from);
    fs::create_dir_all(&runner_temp)
        .with_context(|| format!("creating runner temp directory {}", runner_temp.display()))?;
    let transaction = tempfile::Builder::new()
        .prefix("jackin-preview-legacy-migration-")
        .tempdir_in(&runner_temp)
        .context("creating preview migration transaction")?;
    let downloaded = transaction.path().join("downloaded-assets");
    fs::create_dir(&downloaded).context("creating preview migration download directory")?;
    download_rolling_release(&token, &downloaded)?;
    let archive = archive_known_legacy_rolling_release(&snapshot, &downloaded, transaction.path())?;
    println!(
        "::notice::archived known invalid rolling preview as unverified evidence at {}",
        archive.display()
    );
    // Keep the transaction archive if any later observation or mutation step
    // fails. It is never used as trusted package input.
    let _transaction_root = transaction.keep();

    let current_release = github_release_by_tag(&token)?
        .context("rolling preview disappeared before migration mutation")?;
    let current_target = git_tag_target(&source_checkout, &token, LEGACY_TAG)?
        .context("rolling preview tag disappeared before migration mutation")?;
    ensure!(
        current_release.snapshot(current_target.clone())? == snapshot,
        "rolling preview changed during migration; refusing mutation"
    );

    delete_rolling_release(&token, snapshot.release_id)?;
    wait_for_release_absent(&token)?;
    let tag_after_release_delete = git_tag_target(&source_checkout, &token, LEGACY_TAG)?
        .context("rolling preview tag disappeared after release deletion")?;
    ensure!(
        tag_after_release_delete == snapshot.tag_target,
        "rolling preview tag changed during migration; refusing mutation"
    );
    delete_rolling_tag(&token)?;
    wait_for_tag_absent(&source_checkout, &token)?;
    println!("preview migration: retired the known invalid rolling release and tag");

    Ok(())
}

fn release_asset_names(release: &GithubRelease) -> Result<std::collections::BTreeSet<String>> {
    let mut names = std::collections::BTreeSet::new();
    for asset in &release.assets {
        ensure!(
            names.insert(asset.name.clone()),
            "rolling preview release contains a duplicate asset: {}",
            asset.name
        );
    }
    Ok(names)
}

fn github_release_by_tag(token: &str) -> Result<Option<GithubRelease>> {
    let endpoint = format!("repos/{LEGACY_SOURCE_REPOSITORY}/releases/tags/{LEGACY_TAG}");
    let Some(value) = github_api_json(&endpoint, token)? else {
        return Ok(None);
    };
    serde_json::from_value(value)
        .context("parsing the rolling preview GitHub release")
        .map(Some)
}

fn github_api_json(endpoint: &str, token: &str) -> Result<Option<Value>> {
    let mut command = crate::cmd::command("gh");
    command
        .args(["api", "--repo", LEGACY_SOURCE_REPOSITORY, "-i", endpoint])
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
        let mut command = crate::cmd::command("git");
        command
            .args([
                "-C",
                source_checkout
                    .to_str()
                    .context("source checkout path is not UTF-8")?,
                "ls-remote",
                "origin",
                &refspec,
            ])
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.extraheader")
            .env(
                "GIT_CONFIG_VALUE_0",
                format!("AUTHORIZATION: bearer {token}"),
            );
        let output = crate::cmd::output(&mut command).context("reading rolling preview tag")?;
        let Some(line) = String::from_utf8(output)
            .context("rolling preview tag response is not UTF-8")?
            .lines()
            .next()
            .map(str::to_owned)
        else {
            continue;
        };
        let sha = line
            .split_whitespace()
            .next()
            .context("rolling preview tag response omitted its object")?;
        ensure!(
            sha.len() == 40 && is_lower_hex(sha),
            "rolling preview tag object is not a commit SHA"
        );
        return Ok(Some(sha.to_owned()));
    }
    Ok(None)
}

fn download_rolling_release(token: &str, destination: &Path) -> Result<()> {
    let mut command = crate::cmd::command("gh");
    command
        .args([
            "release",
            "download",
            LEGACY_TAG,
            "--repo",
            LEGACY_SOURCE_REPOSITORY,
            "--dir",
        ])
        .arg(destination)
        .args(["--clobber"])
        .env("GH_TOKEN", token);
    crate::cmd::run(&mut command).context("downloading known legacy preview assets")
}

fn delete_rolling_release(token: &str, release_id: u64) -> Result<()> {
    let endpoint = format!("repos/{LEGACY_SOURCE_REPOSITORY}/releases/{release_id}");
    let mut command = crate::cmd::command("gh");
    command
        .args([
            "api",
            "--method",
            "DELETE",
            "--repo",
            LEGACY_SOURCE_REPOSITORY,
            &endpoint,
        ])
        .env("GH_TOKEN", token);
    crate::cmd::run(&mut command).context("deleting known legacy preview release")
}

fn delete_rolling_tag(token: &str) -> Result<()> {
    let endpoint = format!("repos/{LEGACY_SOURCE_REPOSITORY}/git/refs/tags/{LEGACY_TAG}");
    let mut command = crate::cmd::command("gh");
    command
        .args([
            "api",
            "--method",
            "DELETE",
            "--repo",
            LEGACY_SOURCE_REPOSITORY,
            &endpoint,
        ])
        .env("GH_TOKEN", token);
    crate::cmd::run(&mut command).context("deleting known legacy preview tag")
}

fn wait_for_release_absent(token: &str) -> Result<()> {
    for attempt in 0..5 {
        if github_release_by_tag(token)?.is_none() {
            return Ok(());
        }
        if attempt < 4 {
            thread::sleep(Duration::from_secs(1));
        }
    }
    bail!("known legacy preview release is still visible after deletion")
}

fn wait_for_tag_absent(source_checkout: &Path, token: &str) -> Result<()> {
    for attempt in 0..5 {
        if git_tag_target(source_checkout, token, LEGACY_TAG)?.is_none() {
            return Ok(());
        }
        if attempt < 4 {
            thread::sleep(Duration::from_secs(1));
        }
    }
    bail!("known legacy preview tag is still visible after deletion")
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

    let metadata = LegacyArchiveMetadata {
        schema: LEGACY_ARCHIVE_SCHEMA.to_owned(),
        verification_status: "unverified-legacy-bytes".to_owned(),
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
        let key = line
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"');
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
