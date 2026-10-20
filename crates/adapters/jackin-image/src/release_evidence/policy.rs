// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, ensure};
use sigstore_verify::VerificationResult;
use velnor_actions_contract::{
    config::RustBinaryReleaseCompanion,
    package_release::{PackageReleaseChannel, PackageReleasePayloadFormat, PackageReleaseReceipt},
    package_release_plan::{
        PACKAGE_RELEASE_PLAN_SCHEMA_V1, PackageReleasePlan, PackageReleasePlanProduct,
    },
};

pub(super) const REPOSITORY: &str = "https://github.com/jackin-project/jackin";
pub(super) const ISSUER: &str = "https://token.actions.githubusercontent.com";
pub(super) const PREVIEW_RELEASE_ID: u64 = 328_385_904;

pub(super) fn identity(channel: PackageReleaseChannel) -> &'static str {
    match channel {
        PackageReleaseChannel::Preview => {
            "https://github.com/jackin-project/jackin/.github/workflows/binary-preview.yml@refs/heads/main"
        }
        PackageReleaseChannel::Stable => {
            "https://github.com/jackin-project/jackin/.github/workflows/binary-release.yml@refs/heads/main"
        }
    }
}

pub(crate) fn release_channel(version: &str) -> Result<PackageReleaseChannel> {
    if version.contains("-preview.") {
        preview_base(version)?;
        Ok(PackageReleaseChannel::Preview)
    } else {
        validate_base(version)?;
        Ok(PackageReleaseChannel::Stable)
    }
}

pub(crate) fn release_tag(version: &str) -> Result<String> {
    Ok(match release_channel(version)? {
        PackageReleaseChannel::Preview => "preview".to_owned(),
        PackageReleaseChannel::Stable => format!("v{version}"),
    })
}

fn validate_base(base: &str) -> Result<()> {
    let parts: Vec<_> = base.split('.').collect();
    ensure!(
        parts.len() == 3
            && parts.iter().all(|part| !part.is_empty()
                && (*part == "0" || !part.starts_with('0'))
                && part.bytes().all(|b| b.is_ascii_digit())
                && part.parse::<u64>().is_ok()),
        "invalid release base version"
    );
    Ok(())
}

pub(super) const LINUX_TARGETS: [&str; 2] =
    ["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"];

fn preview_base(version: &str) -> Result<&str> {
    let (base, suffix) = version
        .split_once("-preview.")
        .context("remote preview capsule requires X.Y.Z-preview.N+sha7 version")?;
    validate_base(base)?;
    let (count, sha) = suffix
        .split_once('+')
        .context("preview version requires count and sha7")?;
    ensure!(
        !count.starts_with('0')
            && count.bytes().all(|b| b.is_ascii_digit())
            && count.parse::<u64>().is_ok_and(|n| n > 0)
            && sha.len() == 7
            && sha
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid preview version"
    );
    Ok(base)
}

fn trusted_plan(version: &str) -> Result<PackageReleasePlan> {
    Ok(PackageReleasePlan {
        schema: PACKAGE_RELEASE_PLAN_SCHEMA_V1.into(),
        channel: release_channel(version)?,
        repository: "jackin-project/jackin".into(),
        default_branch: "main".into(),
        base_version: match release_channel(version)? {
            PackageReleaseChannel::Preview => preview_base(version)?.into(),
            PackageReleaseChannel::Stable => version.into(),
        },
        products: vec![
            PackageReleasePlanProduct {
                id: "jackin".into(),
                package: "jackin".into(),
                bins: vec![
                    "jackin".into(),
                    "jackin-role".into(),
                    "jackin-usage-broker".into(),
                ],
                targets: [
                    "aarch64-apple-darwin",
                    "aarch64-unknown-linux-gnu",
                    "x86_64-apple-darwin",
                    "x86_64-unknown-linux-gnu",
                ]
                .map(str::to_owned)
                .to_vec(),
                format: PackageReleasePayloadFormat::TarGz,
            },
            PackageReleasePlanProduct {
                id: "jackin-capsule".into(),
                package: "jackin-capsule".into(),
                bins: vec!["jackin-capsule".into()],
                targets: LINUX_TARGETS.map(str::to_owned).to_vec(),
                format: PackageReleasePayloadFormat::TarGz,
            },
        ],
        companions: vec![RustBinaryReleaseCompanion {
            id: "capsule-manifest".into(),
            path: "capsule-manifest.json".into(),
            asset_name: "capsule-manifest.json".into(),
        }],
    })
}

