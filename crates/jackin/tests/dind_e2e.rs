//! End-to-end smoke that drives `jackin load` against a real Docker daemon
//! with proxy env declared in role config, then asserts the launched agent
//! container's environment carries the `DinD` hostname in both `NO_PROXY`
//! and `no_proxy`. Regression guard for the proxy-routed `DinD`-handshake
//! bug fixed in `src/runtime/launch.rs`.

#![cfg(feature = "e2e")]
#![allow(clippy::disallowed_methods)]
#![expect(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Docker integration fixtures should fail immediately with source location and command context"
)]

use std::time::Duration;

use jackin_runtime::instance::naming::is_dns_label;
use tempfile::tempdir;

#[path = "dind_e2e/chaos.rs"]
mod chaos;
#[path = "dind_e2e/common.rs"]
mod common;
#[path = "dind_e2e/diagnostics.rs"]
mod diagnostics;
#[path = "dind_e2e/fixtures.rs"]
mod fixtures;
#[path = "dind_e2e/pty_runner.rs"]
mod pty_runner;
#[path = "dind_e2e/transcript.rs"]
mod transcript;
#[path = "dind_e2e/util.rs"]
mod util;

use common::{e2e_construct_image, e2e_serial_lock, require_e2e_prereqs};
use diagnostics::e2e_failure_context;
use fixtures::{
    seed_agent_smith_role_repo, seed_all_agent_stubs, seed_claude_installer_stub,
    seed_existing_construct_entry, seed_sentinel_role_repo, seed_slow_exit_role_repo, write_config,
    write_sentinel_config, write_slow_exit_config,
};
use pty_runner::{
    PtyFileSentinel, PtyQuickExit, run_in_pty_until_file, run_in_pty_until_quick_exit_after_input,
    scripted_sentinel_launch_input,
};
use util::{
    REPORT_BEGIN, REPORT_END, assert_sentinel_build_output_routed_to_log, assert_sentinel_report,
    cleanup_role, find_report_value,
};

const ROLE_KEY: &str = "jackin-e2e/agent-smith";
const ROLE_CONTAINER_PREFIX: &str = "jackin-jackin-e2e__agent-smith";
const SENTINEL_ROLE_KEY: &str = "jackin-e2e/sentinel";
const SENTINEL_CONTAINER_PREFIX: &str = "jackin-jackin-e2e__sentinel";
const SLOW_EXIT_ROLE_KEY: &str = "jackin-e2e/slow-exit";
const SLOW_EXIT_CONTAINER_PREFIX: &str = "jackin-jackin-e2e__slow-exit";
const TESTCONTAINERS_SMOKE_OK: &str = "TESTCONTAINERS_SMOKE=ok";

/// RAII cleanup so the test's Docker resources are removed even if an
/// assertion or `script(1)` invocation panics. Without this, a flaky run
/// leaks a container/network/volume and the next run fails on name
/// collision — turning a transient failure into a sticky red CI.
struct E2eRoleCleanup {
    role_key: &'static str,
    container_prefix: &'static str,
}

impl Drop for E2eRoleCleanup {
    fn drop(&mut self) {
        cleanup_role(self.role_key, self.container_prefix);
    }
}

