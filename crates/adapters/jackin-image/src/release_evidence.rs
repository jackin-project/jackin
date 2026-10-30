// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod freshness;
mod policy;
mod provenance;
mod transport;

use std::{collections::BTreeMap, fmt};

use anyhow::{Context, Result, ensure};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use sha2::{Digest as _, Sha256};
use sigstore_types::{Artifact, Bundle, MediaType, Sha256Hash, SignatureContent};
use sigstore_verify::{IdentityMatcher, VerificationPolicy, Verifier};
use velnor_actions_contract::package_release::{
    PackageReleaseAsset, PackageReleaseReceipt, parse_package_release_receipt,
};

pub(crate) use policy::{release_channel, release_tag};
use transport::{GithubTransport, Transport};

const RECEIPT_LIMIT: usize = 1024 * 1024;
const EVIDENCE_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct VerifiedCapsule {
    pub(crate) sha256: String,
    pub(crate) size: u64,
    publication: Box<freshness::PublicationPin>,
}

pub(crate) async fn verify_release(
    version: &str,
    target: &str,
    source: &str,
) -> Result<VerifiedCapsule> {
    release_channel(version)?;
    let transport = GithubTransport::new()?;
    let root = sigstore_trust_root::TrustedRoot::production().await
        .context("refreshing production Sigstore trust through TUF; refusing unauthenticated capsule download")?;
    let verifier = Verifier::new(&root)?;
    verify_with_transport(&transport, &verifier, version, target, Some(source)).await
}

pub(crate) async fn recheck_release(evidence: &VerifiedCapsule) -> Result<()> {
    evidence.publication.recheck(&GithubTransport::new()?).await
}

fn signing_policy(
    channel: velnor_actions_contract::package_release::PackageReleaseChannel,
) -> VerificationPolicy {
    VerificationPolicy::new(
        IdentityMatcher::Uri(policy::identity(channel).into()),
        policy::ISSUER,
    )
}

fn parse_bundle(bytes: &[u8]) -> Result<Bundle> {
    let bundle: Bundle =
        serde_json::from_slice(bytes).context("parsing standard Sigstore bundle")?;
    ensure!(
        bundle.media_type == MediaType::Bundle0_3,
        "capsule evidence requires standard Sigstore bundle v0.3"
    );
    sigstore_bundle::validate_bundle(&bundle).context("invalid Sigstore bundle structure")?;
    Ok(bundle)
}

fn asset<'a>(receipt: &'a PackageReleaseReceipt, name: &str) -> Result<&'a PackageReleaseAsset> {
    receipt
        .assets
        .iter()
        .find(|a| a.name == name)
        .with_context(|| format!("release receipt missing {name}"))
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn verify_bytes(asset: &PackageReleaseAsset, bytes: &[u8]) -> Result<()> {
    ensure!(
        asset.size == bytes.len() as u64 && asset.sha256 == digest(bytes),
        "receipt size/SHA-256 mismatch for {}",
        asset.name
    );
    Ok(())
}

async fn signed_asset(
    transport: &impl Transport,
    verifier: &Verifier,
    receipt: &PackageReleaseReceipt,
    base: &str,
    name: &str,
) -> Result<()> {
    let signature_name = format!("{name}.bundle");
    let bytes = transport
        .get(&format!("{base}/{signature_name}"), EVIDENCE_LIMIT)
        .await?;
    verify_bytes(asset(receipt, &signature_name)?, &bytes)?;
    let bundle = parse_bundle(&bytes)?;
    ensure!(
        matches!(bundle.content, SignatureContent::MessageSignature(_)),
        "detached asset requires a message signature"
    );
    let hash = Sha256Hash::from_hex(&asset(receipt, name)?.sha256)?;
    let result = verifier
        .verify(
            Artifact::from_digest(hash),
            &bundle,
            &signing_policy(receipt.channel),
        )
        .with_context(|| format!("verifying detached signature for {name}"))?;
    policy::validate_claims(&result, receipt)
}

