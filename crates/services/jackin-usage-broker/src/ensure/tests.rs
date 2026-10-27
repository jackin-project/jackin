// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use jackin_protocol::usage_broker::UsageCoordinationErrorKind;
use jackin_usage_discovery::UsageDiscoveryScope;

use super::{ensure_usage_broker_process, ensure_usage_monitor_process};
use crate::{BROKER_DIR, BROKER_RUN_DIR, BROKER_SOCKET, UNIX_SOCKET_PATH_LIMIT, UsageBrokerConfig};

fn host_scope(root: &Path) -> UsageDiscoveryScope {
    UsageDiscoveryScope::HostDesktop {
        config_root: root.join("config"),
        operator_home: root.join("home"),
    }
}

fn nonexecutable_config(root: &Path) -> (UsageBrokerConfig, PathBuf) {
    let executable = root.join("jackin-usage-broker");
    fs::write(&executable, b"not a broker").expect("fixture executable");
    let mut permissions = fs::metadata(&executable)
        .expect("executable metadata")
        .permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(&executable, permissions).expect("fixture mode");
    let metadata = fs::metadata(&executable).expect("updated executable metadata");
    assert!(
        metadata.is_file(),
        "fixture executable is not a regular file"
    );
    assert_eq!(metadata.permissions().mode() & 0o111, 0);

    let config = UsageBrokerConfig::for_data_dir(root.join("data"));
    assert!(config.socket_alias_root.is_none());
    let socket_path = config
        .data_dir
        .join(BROKER_DIR)
        .join(BROKER_RUN_DIR)
        .join(BROKER_SOCKET);
    assert!(
        socket_path.as_os_str().as_encoded_bytes().len() < UNIX_SOCKET_PATH_LIMIT,
        "fixture socket path exceeds the platform limit: {} bytes",
        socket_path.as_os_str().as_encoded_bytes().len()
    );
    assert!(!socket_path.exists(), "fixture socket already exists");
    let run_dir = config.data_dir.join(BROKER_DIR).join(BROKER_RUN_DIR);
    assert!(!run_dir.exists(), "fixture run directory already exists");
    let mut config = config;
    config.service_executable = Some(executable);
    (config, run_dir)
}

fn assert_actionable_spawn_failure(message: &str) {
    assert!(
        message.contains(
            "from a source checkout run `mise exec -- mbx +1.99.1 build --bins -p jackin`"
        ),
        "missing source-build guidance: {message}"
    );
    assert!(
        message.contains("reinstall the complete jackin package"),
        "missing package-reinstall guidance: {message}"
    );
}

#[test]
fn nonexecutable_broker_is_unavailable_before_a_run_directory_exists() {
    let root = tempfile::Builder::new()
        .prefix("jb-")
        .tempdir_in("/tmp")
        .expect("short owned fixture root");
    let (config, run_dir) = nonexecutable_config(root.path());
    let error = ensure_usage_broker_process(config, &host_scope(root.path()))
        .expect_err("nonexecutable broker");
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert!(
        error
            .message
            .contains("cannot start usage broker executable"),
        "unexpected broker error: {}",
        error.message
    );
    assert!(
        !run_dir.exists(),
        "failed spawn created a broker run directory"
    );
    assert_actionable_spawn_failure(&error.message);
}

#[test]
fn nonexecutable_monitor_broker_is_unavailable_before_a_run_directory_exists() {
    let root = tempfile::Builder::new()
        .prefix("jb-")
        .tempdir_in("/tmp")
        .expect("short owned fixture root");
    let (config, run_dir) = nonexecutable_config(root.path());
    let error = ensure_usage_monitor_process(config, &host_scope(root.path()))
        .expect_err("nonexecutable monitor broker");
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert!(
        error
            .message
            .contains("cannot start local-only usage broker executable"),
        "unexpected monitor error: {}",
        error.message
    );
    assert!(
        !run_dir.exists(),
        "failed spawn created a broker run directory"
    );
    assert_actionable_spawn_failure(&error.message);
}
