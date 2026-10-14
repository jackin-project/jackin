// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::linux::{
    FULL, NULL_DEVICE, READ_FILE_ONLY, READ_ONLY, Rule, add_execute_only_ancestors,
    drop_privileges, install_landlock, rules_for, rules_for_test, support,
};
use anyhow::Context;
use jackin_protocol::CapsuleConfig;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[test]
fn null_stdio_has_only_an_exact_null_device_write_grant() {
    let config = CapsuleConfig::default();
    let rules = rules_for(
        &config,
        None,
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/1"),
    )
    .expect("construct Landlock rules");

    assert_eq!(
        rules
            .iter()
            .find(|rule| rule.path == Path::new("/dev/null"))
            .expect("exact null-device rule")
            .access,
        NULL_DEVICE
    );
    assert!(
        rules
            .iter()
            .any(|rule| { rule.path == Path::new("/dev") && rule.access == READ_ONLY })
    );
    assert!(
        !rules
            .iter()
            .any(|rule| { rule.path == Path::new("/dev") && rule.access & support::WRITABLE != 0 })
    );
    assert!(!rules.iter().any(|rule| {
        rule.path.starts_with(Path::new("/dev/"))
            && rule.path != Path::new("/dev/null")
            && rule.access & support::WRITABLE != 0
    }));
}

#[test]
fn dind_client_cert_mount_is_exactly_read_only() {
    let rules = rules_for(
        &CapsuleConfig::default(),
        None,
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/1"),
    )
    .expect("construct DinD isolation rules");

    let certs = rules
        .iter()
        .find(|rule| rule.path == Path::new(jackin_core::container_paths::DIND_CERTS_CLIENT_DIR))
        .expect("exact DinD client-cert mount rule");
    assert_eq!(certs.access, READ_ONLY);
    assert!(!certs.required);
    assert!(!rules.iter().any(|rule| {
        rule.path == Path::new(jackin_core::container_paths::DIND_CERTS_CLIENT_DIR)
            && rule.access & support::WRITABLE != 0
    }));
}

