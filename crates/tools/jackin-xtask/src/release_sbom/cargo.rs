// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Inventory comes from actual Cargo compiler-artifact records, including fresh
//! cached units. Metadata supplies identities only, never workspace-wide features
//! or a guessed package dependency closure.

use super::{
    BuildEvidence,
    archive::{Member, hash_reader},
};
use crate::{cmd, release_archive::PayloadIdentity};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
pub(super) struct Metadata {
    pub(super) packages: Vec<Package>,
    pub(super) workspace_root: PathBuf,
}
#[derive(Deserialize)]
pub(super) struct Package {
    id: String,
    name: String,
    version: String,
    source: Option<String>,
    manifest_path: PathBuf,
    license: Option<String>,
}
#[derive(Deserialize)]
struct Artifact {
    package_id: String,
    target: Target,
    profile: Profile,
    features: BTreeSet<String>,
    filenames: Vec<PathBuf>,
    executable: Option<PathBuf>,
    fresh: bool,
}
#[derive(Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
}
#[derive(Deserialize)]
struct Profile {
    test: bool,
}

pub(super) type Checksums = BTreeMap<(String, String, String), String>;

pub(super) fn metadata() -> Result<Metadata> {
    // A fresh qualified Cargo home need not contain every workspace source.
    // Full metadata can need other members' dependency manifests: retrieve
    // their locked sources here. Compiler records still own SBOM coverage;
    // this identity lookup never broadens the compiled component inventory.
    let bytes =
        cmd::output(cmd::command("cargo").args(["metadata", "--format-version", "1", "--locked"]))?;
    serde_json::from_slice(&bytes).context("parsing Cargo package identities")
}

#[expect(
    clippy::disallowed_methods,
    reason = "release build input verification reads producer-side Cargo artifacts"
)]
pub(super) fn components(
    bytes: &[u8],
    metadata: &Metadata,
    identity: &PayloadIdentity,
    evidence: &BuildEvidence<'_>,
    members: &BTreeMap<String, Member>,
    checksums: &Checksums,
) -> Result<Vec<Value>> {
    let packages: BTreeMap<_, _> = metadata
        .packages
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let mut units = BTreeMap::<String, BTreeSet<String>>::new();
    let mut binaries = BTreeSet::new();
    let mut finished = false;
    let root_id = metadata
        .packages
        .iter()
        .find(|p| p.name == identity.package && p.source.is_none())
        .context("release Cargo package is absent from metadata")?
        .id
        .as_str();
    for (index, line) in bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .enumerate()
    {
        ensure!(!finished, "Cargo records continue after build-finished");
        let value: Value =
            serde_json::from_slice(line).context("malformed Cargo build JSON record")?;
        match value
            .get("reason")
            .and_then(Value::as_str)
            .context("Cargo record has no reason")?
        {
            "build-started" => {
                ensure!(
                    index == 0,
                    "Cargo build-started must occur once, before all other records"
                );
                validate_build_started(line)?;
            }
            "build-finished" => {
                ensure!(
                    value.get("success") == Some(&Value::Bool(true)),
                    "Cargo build did not succeed"
                );
                finished = true;
            }
            "compiler-artifact" => {
                let artifact: Artifact =
                    serde_json::from_value(value).context("malformed compiler-artifact record")?;
                let package = packages
                    .get(artifact.package_id.as_str())
                    .context("compiler artifact package is absent from Cargo metadata")?;
                ensure!(
                    !artifact.profile.test
                        && !artifact.target.kind.is_empty()
                        && !artifact.filenames.is_empty(),
                    "compiler artifact is not a release build input"
                );
                // Both fresh and newly compiled records describe actual build units.
                let _fresh = artifact.fresh;
                let role = artifact_role(&artifact, evidence)?;
                if artifact.target.kind.iter().any(|kind| kind == "bin") {
                    ensure!(
                        artifact.package_id == root_id
                            && identity.binaries.contains(&artifact.target.name.as_str()),
                        "unexpected root binary in Cargo build records"
                    );
                    let executable = artifact
                        .executable
                        .as_ref()
                        .context("root binary artifact has no executable")?;
                    ensure!(
                        executable == &evidence.layout.target_output().join(&artifact.target.name)
                            && artifact.filenames.contains(executable),
                        "root binary artifact has wrong target/profile path"
                    );
                    let expected = members
                        .get(&artifact.target.name)
                        .context("binary artifact is absent from archive")?;
                    let (size, sha256) = hash_reader(&mut fs::File::open(executable)?)?;
                    ensure!(
                        size == expected.size && sha256 == expected.sha256,
                        "archive member differs from Cargo root binary: {}",
                        artifact.target.name
                    );
                    ensure!(
                        binaries.insert(artifact.target.name.clone()),
                        "duplicate root binary artifact"
                    );
                } else {
                    ensure!(
                        artifact.executable.is_none()
                            || artifact
                                .target
                                .kind
                                .iter()
                                .any(|kind| kind == "custom-build"),
                        "unexpected executable artifact"
                    );
                }
                let unit = json!({"target":artifact.target.name,"kind":artifact.target.kind,"features":artifact.features,"role":role});
                units
                    .entry(package.id.clone())
                    .or_default()
                    .insert(serde_json::to_string(&unit)?);
            }
            "compiler-message" | "build-script-executed" => {}
            _ => anyhow::bail!("unsupported Cargo build record"),
        }
    }
    ensure!(finished, "Cargo build-finished success record is missing");
    ensure!(
        binaries.len() == identity.binaries.len(),
        "Cargo root binary inventory is incomplete"
    );
    ensure!(
        units.len() > 1,
        "Cargo build input inventory has no dependency components"
    );
    let mut result = Vec::new();
    for (id, units) in units {
        let package = packages
            .get(id.as_str())
            .context("unknown compiled package")?;
        result.push(package_component(
            package,
            id == root_id,
            units,
            &metadata.workspace_root,
            checksums,
        )?);
    }
    result.sort_by(|a, b| a["bom-ref"].as_str().cmp(&b["bom-ref"].as_str()));
    Ok(result)
}

