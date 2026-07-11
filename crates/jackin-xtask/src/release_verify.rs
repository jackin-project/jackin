#![expect(
    clippy::print_stdout,
    reason = "release-verify writes its verification report to stdout"
)]

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use clap::Args;
use sha2::{Digest, Sha256};

const CERTIFICATE_IDENTITY_REGEXP: &str = "https://github.com/jackin-project/jackin/";
const CERTIFICATE_OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";
const GITHUB_REPO: &str = "jackin-project/jackin";

#[derive(Debug, Args)]
pub(crate) struct ReleaseVerifyArgs {
    /// Release archive to verify.
    archive: PathBuf,
    /// Skip the GitHub artifact attestation check. Use only when verifying an
    /// unreleased local archive before CI has produced provenance.
    #[arg(long)]
    skip_attestation: bool,
    /// Skip cosign bundle verification. Use only for digest-only tamper drills.
    #[arg(long)]
    skip_signature: bool,
}

pub(crate) fn run(args: ReleaseVerifyArgs) -> Result<()> {
    let archive = args.archive;
    ensure!(
        archive.is_file(),
        "archive does not exist or is not a file: {}",
        archive.display()
    );

    let sha_path = sibling_with_suffix(&archive, "sha256");
    let bundle_path = sibling_with_suffix(&archive, "bundle");
    let sbom_path = sibling_with_suffix(&archive, "sbom.json");

    verify_sha256_file(&archive, &sha_path)?;
    println!("ok: sha256 digest matches {}", sha_path.display());

    if args.skip_signature {
        println!("skip: cosign bundle verification");
    } else {
        verify_cosign_bundle(&archive, &bundle_path)?;
        println!("ok: cosign bundle verifies {}", bundle_path.display());
    }

    if args.skip_attestation {
        println!("skip: GitHub artifact attestation");
    } else {
        verify_github_attestation(&archive)?;
        println!("ok: GitHub artifact attestation verifies");
    }

    verify_sbom(&sbom_path)?;
    println!("ok: SBOM parses {}", sbom_path.display());
    Ok(())
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(".");
    value.push(suffix);
    PathBuf::from(value)
}

fn verify_sha256_file(archive: &Path, sha_path: &Path) -> Result<()> {
    ensure!(
        sha_path.is_file(),
        "sha256 sidecar does not exist: {}",
        sha_path.display()
    );
    let expected = read_expected_sha256(sha_path)?;
    let actual = archive_sha256(archive)?;
    ensure!(
        actual == expected,
        "sha256 mismatch for {}: expected {expected}, got {actual}",
        archive.display()
    );
    Ok(())
}

fn read_expected_sha256(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading sha256 sidecar {}", path.display()))?;
    let digest = content
        .split_whitespace()
        .next()
        .with_context(|| format!("sha256 sidecar is empty: {}", path.display()))?;
    ensure!(
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "sha256 sidecar has invalid digest {digest:?}: {}",
        path.display()
    );
    Ok(digest.to_ascii_lowercase())
}

fn archive_sha256(path: &Path) -> Result<String> {
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

fn verify_cosign_bundle(archive: &Path, bundle_path: &Path) -> Result<()> {
    ensure!(
        bundle_path.is_file(),
        "cosign bundle sidecar does not exist: {}",
        bundle_path.display()
    );
    let archive_arg = path_arg(archive)?;
    let bundle_arg = path_arg(bundle_path)?;
    run_checked(
        Command::new("cosign").args([
            "verify-blob",
            "--bundle",
            bundle_arg,
            "--certificate-identity-regexp",
            CERTIFICATE_IDENTITY_REGEXP,
            "--certificate-oidc-issuer",
            CERTIFICATE_OIDC_ISSUER,
            archive_arg,
        ]),
        "cosign verify-blob",
    )
}

fn verify_github_attestation(archive: &Path) -> Result<()> {
    let archive_arg = path_arg(archive)?;
    run_checked(
        Command::new("gh").args(["attestation", "verify", archive_arg, "--repo", GITHUB_REPO]),
        "gh attestation verify",
    )
}

fn verify_sbom(path: &Path) -> Result<()> {
    ensure!(
        path.is_file(),
        "SBOM sidecar does not exist: {}",
        path.display()
    );
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str::<serde_json::Value>(&content)
        .with_context(|| format!("parsing SBOM JSON {}", path.display()))?;
    Ok(())
}

fn path_arg(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn run_checked(cmd: &mut Command, _label: &str) -> Result<()> {
    crate::cmd::run(cmd)
}


#[cfg(test)]
mod tests;
