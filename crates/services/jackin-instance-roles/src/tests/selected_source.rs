// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::*;

const SELECTED: &[u8] = br#"{"tokens":{"access_token":"selected-fixture"}}"#;
const REPLACEMENT: &[u8] = br#"{"tokens":{"access_token":"replacement-fixture"}}"#;

fn selected_codex(source: &Path) -> InstanceAuthBinding {
    let mut binding = InstanceAuthBinding::new(
        "selected-account",
        Agent::Codex,
        AuthForwardMode::Sync,
        Some(source.to_owned()),
    );
    binding.source_provider = Some(AiProvider::OpenAi);
    binding
}

#[test]
fn selected_source_capture_binds_worker_bytes_and_revision_across_source_replacement() {
    let fixture = tempdir().unwrap();
    let source = fixture.path().join("selected-source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("auth.json"), SELECTED).unwrap();
    let binding = selected_codex(&source);
    let captured = capture_selected_account_sources(
        std::slice::from_ref(&binding),
        fixture.path(),
        &fixture.path().join("role"),
    )
    .unwrap();
    let revision = captured[0].selected_source_revision().unwrap().to_owned();
    std::fs::rename(&source, fixture.path().join("old-source")).unwrap();
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("auth.json"), REPLACEMENT).unwrap();

    let worker_binding = captured[0].clone();
    let role = fixture.path().join("role");
    let home = role.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let (slot, outcome) = jackin_instance_agents::provision_codex_slot(
        &role,
        &home,
        fixture.path(),
        &worker_binding,
        None,
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(std::fs::read(&slot.credential_paths[0]).unwrap(), SELECTED);
    assert_eq!(
        worker_binding.selected_source_revision(),
        Some(revision.as_str())
    );
    assert_eq!(
        worker_binding.sync_source_dir.as_deref(),
        Some(source.as_path())
    );

    let refreshed =
        capture_selected_account_sources(&[binding], fixture.path(), &fixture.path().join("role"))
            .unwrap();
    assert_ne!(
        refreshed[0].selected_source_revision(),
        Some(revision.as_str())
    );
    assert_eq!(
        std::fs::read(
            refreshed[0]
                .provision_source_dir()
                .unwrap()
                .join("auth.json")
        )
        .unwrap(),
        REPLACEMENT
    );
}

#[test]
fn selected_source_descriptor_cannot_change_after_capture() {
    let fixture = tempdir().unwrap();
    let source = fixture.path().join("selected-source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("auth.json"), SELECTED).unwrap();
    let mut captured = capture_selected_account_sources(
        &[selected_codex(&source)],
        fixture.path(),
        &fixture.path().join("role"),
    )
    .unwrap();
    captured[0].source_provider = Some(AiProvider::Anthropic);
    let error =
        capture_selected_account_sources(&captured, fixture.path(), &fixture.path().join("role"))
            .unwrap_err();
    assert!(format!("{error:#}").contains("descriptor changed after credential capture"));
}

#[test]
fn selected_source_absence_fails_before_a_worker_can_reopen_a_new_source() {
    let fixture = tempdir().unwrap();
    let source = fixture.path().join("selected-source");
    let error = capture_selected_account_sources(
        &[selected_codex(&source)],
        fixture.path(),
        &fixture.path().join("role"),
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("credentials disappeared before capture"));
}

#[test]
fn xdg_selected_sources_are_captured_before_worker_source_replacement() {
    for agent in [Agent::Amp, Agent::Opencode] {
        let fixture = tempdir().unwrap();
        let data = fixture.path().join("xdg-data");
        let source = data.join(agent.slug());
        std::fs::create_dir_all(&source).unwrap();
        let (file, selected, replacement) = if agent == Agent::Amp {
            (
                "secrets.json",
                r#"{"token":"selected-fixture"}"#,
                r#"{"token":"replacement-fixture"}"#,
            )
        } else {
            (
                "auth.json",
                r#"{"opencode-go":{"type":"api","key":"selected-fixture"}}"#,
                r#"{"opencode-go":{"type":"api","key":"replacement-fixture"}}"#,
            )
        };
        std::fs::write(source.join(file), selected).unwrap();
        let mut binding =
            InstanceAuthBinding::new("selected-xdg", agent, AuthForwardMode::Sync, None);
        binding.xdg_roots = Some(jackin_config::XdgRoots {
            data,
            config: fixture.path().join("xdg-config"),
            cache: fixture.path().join("xdg-cache"),
        });
        if agent == Agent::Opencode {
            binding.source_provider = Some(AiProvider::Opencode);
        }
        let root = fixture.path().join("role");
        let captured = capture_selected_account_sources(&[binding], fixture.path(), &root).unwrap();
        assert!(captured[0].selected_source_revision().is_some());
        assert_eq!(
            captured[0]
                .selected_source
                .as_ref()
                .unwrap()
                .descriptor()
                .source_dir,
            source
        );
        std::fs::rename(&source, fixture.path().join("old-source")).unwrap();
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join(file), replacement).unwrap();
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let (slot, outcome) = if agent == Agent::Amp {
            jackin_instance_agents::provision_amp_slot(
                &root,
                &home,
                fixture.path(),
                &captured[0],
                None,
            )
        } else {
            jackin_instance_agents::provision_opencode_slot(
                &root,
                &home,
                fixture.path(),
                &captured[0],
                None,
            )
        }
        .unwrap();
        assert_eq!(outcome, AuthProvisionOutcome::Synced);
        let provisioned: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&slot.credential_paths[0]).unwrap()).unwrap();
        let selected: serde_json::Value = serde_json::from_str(selected).unwrap();
        assert_eq!(
            provisioned, selected,
            "XDG source precedence must never bypass the captured selected source"
        );
    }
}