async fn verify_with_transport(
    transport: &impl Transport,
    verifier: &Verifier,
    version: &str,
    target: &str,
    source: Option<&str>,
) -> Result<VerifiedCapsule> {
    let source =
        source.context("native release origin is required before remote evidence acquisition")?;
    release_channel(version)?;
    ensure!(
        policy::LINUX_TARGETS.contains(&target),
        "unsupported capsule target"
    );
    let channel = release_channel(version)?;
    let base = format!(
        "https://github.com/jackin-project/jackin/releases/download/{}",
        release_tag(version, source)?
    );
    let receipt_bytes = transport
        .get(&format!("{base}/release.json"), RECEIPT_LIMIT)
        .await?;
    let bundle_bytes = transport
        .get(&format!("{base}/release.json.bundle"), EVIDENCE_LIMIT)
        .await?;
    let bundle = parse_bundle(&bundle_bytes)?;
    ensure!(
        matches!(bundle.content, SignatureContent::MessageSignature(_)),
        "release receipt requires a detached message signature"
    );

    let result = verifier
        .verify(receipt_bytes.as_slice(), &bundle, &signing_policy(channel))
        .context("authenticating signed release.json")?;
    let receipt = parse_package_release_receipt(&receipt_bytes)
        .context("validating canonical Velnor release receipt")?;
    policy::validate_receipt_source(&receipt, version, source)?;
    policy::validate_claims(&result, &receipt)?;

    let name = format!("jackin-capsule-{target}.tar.gz");
    let selected = asset(&receipt, &name)?;
    signed_asset(transport, verifier, &receipt, &base, &name).await?;
    let companion_bytes = transport
        .get(&format!("{base}/capsule-manifest.json"), RECEIPT_LIMIT)
        .await?;
    verify_bytes(asset(&receipt, "capsule-manifest.json")?, &companion_bytes)?;
    signed_asset(
        transport,
        verifier,
        &receipt,
        &base,
        "capsule-manifest.json",
    )
    .await?;
    validate_companion(&companion_bytes, &receipt, version)?;

    let mut subjects = BTreeMap::new();
    for product in &receipt.products {
        for payload in &product.payloads {
            subjects.insert(
                payload.name.clone(),
                asset(&receipt, &payload.name)?.sha256.clone(),
            );
        }
    }
    subjects.insert(
        "capsule-manifest.json".into(),
        asset(&receipt, "capsule-manifest.json")?.sha256.clone(),
    );
    subjects.insert("release.json".into(), digest(&receipt_bytes));
    provenance::verify(transport, verifier, &receipt, &selected.sha256, &subjects).await?;
    let publication = freshness::pin(transport, &receipt, &receipt_bytes, &bundle_bytes).await?;
    Ok(VerifiedCapsule {
        publication,
        sha256: selected.sha256.clone(),
        size: selected.size,
    })
}

const CAPSULE_MANIFEST_V2_SCHEMA: &str = "jackin.capsule-manifest.v2";
const CAPSULE_FEATURE_USAGE_RELAY_PROXY_V2: &str = "usage-relay-proxy-v2";
const CAPSULE_BINARY_MEMBER: &str = "jackin-capsule";

#[derive(Debug)]
enum StrictJson {
    Scalar,
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

fn insert_unique_json_object_entry<'de, A>(
    values: &mut BTreeMap<String, StrictJson>,
    map: &mut A,
    key: String,
) -> std::result::Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    if values.contains_key(&key) {
        return Err(de::Error::custom(format!(
            "duplicate JSON object key {key:?}"
        )));
    }
    values.insert(key, map.next_value()?);
    Ok(())
}

impl<'de> Deserialize<'de> for StrictJson {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StrictJsonVisitor;

        impl<'de> Visitor<'de> for StrictJsonVisitor {
            type Value = StrictJson;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON value with unique object keys")
            }

            fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(StrictJson::Scalar)
            }

            fn visit_bool<E>(self, _value: bool) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(StrictJson::Scalar)
            }

            fn visit_i64<E>(self, _value: i64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(StrictJson::Scalar)
            }

            fn visit_u64<E>(self, _value: u64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(StrictJson::Scalar)
            }

            fn visit_f64<E>(self, _value: f64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(StrictJson::Scalar)
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(StrictJson::String(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(StrictJson::String(value))
            }

            fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element()? {
                    values.push(value);
                }
                Ok(StrictJson::Array(values))
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    insert_unique_json_object_entry(&mut values, &mut map, key)?;
                }
                Ok(StrictJson::Object(values))
            }
        }

        deserializer.deserialize_any(StrictJsonVisitor)
    }
}

fn object<'a>(value: &'a StrictJson, label: &str) -> Result<&'a BTreeMap<String, StrictJson>> {
    match value {
        StrictJson::Object(fields) => Ok(fields),
        _ => anyhow::bail!("{label} must be an object"),
    }
}

fn string<'a>(value: &'a StrictJson, label: &str) -> Result<&'a str> {
    match value {
        StrictJson::String(value) => Ok(value),
        _ => anyhow::bail!("{label} must be a string"),
    }
}

