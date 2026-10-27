// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{RustRegistryMetadata, YamlValue, yaml_required, yaml_sequence, yaml_string};
use anyhow::{Context as _, Result, bail};
use std::{collections::BTreeMap, fs, path::Path};

const LOCAL_PATH: &str = "crates/services/jackin-telemetry/registry/attributes.yaml";

pub(super) struct EnumPolicy {
    authored_types: BTreeMap<String, YamlValue>,
}

impl EnumPolicy {
    pub(super) fn load(
        root: &Path,
        groups: &[YamlValue],
        metadata: &RustRegistryMetadata,
    ) -> Result<Self> {
        let local = read_yaml(&root.join(LOCAL_PATH))?;
        let upstream = read_yaml(
            &root
                .join(&metadata.dependency_registry_path)
                .join("registry.yaml"),
        )?;
        Self::admit(groups, &local, &upstream, metadata)
    }

    fn admit(
        groups: &[YamlValue],
        local: &YamlValue,
        upstream: &YamlValue,
        metadata: &RustRegistryMetadata,
    ) -> Result<Self> {
        let raw_groups = yaml_sequence(local, "groups")
            .ok_or_else(|| anyhow::anyhow!("local registry has no groups"))?;
        if raw_groups.len() != 1 || raw_groups[0].get("extends").is_some() {
            bail!("local enum owner must be exactly one authored attribute group");
        }
        validate_group(&raw_groups[0])?;
        let authored_types = explicit_types(&raw_groups[0], "id")?;
        let owners = groups
            .iter()
            .filter(|group| group.get("id").and_then(YamlValue::as_str) == Some("registry.jackin"))
            .collect::<Vec<_>>();
        if owners.len() != 1 {
            bail!("resolved local enum owner must be unique");
        }
        let owner = owners[0];
        validate_group(owner)?;
        let provenance = owner
            .pointer("/lineage/provenance")
            .ok_or_else(|| anyhow::anyhow!("local enum owner has no source provenance"))?;
        if yaml_string(provenance, "schema_url")? != metadata.registry_schema_url
            || yaml_string(provenance, "path")? != LOCAL_PATH
        {
            bail!("local enum owner source identity mismatch");
        }
        if explicit_types(owner, "name")? != authored_types {
            bail!("resolved local enum definitions differ from authored source");
        }
        validate_overlaps(&authored_types, upstream, metadata)?;
        for group in groups {
            for attribute in yaml_sequence(group, "attributes").unwrap_or_default() {
                let name = yaml_string(attribute, "name")?;
                if let Some(expected) = authored_types.get(name)
                    && yaml_required(attribute, "type")? != expected
                {
                    bail!("signal overrides authored attribute type: {name}");
                }
            }
        }
        Ok(Self { authored_types })
    }

    pub(super) fn allowed_values(&self, name: &str) -> Result<String> {
        let Some(value) = self.authored_types.get(name) else {
            return Ok("&[]".to_owned());
        };
        let values = member_values(value)?;
        Ok(format!(
            "&[{}]",
            values
                .iter()
                .map(|value| format!("{value:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

fn read_yaml(path: &Path) -> Result<YamlValue> {
    serde_saphyr::from_str(
        &fs::read_to_string(path)
            .with_context(|| format!("reading enum ownership at {}", path.display()))?,
    )
    .with_context(|| format!("parsing enum ownership at {}", path.display()))
}

fn validate_group(group: &YamlValue) -> Result<()> {
    if yaml_string(group, "id")? != "registry.jackin"
        || yaml_string(group, "type")? != "attribute_group"
        || group.get("extends").is_some()
    {
        bail!("local enum owner is not the authored registry.jackin attribute group");
    }
    Ok(())
}

fn explicit_types(group: &YamlValue, key: &str) -> Result<BTreeMap<String, YamlValue>> {
    let attributes = yaml_sequence(group, "attributes")
        .ok_or_else(|| anyhow::anyhow!("attribute owner has no definitions"))?;
    let mut types = BTreeMap::new();
    for attribute in attributes {
        if attribute.get("ref").is_some() || attribute.get("extends").is_some() {
            bail!("enum ownership requires explicit authored definitions");
        }
        let name = yaml_string(attribute, key)?;
        let value = yaml_required(attribute, "type")?.clone();
        if types.insert(name.to_owned(), value).is_some() {
            bail!("duplicate authored attribute definition: {name}");
        }
    }
    Ok(types)
}

fn member_values(value: &YamlValue) -> Result<Vec<String>> {
    yaml_sequence(value, "members")
        .unwrap_or_default()
        .iter()
        .map(|member| yaml_string(member, "value").map(str::to_owned))
        .collect()
}

fn validate_overlaps(
    authored: &BTreeMap<String, YamlValue>,
    upstream: &YamlValue,
    metadata: &RustRegistryMetadata,
) -> Result<()> {
    let mut names = BTreeMap::new();
    for group in yaml_sequence(upstream, "groups").unwrap_or_default() {
        for (name, value) in explicit_types(group, "id")? {
            if names.insert(name.clone(), value).is_some() {
                bail!("duplicate upstream attribute definition: {name}");
            }
        }
    }
    for (name, value) in authored {
        if !names.contains_key(name) {
            continue;
        }
        let values = member_values(value)?;
        let declared_local = metadata.standard_local.values().any(|local| local == name);
        let declared_refinement = !values.is_empty()
            && metadata
                .standard_enum
                .iter()
                .any(|item| item.attribute == *name && item.values == values);
        if !declared_local && !declared_refinement {
            bail!("undeclared local refinement of upstream attribute: {name}");
        }
    }
    Ok(())
}
