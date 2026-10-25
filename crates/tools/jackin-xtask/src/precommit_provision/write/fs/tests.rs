// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(target_os = "macos")]
#[test]
fn inherited_mutation_acl_is_rejected_on_the_held_install_directory() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let path = temporary
        .path()
        .to_str()
        .context("ACL fixture path is not UTF-8")?;
    let request =
        jackin_process::ExecRequest::new("/bin/chmod", ["+a", "everyone allow delete", path])
            .env_clear();
    let result = jackin_process::exec_sync(&request)?;
    ensure!(result.success, "installing macOS ACL fixture failed");
    let held = open_path(temporary.path())?;
    assert!(validate_acl_directory(&held, "ACL test directory").is_err());
    Ok(())
}
