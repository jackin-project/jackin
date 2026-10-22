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

#[cfg(unix)]
#[test]
fn codex_test_stub_installs_complete_fail_fast_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(dir.path());

    install_test_stub(&paths, Agent::Codex).unwrap();

    let codex = test_stub_path(&paths, Agent::Codex);
    let sibling = codex.with_file_name(jackin_core::CODEX_CODE_MODE_HOST_BINARY);
    assert!(is_executable_file(&codex));
    assert!(is_executable_file(&sibling));
    let output = std::process::Command::new(&sibling).output().unwrap();
    assert_eq!(output.status.code(), Some(127));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Codex code-mode host is not implemented by this test stub")
    );
}

#[test]
fn codex_test_stub_preserves_helper_and_other_agents_get_no_helper() {
    let dir = tempfile::tempdir().unwrap();
    let codex_paths = JackinPaths::for_tests(&dir.path().join("codex"));
    let codex = test_stub_path(&codex_paths, Agent::Codex);
    let sibling = codex.with_file_name(jackin_core::CODEX_CODE_MODE_HOST_BINARY);
    std::fs::create_dir_all(sibling.parent().unwrap()).unwrap();
    let custom_helper = b"#!/bin/sh\nprintf '%s\\n' custom\n";
    std::fs::write(&sibling, custom_helper).unwrap();
    chmod_executable(&sibling).unwrap();

    install_test_stub(&codex_paths, Agent::Codex).unwrap();
    assert!(is_executable_file(&sibling));
    assert_eq!(std::fs::read(&sibling).unwrap(), custom_helper);

    let claude_paths = JackinPaths::for_tests(&dir.path().join("claude"));
    install_test_stub(&claude_paths, Agent::Claude).unwrap();
    let claude = test_stub_path(&claude_paths, Agent::Claude);
    assert!(
        !claude
            .with_file_name(jackin_core::CODEX_CODE_MODE_HOST_BINARY)
            .exists()
    );
}
