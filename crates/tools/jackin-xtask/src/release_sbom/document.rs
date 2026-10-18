// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{
    archive::{self, Member},
    archive_sha256,
};
use crate::release_archive::PayloadIdentity;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const COVERAGE: &str = "Actual Cargo build inputs for the selected package and Rust target, including host build dependencies and proc-macros. Not a per-executable linked-library inventory. Native/system dependencies and the Rust standard library are not inventoried.";

pub(super) fn build(
    archive: &Path,
    version: &str,
    identity: &PayloadIdentity,
    members: &BTreeMap<String, Member>,
    mut components: Vec<Value>,
    workspace: &Path,
) -> Result<Value> {
    for (name, member) in members {
        components.push(json!({"type":"file","bom-ref":format!("executable:{name}"),"name":name,"version":version,"hashes":[{"alg":"SHA-256","content":member.sha256}],"properties":[{"name":"jackin:executable:size","value":member.size.to_string()}]}));
    }
    components.sort_by(|a, b| a["bom-ref"].as_str().cmp(&b["bom-ref"].as_str()));
    Ok(json!({
        "$schema":"http://cyclonedx.org/schema/bom-1.7.schema.json", "bomFormat":"CycloneDX", "specVersion":"1.7", "version":1,
        "serialNumber":format!("urn:uuid:{}",uuid::Uuid::new_v4()),
        "metadata":{
            "tools":{"components":[{"type":"application","name":"jackin-xtask","version":env!("CARGO_PKG_VERSION")}]},
            "component":{"type":"file","bom-ref":"release-archive","name":archive.file_name().and_then(|v|v.to_str()).context("archive has no filename")?,"version":version,"hashes":[{"alg":"SHA-256","content":archive_sha256(archive)?}]},
            "properties":[
                {"name":"jackin:cargo:package","value":identity.package},
                {"name":"jackin:rust-target","value":identity.target},
                {"name":"jackin:inventory:coverage","value":COVERAGE},
                {"name":"jackin:cargo:lock-sha256","value":archive_sha256(&workspace.join("Cargo.lock"))?},
                {"name":"jackin:cargo:workspace-manifest-sha256","value":archive_sha256(&workspace.join("Cargo.toml"))?}
            ]
        },
        "components":components,
        "compositions":[{"aggregate":"incomplete","assemblies":["release-archive"]}]
    }))
}

fn property<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    let properties = value
        .get("properties")
        .and_then(Value::as_array)
        .context("SBOM properties must be an array")?;
    let mut found = properties
        .iter()
        .filter(|property| property.get("name").and_then(Value::as_str) == Some(name));
    let value = found
        .next()
        .and_then(|p| p.get("value"))
        .and_then(Value::as_str)
        .with_context(|| format!("SBOM property missing: {name}"))?;
    ensure!(found.next().is_none(), "duplicate SBOM property {name}");
    Ok(value)
}

pub(super) fn validate(document: &Value, archive: &Path) -> Result<()> {
    ensure!(
        document["bomFormat"] == "CycloneDX" && document["specVersion"] == "1.7",
        "SBOM must use CycloneDX 1.7"
    );
    let serial = document["serialNumber"]
        .as_str()
        .and_then(|s| s.strip_prefix("urn:uuid:"))
        .context("SBOM serialNumber must be a UUID URN")?;
    uuid::Uuid::parse_str(serial).context("invalid SBOM UUID")?;
    let identity = crate::release_archive::payload_identity(archive)?;
    let metadata = &document["metadata"];
    ensure!(
        property(metadata, "jackin:cargo:package")? == identity.package
            && property(metadata, "jackin:rust-target")? == identity.target,
        "SBOM package/target identity mismatch"
    );
    ensure!(
        property(metadata, "jackin:inventory:coverage")? == COVERAGE,
        "SBOM must declare build-input coverage limits"
    );
    for key in [
        "jackin:cargo:lock-sha256",
        "jackin:cargo:workspace-manifest-sha256",
    ] {
        let hash = property(metadata, key)?;
        ensure!(
            hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "SBOM build-input digest is invalid"
        );
    }
    let component = &metadata["component"];
    ensure!(
        component["type"] == "file" && component["bom-ref"] == "release-archive",
        "SBOM root must identify release archive"
    );
    ensure!(
        component["name"].as_str() == archive.file_name().and_then(|v| v.to_str()),
        "SBOM archive name mismatch"
    );
    ensure!(
        component["hashes"] == json!([{"alg":"SHA-256","content":archive_sha256(archive)?}]),
        "SBOM archive digest binding mismatch"
    );
    let version = component["version"]
        .as_str()
        .filter(|v| !v.is_empty())
        .context("SBOM application version missing")?;
    ensure!(
        document["compositions"]
            == json!([{"aggregate":"incomplete","assemblies":["release-archive"]}]),
        "SBOM must not claim complete native/system coverage"
    );
    let members = archive::members(archive, &identity)?;
    validate_components(document, &identity, &members, version)
}

