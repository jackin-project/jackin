// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Verification of the immutable six-payload preview package handoff.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use clap::Args;
use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::Value;
use tar::Archive;

use super::{
    archive_sha256, sibling_with_suffix, verify_cosign_bundle, verify_sbom, verify_sha256_file,
};
use crate::fs_util::read_dir_sorted;

#[cfg(test)]
mod tests;

const SOURCE_REPOSITORY: &str = "jackin-project/jackin";
const SOURCE_REF: &str = "refs/heads/main";
const MANIFEST_SCHEMA: &str = "velnor.package-release.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreviewPayload {
    name: &'static str,
    target: &'static str,
    binary: &'static str,
}

const PAYLOADS: [PreviewPayload; 6] = [
    PreviewPayload {
        name: "jackin-aarch64-apple-darwin.tar.gz",
        target: "aarch64-apple-darwin",
        binary: "jackin",
    },
    PreviewPayload {
        name: "jackin-x86_64-apple-darwin.tar.gz",
        target: "x86_64-apple-darwin",
        binary: "jackin",
    },
    PreviewPayload {
        name: "jackin-aarch64-unknown-linux-gnu.tar.gz",
        target: "aarch64-unknown-linux-gnu",
        binary: "jackin",
    },
    PreviewPayload {
        name: "jackin-x86_64-unknown-linux-gnu.tar.gz",
        target: "x86_64-unknown-linux-gnu",
        binary: "jackin",
    },
    PreviewPayload {
        name: "jackin-capsule-aarch64-unknown-linux-gnu.tar.gz",
        target: "aarch64-unknown-linux-gnu",
        binary: "jackin-capsule",
    },
    PreviewPayload {
        name: "jackin-capsule-x86_64-unknown-linux-gnu.tar.gz",
        target: "x86_64-unknown-linux-gnu",
        binary: "jackin-capsule",
    },
];

const SUPPORTING_ASSETS: [&str; 21] = [
    "jackin-aarch64-apple-darwin.tar.gz.sha256",
    "jackin-aarch64-apple-darwin.tar.gz.bundle",
    "jackin-aarch64-apple-darwin.tar.gz.sbom.json",
    "jackin-x86_64-apple-darwin.tar.gz.sha256",
    "jackin-x86_64-apple-darwin.tar.gz.bundle",
    "jackin-x86_64-apple-darwin.tar.gz.sbom.json",
    "jackin-aarch64-unknown-linux-gnu.tar.gz.sha256",
    "jackin-aarch64-unknown-linux-gnu.tar.gz.bundle",
    "jackin-aarch64-unknown-linux-gnu.tar.gz.sbom.json",
    "jackin-x86_64-unknown-linux-gnu.tar.gz.sha256",
    "jackin-x86_64-unknown-linux-gnu.tar.gz.bundle",
    "jackin-x86_64-unknown-linux-gnu.tar.gz.sbom.json",
    "jackin-capsule-aarch64-unknown-linux-gnu.tar.gz.sha256",
    "jackin-capsule-aarch64-unknown-linux-gnu.tar.gz.bundle",
    "jackin-capsule-aarch64-unknown-linux-gnu.tar.gz.sbom.json",
    "jackin-capsule-x86_64-unknown-linux-gnu.tar.gz.sha256",
    "jackin-capsule-x86_64-unknown-linux-gnu.tar.gz.bundle",
    "jackin-capsule-x86_64-unknown-linux-gnu.tar.gz.sbom.json",
    "SHA256SUMS",
    "capsule-manifest.json",
    "capsule-manifest.json.bundle",
];

