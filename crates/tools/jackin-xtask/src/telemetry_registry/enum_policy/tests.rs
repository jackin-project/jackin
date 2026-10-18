// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use serde_json::json;

fn metadata() -> RustRegistryMetadata {
    super::super::load_rust_metadata(
        &super::super::repo_root()
            .unwrap()
            .join("crates/services/jackin-telemetry/registry"),
    )
    .unwrap()
}

fn local() -> YamlValue {
    json!({"groups": [{"id": "registry.jackin", "type": "attribute_group", "attributes": [
        {"id": "app.mode", "type": {"members": [{"id": "interactive", "value": "interactive"}]}}
    ]}]})
}

fn resolved(local: &YamlValue) -> Vec<YamlValue> {
    let attributes = local["groups"][0]["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|attribute| {
            let mut attribute = attribute.clone();
            attribute["name"] = attribute["id"].clone();
            attribute.as_object_mut().unwrap().remove("id");
            attribute
        })
        .collect::<Vec<_>>();
    vec![
        json!({"id": "registry.jackin", "type": "attribute_group", "attributes": attributes,
        "lineage": {"provenance": {"schema_url": metadata().registry_schema_url, "path": LOCAL_PATH}}}),
    ]
}

fn upstream(attributes: Vec<YamlValue>) -> YamlValue {
    json!({"groups": [{"id": "registry.upstream", "attributes": attributes}]})
}

fn fallback_type() -> YamlValue {
    json!({"members": [{"id": "other", "value": "_OTHER"}]})
}

#[test]
fn local_enums_are_closed_and_upstream_known_values_remain_open() {
    let local = local();
    let groups = resolved(&local);
    let upstream = upstream(vec![json!({"id": "error.type", "type": fallback_type()})]);
    let policy = EnumPolicy::admit(&groups, &local, &upstream, &metadata()).unwrap();
    assert_eq!(
        policy.allowed_values("app.mode").unwrap(),
        "&[\"interactive\"]"
    );
    for name in [
        "error.type",
        "cpu.mode",
        "http.request.method",
        "db.system.name",
    ] {
        assert_eq!(policy.allowed_values(name).unwrap(), "&[]");
    }
}

#[test]
fn local_owner_requires_exact_unique_schema_path_and_group_type() {
    let local = local();
    let original = resolved(&local);
    for (pointer, value) in [
        ("/id", json!("registry.foreign")),
        ("/type", json!("event")),
        (
            "/lineage/provenance/schema_url",
            json!("https://example.invalid/schema"),
        ),
        ("/lineage/provenance/path", json!("copied/attributes.yaml")),
    ] {
        let mut groups = original.clone();
        *groups[0].pointer_mut(pointer).unwrap() = value;
        assert!(
            EnumPolicy::admit(&groups, &local, &upstream(vec![]), &metadata()).is_err(),
            "{pointer}"
        );
    }
    let duplicated = vec![original[0].clone(), original[0].clone()];
    assert!(EnumPolicy::admit(&duplicated, &local, &upstream(vec![]), &metadata()).is_err());
}

#[test]
fn resolved_convenience_enums_cannot_forge_authored_ownership() {
    let local = local();
    for name in ["error.type", "cpu.mode"] {
        let mut groups = resolved(&local);
        groups[0]["attributes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": name, "type": fallback_type()}));
        assert!(EnumPolicy::admit(&groups, &local, &upstream(vec![]), &metadata()).is_err());
    }
}

#[test]
fn duplicate_ref_inherited_or_modified_authored_types_are_rejected() {
    let original = local();
    for mutation in ["duplicate", "ref", "extends", "type"] {
        let mut local = original.clone();
        let mut groups = resolved(&original);
        match mutation {
            "duplicate" => {
                let attribute = local["groups"][0]["attributes"][0].clone();
                local["groups"][0]["attributes"]
                    .as_array_mut()
                    .unwrap()
                    .push(attribute);
            }
            "ref" => local["groups"][0]["attributes"][0]["ref"] = json!("upstream.mode"),
            "extends" => local["groups"][0]["extends"] = json!("registry.upstream"),
            "type" => groups[0]["attributes"][0]["type"] = json!("string"),
            _ => unreachable!(),
        }
        assert!(
            EnumPolicy::admit(&groups, &local, &upstream(vec![]), &metadata()).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn existing_structured_local_and_standard_enum_refinements_are_admitted() {
    let metadata = metadata();
    for name in ["gen_ai.provider.name", "db.operation.name"] {
        let values = &metadata
            .standard_enum
            .iter()
            .find(|item| item.attribute == name)
            .unwrap()
            .values;
        let members = values
            .iter()
            .map(|value| json!({"id": value, "value": value}))
            .collect::<Vec<_>>();
        let local = json!({"groups": [{"id": "registry.jackin", "type": "attribute_group", "attributes": [
            {"id": name, "type": {"members": members}}
        ]}]});
        let upstream = upstream(vec![json!({"id": name, "type": "string"})]);
        EnumPolicy::admit(&resolved(&local), &local, &upstream, &metadata).unwrap();
    }
}

#[test]
fn undeclared_or_conflicting_upstream_refinements_are_rejected() {
    for name in ["cpu.mode", "error.type", "db.operation.name"] {
        let mut local = local();
        local["groups"][0]["attributes"][0]["id"] = json!(name);
        let upstream = upstream(vec![json!({"id": name, "type": fallback_type()})]);
        assert!(EnumPolicy::admit(&resolved(&local), &local, &upstream, &metadata()).is_err());
    }
}

#[test]
fn signal_cannot_override_an_admitted_local_attribute_type() {
    let local = local();
    let mut groups = resolved(&local);
    groups.push(
        json!({"id": "event.example", "type": "event", "attributes": [
            {"name": "app.mode", "type": "string"}
        ]}),
    );
    assert!(EnumPolicy::admit(&groups, &local, &upstream(vec![]), &metadata()).is_err());
}
