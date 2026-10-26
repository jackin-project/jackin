// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::linux::{
    FULL, NULL_DEVICE, READ_FILE_ONLY, READ_ONLY, Rule, add_execute_only_ancestors,
    drop_privileges, install_landlock, rules_for, rules_for_test, support,
};
use anyhow::Context;
use jackin_protocol::CapsuleConfig;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use jackin_process::{ExecRequest, ExecResult};

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

const CHILD_TEST: &str = "JACKIN_LINUX_PRIVILEGE_PROBE_TEST";
const CHILD_FIXTURE: &str = "JACKIN_LINUX_PRIVILEGE_PROBE_FIXTURE";
const CHILD_CONTROL: &str = "JACKIN_LINUX_PRIVILEGE_PROBE_CONTROL";
const OUTPUT_LIMIT: usize = 64 * 1024;

fn running_as_root() -> bool {
    // SAFETY: geteuid has no pointer arguments and reports this process's uid.
    #[expect(unsafe_code, reason = "audited isolation test-probe syscall")]
    unsafe {
        libc::geteuid() == 0
    }
}

fn child_fixture(name: &str) -> Option<PathBuf> {
    if std::env::var(CHILD_TEST).ok().as_deref() != Some(name) {
        return None;
    }
    assert!(
        running_as_root(),
        "privilege probe child must start as root"
    );
    Some(PathBuf::from(
        std::env::var_os(CHILD_FIXTURE).expect("parent fixture path"),
    ))
}

fn marker(name: &str) -> String {
    format!("JACKIN_LINUX_PRIVILEGE_PROBE_OK:{name}")
}

fn child_request(name: &str, root: &Path, timeout: Duration) -> ExecRequest {
    ExecRequest::new(
        std::env::current_exe().expect("current test executable"),
        ["--exact", name, "--nocapture", "--test-threads=1"],
    )
    .env_remove([CHILD_CONTROL])
    .envs([
        (CHILD_TEST, std::ffi::OsStr::new(name)),
        (CHILD_FIXTURE, root.as_os_str()),
    ])
    .timeout(timeout)
    .output_limits(OUTPUT_LIMIT, OUTPUT_LIMIT)
}

fn probe_passed(output: &ExecResult, name: &str) -> bool {
    let text = String::from_utf8_lossy(&output.stdout);
    output.success
        && output.code == Some(0)
        && !output.timed_out
        && text.lines().filter(|line| *line == marker(name)).count() == 1
        && text
            .lines()
            .filter(|line| *line == "running 1 test")
            .count()
            == 1
        && text
            .lines()
            .filter(|line| {
                line.starts_with("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; ")
            })
            .count()
            == 1
}

fn require_probe(name: &str, root: &Path) {
    let output = jackin_process::exec_sync(&child_request(name, root, Duration::from_secs(30)))
        .expect("bounded privilege probe subprocess");
    assert!(
        probe_passed(&output, name),
        "privilege probe failed: {output:?}"
    );
}

#[test]
fn isolated_runtime_setup_can_spawn_git_config_with_null_stdio() {
    let name = concat!(
        module_path!(),
        "::isolated_runtime_setup_can_spawn_git_config_with_null_stdio"
    )
    .split_once("::")
    .expect("crate-qualified test path")
    .1;
    if let Some(root) = child_fixture(name) {
        runtime_setup_probe(&root).expect("isolated runtime setup probe");
        println!("\n{}", marker(name));
        return;
    }
    if !running_as_root() {
        return;
    }
    let temp = tempfile::tempdir().expect("temporary runtime setup fixture");
    for path in [temp.path().join("workspace"), temp.path().join("session")] {
        fs::create_dir(&path).expect("runtime fixture directory");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o777))
            .expect("runtime fixture permissions");
    }
    require_probe(name, temp.path());
}

fn runtime_setup_probe(root: &Path) -> anyhow::Result<()> {
    let workspace = root.join("workspace");
    let session_root = root.join("session");
    drop_privileges(jackin_protocol::SessionIdentity {
        uid: 65_534,
        gid: 65_534,
    })?;
    let rules = rules_for_test(&CapsuleConfig::default(), None, &workspace, &session_root)?;
    install_landlock(&rules)?;
    let global_config = session_root.join("gitconfig");
    let request = ExecRequest::new(
        "git",
        [
            "config",
            "--global",
            "--get-all",
            "url.https://github.com/.insteadOf",
        ],
    )
    .cwd(&workspace)
    .envs([("GIT_CONFIG_GLOBAL", global_config.as_os_str())])
    .timeout(Duration::from_secs(5))
    .output_limits(OUTPUT_LIMIT, OUTPUT_LIMIT);
    let output =
        jackin_process::exec_sync(&request).context("spawn git config under session Landlock")?;
    anyhow::ensure!(
        output.code == Some(1) && !output.timed_out,
        "git config probe exited unexpectedly: {:?}, stderr={}",
        output.code,
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn sibling_home_read_and_write_are_denied_after_uid_drop() {
    let name = concat!(
        module_path!(),
        "::sibling_home_read_and_write_are_denied_after_uid_drop"
    )
    .split_once("::")
    .expect("crate-qualified test path")
    .1;
    if let Some(root) = child_fixture(name) {
        sibling_access_probe(&root).expect("Linux isolation probe");
        println!("\n{}", marker(name));
        return;
    }
    if !running_as_root() {
        return;
    }
    let temp = tempfile::tempdir().expect("temporary isolation fixture");
    let home = temp.path().join("home");
    fs::create_dir(&home).expect("home root");
    for path in [
        home.join("slot-a"),
        home.join("slot-b"),
        temp.path().join("account-credentials"),
        temp.path().join("workspace"),
    ] {
        fs::create_dir(&path).expect("isolation fixture directory");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .expect("isolation fixture permissions");
    }
    for (path, bytes) in [
        (home.join("slot-a/credential"), b"own".as_slice()),
        (home.join("slot-b/credential"), b"sibling".as_slice()),
        (
            temp.path().join("account-credentials/acct-slot-b.json"),
            b"sibling-staged".as_slice(),
        ),
        (
            temp.path().join("selected-auth.json"),
            b"selected-auth".as_slice(),
        ),
    ] {
        fs::write(path, bytes).expect("isolation fixture credential");
    }
    require_probe(name, temp.path());
}

#[expect(
    clippy::disallowed_methods,
    reason = "owned subprocess probe performs blocking permission checks outside render/runtime threads"
)]
fn create_for_write(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
}

