// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn namespace_scan_detects_telemetry_literals_without_flagging_identifiers() {
    let path = "fixture.rs";
    assert!(contains_legacy_telemetry_name(
        path,
        "use jackin_telemetry::Attr; let _ = Attr { key: \"jackin.unregistered.field\", value: () };"
    ));
    assert!(!contains_legacy_telemetry_name(
        path,
        "const LABEL_ROLE_KEY: &str = \"jackin.role\"; let _label = LABEL_ROLE_KEY;"
    ));
}

#[test]
fn namespace_scan_handles_rust_literal_forms_and_macro_construction() {
    let path = "fixture.rs";
    assert!(contains_legacy_telemetry_name(
        path,
        r##"use jackin_telemetry::Attr; let _ = Attr { key: r#"jackin.raw.field"#, value: () };"##
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "use jackin_telemetry::Attr; let _ = Attr { key: concat!(\"jackin.\", \"concat.field\"), value: () };"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "use jackin_telemetry::Attr; let _ = Attr { key: stringify!(parallax.stringify.field), value: () };"
    ));
    assert!(!contains_legacy_telemetry_name(
        path,
        "custom_macro!(\"jackin.hidden.in.opaque.macro\");"
    ));
    assert!(!contains_legacy_telemetry_name(
        path,
        "custom_macro!(\"ordinary.value\");"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "use jackin_telemetry::Attr; custom_macro!(Attr { key: \"jackin.macro.field\", value: () });"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "use jackin_telemetry::Attr; custom_macro!(ordinary_filename!(\"readme.md\"), Attr { key: \"jackin.trailing.comma\", value: () },);"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "use jackin_telemetry::Attr; macro_rules! attrs { () => { Attr { key: \"jackin.macro.definition\", value: () } }; }"
    ));
}

