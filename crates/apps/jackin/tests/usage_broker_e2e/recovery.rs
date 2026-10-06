// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use super::*;

const RECOVERY_CLIENTS: usize = 8;

#[test]
fn usage_broker_killed_owner_recovers_once_without_a_herd() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    let executable = std::env::current_exe()?;
    let owner_request = child_request(&executable, root, "owner", "owner", None);
    let mut owner = jackin_process::spawn_sync(&owner_request)?;
    wait_until(Duration::from_secs(10), || {
        root.join("owner-active").exists()
    });
    owner.kill()?;
    let _owner_status = owner.wait()?;

    let mut recovery = Vec::new();
    for child in 0..RECOVERY_CLIENTS {
        let name = format!("recovery-{child}");
        let request = child_request(&executable, root, &name, "recovery", Some(RECOVERY_CLIENTS));
        recovery.push(jackin_process::spawn_sync(&request)?);
    }
    wait_until(Duration::from_secs(10), || {
        entries_with_prefix(root, "ready-") == RECOVERY_CLIENTS
    });
    fs::write(root.join("go"), b"go\n")?;
    for mut child in recovery {
        assert!(child.wait()?.success());
    }
    assert_eq!(entries_with_prefix(root, "provider-call-"), 2);
    Ok(())
}

fn child_request(
    executable: &Path,
    root: &Path,
    child: &str,
    mode: &str,
    expected: Option<usize>,
) -> jackin_process::ExecRequest {
    let mut envs: Vec<(std::ffi::OsString, std::ffi::OsString)> = vec![
        (CHILD_ENV.into(), child.into()),
        (ROOT_ENV.into(), root.as_os_str().to_owned()),
        (MODE_ENV.into(), mode.into()),
    ];
    if let Some(expected) = expected {
        envs.push((EXPECTED_ENV.into(), expected.to_string().into()));
    }
    jackin_process::ExecRequest::new(executable, ["--exact", "usage_broker_child", "--nocapture"])
        .envs(envs)
        .stdout_mode(jackin_process::StdioMode::Inherit)
        .stderr_mode(jackin_process::StdioMode::Inherit)
}