pub(super) fn validate_receipt(receipt: &PackageReleaseReceipt, version: &str) -> Result<()> {
    receipt.validate_against_plan(&trusted_plan(version)?)?;
    ensure!(
        receipt.repository.id == 1_197_700_841,
        "release repository numeric identity mismatch"
    );
    ensure!(
        receipt.version.value == version && receipt.tag == release_tag(version)?,
        "release version/tag mismatch"
    );
    Ok(())
}

pub(super) fn validate_receipt_source(
    receipt: &PackageReleaseReceipt,
    version: &str,
    source: &str,
) -> Result<()> {
    validate_receipt(receipt, version)?;
    ensure!(
        receipt.source.commit_sha == source,
        "release source differs from the native application build"
    );
    Ok(())
}

pub(super) fn validate_claims(
    result: &VerificationResult,
    receipt: &PackageReleaseReceipt,
) -> Result<()> {
    ensure!(
        result.certificate_verified()
            && result.sct_verified()
            && result.tlog_verified()
            && result.identity_policy_checked()
            && !result.verified_timestamps().is_empty(),
        "incomplete authenticated Sigstore evidence"
    );
    let claims = &result
        .certificate()
        .context("missing authenticated signing certificate")?
        .ci_claims;
    validate_ci_claims(claims, receipt)
}

pub(super) fn validate_ci_claims(
    claims: &sigstore_crypto::FulcioCiClaims,
    receipt: &PackageReleaseReceipt,
) -> Result<()> {
    validate_source_claims(claims, receipt)?;
    let invocation = format!(
        "{REPOSITORY}/actions/runs/{}/attempts/{}",
        receipt.workflow.run_id, receipt.workflow.run_attempt
    );
    let expected = [
        (
            "build signer URI",
            claims.build_signer_uri.as_deref(),
            identity(receipt.channel),
        ),
        (
            "build signer digest",
            claims.build_signer_digest.as_deref(),
            receipt.workflow.authority_sha.as_str(),
        ),
        (
            "build config URI",
            claims.build_config_uri.as_deref(),
            identity(receipt.channel),
        ),
        (
            "build config digest",
            claims.build_config_digest.as_deref(),
            receipt.workflow.authority_sha.as_str(),
        ),
        (
            "trigger",
            claims.build_trigger.as_deref(),
            receipt.channel.event().as_str(),
        ),
        (
            "runner",
            claims.runner_environment.as_deref(),
            "github-hosted",
        ),
        (
            "invocation",
            claims.run_invocation_uri.as_deref(),
            invocation.as_str(),
        ),
        (
            "visibility",
            claims.source_repository_visibility_at_signing.as_deref(),
            "public",
        ),
    ];
    for (name, actual, value) in expected {
        ensure!(
            actual == Some(value),
            "authenticated certificate {name} mismatch"
        );
    }
    Ok(())
}

fn validate_source_claims(
    claims: &sigstore_crypto::FulcioCiClaims,
    receipt: &PackageReleaseReceipt,
) -> Result<()> {
    for (name, actual, value) in [
        (
            "source URI",
            claims.source_repository_uri.as_deref(),
            REPOSITORY,
        ),
        (
            "source ID",
            claims.source_repository_identifier.as_deref(),
            "1197700841",
        ),
        (
            "source ref",
            claims.source_repository_ref.as_deref(),
            "refs/heads/main",
        ),
        (
            "source digest",
            claims.source_repository_digest.as_deref(),
            receipt.source.commit_sha.as_str(),
        ),
        (
            "owner URI",
            claims.source_repository_owner_uri.as_deref(),
            "https://github.com/jackin-project",
        ),
        (
            "owner ID",
            claims.source_repository_owner_identifier.as_deref(),
            "273541039",
        ),
    ] {
        ensure!(
            actual == Some(value),
            "authenticated certificate {name} mismatch"
        );
    }
    Ok(())
}
