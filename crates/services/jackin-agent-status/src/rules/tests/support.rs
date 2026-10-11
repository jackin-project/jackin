// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn pack_with_versions(versions: &str) -> anyhow::Result<RulePack> {
    let pack: RulePack = toml::from_str(&format!(
        "schema_version = 1\nagent = \"test\"\nvalidated_versions = \"{versions}\"\n"
    ))
    .unwrap();
    pack.validate().map(|()| pack)
}

pub(super) fn validate_gate(gate: &str) -> anyhow::Result<()> {
    let pack: RulePack = toml::from_str(&format!(
        "schema_version = 1\nagent = \"test\"\nvalidated_versions = \">=1.0.0, <2\"\n\
         [[rule]]\nid = \"g\"\nstate = \"blocked\"\npriority = 1\nregion = \"bottom:5\"\n\
         gate = {gate}\n"
    ))
    .unwrap();
    pack.validate()
}

pub(super) fn fixture(path: &str) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

pub(super) fn fixture_for_detection(path: &Path) -> (Option<RawAgentState>, Vec<String>) {
    let mut rows = fixture(path.to_str().unwrap());
    let forbidden = rows
        .first()
        .and_then(|line| line.trim().strip_prefix("# not:"))
        .map(str::trim)
        .map(|state| match state {
            "working" => RawAgentState::Working,
            "blocked" => RawAgentState::Blocked,
            "idle" => RawAgentState::Idle,
            other => panic!("unknown forbidden state {other:?} in {path:?}"),
        });
    if forbidden.is_some() {
        rows.remove(0);
    }
    // Drop `# provenance:` / other harness comments so goldens can document
    // origin without poisoning screen matchers (plan 005).
    rows.retain(|line| !line.trim_start().starts_with('#'));
    (forbidden, rows)
}

pub(super) fn write_test_pack(dir: &Path, agent: &str, id: &str, state: &str, needle: &str) {
    fs::write(
        dir.join(format!("{agent}.toml")),
        format!(
            r#"
schema_version = 1
agent = "{agent}"
validated_versions = ">=1.0.0, <2.0.0"

[[rule]]
id = "{id}"
state = "{state}"
priority = 1
region = "bottom:12"
strength = "strong"
requires_all = ["{needle}"]
"#
        ),
    )
    .unwrap();
}

pub(super) fn test_pack_toml(agent: &str, id: &str, state: &str, needle: &str) -> String {
    format!(
        r#"
schema_version = 1
agent = "{agent}"
validated_versions = ">=1.0.0, <2.0.0"

[[rule]]
id = "{id}"
state = "{state}"
priority = 100
region = "bottom:12"
strength = "strong"
requires_all = ["{needle}"]
"#
    )
}

pub(super) fn bundle_entries(entries: Vec<(&str, String)>) -> Vec<SignedPackEntry> {
    entries
        .into_iter()
        .map(|(label, content)| SignedPackEntry {
            label: label.to_owned(),
            content,
        })
        .collect()
}

pub(super) fn registry_with_entries_for_parser_test(
    entries: &[SignedPackEntry],
) -> RulePackRegistry {
    let mut registry = RulePackRegistry::from_sources([PackSource::Embedded]).unwrap();
    registry
        .notes
        .extend(load_bundle_entries(&mut registry.packs, entries));
    registry
}