#[test]
fn jackin_load_agent_smith_can_reach_its_dind_daemon_with_proxy_env() {
    require_e2e_prereqs();
    let _serial = e2e_serial_lock();
    let _cleanup = E2eRoleCleanup {
        role_key: ROLE_KEY,
        container_prefix: ROLE_CONTAINER_PREFIX,
    };

    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let config_dir = home.join(".config/jackin");
    let role_source = temp.path().join("agent-smith-source");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&workspace_dir).unwrap();

    seed_agent_smith_role_repo(&role_source);
    write_config(&config_dir.join("config.toml"), &role_source);
    seed_claude_installer_stub(&home);

    let jackin = std::env::var("CARGO_BIN_EXE_jackin").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .join("target/debug/jackin")
            .display()
            .to_string()
    });

    let target = format!("{}:/workspace", workspace_dir.display());
    let args = ["load", ROLE_KEY, &target, "--agent", "claude"];
    // The Dockerfile pins FROM to 0.1-trixie (versioned, as required by
    // jackin-role validate). That tag doesn't exist until the first construct CI
    // build runs after this PR lands. Override with the published floating tag
    // so the E2E build succeeds in CI while the Dockerfile stays correctly
    // pinned for validation purposes.
    let construct_image = e2e_construct_image();
    let extra_env = [("JACKIN_CONSTRUCT_IMAGE", construct_image.as_str())];
    let report_path = workspace_dir.join("jackin-e2e-report.txt");
    let output = run_in_pty_until_file(
        &jackin,
        &args,
        &home,
        &workspace_dir,
        &extra_env,
        &[],
        PtyFileSentinel {
            path: &report_path,
            text: TESTCONTAINERS_SMOKE_OK,
            timeout: Duration::from_mins(6),
        },
    );

    // The capsule multiplexer is a full-screen renderer, so agent stdout is a
    // terminal transcript, not a stable report channel. The fake agent writes
    // the same report into the bound workspace; the PTY remains only the launch
    // driver.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = std::fs::read_to_string(&report_path).unwrap_or_else(|error| {
        panic!(
            "agent report file missing at {}: {error}\n{}",
            report_path.display(),
            e2e_failure_context(&home, stdout.as_ref(), stderr.as_ref())
        )
    });
    assert!(
        report.contains(REPORT_BEGIN),
        "agent did not emit {REPORT_BEGIN} marker\nreport:\n{report}\n{}",
        e2e_failure_context(&home, stdout.as_ref(), stderr.as_ref())
    );
    // REPORT_END proves the report block completed. Without this check a
    // partial transcript (agent crashed mid-print, PTY truncation) would
    // still satisfy the contains-substring asserts below on whatever
    // happened to land before the cut.
    assert!(
        report.contains(REPORT_END),
        "agent did not emit {REPORT_END} marker — report is truncated\nreport:\n{report}\n{}",
        e2e_failure_context(&home, stdout.as_ref(), stderr.as_ref())
    );

    let dind_hostname = find_report_value(&report, "JACKIN_DIND_HOSTNAME=")
        .unwrap_or_else(|| panic!("report must include JACKIN_DIND_HOSTNAME\n{report}"));
    assert!(is_dns_label(dind_hostname), "{dind_hostname}");
    assert!(!dind_hostname.contains("__"));
    assert!(!dind_hostname.contains("clone-"));

    assert!(report.contains(&format!("DOCKER_HOST=tcp://{dind_hostname}:2376")));
    assert!(report.contains("DOCKER_TLS_VERIFY=1"));
    assert!(report.contains("DOCKER_CERT_PATH=/jackin/run/dind-certs/client"));
    assert!(report.contains(&format!("JACKIN_DIND_HOSTNAME={dind_hostname}")));
    assert!(report.contains(&format!("TESTCONTAINERS_HOST_OVERRIDE={dind_hostname}")));
    // Both casings carry the merged list — operator's localhost,127.0.0.1
    // must reach tools that read either uppercase NO_PROXY (Go runtime) or
    // lowercase no_proxy (curl, Python requests, wget).
    let merged = format!("NO_PROXY=localhost,127.0.0.1,{dind_hostname}");
    let merged_lower = format!("no_proxy=localhost,127.0.0.1,{dind_hostname}");
    assert!(report.contains(&merged), "missing {merged}\n{report}");
    assert!(
        report.contains(&merged_lower),
        "missing {merged_lower}\n{report}"
    );
    assert!(
        report.contains("DIND_DOCKER_RUN_CHILD="),
        "agent did not emit the child container id\n{report}"
    );
    assert!(
        report.contains("DIND_DOCKER_RUN_STATE=running"),
        "agent's child container was not running\n{report}"
    );
    assert!(
        report.contains(TESTCONTAINERS_SMOKE_OK),
        "agent's Java Testcontainers smoke did not pass\n{}",
        e2e_failure_context(&home, stdout.as_ref(), stderr.as_ref())
    );
}

