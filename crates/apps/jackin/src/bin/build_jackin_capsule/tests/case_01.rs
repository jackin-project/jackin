// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn feature_suffix_is_stable_and_filename_safe() {
    assert_eq!(feature_suffix(&[]), "");
    assert_eq!(
        feature_suffix(&["dhat-heap".to_owned(), "trace/json".to_owned()]),
        "-features-dhat-heap-trace-json"
    );
}

#[test]
fn feature_builds_do_not_overwrite_normal_cache_entry() {
    let cache = PathBuf::from("/tmp/cache");
    let normal = binary_cache_path(&cache, "0.6.0-dev+abc", "arm64", BuildProfile::Release, &[]);
    let dhat = binary_cache_path(
        &cache,
        "0.6.0-dev+abc",
        "arm64",
        BuildProfile::Release,
        &["dhat-heap".to_owned()],
    );

    assert_ne!(normal, dhat);
    assert!(normal.ends_with("jackin-capsule"));
    assert!(dhat.ends_with("jackin-capsule-features-dhat-heap"));
}

#[test]
fn target_directory_uses_cargo_environment_path_rules() {
    let workspace = Path::new("/workspace");

    assert_eq!(target_directory(workspace, None), workspace.join("target"));
    assert_eq!(
        target_directory(workspace, Some(OsStr::new("artifacts"))),
        workspace.join("artifacts")
    );
    assert_eq!(
        target_directory(workspace, Some(OsStr::new("/scratch/jackin-target"))),
        PathBuf::from("/scratch/jackin-target")
    );
}

#[cfg(unix)]
#[test]
fn target_directory_preserves_non_utf8_relative_path() {
    use std::os::unix::ffi::OsStrExt;

    let workspace = Path::new("/workspace");
    let configured = OsStr::from_bytes(b"target-\xff");
    let resolved = target_directory(workspace, Some(configured));

    assert_eq!(resolved.as_os_str().as_bytes(), b"/workspace/target-\xff");
    let args = mbx_zigbuild_args("x86_64-unknown-linux-gnu.2.17", "release", &[], &resolved);
    assert_eq!(args[10].as_os_str().as_bytes(), b"/workspace/target-\xff");
}

#[cfg(unix)]
#[test]
fn mbx_zigbuild_invocation_preserves_arguments_cwd_and_exit_status() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    std::fs::create_dir(&workspace).unwrap();
    let target_dir = workspace.join("target with spaces");
    let fake_mise = fake_mise(temp.path());

    let features = ["dhat-heap".to_owned(), "trace/json".to_owned()];
    let args = mbx_zigbuild_args(
        "x86_64-unknown-linux-gnu.2.17",
        "release",
        &features,
        &target_dir,
    );
    assert_ne!(std::env::current_dir().unwrap(), workspace);
    let mut child = mise_command_with_fd_limit(fake_mise.as_os_str(), &workspace, &args)
        .spawn()
        .unwrap();
    let child_pid = child.id();
    let status = child.wait().unwrap();

    assert_eq!(status.code(), Some(37));
    assert_eq!(
        std::fs::read_to_string(workspace.join(".fake-mise-pid"))
            .unwrap()
            .trim(),
        child_pid.to_string()
    );
    let captured_args = std::fs::read_to_string(workspace.join(".fake-mise-args")).unwrap();
    assert_eq!(
        captured_args.lines().collect::<Vec<_>>(),
        [
            "exec",
            "--",
            "mbx",
            "zigbuild",
            "--profile",
            "release",
            "-p",
            "jackin-capsule",
            "--target",
            "x86_64-unknown-linux-gnu.2.17",
            "--locked",
            "--target-dir",
            target_dir.to_str().unwrap(),
            "--features",
            "dhat-heap,trace/json",
            "--END-COMMAND--",
        ]
    );
    assert!(
        std::fs::read_to_string(workspace.join(".fake-mise-cwd"))
            .unwrap()
            .trim_end()
            .ends_with("workspace with spaces")
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join(".fake-mise-fd-limit"))
            .unwrap()
            .trim(),
        expected_fd_limit_after_best_effort_raise()
    );
}