#[test]
fn isolated_runtime_setup_can_spawn_git_config_with_null_stdio() {
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    if unsafe { libc::geteuid() } != 0 {
        return;
    }

    let temp = tempfile::tempdir().expect("temporary runtime setup fixture");
    let workspace = temp.path().join("workspace");
    let session_root = temp.path().join("session");
    fs::create_dir(&workspace).expect("workspace");
    fs::create_dir(&session_root).expect("session root");
    fs::set_permissions(&workspace, fs::Permissions::from_mode(0o777))
        .expect("workspace permissions");
    fs::set_permissions(&session_root, fs::Permissions::from_mode(0o777))
        .expect("session root permissions");

    let (read_fd, write_fd) = {
        let mut fds = [0; 2];
        #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
        let pipe_status = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(pipe_status, 0);
        (fds[0], fds[1])
    };
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    let child = unsafe { libc::fork() };
    assert!(child >= 0, "fork runtime setup probe");
    if child == 0 {
        #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
        unsafe {
            libc::close(read_fd)
        };
        let result = (|| -> anyhow::Result<()> {
            drop_privileges(jackin_protocol::SessionIdentity {
                uid: 65_534,
                gid: 65_534,
            })?;
            let rules = rules_for_test(&CapsuleConfig::default(), None, &workspace, &session_root)?;
            install_landlock(&rules)?;

            let global_config = session_root.join("gitconfig");
            let request = jackin_process::ExecRequest::new(
                "git",
                [
                    "config",
                    "--global",
                    "--get-all",
                    "url.https://github.com/.insteadOf",
                ],
            )
            .cwd(&workspace)
            .envs([("GIT_CONFIG_GLOBAL", global_config.as_os_str())]);
            let output = jackin_process::exec_sync(&request)
                .context("spawn git config under session Landlock");
            let output = output?;
            anyhow::ensure!(
                output.code == Some(1),
                "git config probe exited unexpectedly: {:?}, stderr={}",
                output.code,
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(())
        })();
        if let Err(error) = &result {
            eprintln!("isolated runtime setup probe failed: {error:#}");
        }
        let status = [u8::from(result.is_ok())];
        #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
        unsafe {
            libc::write(write_fd, status.as_ptr().cast(), 1)
        };
        #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
        unsafe {
            libc::close(write_fd)
        };
        #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
        unsafe {
            libc::_exit(i32::from(result.is_err()))
        };
    }
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    unsafe {
        libc::close(write_fd)
    };
    let mut status = [0u8; 1];
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    let bytes_read = unsafe { libc::read(read_fd, status.as_mut_ptr().cast(), 1) };
    assert_eq!(bytes_read, 1);
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    unsafe {
        libc::close(read_fd)
    };
    let mut wait_status = 0;
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    let wait_result = unsafe { libc::waitpid(child, &raw mut wait_status, 0) };
    assert_eq!(wait_result, child);
    assert_eq!(status[0], 1, "isolated runtime setup probe failed");
    assert!(libc::WIFEXITED(wait_status));
    assert_eq!(libc::WEXITSTATUS(wait_status), 0);
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the UID-drop denial probe is one fork sequence and is one line over the limit"
)]
fn sibling_home_read_and_write_are_denied_after_uid_drop() {
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    if unsafe { libc::geteuid() } != 0 {
        return;
    }
    let temp = tempfile::tempdir().expect("temporary isolation fixture");
    let own = temp.path().join("home/slot-a");
    let sibling = temp.path().join("home/slot-b");
    let staged = temp.path().join("account-credentials");
    let workspace = temp.path().join("workspace");
    let selected_auth = temp.path().join("selected-auth.json");
    fs::create_dir_all(own.parent().expect("home parent")).expect("home root");
    fs::create_dir(&own).expect("own slot");
    fs::create_dir(&sibling).expect("sibling slot");
    fs::create_dir(&staged).expect("staged credential root");
    fs::create_dir(&workspace).expect("workspace");
    fs::set_permissions(&own, fs::Permissions::from_mode(0o700)).expect("own permissions");
    fs::set_permissions(&sibling, fs::Permissions::from_mode(0o700)).expect("sibling permissions");
    fs::set_permissions(&staged, fs::Permissions::from_mode(0o700)).expect("staged permissions");
    fs::set_permissions(&workspace, fs::Permissions::from_mode(0o700))
        .expect("workspace permissions");
    let own_secret = own.join("credential");
    let sibling_secret = sibling.join("credential");
    let sibling_staged = staged.join("acct-slot-b.json");
    fs::write(&own_secret, b"own").expect("own credential");
    fs::write(&sibling_secret, b"sibling").expect("sibling credential");
    fs::write(&sibling_staged, b"sibling-staged").expect("sibling staged credential");
    fs::write(&selected_auth, b"selected-auth").expect("selected auth mount");

    let (read_fd, write_fd) = {
        let mut fds = [0; 2];
        #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
        let pipe_status = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(pipe_status, 0);
        (fds[0], fds[1])
    };
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    let child = unsafe { libc::fork() };
    assert!(child >= 0, "fork isolation probe");
    if child == 0 {
        #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
        unsafe {
            libc::close(read_fd)
        };
        let result = (|| -> anyhow::Result<()> {
            drop_privileges(jackin_protocol::SessionIdentity {
                uid: 65_534,
                gid: 65_534,
            })?;
            let mut rules = Vec::new();
            add_execute_only_ancestors(&mut rules, &own);
            rules.push(Rule {
                path: own.clone(),
                access: FULL,
                required: true,
            });
            add_execute_only_ancestors(&mut rules, &workspace);
            rules.push(Rule {
                path: workspace.clone(),
                access: FULL,
                required: true,
            });
            add_execute_only_ancestors(&mut rules, &selected_auth);
            rules.push(Rule {
                path: selected_auth.clone(),
                access: READ_FILE_ONLY,
                required: true,
            });
            install_landlock(&rules)?;
            let own_fd = open(&own_secret, libc::O_RDONLY);
            anyhow::ensure!(own_fd >= 0, "selected slot read denied");
            #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
            unsafe {
                libc::close(own_fd)
            };
            let own_write = open(&own.join("selected-write"), libc::O_WRONLY | libc::O_CREAT);
            anyhow::ensure!(own_write >= 0, "selected slot write denied");
            #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
            unsafe {
                libc::close(own_write)
            };
            let workspace_write = open(
                &workspace.join("agent-write"),
                libc::O_WRONLY | libc::O_CREAT,
            );
            anyhow::ensure!(workspace_write >= 0, "workspace write denied");
            #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
            unsafe {
                libc::close(workspace_write)
            };
            let selected_auth_fd = open(&selected_auth, libc::O_RDONLY);
            anyhow::ensure!(selected_auth_fd >= 0, "selected auth read denied");
            #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
            unsafe {
                libc::close(selected_auth_fd)
            };
            let selected_auth_write = open(&selected_auth, libc::O_WRONLY | libc::O_CREAT);
            anyhow::ensure!(
                selected_auth_write < 0,
                "read-only selected auth mount was writable"
            );
            let sibling_fd = open(&sibling_secret, libc::O_RDONLY);
            anyhow::ensure!(sibling_fd < 0, "sibling slot read was allowed");
            let sibling_write = open(&sibling.join("new-file"), libc::O_WRONLY | libc::O_CREAT);
            anyhow::ensure!(sibling_write < 0, "sibling slot write was allowed");
            let sibling_staged_fd = open(&sibling_staged, libc::O_RDONLY);
            anyhow::ensure!(
                sibling_staged_fd < 0,
                "sibling staged credential read was allowed"
            );
            let sibling_staged_write = open(&sibling_staged, libc::O_WRONLY | libc::O_CREAT);
            anyhow::ensure!(
                sibling_staged_write < 0,
                "sibling staged credential write was allowed"
            );
            Ok(())
        })();
        if let Err(error) = &result {
            eprintln!("Linux isolation probe failed: {error:#}");
        }
        let status = [u8::from(result.is_ok())];
        #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
        unsafe {
            libc::write(write_fd, status.as_ptr().cast(), 1)
        };
        #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
        unsafe {
            libc::close(write_fd)
        };
        #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
        unsafe {
            libc::_exit(i32::from(result.is_err()))
        };
    }
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    unsafe {
        libc::close(write_fd)
    };
    let mut status = [0u8; 1];
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    let bytes_read = unsafe { libc::read(read_fd, status.as_mut_ptr().cast(), 1) };
    assert_eq!(bytes_read, 1);
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    unsafe {
        libc::close(read_fd)
    };
    let mut wait_status = 0;
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    let wait_result = unsafe { libc::waitpid(child, &raw mut wait_status, 0) };
    assert_eq!(wait_result, child);
    assert_eq!(status[0], 1, "isolation probe failed inside child");
    assert!(libc::WIFEXITED(wait_status));
    assert_eq!(libc::WEXITSTATUS(wait_status), 0);
}

#[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
fn open(path: &Path, flags: libc::c_int) -> libc::c_int {
    let Ok(path) = std::ffi::CString::new(path.to_string_lossy().as_bytes()) else {
        return -1;
    };

    unsafe { libc::open(path.as_ptr(), flags, 0o600) }
}
