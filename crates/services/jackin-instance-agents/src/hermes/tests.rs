// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::*;

use jackin_config::{AiProvider, AuthForwardMode, ProfileSelector};
use jackin_instance_credentials::AuthProvisionOutcome;

use std::cell::Cell;
use std::os::unix::fs::PermissionsExt;
use std::rc::Rc;

use tempfile::tempdir;

#[test]
fn hermes_missing_source_does_not_copy_a_source_that_reappears_unvalidated() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("host/.hermes");
    let target = temp.path().join("role/.hermes");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    let hook_called = Rc::new(Cell::new(false));
    let hook_called_by_callback = Rc::clone(&hook_called);
    let source_for_callback = source.clone();
    let hook = set_hermes_absent_source_hook(move || {
        std::fs::create_dir_all(&source_for_callback).unwrap();
        std::fs::write(
            source_for_callback.join("config.yaml"),
            "profiles:\n  personal:\n    provider: anthropic\n",
        )
        .unwrap();
        std::fs::write(
            source_for_callback.join("auth.json"),
            r#"{"anthropic":{"type":"api","key":"reappeared-source-secret"}}"#,
        )
        .unwrap();
        hook_called_by_callback.set(true);
    });

    let selector = ProfileSelector {
        entry: "openai".to_owned(),
        profile: Some("work".to_owned()),
    };
    let result = provision_hermes_auth_from_source_dir(
        &target,
        AuthForwardMode::Sync,
        &source,
        Some(AiProvider::OpenAi),
        Some(&selector),
    );
    drop(hook);

    let (outcome, forward_auth) = result.unwrap();
    assert!(hook_called.get());
    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(forward_auth);
    assert!(target.is_dir());
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(std::fs::read_dir(&target).unwrap().next().is_none());
    assert!(source.join("auth.json").exists());
    assert!(!target.join("auth.json").exists());
}

#[test]
fn hermes_sync_still_copies_a_present_validated_source() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("host/.hermes");
    let target = temp.path().join("role/.hermes");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("config.yaml"),
        "profiles:\n  work:\n    provider: openai\n",
    )
    .unwrap();
    std::fs::write(
        source.join("auth.json"),
        r#"{"openai":{"type":"api","key":"selected-source-secret"}}"#,
    )
    .unwrap();

    let selector = ProfileSelector {
        entry: "openai".to_owned(),
        profile: Some("work".to_owned()),
    };
    let (outcome, forward_auth) = provision_hermes_auth_from_source_dir(
        &target,
        AuthForwardMode::Sync,
        &source,
        Some(AiProvider::OpenAi),
        Some(&selector),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(
        std::fs::read_to_string(target.join("auth.json"))
            .unwrap()
            .contains("selected-source-secret")
    );
}