#[expect(
    clippy::disallowed_methods,
    reason = "fresh subprocess owns blocking filesystem permission probes after privilege isolation"
)]
fn sibling_access_probe(root: &Path) -> anyhow::Result<()> {
    let own = root.join("home/slot-a");
    let sibling = root.join("home/slot-b");
    let staged = root.join("account-credentials/acct-slot-b.json");
    let workspace = root.join("workspace");
    let selected_auth = root.join("selected-auth.json");
    drop_privileges(jackin_protocol::SessionIdentity {
        uid: 65_534,
        gid: 65_534,
    })?;
    let mut rules = Vec::new();
    for (path, access) in [
        (&own, FULL),
        (&workspace, FULL),
        (&selected_auth, READ_FILE_ONLY),
    ] {
        add_execute_only_ancestors(&mut rules, path);
        rules.push(Rule {
            path: path.clone(),
            access,
            required: true,
        });
    }
    install_landlock(&rules)?;
    drop(File::open(own.join("credential")).context("selected slot read denied")?);
    drop(create_for_write(&own.join("selected-write")).context("selected slot write denied")?);
    drop(create_for_write(&workspace.join("agent-write")).context("workspace write denied")?);
    drop(File::open(&selected_auth).context("selected auth read denied")?);
    anyhow::ensure!(
        create_for_write(&selected_auth).is_err(),
        "read-only selected auth mount was writable"
    );
    anyhow::ensure!(
        File::open(sibling.join("credential")).is_err(),
        "sibling slot read was allowed"
    );
    anyhow::ensure!(
        create_for_write(&sibling.join("new-file")).is_err(),
        "sibling slot write was allowed"
    );
    anyhow::ensure!(
        File::open(&staged).is_err(),
        "sibling staged credential read was allowed"
    );
    anyhow::ensure!(
        create_for_write(&staged).is_err(),
        "sibling staged credential write was allowed"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "intentional sleeping child verifies bounded subprocess cancellation outside render/runtime threads"
)]
fn bounded_privilege_probe_rejects_panic_zero_tests_and_timeout() {
    let name = concat!(
        module_path!(),
        "::bounded_privilege_probe_rejects_panic_zero_tests_and_timeout"
    )
    .split_once("::")
    .expect("crate-qualified test path")
    .1;
    if std::env::var(CHILD_TEST).ok().as_deref() == Some(name) {
        match std::env::var(CHILD_CONTROL).as_deref() {
            Ok("panic") => panic!("intentional privilege probe control panic"),
            Ok("timeout") => {
                let root =
                    PathBuf::from(std::env::var_os(CHILD_FIXTURE).expect("control fixture path"));
                fs::write(
                    root.join("timeout-entered"),
                    b"JACKIN_PRIVILEGE_TIMEOUT_CONTROL_ENTERED",
                )
                .expect("timeout control handshake");
                std::thread::sleep(Duration::from_secs(60));
            }
            _ => panic!("unknown privilege probe control"),
        }
        return;
    }
    let temp = tempfile::tempdir().expect("subprocess controls");
    let absent = "jackin_privilege_probe_selector_that_does_not_exist";
    let zero =
        jackin_process::exec_sync(&child_request(absent, temp.path(), Duration::from_secs(5)))
            .expect("zero-test subprocess");
    assert!(zero.success && !zero.timed_out);
    assert!(String::from_utf8_lossy(&zero.stdout).contains("running 0 tests"));
    assert!(!probe_passed(&zero, absent), "zero selected tests accepted");
    let mut panic_request = child_request(name, temp.path(), Duration::from_secs(5));
    panic_request.env_remove.clear();
    let panic_output = jackin_process::exec_sync(&panic_request.envs([(CHILD_CONTROL, "panic")]))
        .expect("panic control subprocess");
    assert!(!panic_output.success && !panic_output.timed_out);
    assert!(
        String::from_utf8_lossy(&panic_output.stdout)
            .contains("intentional privilege probe control panic")
            || String::from_utf8_lossy(&panic_output.stderr)
                .contains("intentional privilege probe control panic")
    );
    assert!(!probe_passed(&panic_output, name));
    let mut timeout_request = child_request(name, temp.path(), Duration::from_secs(1));
    timeout_request.env_remove.clear();
    let timeout_output =
        jackin_process::exec_sync(&timeout_request.envs([(CHILD_CONTROL, "timeout")]))
            .expect("timeout control subprocess");
    assert!(timeout_output.timed_out && !timeout_output.success);
    assert_eq!(
        fs::read(temp.path().join("timeout-entered")).expect("child entered timeout branch"),
        b"JACKIN_PRIVILEGE_TIMEOUT_CONTROL_ENTERED"
    );
    assert!(!probe_passed(&timeout_output, name));
}
