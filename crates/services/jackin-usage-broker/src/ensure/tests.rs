// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use jackin_protocol::usage_broker::UsageCoordinationErrorKind;
use jackin_usage_discovery::UsageDiscoveryScope;

use super::{ensure_usage_broker_process, ensure_usage_monitor_process};
use crate::{BROKER_DIR, BROKER_RUN_DIR, UsageBrokerConfig};

fn host_scope(root: &Path) -> UsageDiscoveryScope {
    UsageDiscoveryScope::HostDesktop {
        config_root: root.join("config"),
        operator_home: root.join("home"),
    }
}

fn nonexecutable_config(root: &Path) -> UsageBrokerConfig {
    let executable = root.join("jackin-usage-broker");
    fs::write(&executable, b"not a broker").expect("fixture executable");
    let mut permissions = fs::metadata(&executable).expect("metadata").permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(&executable, permissions).expect("mode");
    let mut config = UsageBrokerConfig::for_data_dir(root.join("data"));
    config.service_executable = Some(executable);
    config
}

fn assert_actionable_spawn_failure(root: &Path, message: &str) {
    assert!(
        message.contains("from a source checkout run `mise exec -- mbx +"),
        "missing source-build guidance: {message}"
    );
    assert!(
        message.contains("build --bins -p jackin`"),
        "missing build command: {message}"
    );
    assert!(
        message.contains("reinstall the complete jackin package"),
        "missing reinstall guidance: {message}"
    );
    assert!(
        !root
            .join("data")
            .join(BROKER_DIR)
            .join(BROKER_RUN_DIR)
            .exists(),
        "failed to spawn a broker but it created a run directory"
    );
}

#[test]
fn nonexecutable_broker_is_unavailable_before_a_run_directory_exists() {
    let root = tempfile::tempdir().expect("tempdir");
    let error =
        ensure_usage_broker_process(nonexecutable_config(root.path()), &host_scope(root.path()))
            .expect_err("non-executable broker");
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert!(
        error
            .message
            .contains("cannot start usage broker executable"),
        "unexpected broker error: {}",
        error.message
    );
    assert_actionable_spawn_failure(root.path(), &error.message);
}

#[test]
fn nonexecutable_monitor_broker_is_unavailable_before_a_run_directory_exists() {
    let root = tempfile::tempdir().expect("tempdir");
    let error =
        ensure_usage_monitor_process(nonexecutable_config(root.path()), &host_scope(root.path()))
            .expect_err("non-executable monitor broker");
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert!(
        error
            .message
            .contains("cannot start local-only usage broker executable"),
        "unexpected monitor error: {}",
        error.message
    );
    assert_actionable_spawn_failure(root.path(), &error.message);
}
