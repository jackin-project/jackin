// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{OpenConfig, UsageBridgeError, UsageMenuBarBridge};

const TEST_NAME: &str = "bridge::tests::nonexecutable_broker_reports_startup_failure";
const CHILD_PROCESS_NAME: &str = "jackin-usage-ffi-startup-child";
const CHILD_ROOT_ENV: &str = "JACKIN_USAGE_FFI_MISSING_BROKER_CHILD_ROOT";
const BROKER_BIN_ENV: &str = "JACKIN_USAGE_BROKER_BIN";
const MAX_DATA_DIRECTORY_PATH_BYTES: usize = 64;

#[test]
fn nonexecutable_broker_reports_startup_failure() -> Result<(), Box<dyn Error>> {
    if std::env::args_os().next().as_deref() == Some(OsStr::new(CHILD_PROCESS_NAME)) {
        let root = std::env::var_os(CHILD_ROOT_ENV)
            .ok_or_else(|| io::Error::other("child fixture root is missing"))?;
        verify_failed_open(PathBuf::from(root))?;
        return Ok(());
    }

    let fixture = tempfile::Builder::new()
        .prefix("jffi-")
        .tempdir_in("/tmp")?;
    let root = fs::canonicalize(fixture.path())?;
    let data_dir = root.join("d");
    let config_root = root.join("c");
    let broker_executable = root.join("broker");
    fs::create_dir(&data_dir)?;
    fs::create_dir(&config_root)?;
    fs::write(&broker_executable, b"not a broker")?;
    let mut permissions = fs::metadata(&broker_executable)?.permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(&broker_executable, permissions)?;
    assert_nonexecutable_regular_file(&broker_executable)?;
    assert_directory_empty(&data_dir)?;
    assert_directory_empty(&config_root)?;
    assert_data_directory_path_is_short(&data_dir);

    let output = Command::new(std::env::current_exe()?)
        .arg0(CHILD_PROCESS_NAME)
        .arg("--exact")
        .arg(TEST_NAME)
        .arg("--nocapture")
        .env(CHILD_ROOT_ENV, &root)
        .env(BROKER_BIN_ENV, &broker_executable)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "child test failed with {}; stdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    assert!(
        stdout.contains(TEST_NAME),
        "child process did not report the exact receiver; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    Ok(())
}

fn verify_failed_open(root: PathBuf) -> Result<(), Box<dyn Error>> {
    let root = fs::canonicalize(root)?;
    let data_dir = root.join("d");
    let config_root = root.join("c");
    let broker_executable = root.join("broker");
    assert!(root.is_dir(), "child fixture root is missing");
    assert_nonexecutable_regular_file(&broker_executable)?;
    assert_directory_empty(&data_dir)?;
    assert_directory_empty(&config_root)?;
    assert_data_directory_path_is_short(&data_dir);
    assert_eq!(
        std::env::var_os(BROKER_BIN_ENV).map(PathBuf::from),
        Some(broker_executable.clone()),
        "child broker override does not name the fixture executable"
    );

    let bridge = UsageMenuBarBridge::create();
    assert!(matches!(
        bridge.list_surfaces(),
        Err(UsageBridgeError::RuntimeUnavailable)
    ));
    let Err(error) = bridge.open_runtime(OpenConfig {
        data_dir_override: Some(path_string(&data_dir)?),
        config_root_override: Some(path_string(&config_root)?),
        refresh_floor_secs: 60,
        enabled_surface_ids: Vec::new(),
        allow_live_probes: true,
    }) else {
        return Err(io::Error::other("nonexecutable broker opened the runtime").into());
    };
    let message = match &error {
        UsageBridgeError::Rejected { message, .. } => message,
        other => {
            return Err(
                io::Error::other(format!("unexpected public open error: {other:?}")).into(),
            );
        }
    };
    assert!(matches!(
        bridge.list_surfaces(),
        Err(UsageBridgeError::RuntimeUnavailable)
    ));
    assert_directory_empty(&data_dir)?;
    assert!(
        message.contains("cannot start usage broker executable"),
        "missing spawn failure in public error: {message}"
    );
    assert!(
        message.contains(
            "from a source checkout run `mise exec -- mbx +1.99.1 build --bins -p jackin`"
        ),
        "missing source-build guidance in public error: {message}"
    );
    assert!(
        message.contains("reinstall the complete jackin package"),
        "missing package-reinstall guidance in public error: {message}"
    );

    match error {
        UsageBridgeError::Rejected { code, .. } => {
            assert_eq!(code, "coordination_unavailable");
        }
        other => {
            return Err(
                io::Error::other(format!("unexpected public open error: {other:?}")).into(),
            );
        }
    }
    Ok(())
}

fn assert_nonexecutable_regular_file(path: &Path) -> io::Result<()> {
    let metadata = fs::metadata(path)?;
    assert!(metadata.is_file(), "broker fixture is not a regular file");
    assert_eq!(metadata.permissions().mode() & 0o111, 0);
    assert_eq!(metadata.permissions().mode() & 0o777, 0o644);
    Ok(())
}

fn assert_directory_empty(path: &Path) -> io::Result<()> {
    let mut entries = fs::read_dir(path)?;
    assert!(entries.next().is_none(), "fixture directory is not empty");
    Ok(())
}

fn assert_data_directory_path_is_short(path: &Path) {
    let path_bytes = path.as_os_str().as_encoded_bytes().len();
    assert!(
        path_bytes <= MAX_DATA_DIRECTORY_PATH_BYTES,
        "data directory path is not short enough: {path_bytes} bytes"
    );
}

fn path_string(path: &Path) -> io::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("fixture path is not UTF-8"))
}

#[test]
fn broker_conflict_maps_to_specific_error_code() {
    let error = jackin_protocol::usage_broker::UsageCoordinationError {
        kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::BrokerConflict,
        message: "another process owns the usage broker lease".to_owned(),
    };
    let mapped = super::map_coordination_err(error);
    let UsageBridgeError::Rejected { code, .. } = mapped else {
        panic!("broker conflict did not map to a rejected bridge error: {mapped:?}");
    };

    assert_eq!(code, "coordination_broker_conflict");
}