fn artifact_role(artifact: &Artifact, evidence: &BuildEvidence<'_>) -> Result<&'static str> {
    let layout = evidence.layout;
    for path in artifact.filenames.iter().chain(artifact.executable.iter()) {
        layout.validate_path(path)?;
    }
    let role = if artifact
        .filenames
        .iter()
        .all(|p| p.starts_with(layout.target_output()))
    {
        "target"
    } else {
        ensure!(
            artifact
                .filenames
                .iter()
                .all(|p| p.starts_with(layout.host_output())),
            "compiler artifact has wrong target or host-build path"
        );
        "host-build"
    };
    if let Some(executable) = &artifact.executable {
        let root = if role == "target" {
            layout.target_output()
        } else {
            layout.host_output()
        };
        ensure!(
            executable.starts_with(root),
            "compiler executable has mixed target/profile custody"
        );
    }
    Ok(role)
}

// Cargo 1.99.1's BuildStarted/RunId grammar, emitted by --timings:
// https://github.com/rust-lang/cargo/blob/5f94df4789f005f9a352888e8355ffc645b7ed0e/src/util/logger.rs
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildStarted {
    reason: String,
    run_id: String,
}

fn validate_build_started(bytes: &[u8]) -> Result<()> {
    let record: BuildStarted =
        serde_json::from_slice(bytes).context("malformed Cargo build-started record")?;
    ensure!(
        record.reason == "build-started",
        "wrong Cargo bookkeeping reason"
    );
    let run_id = record.run_id;
    let (timestamp, hash) = run_id
        .rsplit_once('-')
        .context("malformed Cargo build-started run_id")?;
    ensure!(
        hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid Cargo build-started workspace hash"
    );
    let format = "%Y%m%dT%H%M%S%3fZ";
    let parsed = chrono::NaiveDateTime::parse_from_str(timestamp, format)
        .context("invalid Cargo build-started timestamp")?;
    ensure!(
        parsed.format(format).to_string() == timestamp,
        "noncanonical Cargo build-started timestamp"
    );
    Ok(())
}

fn package_component(
    package: &Package,
    is_root: bool,
    units: BTreeSet<String>,
    workspace: &Path,
    checksums: &Checksums,
) -> Result<Value> {
    let source = match &package.source {
        Some(source) => source.clone(),
        None => format!(
            "workspace:{}",
            package
                .manifest_path
                .strip_prefix(workspace)
                .context("workspace package manifest is outside repository")?
                .to_str()
                .context("non-UTF-8 workspace manifest")?
                .replace('\\', "/")
        ),
    };
    let identity = format!("{}@{}#{source}", package.name, package.version);
    let bom_ref = format!("cargo:{}", super::digest(identity.as_bytes()));
    let mut component = json!({"type":if is_root { "application" } else { "library" }, "bom-ref":bom_ref,"name":package.name,"version":package.version,
            "properties":[{"name":"jackin:cargo:source","value":source},{"name":"jackin:cargo:compiled-units","value":format!("[{}]",units.into_iter().collect::<Vec<_>>().join(","))}]});
    if let Some(license) = &package.license {
        component["licenses"] = json!([{"expression":license}]);
    }
    if let Some(source) = &package.source
        && source.starts_with("registry+")
    {
        ensure!(
            source == "registry+https://github.com/rust-lang/crates.io-index",
            "unsupported Cargo registry distribution source"
        );
        let checksum = checksums
            .get(&(
                package.name.clone(),
                package.version.clone(),
                source.clone(),
            ))
            .context("compiled registry dependency has no Cargo.lock checksum")?;
        component["externalReferences"] = json!([{"type":"distribution","url":format!("https://static.crates.io/crates/{}/{}-{}.crate",package.name,package.name,package.version),"hashes":[{"alg":"SHA-256","content":checksum}]}]);
    }
    Ok(component)
}

pub(super) fn checksums(path: &Path) -> Result<Checksums> {
    let lock: toml::Value = toml::from_str(&fs::read_to_string(path)?)?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .context("Cargo.lock has no packages")?;
    let mut checksums = BTreeMap::new();
    for package in packages {
        if let (Some(source), Some(checksum)) = (
            package.get("source").and_then(toml::Value::as_str),
            package.get("checksum").and_then(toml::Value::as_str),
        ) {
            let name = package
                .get("name")
                .and_then(toml::Value::as_str)
                .context("lock package has no name")?;
            let version = package
                .get("version")
                .and_then(toml::Value::as_str)
                .context("lock package has no version")?;
            ensure!(
                checksum.len() == 64 && checksum.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid Cargo.lock checksum"
            );
            ensure!(
                checksums
                    .insert(
                        (name.into(), version.into(), source.into()),
                        checksum.into()
                    )
                    .is_none(),
                "duplicate Cargo.lock identity"
            );
        }
    }
    Ok(checksums)
}
