// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn codex_release_with_sibling() -> AgentRelease {
    let mut release = release_fixture_for(Agent::Codex, "0.162.1");
    release.extra_files = vec![AgentExtraFile {
        name: jackin_core::CODEX_CODE_MODE_HOST_BINARY.to_owned(),
        url: "https://example.test/codex-code-mode-host".to_owned(),
        checksum: Some("def".to_owned()),
        archive_member: None,
    }];
    release
}

#[test]
fn codex_release_with_sibling_metadata_is_usable() {
    assert!(release_has_required_extras(&codex_release_with_sibling()));
}

#[test]
fn legacy_codex_release_without_sibling_metadata_is_rejected() {
    // Releases cached before sibling files existed serialize without the
    // extra_files field; they must re-resolve instead of installing a
    // known-incomplete binary set.
    let mut value = serde_json::to_value(codex_release_with_sibling()).unwrap();
    value
        .as_object_mut()
        .expect("release serializes as object")
        .remove("extra_files");
    let legacy: AgentRelease = serde_json::from_value(value).unwrap();
    assert!(legacy.extra_files.is_empty());
    assert!(
        !release_has_required_extras(&legacy),
        "legacy codex release must be treated as stale"
    );
}

#[test]
fn releases_without_sibling_requirements_stay_usable() {
    for agent in Agent::ALL {
        if *agent == Agent::Codex {
            continue;
        }
        let release = release_fixture_for(*agent, "1.2.3");
        assert!(
            release_has_required_extras(&release),
            "{agent:?} without extras must stay usable"
        );
    }
}