fn validate_components(
    document: &Value,
    identity: &PayloadIdentity,
    members: &BTreeMap<String, Member>,
    version: &str,
) -> Result<()> {
    let components = document["components"]
        .as_array()
        .context("SBOM components must be an array")?;
    ensure!(!components.is_empty(), "SBOM components must not be empty");
    let mut files = BTreeSet::new();
    let mut refs = BTreeSet::new();
    let mut root = false;
    let mut libraries = 0;
    for component in components {
        let reference = component["bom-ref"]
            .as_str()
            .context("SBOM component has no bom-ref")?;
        ensure!(refs.insert(reference), "duplicate SBOM component identity");
        let name = component["name"]
            .as_str()
            .filter(|v| !v.is_empty())
            .context("SBOM component has no name")?;
        let base = component["version"]
            .as_str()
            .filter(|v| !v.is_empty())
            .context("SBOM component has no version")?;
        match component["type"].as_str() {
            Some("file") => {
                let member = members
                    .get(name)
                    .context("SBOM lists unexpected executable")?;
                ensure!(
                    reference == format!("executable:{name}"),
                    "SBOM executable reference mismatch"
                );
                ensure!(files.insert(name), "duplicate SBOM executable");
                ensure!(
                    base == version
                        && component["hashes"]
                            == json!([{"alg":"SHA-256","content":member.sha256}])
                        && property(component, "jackin:executable:size")?
                            == member.size.to_string(),
                    "SBOM executable identity/hash/size mismatch"
                );
            }
            Some("library" | "application") => {
                validate_cargo_component(component)?;
                if component["type"] == "application" {
                    ensure!(
                        !root
                            && name == identity.package
                            && base == version.split("-preview.").next().unwrap_or(version),
                        "SBOM root Cargo package/version mismatch"
                    );
                    root = true;
                } else {
                    libraries += 1;
                }
            }
            _ => anyhow::bail!("unsupported SBOM component type"),
        }
    }
    ensure!(
        root && libraries > 0 && files.len() == members.len(),
        "SBOM Cargo dependency/executable inventory is incomplete"
    );
    Ok(())
}

fn validate_cargo_component(component: &Value) -> Result<()> {
    let source = property(component, "jackin:cargo:source")?;
    ensure!(
        !source.is_empty() && !source.starts_with("path+") && !source.starts_with("file:"),
        "SBOM must normalize host-specific Cargo identities"
    );
    if let Some(path) = source.strip_prefix("workspace:") {
        ensure!(
            !Path::new(path).is_absolute()
                && !path.split('/').any(|part| matches!(part, "" | "." | "..")),
            "invalid repository-relative workspace identity"
        );
    }
    let identity = format!(
        "{}@{}#{source}",
        component["name"]
            .as_str()
            .context("missing Cargo component name")?,
        component["version"]
            .as_str()
            .context("missing Cargo component version")?
    );
    ensure!(
        component["bom-ref"] == format!("cargo:{}", super::digest(identity.as_bytes())),
        "SBOM Cargo component identity mismatch"
    );
    let units: Vec<Value> =
        serde_json::from_str(property(component, "jackin:cargo:compiled-units")?)?;
    ensure!(
        !units.is_empty(),
        "SBOM Cargo component has no actual compilation units"
    );
    for unit in units {
        ensure!(
            unit["target"].as_str().is_some_and(|s| !s.is_empty())
                && unit["kind"].as_array().is_some_and(|a| !a.is_empty())
                && unit["features"].is_array()
                && matches!(unit["role"].as_str(), Some("target" | "host-build")),
            "invalid SBOM compilation unit"
        );
    }
    Ok(())
}