#[cfg(unix)]
#[test]
fn rustup_target_preflight_uses_workspace_mise_and_reports_install_failure() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("selected workspace with spaces");
    std::fs::create_dir(&workspace).unwrap();
    let fake_mise = fake_mise(temp.path());
    let triple = "aarch64-unknown-linux-gnu";

    assert_ne!(std::env::current_dir().unwrap(), workspace);
    let error =
        ensure_rustup_target_with_mise(fake_mise.as_os_str(), &workspace, triple).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("rustup target add aarch64-unknown-linux-gnu failed")
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join(".fake-mise-args"))
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        [
            "exec",
            "--",
            "rustup",
            "target",
            "list",
            "--installed",
            "--END-COMMAND--",
            "exec",
            "--",
            "rustup",
            "target",
            "add",
            triple,
            "--END-COMMAND--",
        ]
    );
    assert!(
        std::fs::read_to_string(workspace.join(".fake-mise-cwd"))
            .unwrap()
            .trim_end()
            .ends_with("selected workspace with spaces")
    );
}

#[cfg(unix)]
#[test]
fn rustup_target_preflight_skips_installed_targets_and_stops_on_list_failure() {
    let temp = tempfile::tempdir().unwrap();
    let fake_mise = fake_mise(temp.path());
    let triple = "x86_64-unknown-linux-gnu";

    let installed_workspace = temp.path().join("installed workspace");
    std::fs::create_dir(&installed_workspace).unwrap();
    std::fs::write(
        installed_workspace.join(".fake-rustup-installed-targets"),
        triple,
    )
    .unwrap();
    ensure_rustup_target_with_mise(fake_mise.as_os_str(), &installed_workspace, triple).unwrap();
    assert_eq!(
        std::fs::read_to_string(installed_workspace.join(".fake-mise-args"))
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        [
            "exec",
            "--",
            "rustup",
            "target",
            "list",
            "--installed",
            "--END-COMMAND--",
        ]
    );

    let failed_workspace = temp.path().join("failed query workspace");
    std::fs::create_dir(&failed_workspace).unwrap();
    std::fs::write(failed_workspace.join(".fake-rustup-list-failure"), "").unwrap();
    let error = ensure_rustup_target_with_mise(fake_mise.as_os_str(), &failed_workspace, triple)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("rustup target list --installed failed")
    );
    assert_eq!(
        std::fs::read_to_string(failed_workspace.join(".fake-mise-args"))
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        [
            "exec",
            "--",
            "rustup",
            "target",
            "list",
            "--installed",
            "--END-COMMAND--",
        ]
    );
}

#[cfg(unix)]
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "test polls a fixture readiness file on its owned thread"
)]
fn fd_limited_mise_command_delivers_signal_to_execed_process() {
    use std::os::unix::process::ExitStatusExt;

    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("signal workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join(".fake-mise-wait"), "").unwrap();
    let fake_mise = fake_mise(temp.path());
    let args = [OsString::from("mbx"), OsString::from("zigbuild")];
    let mut child = mise_command_with_fd_limit(fake_mise.as_os_str(), &workspace, &args)
        .spawn()
        .unwrap();
    let child_pid = child.id();
    let pid_path = workspace.join(".fake-mise-pid");
    let ready_path = workspace.join(".fake-mise-ready");
    let start = std::time::Instant::now();
    while !ready_path.exists() && start.elapsed() < std::time::Duration::from_secs(2) {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    if !ready_path.exists() {
        drop(child.kill());
        drop(child.wait());
        panic!("fake Mise did not start within two seconds");
    }
    let fake_pid_text = match std::fs::read_to_string(pid_path) {
        Ok(text) => text,
        Err(error) => {
            drop(child.kill());
            drop(child.wait());
            panic!("fake Mise PID capture failed: {error}");
        }
    };
    let fake_pid = match fake_pid_text.trim().parse::<u32>() {
        Ok(pid) => pid,
        Err(error) => {
            drop(child.kill());
            drop(child.wait());
            panic!("fake Mise PID capture was invalid: {error}");
        }
    };

    if fake_pid != child_pid {
        let kill_status = process::Command::new("sh")
            .args(["-c", "kill -KILL \"$1\"", "kill-fake-mise"])
            .arg(fake_pid.to_string())
            .status();
        if !kill_status.is_ok_and(|status| status.success()) {
            drop(child.kill());
            drop(child.wait());
            panic!("could not terminate fake Mise process {fake_pid}");
        }
    }
    child.kill().unwrap();
    let status = child.wait().unwrap();

    assert_eq!(fake_pid, child_pid);
    assert_eq!(status.signal(), Some(9));
    assert_eq!(
        std::fs::read_to_string(workspace.join(".fake-mise-fd-limit"))
            .unwrap()
            .trim(),
        expected_fd_limit_after_best_effort_raise()
    );
}