fn exact_fields(
    fields: &BTreeMap<String, StrictJson>,
    expected: &[&str],
    label: &str,
) -> Result<()> {
    anyhow::ensure!(
        fields.len() == expected.len() && expected.iter().all(|key| fields.contains_key(*key)),
        "{label} has missing or unknown fields"
    );
    Ok(())
}

fn canonical_sha256(value: &str, label: &str) -> Result<()> {
    anyhow::ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{label} must be a canonical lowercase SHA-256 digest"
    );
    Ok(())
}

fn validate_companion(bytes: &[u8], receipt: &PackageReleaseReceipt, version: &str) -> Result<()> {
    let manifest: StrictJson =
        serde_json::from_slice(bytes).context("parsing authenticated capsule companion")?;
    let fields = object(&manifest, "capsule companion")?;
    let is_v2 = match fields.get("schema") {
        None => false,
        Some(schema) => {
            anyhow::ensure!(
                string(schema, "capsule companion schema")? == CAPSULE_MANIFEST_V2_SCHEMA,
                "unsupported authenticated capsule companion schema"
            );
            true
        }
    };
    if is_v2 {
        exact_fields(fields, &["schema", "version", "targets"], "V2 companion")?;
    } else {
        exact_fields(fields, &["version", "targets"], "legacy companion")?;
    }
    anyhow::ensure!(
        string(
            fields
                .get("version")
                .context("capsule companion version missing")?,
            "version",
        )? == version,
        "capsule companion version mismatch"
    );
    let targets = object(
        fields
            .get("targets")
            .context("capsule companion targets missing")?,
        "capsule companion targets",
    )?;
    anyhow::ensure!(
        targets.len() == policy::LINUX_TARGETS.len()
            && policy::LINUX_TARGETS
                .iter()
                .all(|target| targets.contains_key(*target)),
        "capsule companion must contain exactly the two supported Linux targets"
    );
    let mut declared_features: Option<Vec<String>> = None;
    for target in policy::LINUX_TARGETS {
        let value = targets.get(target).context("capsule target missing")?;
        let archive_sha256 = if is_v2 {
            let target_fields = object(value, "V2 capsule target")?;
            exact_fields(
                target_fields,
                &[
                    "archive_sha256",
                    "binary_member",
                    "binary_sha256",
                    "features",
                ],
                "V2 capsule target",
            )?;
            let binary_member = string(
                target_fields
                    .get("binary_member")
                    .context("binary member missing")?,
                "binary member",
            )?;
            anyhow::ensure!(
                binary_member == CAPSULE_BINARY_MEMBER,
                "V2 capsule target names an unsupported binary member"
            );
            let binary_sha256 = string(
                target_fields
                    .get("binary_sha256")
                    .context("binary digest missing")?,
                "binary digest",
            )?;
            canonical_sha256(binary_sha256, "binary digest")?;
            let Some(StrictJson::Array(features)) = target_fields.get("features") else {
                anyhow::bail!("V2 capsule target features must be an array");
            };
            anyhow::ensure!(!features.is_empty(), "V2 target must declare a feature");
            let mut target_features: Vec<String> = Vec::new();
            for feature in features {
                let feature = string(feature, "capsule feature")?;
                anyhow::ensure!(
                    feature == CAPSULE_FEATURE_USAGE_RELAY_PROXY_V2,
                    "unsupported authenticated capsule feature"
                );
                if let Some(previous) = target_features.last() {
                    anyhow::ensure!(
                        previous.as_str() < feature,
                        "capsule features must be unique and sorted"
                    );
                }
                target_features.push(feature.to_owned());
            }
            if let Some(previous) = &declared_features {
                anyhow::ensure!(
                    previous.as_slice() == target_features.as_slice(),
                    "V2 targets must declare the same capsule features"
                );
            } else {
                declared_features = Some(target_features);
            }
            string(
                target_fields
                    .get("archive_sha256")
                    .context("archive digest missing")?,
                "archive digest",
            )?
        } else {
            string(value, "legacy target archive digest")?
        };
        canonical_sha256(archive_sha256, "archive digest")?;
        let name = format!("jackin-capsule-{target}.tar.gz");
        let expected_archive_sha256 = asset(receipt, &name)?;
        anyhow::ensure!(
            archive_sha256 == expected_archive_sha256.sha256.as_str(),
            "capsule companion target/digest mismatch for {target}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
