// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::apple_container_client::{
    AppleContainerMount, AppleContainerStatus, BACKEND_NAME, DOCKER_BACKEND_NAME,
};

#[test]
fn only_running_status_reports_running() {
    assert!(AppleContainerStatus::Running.is_running());
    assert!(!AppleContainerStatus::Stopped.is_running());
    assert!(!AppleContainerStatus::Unknown.is_running());
    assert!(!AppleContainerStatus::Stopping.is_running());
}

#[test]
fn backend_names_are_distinct() {
    assert_eq!(BACKEND_NAME, "apple-container");
    assert_eq!(DOCKER_BACKEND_NAME, "docker");
}

#[test]
fn mount_new_preserves_fields() {
    let mount = AppleContainerMount::new("/src", "/dst", true);
    assert_eq!(mount.source, PathBuf::from("/src"));
    assert_eq!(mount.target, PathBuf::from("/dst"));
    assert!(mount.readonly);
}

#[test]
fn status_deserializes_from_daemon_json() -> anyhow::Result<()> {
    let running: AppleContainerStatus = serde_json::from_str("\"running\"")?;
    assert_eq!(running, AppleContainerStatus::Running);
    let stopped: AppleContainerStatus = serde_json::from_str("\"stopped\"")?;
    assert_eq!(stopped, AppleContainerStatus::Stopped);
    Ok(())
}

#[test]
fn unknown_status_is_rejected() -> Result<(), String> {
    let error = match serde_json::from_str::<AppleContainerStatus>("\"paused\"") {
        Ok(status) => return Err(format!("daemon state was accepted: {status:?}")),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("unrecognized Apple Container state")
    );
    Ok(())
}