#[test]
fn jackin_load_sentinel_role_runs_hooks_and_keeps_build_output_off_screen() {
    require_e2e_prereqs();
    let _serial = e2e_serial_lock();
    let _cleanup = E2eRoleCleanup {
        role_key: SENTINEL_ROLE_KEY,
        container_prefix: SENTINEL_CONTAINER_PREFIX,
    };

    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let config_dir = home.join(".config/jackin");
    let role_source = temp.path().join("sentinel-source");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&workspace_dir).unwrap();
    // The container runs as the host (test-runner) UID/GID via `--user` on
    // docker run, so the agent writes its report into the test-owned workspace
    // dir with no special permissions.

    seed_sentinel_role_repo(&role_source);
    write_sentinel_config(&config_dir.join("config.toml"), &role_source);
    seed_all_agent_stubs(&home);
    seed_existing_construct_entry(&home);

    let jackin = std::env::var("CARGO_BIN_EXE_jackin").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .join("target/debug/jackin")
            .display()
            .to_string()
    });

    let target = format!("{}:/workspace", workspace_dir.display());
    let args = ["load", SENTINEL_ROLE_KEY, &target];
    let construct_image = e2e_construct_image();
    let extra_env = [("JACKIN_CONSTRUCT_IMAGE", construct_image.as_str())];
    let report_path = workspace_dir.join("jackin-sentinel-report.txt");
    let script = scripted_sentinel_launch_input();
    let output = run_in_pty_until_file(
        &jackin,
        &args,
        &home,
        &workspace_dir,
        &extra_env,
        &script,
        PtyFileSentinel {
            path: &report_path,
            text: "JACKIN_SENTINEL_REPORT_END",
            timeout: Duration::from_mins(5),
        },
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = std::fs::read_to_string(&report_path).unwrap_or_else(|error| {
        panic!(
            "sentinel report file missing at {}: {error}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            report_path.display()
        )
    });
    assert_sentinel_report(&report, &stdout, &stderr);
    assert_sentinel_build_output_routed_to_log(&home, &stdout, &stderr);
}

#[test]
fn jackin_load_ctrl_q_yes_exits_cold_build_quickly() {
    let output = run_slow_exit_load_until_quick_exit("\x11\r");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Ctrl+Q then Enter should hard-exit successfully during a cold build\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Exit jackin❯?"),
        "exit confirmation did not render before quick exit\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn jackin_load_double_ctrl_c_exits_cold_build_quickly() {
    let output = run_slow_exit_load_until_quick_exit("\x03\x03");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "double Ctrl+C should hard-exit successfully during a cold build\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn jackin_load_single_ctrl_c_exits_responsive_cold_build_quickly() {
    let output = run_slow_exit_load_until_quick_exit("\x03");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "single Ctrl+C should hard-exit successfully during a responsive cold build\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn run_slow_exit_load_until_quick_exit(input: &str) -> std::process::Output {
    require_e2e_prereqs();
    let _serial = e2e_serial_lock();
    let _cleanup = E2eRoleCleanup {
        role_key: SLOW_EXIT_ROLE_KEY,
        container_prefix: SLOW_EXIT_CONTAINER_PREFIX,
    };

    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let config_dir = home.join(".config/jackin");
    let role_source = temp.path().join("slow-exit-source");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&workspace_dir).unwrap();

    seed_slow_exit_role_repo(&role_source);
    write_slow_exit_config(&config_dir.join("config.toml"), &role_source);
    seed_claude_installer_stub(&home);

    let jackin = std::env::var("CARGO_BIN_EXE_jackin").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .join("target/debug/jackin")
            .display()
            .to_string()
    });

    let target = format!("{}:/workspace", workspace_dir.display());
    let args = ["load", SLOW_EXIT_ROLE_KEY, &target, "--agent", "claude"];
    let construct_image = e2e_construct_image();
    let extra_env = [("JACKIN_CONSTRUCT_IMAGE", construct_image.as_str())];
    run_in_pty_until_quick_exit_after_input(
        &jackin,
        &args,
        &home,
        &workspace_dir,
        &extra_env,
        PtyQuickExit {
            wait_for: "Building role base image",
            input,
            max_exit_after_input: Duration::from_secs(1),
        },
    )
}