#[derive(Debug, Args)]
pub(crate) struct ReleaseVerifyPackageArgs {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageAsset {
    name: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageManifest {
    assets: Vec<PackageAsset>,
    schema: String,
    source_commit: String,
    source_ref: String,
    source_repository: String,
    supporting_assets: Vec<PackageAsset>,
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageIdentity {
    manifest: Value,
    source_digest: String,
    source_ref: String,
    source_repository: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapsuleManifest {
    targets: BTreeMap<String, String>,
    version: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BinaryFormat {
    Elf64,
    MachO64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BinaryArchitecture {
    Aarch64,
    X86_64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BinaryMetadata {
    format: BinaryFormat,
    architecture: BinaryArchitecture,
}

pub(crate) fn run_package(_args: ReleaseVerifyPackageArgs) -> Result<()> {
    let package_dir = env::var_os("VELNOR_VERIFIED_PACKAGE_DIR")
        .map(PathBuf::from)
        .context("missing VELNOR_VERIFIED_PACKAGE_DIR")?;
    let source_checkout =
        env::var_os("VELNOR_SOURCE_CHECKOUT_DIR").map_or_else(|| PathBuf::from("."), PathBuf::from);
    verify_preview_package(&package_dir, &source_checkout)?;
    println!("ok: verified preview package {}", package_dir.display());
    Ok(())
}

pub(crate) fn verify_preview_package(package_dir: &Path, source_checkout: &Path) -> Result<()> {
    ensure!(
        package_dir.is_dir(),
        "verified package directory does not exist or is not a directory: {}",
        package_dir.display()
    );
    verify_context()?;
    verify_exact_package_files(package_dir)?;

    let manifest_path = package_dir.join("release-manifest.json");
    let identity_path = package_dir.join("identity.json");
    let manifest_value = read_json(&manifest_path)?;
    let manifest: PackageManifest = serde_json::from_value(manifest_value.clone())
        .with_context(|| format!("parsing release manifest {}", manifest_path.display()))?;
    let identity_value = read_json(&identity_path)?;
    let identity: PackageIdentity = serde_json::from_value(identity_value)
        .with_context(|| format!("parsing package identity {}", identity_path.display()))?;

    verify_manifest_provenance(&manifest, &identity, &manifest_value)?;
    verify_source_checkout(source_checkout, &manifest)?;

    let payload_digests = verify_manifest_assets(
        package_dir,
        &manifest.assets,
        &PAYLOADS
            .iter()
            .map(|payload| payload.name)
            .collect::<Vec<_>>(),
        "payload",
    )?;
    let supporting_digests = verify_manifest_assets(
        package_dir,
        &manifest.supporting_assets,
        &SUPPORTING_ASSETS,
        "supporting asset",
    )?;

    verify_sha256_sums(package_dir, &payload_digests)?;
    verify_capsule_manifest(package_dir, &manifest.version, &payload_digests)?;

    for payload in PAYLOADS {
        verify_archive(package_dir, payload)?;
        verify_binary(package_dir, payload, &manifest.version)?;
    }

    ensure!(
        supporting_digests.len() == SUPPORTING_ASSETS.len(),
        "preview package support asset set is incomplete"
    );
    Ok(())
}

fn verify_context() -> Result<()> {
    for (name, expected) in [
        ("EXPECTED_MANIFEST_SCHEMA", MANIFEST_SCHEMA),
        ("EXPECTED_SOURCE_REPOSITORY", SOURCE_REPOSITORY),
        ("EXPECTED_SOURCE_REF", SOURCE_REF),
        ("VELNOR_SOURCE_REF", SOURCE_REF),
        ("VELNOR_PACKAGE_CHANNEL", "preview"),
    ] {
        if let Some(actual) = env::var_os(name) {
            let actual = actual
                .to_str()
                .with_context(|| format!("{name} is not valid UTF-8"))?;
            ensure!(
                actual == expected,
                "{name} must be {expected:?}, got {actual:?}"
            );
        }
    }
    Ok(())
}

fn verify_exact_package_files(package_dir: &Path) -> Result<()> {
    let expected = expected_file_names();
    let mut actual = BTreeSet::new();
    for entry in read_dir_sorted(package_dir)
        .with_context(|| format!("reading package directory {}", package_dir.display()))?
    {
        let file_type = entry
            .file_type()
            .with_context(|| format!("stating package entry in {}", package_dir.display()))?;
        ensure!(
            file_type.is_file(),
            "verified package directory contains a non-file entry: {}",
            entry.path().display()
        );
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("package filename is not valid UTF-8"))?;
        actual.insert(name);
    }
    if actual != expected {
        let missing = expected.difference(&actual).cloned().collect::<Vec<_>>();
        let extra = actual.difference(&expected).cloned().collect::<Vec<_>>();
        anyhow::bail!("verified package file set mismatch; missing {missing:?}, extra {extra:?}");
    }
    Ok(())
}

pub(crate) fn expected_file_names() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    names.insert("release-manifest.json".to_owned());
    names.insert("identity.json".to_owned());
    for payload in PAYLOADS {
        names.insert(payload.name.to_owned());
    }
    names.extend(SUPPORTING_ASSETS.map(str::to_owned));
    names
}

fn read_json(path: &Path) -> Result<Value> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    ensure!(!bytes.is_empty(), "JSON file is empty: {}", path.display());
    serde_json::from_slice(&bytes).with_context(|| format!("parsing JSON {}", path.display()))
}

fn verify_manifest_provenance(
    manifest: &PackageManifest,
    identity: &PackageIdentity,
    manifest_value: &Value,
) -> Result<()> {
    ensure!(
        manifest.schema == MANIFEST_SCHEMA,
        "unexpected package manifest schema"
    );
    ensure!(
        manifest.source_repository == SOURCE_REPOSITORY,
        "package manifest source repository is not {SOURCE_REPOSITORY}"
    );
    ensure!(
        manifest.source_ref == SOURCE_REF,
        "package manifest source ref is not {SOURCE_REF}"
    );
    validate_commit(&manifest.source_commit, "manifest source_commit")?;
    validate_preview_version(&manifest.version, &manifest.source_commit)?;
    ensure!(
        identity.manifest == *manifest_value,
        "identity manifest does not equal release manifest"
    );
    ensure!(
        identity.source_digest == manifest.source_commit,
        "identity source_digest does not equal manifest source_commit"
    );
    ensure!(
        identity.source_ref == manifest.source_ref,
        "identity source_ref does not equal manifest source_ref"
    );
    ensure!(
        identity.source_repository == manifest.source_repository,
        "identity source_repository does not equal manifest source_repository"
    );
    Ok(())
}

fn verify_source_checkout(source_checkout: &Path, manifest: &PackageManifest) -> Result<()> {
    ensure!(
        source_checkout.is_dir(),
        "source checkout directory does not exist: {}",
        source_checkout.display()
    );
    let actual_commit = git_output(source_checkout, &["rev-parse", "HEAD"])?;
    validate_commit(&actual_commit, "source checkout HEAD")?;
    ensure!(
        actual_commit == manifest.source_commit,
        "package manifest source_commit does not match source checkout HEAD"
    );
    let status = git_output(
        source_checkout,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    ensure!(
        status.is_empty(),
        "source checkout is not clean; refusing to verify a package from mutable source"
    );
    verify_source_index_state(source_checkout)?;
    let remote = git_output(source_checkout, &["config", "--get", "remote.origin.url"])?;
    let repository = github_repository(&remote)?;
    ensure!(
        repository == SOURCE_REPOSITORY,
        "source checkout origin is not {SOURCE_REPOSITORY}: {remote}"
    );
    let remote_main = git_remote_branch(source_checkout, &remote, "refs/heads/main")?;
    validate_commit(&remote_main, "origin/main")?;
    ensure!(
        remote_main == manifest.source_commit,
        "package manifest source_commit does not match the live origin/main"
    );
    for name in ["EXPECTED_SOURCE_COMMIT", "VELNOR_SOURCE_COMMIT"] {
        if let Some(expected) = env::var_os(name) {
            let expected = expected
                .to_str()
                .with_context(|| format!("{name} is not valid UTF-8"))?;
            ensure!(
                expected == actual_commit,
                "{name} does not match source checkout HEAD"
            );
        }
    }

    Ok(())
}

fn verify_source_index_state(source_checkout: &Path) -> Result<()> {
    let assume_unchanged = git_output(source_checkout, &["ls-files", "-v", "-z"])?;
    for record in assume_unchanged
        .split('\0')
        .filter(|record| !record.is_empty())
    {
        ensure!(
            !record.starts_with("h "),
            "source checkout has an assume-unchanged index entry: {}",
            record[2..].trim()
        );
    }

    let skip_worktree = git_output(source_checkout, &["ls-files", "-t", "-z"])?;
    for record in skip_worktree
        .split('\0')
        .filter(|record| !record.is_empty())
    {
        ensure!(
            !record.starts_with("S "),
            "source checkout has a skip-worktree index entry: {}",
            record[2..].trim()
        );
    }
    Ok(())
}

fn git_remote_branch(source_checkout: &Path, remote: &str, reference: &str) -> Result<String> {
    let output = git_output(source_checkout, &["ls-remote", remote, reference])?;
    let mut lines = output.lines();
    let line = lines
        .next()
        .with_context(|| format!("{remote} did not publish {reference}"))?;
    ensure!(
        lines.next().is_none(),
        "{remote} published multiple results for {reference}"
    );
    let mut fields = line.split_whitespace();
    let commit = fields
        .next()
        .with_context(|| format!("{remote} response omitted the object for {reference}"))?;
    let actual_reference = fields
        .next()
        .with_context(|| format!("{remote} response omitted the ref for {reference}"))?;
    ensure!(
        fields.next().is_none() && actual_reference == reference,
        "{remote} response did not identify {reference} exactly"
    );
    Ok(commit.to_owned())
}

fn git_output(source_checkout: &Path, args: &[&str]) -> Result<String> {
    let mut command = crate::cmd::command("git");
    command.arg("-C").arg(source_checkout).args(args);
    let output = crate::cmd::output(&mut command)?;
    String::from_utf8(output)
        .context("git output is not valid UTF-8")
        .map(|output| output.trim().to_owned())
}

fn github_repository(remote: &str) -> Result<String> {
    let repository = remote
        .strip_prefix("https://github.com/")
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))
        .or_else(|| remote.strip_prefix("git@github.com:"))
        .context("source checkout origin is not a GitHub repository URL")?;
    let repository = repository.strip_suffix(".git").unwrap_or(repository);
    ensure!(
        !repository.is_empty() && !repository.ends_with('/') && !repository.contains(['?', '#']),
        "source checkout origin has an invalid GitHub repository path: {remote}"
    );
    Ok(repository.to_owned())
}

fn validate_commit(value: &str, label: &str) -> Result<()> {
    ensure!(
        value.len() == 40 && is_lower_hex(value),
        "{label} must be a 40-character lowercase hex commit: {value:?}"
    );
    Ok(())
}

fn validate_preview_version(version: &str, source_commit: &str) -> Result<()> {
    let (base_and_channel, commit_suffix) = version
        .split_once('+')
        .context("preview version is missing its source commit suffix")?;
    let source_prefix = source_commit.get(..7).unwrap_or("");
    ensure!(
        !commit_suffix.contains('+')
            && commit_suffix.len() == 7
            && is_lower_hex(commit_suffix)
            && commit_suffix == source_prefix,
        "preview version does not bind its source commit: {version:?}"
    );
    let (base, build) = base_and_channel
        .split_once("-preview.")
        .context("preview version is missing the -preview.<build> channel")?;
    ensure!(
        base.split('.').count() == 3
            && base
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            && !build.is_empty()
            && build.bytes().all(|byte| byte.is_ascii_digit()),
        "preview version is not X.Y.Z-preview.N+<commit>: {version:?}"
    );
    Ok(())
}

fn verify_manifest_assets(
    package_dir: &Path,
    assets: &[PackageAsset],
    expected_names: &[&str],
    kind: &str,
) -> Result<BTreeMap<String, String>> {
    ensure!(
        assets.len() == expected_names.len(),
        "preview package has {} {} entries; expected {}",
        assets.len(),
        kind,
        expected_names.len()
    );
    let expected = expected_names.iter().copied().collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    let mut digests = BTreeMap::new();
    for asset in assets {
        ensure!(
            expected.contains(asset.name.as_str()),
            "manifest declares an unexpected {kind}: {}",
            asset.name
        );
        ensure!(
            actual.insert(asset.name.as_str()),
            "manifest declares duplicate {kind}: {}",
            asset.name
        );
        ensure!(
            is_lower_hex(&asset.sha256) && asset.sha256.len() == 64,
            "manifest {kind} has an invalid SHA256: {}",
            asset.name
        );
        let path = package_dir.join(&asset.name);
        let metadata =
            fs::metadata(&path).with_context(|| format!("stating {kind} {}", path.display()))?;
        ensure!(
            metadata.is_file() && metadata.len() > 0,
            "{kind} is missing or empty: {}",
            path.display()
        );
        let actual_digest = archive_sha256(&path)?;
        ensure!(
            actual_digest == asset.sha256,
            "{kind} checksum mismatch: {}",
            asset.name
        );
        digests.insert(asset.name.clone(), asset.sha256.clone());
    }
    ensure!(
        actual == expected,
        "manifest {kind} names do not equal the required asset set"
    );
    Ok(digests)
}

fn verify_sha256_sums(
    package_dir: &Path,
    payload_digests: &BTreeMap<String, String>,
) -> Result<()> {
    let path = package_dir.join("SHA256SUMS");
    let content =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let expected_names = PAYLOADS
        .iter()
        .map(|payload| payload.name)
        .collect::<BTreeSet<_>>();
    let mut actual_names = BTreeSet::new();
    for line in content.lines() {
        let mut fields = line.split_whitespace();
        let digest = fields.next().context("SHA256SUMS contains an empty line")?;
        let name = fields
            .next()
            .context("SHA256SUMS line is missing its filename")?;
        ensure!(
            fields.next().is_none(),
            "SHA256SUMS line has too many fields: {line:?}"
        );
        let name = name.strip_prefix('*').unwrap_or(name);
        ensure!(
            expected_names.contains(name),
            "SHA256SUMS names an undeclared payload: {name}"
        );
        ensure!(
            actual_names.insert(name),
            "SHA256SUMS names a payload more than once: {name}"
        );
        ensure!(
            digest.len() == 64 && is_lower_hex(digest),
            "SHA256SUMS has an invalid digest for {name}"
        );
        let expected = payload_digests
            .get(name)
            .with_context(|| format!("manifest has no digest for payload {name}"))?;
        ensure!(
            digest == expected,
            "SHA256SUMS digest disagrees with manifest for {name}"
        );
        let actual = archive_sha256(&package_dir.join(name))?;
        ensure!(
            digest == actual,
            "SHA256SUMS digest does not match payload {name}"
        );
    }
    ensure!(
        actual_names == expected_names,
        "SHA256SUMS does not name exactly the six preview payloads"
    );
    Ok(())
}

fn verify_archive(package_dir: &Path, payload: PreviewPayload) -> Result<()> {
    let archive = package_dir.join(payload.name);
    let checksum = sibling_with_suffix(&archive, "sha256");
    let bundle = sibling_with_suffix(&archive, "bundle");
    let sbom = sibling_with_suffix(&archive, "sbom.json");
    let expected = read_strict_sha256(&checksum, payload.name)?;
    let actual = archive_sha256(&archive)?;
    ensure!(
        actual == expected,
        "archive checksum sidecar does not match {}",
        payload.name
    );
    verify_sha256_file(&archive, &checksum)?;
    verify_cosign_bundle(&archive, &bundle)
        .with_context(|| format!("verifying archive cosign bundle for {}", payload.name))?;
    verify_sbom(&archive, &sbom)
        .with_context(|| format!("verifying archive SBOM for {}", payload.name))?;
    Ok(())
}

fn read_strict_sha256(path: &Path, expected_name: &str) -> Result<String> {
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    ensure!(
        !content.is_empty() && content.len() <= 4096,
        "invalid checksum sidecar size: {}",
        path.display()
    );
    let mut lines = content.lines();
    let line = lines.next().context("checksum sidecar is empty")?;
    ensure!(
        lines.next().is_none(),
        "checksum sidecar must contain one line: {}",
        path.display()
    );
    let mut fields = line.split_whitespace();
    let digest = fields
        .next()
        .context("checksum sidecar is missing its digest")?;
    if let Some(name) = fields.next() {
        let name = name.strip_prefix('*').unwrap_or(name);
        ensure!(
            name == expected_name,
            "checksum sidecar names the wrong archive: {path:?}"
        );
    }
    ensure!(
        fields.next().is_none(),
        "checksum sidecar has too many fields: {}",
        path.display()
    );
    ensure!(
        digest.len() == 64 && is_lower_hex(digest),
        "checksum sidecar has an invalid digest: {}",
        path.display()
    );
    Ok(digest.to_owned())
}

fn verify_capsule_manifest(
    package_dir: &Path,
    package_version: &str,
    payload_digests: &BTreeMap<String, String>,
) -> Result<()> {
    verify_capsule_manifest_with(
        package_dir,
        package_version,
        payload_digests,
        verify_cosign_bundle,
    )
}

fn verify_capsule_manifest_with<F>(
    package_dir: &Path,
    package_version: &str,
    payload_digests: &BTreeMap<String, String>,
    verify_bundle: F,
) -> Result<()>
where
    F: FnOnce(&Path, &Path) -> Result<()>,
{
    let manifest_path = package_dir.join("capsule-manifest.json");
    let bundle_path = package_dir.join("capsule-manifest.json.bundle");
    let capsule = read_capsule_manifest(&manifest_path)?;
    let expected = expected_capsule_targets(payload_digests)?;
    validate_capsule_manifest(&capsule, package_version, &expected)?;
    verify_bundle(&manifest_path, &bundle_path)
        .context("verifying capsule-manifest.json cosign bundle")?;
    Ok(())
}

fn read_capsule_manifest(path: &Path) -> Result<CapsuleManifest> {
    let value = read_json(path)?;
    serde_json::from_value(value)
        .with_context(|| format!("parsing capsule manifest {}", path.display()))
}

fn expected_capsule_targets(
    payload_digests: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let mut expected = BTreeMap::new();
    for payload in PAYLOADS
        .iter()
        .filter(|payload| payload.binary == "jackin-capsule")
    {
        let digest = payload_digests
            .get(payload.name)
            .with_context(|| format!("missing {} capsule payload digest", payload.target))?;
        ensure!(
            expected
                .insert(payload.target.to_owned(), digest.clone())
                .is_none(),
            "duplicate capsule target in preview payload contract: {}",
            payload.target
        );
    }
    ensure!(
        expected.len() == 2,
        "preview package must declare exactly two capsule targets"
    );
    Ok(expected)
}

fn validate_capsule_manifest(
    capsule: &CapsuleManifest,
    package_version: &str,
    expected_targets: &BTreeMap<String, String>,
) -> Result<()> {
    ensure!(
        capsule.version == package_version,
        "capsule manifest version does not equal package version"
    );
    ensure!(
        capsule.targets == *expected_targets,
        "capsule manifest target mapping does not equal the capsule payload digests"
    );
    for (target, digest) in &capsule.targets {
        ensure!(
            digest.len() == 64 && is_lower_hex(digest),
            "capsule manifest has an invalid digest for {target}"
        );
    }
    Ok(())
}

fn verify_binary(package_dir: &Path, payload: PreviewPayload, version: &str) -> Result<()> {
    #[expect(
        clippy::disallowed_methods,
        reason = "preview package verification is host CLI tooling, not a render/runtime thread"
    )]
    let archive = fs::File::open(package_dir.join(payload.name))
        .with_context(|| format!("opening {}", payload.name))?;
    let decoder = GzDecoder::new(archive);
    let mut tar = Archive::new(decoder);
    let mut binary = None;
    for entry in tar
        .entries()
        .with_context(|| format!("reading archive entries from {}", payload.name))?
    {
        let mut entry =
            entry.with_context(|| format!("reading archive entry from {}", payload.name))?;
        let path = entry
            .path()
            .context("reading release archive entry path")?
            .into_owned();
        if path != Path::new(payload.binary) {
            continue;
        }
        ensure!(
            binary.is_none(),
            "release archive contains duplicate {}",
            payload.binary
        );
        ensure!(
            entry.header().entry_type().is_file(),
            "release archive member is not a regular file: {}",
            payload.binary
        );
        let mode = entry
            .header()
            .mode()
            .context("reading release archive mode")?;
        ensure!(
            mode & 0o111 != 0,
            "release archive binary is not executable: {}",
            payload.binary
        );
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .with_context(|| format!("reading {} from {}", payload.binary, payload.name))?;
        binary = Some(bytes);
    }
    let bytes =
        binary.with_context(|| format!("{} is missing from {}", payload.binary, payload.name))?;
    verify_binary_metadata(&bytes, payload.target)?;
    if target_is_runnable(payload.target) {
        verify_runnable_version(&bytes, payload.binary, version)?;
    }
    Ok(())
}

fn verify_binary_metadata(bytes: &[u8], target: &str) -> Result<()> {
    let actual = binary_metadata(bytes)?;
    let (expected_format, expected_architecture) = expected_binary_metadata(target)?;
    ensure!(
        actual.format == expected_format && actual.architecture == expected_architecture,
        "binary metadata does not match target {target}: got {actual:?}"
    );
    Ok(())
}

fn expected_binary_metadata(target: &str) -> Result<(BinaryFormat, BinaryArchitecture)> {
    let format = if target.ends_with("-apple-darwin") {
        BinaryFormat::MachO64
    } else if target.ends_with("-unknown-linux-gnu") {
        BinaryFormat::Elf64
    } else {
        anyhow::bail!("unsupported preview binary target: {target}");
    };
    let architecture = if target.starts_with("aarch64-") {
        BinaryArchitecture::Aarch64
    } else if target.starts_with("x86_64-") {
        BinaryArchitecture::X86_64
    } else {
        anyhow::bail!("unsupported preview binary architecture: {target}");
    };
    Ok((format, architecture))
}

fn binary_metadata(bytes: &[u8]) -> Result<BinaryMetadata> {
    if bytes.starts_with(b"\x7fELF") {
        ensure!(bytes.len() >= 20, "ELF binary header is truncated");
        ensure!(bytes[4] == 2, "preview binary is not a 64-bit ELF");
        let machine = match bytes[5] {
            1 => u16::from_le_bytes([bytes[18], bytes[19]]),
            2 => u16::from_be_bytes([bytes[18], bytes[19]]),
            other => anyhow::bail!("ELF binary has unsupported byte order {other}"),
        };
        let architecture = match machine {
            62 => BinaryArchitecture::X86_64,
            183 => BinaryArchitecture::Aarch64,
            other => anyhow::bail!("ELF binary has unsupported machine {other}"),
        };
        return Ok(BinaryMetadata {
            format: BinaryFormat::Elf64,
            architecture,
        });
    }

    let (big_endian, magic) = match bytes.get(..4) {
        Some([0xcf, 0xfa, 0xed, 0xfe]) => (false, "Mach-O 64-bit"),
        Some([0xfe, 0xed, 0xfa, 0xcf]) => (true, "Mach-O 64-bit"),
        _ => anyhow::bail!("binary has no supported ELF or Mach-O 64-bit header"),
    };
    ensure!(bytes.len() >= 8, "Mach-O binary header is truncated");
    let cputype = if big_endian {
        u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
    } else {
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
    };
    let architecture = match cputype {
        0x0100_0007 => BinaryArchitecture::X86_64,
        0x0100_000c => BinaryArchitecture::Aarch64,
        other => anyhow::bail!("{magic} binary has unsupported CPU type {other:#x}"),
    };
    Ok(BinaryMetadata {
        format: BinaryFormat::MachO64,
        architecture,
    })
}

fn target_is_runnable(target: &str) -> bool {
    match target {
        "aarch64-apple-darwin" => cfg!(all(target_os = "macos", target_arch = "aarch64")),
        "x86_64-apple-darwin" => cfg!(all(target_os = "macos", target_arch = "x86_64")),
        "aarch64-unknown-linux-gnu" => cfg!(all(target_os = "linux", target_arch = "aarch64")),
        "x86_64-unknown-linux-gnu" => cfg!(all(target_os = "linux", target_arch = "x86_64")),
        _ => false,
    }
}

fn verify_runnable_version(bytes: &[u8], binary: &str, version: &str) -> Result<()> {
    let directory = tempfile::tempdir().context("creating binary version probe directory")?;
    let path = directory.path().join(binary);
    fs::write(&path, bytes).with_context(|| format!("writing version probe {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions)?;
    }
    let mut command = crate::cmd::command(&path);
    command.arg("--version");
    let output = crate::cmd::output(&mut command)
        .with_context(|| format!("running {binary} --version from release archive"))?;
    let expected = format!("{binary} {version}\n");
    ensure!(
        output == expected.as_bytes(),
        "{binary} --version output does not equal {expected:?}"
    );
    Ok(())
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