#[test]
fn namespace_scan_resolves_telemetry_attr_bindings_and_rejects_unknown_keys() {
    let labels = "crates/services/jackin-runtime-naming/src/naming.rs";
    assert!(contains_legacy_telemetry_name(
        labels,
        "pub const LABEL_KIND: &str = \"jackin.kind\"; const KEY_ALIAS: &str = LABEL_KIND; use jackin_telemetry::Attr as TelemetryAttr; let _ = TelemetryAttr { key: KEY_ALIAS, value: () };"
    ));

    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry as telemetry; use telemetry::Attr as EventAttr; let _ = EventAttr { key: unknown_key, value: () };"
    ));
    assert!(!contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr as EventAttr; use jackin_telemetry::schema::attrs as keys; let _ = EventAttr { key: keys::OUTCOME, value: () };"
    ));
    assert!(!contains_legacy_telemetry_name(
        "crates/services/jackin-telemetry/src/metric/tests/case_01.rs",
        "use super::*; let _ = Attr { key: attrs::CLI_COMMAND_NAME, value: () }; let _ = Attr { key: schema::attrs::std_attrs::ERROR_TYPE, value: () };"
    ));
    assert!(!contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr; use jackin_telemetry::schema::attrs; let _ = Attr { key: attrs::ALL_KEYS[0], value: () };"
    ));
    assert!(!contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::{schema, Attr}; fn project() { for key in [schema::attrs::CLI_INVOCATION_ID, schema::attrs::std_attrs::SESSION_ID] { let _ = Attr { key, value: () }; } }"
    ));
    assert!(!contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::{schema, Attr}; fn project() { const KEYS: &[&str] = &[schema::attrs::CLI_INVOCATION_ID, schema::attrs::std_attrs::SESSION_ID]; for key in KEYS.iter().copied() { let _ = Attr { key, value: () }; } }"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr; fn project() { let supplied = [unknown_key]; for key in supplied { let _ = Attr { key, value: () }; } }"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::{schema, Attr}; fn project(unknown: &str) { for key in [schema::attrs::CLI_INVOCATION_ID, unknown] { let _ = Attr { key, value: () }; } }"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr; use jackin_telemetry::schema::attrs as keys; let _ = Attr { key: keys::definition, value: () };"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr; let _ = Attr { key: custom_key!(\"ordinary.value\"), value: () };"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "type EventAttr = jackin_telemetry::Attr; let _ = EventAttr { key: \"jackin.kind\", value: () };"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::*; let _ = Attr { key: \"jackin.kind\", value: () };"
    ));
    assert!(!contains_legacy_telemetry_name(
        "fixture.rs",
        "struct Attr { key: &'static str } let _ = Attr { key: \"ordinary.value\" };"
    ));
    assert!(!contains_legacy_telemetry_name(
        "fixture.rs",
        "struct Attr { key: &'static str } const LABEL: &str = \"jackin.role\"; let _ = Attr { key: LABEL, value: () };"
    ));
    assert!(!contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::*; struct Attr { key: &'static str } let _ = Attr { key: \"ordinary.value\" };"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr as EventAttr; let _ = EventAttr { key: custom_key!(\"jackin.hidden\"), value: () };"
    ));
    assert!(!contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::{schema, Attr}; fn project() { let selected = schema::ALL_KEYS.iter().copied().find(|key| key.starts_with(\"jackin.\")).expect(\"schema key\"); let _ = Attr { key: selected, value: () }; }"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::{schema, Attr}; fn project() { let selected = schema::ALL_KEYS.iter().copied().map(|_| \"jackin.hidden\").next().expect(\"generated value\"); let _ = Attr { key: selected, value: () }; }"
    ));
    assert!(!contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::schema::EventMetadata as RegistryMetadata; use jackin_telemetry::Attr; fn project(metadata: &RegistryMetadata) { let _ = metadata.attributes.iter().map(|attribute| Attr { key: attribute.name, value: () }); }"
    ));
    assert!(!contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr; fn project() { let metadata = jackin_telemetry::schema::events::definition(\"event\").expect(\"registered event\"); let _ = metadata.attributes.iter().map(|attribute| Attr { key: attribute.name, value: () }); }"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr; struct FakeAttribute { name: &'static str } struct FakeMetadata { attributes: Vec<FakeAttribute> } fn project(metadata: &FakeMetadata) { let _ = metadata.attributes.iter().map(|attribute| Attr { key: attribute.name, value: () }); }"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr; mod local { pub struct FakeAttribute { pub name: &'static str } pub mod schema { use super::FakeAttribute; pub struct EventMetadata { pub attributes: Vec<FakeAttribute> } } } use local::schema::EventMetadata as RegistryMetadata; fn project(metadata: &RegistryMetadata) { let _ = metadata.attributes.iter().map(|attribute| Attr { key: attribute.name, value: () }); }"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr; struct FakeAttribute { name: &'static str } fn project(metadata: &jackin_telemetry::schema::EventMetadata, fake: &FakeAttribute) { let _ = metadata.attributes.iter().map(|attribute| { let _ = Attr { key: attribute.name, value: () }; let shadowed = |attribute: &FakeAttribute| Attr { key: attribute.name, value: () }; shadowed(fake) }); }"
    ));
    assert!(contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr; use jackin_telemetry::schema::EventMetadata as RegistryMetadata; struct FakeAttribute { name: &'static str } struct FakeMetadata { attributes: Vec<FakeAttribute> } fn project(metadata: &RegistryMetadata, fake: FakeMetadata) { { let (metadata, _) = (fake, ()); let _ = metadata.attributes.iter().map(|attribute| Attr { key: attribute.name, value: () }); } let _ = metadata.attributes.iter().map(|attribute| Attr { key: attribute.name, value: () }); }"
    ));
    assert!(!contains_legacy_telemetry_name(
        "fixture.rs",
        "use jackin_telemetry::Attr; use jackin_telemetry::schema::EventMetadata as RegistryMetadata; struct FakeMetadata { attributes: Vec<()> } fn project(metadata: &RegistryMetadata, fake: FakeMetadata) { { let metadata = fake; let _ = metadata.attributes.len(); } let _ = metadata.attributes.iter().map(|attribute| Attr { key: attribute.name, value: () }); }"
    ));
}