#[test]
fn jackin_load_double_ctrl_c_exits_launch_prompt_quickly() {
    require_e2e_prereqs();
    let _serial = e2e_serial_lock();
    let _cleanup = E2eRoleCleanup {
        role_key: SENTINEL_ROLE_KEY,
        container_prefix: SENTINEL_CONTAINER_PREFIX,
    };

    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let config_dir = home.join(".config/jackin");
    let role_source = temp.path().join("sentinel-source");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&workspace_dir).unwrap();

    seed_sentinel_role_repo(&role_source);
    write_sentinel_config(&config_dir.join("config.toml"), &role_source);
    seed_all_agent_stubs(&home);
    seed_existing_construct_entry(&home);

    let jackin = std::env::var("CARGO_BIN_EXE_jackin").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .join("target/debug/jackin")
            .display()
            .to_string()
    });

    let target = format!("{}:/workspace", workspace_dir.display());
    let args = ["load", SENTINEL_ROLE_KEY, &target];
    let construct_image = e2e_construct_image();
    let extra_env = [("JACKIN_CONSTRUCT_IMAGE", construct_image.as_str())];
    let output = run_in_pty_until_quick_exit_after_input(
        &jackin,
        &args,
        &home,
        &workspace_dir,
        &extra_env,
        PtyQuickExit {
            wait_for: "Choose launch agent",
            input: "\x03\x03",
            max_exit_after_input: Duration::from_secs(1),
        },
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "double Ctrl+C should hard-exit successfully from the launch prompt\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn chaos_launch_until_report(
    home: &std::path::Path,
    workspace_dir: &std::path::Path,
    role_source: &std::path::Path,
) -> std::process::Output {
    write_config(&home.join(".config/jackin/config.toml"), role_source);
    seed_claude_installer_stub(home);
    let jackin = std::env::var("CARGO_BIN_EXE_jackin").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .join("target/debug/jackin")
            .display()
            .to_string()
    });
    let target = format!("{}:/workspace", workspace_dir.display());
    let args = ["load", ROLE_KEY, &target, "--agent", "claude"];
    let construct_image = e2e_construct_image();
    let extra_env = [("JACKIN_CONSTRUCT_IMAGE", construct_image.as_str())];
    let report_path = workspace_dir.join("jackin-e2e-report.txt");
    run_in_pty_until_file(
        &jackin,
        &args,
        home,
        workspace_dir,
        &extra_env,
        &[],
        PtyFileSentinel {
            path: &report_path,
            text: TESTCONTAINERS_SMOKE_OK,
            timeout: Duration::from_mins(6),
        },
    )
}

