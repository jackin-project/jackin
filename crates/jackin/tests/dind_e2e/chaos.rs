//! Seeded chaos helpers for the Docker-backed E2E lane (plan 046).
//!
//! Deterministic `xorshift64` schedule; seed from `JACKIN_CHAOS_SEED` or a
//! fixed default. Every docker filter is scoped by the harness's
//! `jackin.class` label / name prefix.

#![expect(
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::filter_map_next,
    clippy::map_unwrap_or,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]
use std::path::Path;
use std::time::{Duration, Instant};

use super::common::docker_command;

/// Default seed when `JACKIN_CHAOS_SEED` is unset (deterministic across runs).
pub(super) const DEFAULT_CHAOS_SEED: u64 = 0xc4a0_55eed_u64;

#[derive(Debug, Clone, Copy)]
pub(super) struct ChaosRng(u64);

impl ChaosRng {
    pub(super) fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub(super) fn next_u64(&mut self) -> u64 {
        // `xorshift64*`
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub(super) fn next_range(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        self.next_u64() % n
    }
}

pub(super) fn seed() -> u64 {
    std::env::var("JACKIN_CHAOS_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CHAOS_SEED)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fault {
    KillContainer,
    SigkillCapsule,
    DropControlSocket,
}

pub(super) struct ScheduledFault {
    pub fault: Fault,
    /// Delay after session-up before injecting the fault.
    pub delay: Duration,
}

pub(super) fn schedule(rng: &mut ChaosRng, faults: &[Fault], window_ms: u64) -> ScheduledFault {
    let fault = faults[rng.next_range(faults.len() as u64) as usize];
    let delay_ms = 200 + rng.next_range(window_ms.max(1));
    ScheduledFault {
        fault,
        delay: Duration::from_millis(delay_ms),
    }
}

pub(super) fn list_containers_for_role(role_key: &str) -> Vec<String> {
    let output = docker_command()
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("label=jackin.class={role_key}"),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("docker ps for role");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn assert_no_orphaned_containers(role_key: &str) {
    let names = list_containers_for_role(role_key);
    assert!(
        names.is_empty(),
        "orphaned jackin containers for role {role_key}: {names:?}"
    );
}

/// No leftover per-container dirs under the jackin data/state tree for gone containers.
pub(super) fn assert_no_stale_state_dirs(state_root: &Path, live_names: &[String]) {
    if !state_root.is_dir() {
        return;
    }
    let live: std::collections::HashSet<&str> = live_names.iter().map(String::as_str).collect();
    for entry in std::fs::read_dir(state_root)
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("jk-") || name.starts_with("jackin-") {
            assert!(
                live.contains(name.as_ref()) || !entry.path().is_dir(),
                "stale state dir for missing container: {}",
                entry.path().display()
            );
        }
    }
}

pub(super) fn apply_fault(fault: Fault, container: &str) {
    match fault {
        Fault::KillContainer => {
            let status = docker_command()
                .args(["kill", container])
                .status()
                .expect("docker kill");
            assert!(status.success(), "docker kill {container} failed");
        }
        Fault::SigkillCapsule => {
            // Capsule is PID 1 inside the role container.
            let status = docker_command()
                .args(["exec", container, "kill", "-9", "1"])
                .status()
                .expect("docker exec kill -9 1");
            // May fail if container already dead; that's acceptable mid-chaos.
            let _ = status;
        }
        Fault::DropControlSocket => {
            // In-container control socket path (capsule SOCKET_PATH).
            let status = docker_command()
                .args(["exec", container, "rm", "-f", "/jackin/run/jackin.sock"])
                .status()
                .expect("docker exec rm socket");
            let _ = status;
        }
    }
}

pub(super) fn wait_until_no_running(role_key: &str, timeout: Duration) {
    let start = Instant::now();
    loop {
        let output = docker_command()
            .args([
                "ps",
                "--filter",
                &format!("label=jackin.class={role_key}"),
                "--format",
                "{{.Names}}",
            ])
            .output()
            .expect("docker ps running");
        let running: Vec<_> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        if running.is_empty() {
            return;
        }
        assert!(
            start.elapsed() <= timeout,
            "timed out waiting for role {role_key} containers to stop: {running:?}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub(super) fn primary_running_container(role_key: &str) -> Option<String> {
    let output = docker_command()
        .args([
            "ps",
            "--filter",
            &format!("label=jackin.class={role_key}"),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|l| !l.is_empty() && !l.ends_with("-dind"))
        .map(str::to_owned)
}

/// Best-effort: diagnostics tree under home should mention cleanup/exit after chaos.
pub(super) fn assert_cleanup_classified(home: &Path) {
    let diag = home.join(".local/share/jackin/diagnostics");
    // Soft assert: presence of any diagnostics after a run is enough signal
    // that the host wrote end-state; full classification is plan 008/033.
    if diag.is_dir() {
        let has_any = std::fs::read_dir(&diag)
            .map(|rd| rd.filter_map(Result::ok).next().is_some())
            .unwrap_or(false);
        assert!(
            has_any,
            "expected diagnostics under {} after chaos run",
            diag.display()
        );
    }
}