#[test]
fn chaos_kill_container_mid_session() {
    require_e2e_prereqs();
    let seed = chaos::seed();
    eprintln!("JACKIN_CHAOS_SEED={seed}");
    let mut rng = chaos::ChaosRng::new(seed);
    let planned = chaos::schedule(&mut rng, &[chaos::Fault::KillContainer], 1500);
    let _serial = e2e_serial_lock();
    let _cleanup = E2eRoleCleanup {
        role_key: ROLE_KEY,
        container_prefix: ROLE_CONTAINER_PREFIX,
    };
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let config_dir = home.join(".config/jackin");
    let role_source = temp.path().join("agent-smith-source");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&workspace_dir).unwrap();
    seed_agent_smith_role_repo(&role_source);

    let home_c = home.clone();
    let ws_c = workspace_dir.clone();
    let rs_c = role_source.clone();
    let handle = std::thread::spawn(move || chaos_launch_until_report(&home_c, &ws_c, &rs_c));

    let start = std::time::Instant::now();
    let container = loop {
        if let Some(name) = chaos::primary_running_container(ROLE_KEY) {
            break name;
        }
        if start.elapsed() > Duration::from_mins(6) {
            panic!("chaos_kill: no container appeared for {ROLE_KEY}");
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    std::thread::sleep(planned.delay);
    chaos::apply_fault(planned.fault, &container);

    let _output = handle.join().expect("launch thread panicked");
    chaos::wait_until_no_running(ROLE_KEY, Duration::from_secs(60));
    cleanup_role(ROLE_KEY, ROLE_CONTAINER_PREFIX);
    chaos::assert_no_orphaned_containers(ROLE_KEY);
    chaos::assert_no_stale_state_dirs(&home.join(".local/share/jackin"), &[]);
    chaos::assert_cleanup_classified(&home);
}

#[test]
fn chaos_sigkill_capsule() {
    require_e2e_prereqs();
    let seed = chaos::seed();
    eprintln!("JACKIN_CHAOS_SEED={seed}");
    let mut rng = chaos::ChaosRng::new(seed.wrapping_add(1));
    let planned = chaos::schedule(&mut rng, &[chaos::Fault::SigkillCapsule], 1500);
    let _serial = e2e_serial_lock();
    let _cleanup = E2eRoleCleanup {
        role_key: ROLE_KEY,
        container_prefix: ROLE_CONTAINER_PREFIX,
    };
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let config_dir = home.join(".config/jackin");
    let role_source = temp.path().join("agent-smith-source");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&workspace_dir).unwrap();
    seed_agent_smith_role_repo(&role_source);

    let home_c = home.clone();
    let ws_c = workspace_dir.clone();
    let rs_c = role_source.clone();
    let handle = std::thread::spawn(move || chaos_launch_until_report(&home_c, &ws_c, &rs_c));

    let start = std::time::Instant::now();
    let container = loop {
        if let Some(name) = chaos::primary_running_container(ROLE_KEY) {
            break name;
        }
        if start.elapsed() > Duration::from_mins(6) {
            panic!("chaos_sigkill: no container appeared");
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    std::thread::sleep(planned.delay);
    chaos::apply_fault(planned.fault, &container);

    let _output = handle.join().expect("launch thread panicked");
    chaos::wait_until_no_running(ROLE_KEY, Duration::from_secs(90));
    cleanup_role(ROLE_KEY, ROLE_CONTAINER_PREFIX);
    chaos::assert_no_orphaned_containers(ROLE_KEY);
    chaos::assert_cleanup_classified(&home);
}

#[test]
fn chaos_drop_control_socket() {
    require_e2e_prereqs();
    let seed = chaos::seed();
    eprintln!("JACKIN_CHAOS_SEED={seed}");
    let mut rng = chaos::ChaosRng::new(seed.wrapping_add(2));
    let planned = chaos::schedule(&mut rng, &[chaos::Fault::DropControlSocket], 1500);
    let _serial = e2e_serial_lock();
    let _cleanup = E2eRoleCleanup {
        role_key: ROLE_KEY,
        container_prefix: ROLE_CONTAINER_PREFIX,
    };
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let config_dir = home.join(".config/jackin");
    let role_source = temp.path().join("agent-smith-source");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&workspace_dir).unwrap();
    seed_agent_smith_role_repo(&role_source);

    let home_c = home.clone();
    let ws_c = workspace_dir.clone();
    let rs_c = role_source.clone();
    let handle = std::thread::spawn(move || chaos_launch_until_report(&home_c, &ws_c, &rs_c));

    let start = std::time::Instant::now();
    let container = loop {
        if let Some(name) = chaos::primary_running_container(ROLE_KEY) {
            break name;
        }
        if start.elapsed() > Duration::from_mins(6) {
            panic!("chaos_drop_socket: no container appeared");
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    std::thread::sleep(planned.delay);
    chaos::apply_fault(planned.fault, &container);

    let _output = handle.join().expect("launch thread panicked");
    cleanup_role(ROLE_KEY, ROLE_CONTAINER_PREFIX);
    chaos::assert_no_orphaned_containers(ROLE_KEY);
    chaos::assert_cleanup_classified(&home);
}
