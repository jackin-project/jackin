#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::manual_assert,
    clippy::duration_suboptimal_units,
    clippy::filter_map_next,
    clippy::map_unwrap_or,
    clippy::redundant_closure,
    unreachable_pub,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]

//! WP0 — Acceptance test matrix harness: Tier 1 mechanism probes.
//!
//! For each Docker security profile (`locked`, `hardened`, `standard`, `compat`),
//! starts a container with the flags jackin❯ would apply, then asserts the
//! effective posture via `docker exec`:
//!
//! - Capability set: `NET_ADMIN`/`NET_RAW` present only for allowlist profiles;
//!   minimum 8-cap set applied under `hardened`/`locked`.
//! - Read-only root + tmpfs: write to `/` fails, write to `/tmp` succeeds.
//! - `no-new-privileges`: set where expected (`standard` with no sudo grant,
//!   `hardened`, `locked`); clear for `compat`.
//! - `cgroup_version`: `v1`/`v2` probe (informational — v2 enforced at launch,
//!   not tested here since the test host may be v1).
//!
//! **Tier 2** (real workloads, expensive, runs nightly or `JACKIN_E2E_TIER2=1`)
//! is scaffolded at the bottom of this file but intentionally left as stubs.
//! Tier 1 is always-on within the `e2e` feature gate.

#![cfg(feature = "e2e")]
use std::process::Command;

// ── image ─────────────────────────────────────────────────────────────────────

/// Lightweight image used for posture probes. Requires only `sh`, `touch`,
/// `cat`, and `sleep` — all present in `BusyBox`.
const PROBE_IMAGE: &str = "busybox:1.36";

// ── helpers ───────────────────────────────────────────────────────────────────

fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn require_docker() {
    assert!(
        docker_available(),
        "profile matrix probes require a running Docker daemon (`docker info` failed). \
         Disable the `e2e` feature or start Docker."
    );
}

fn docker_run_bg(name: &str, extra_args: &[&str]) -> String {
    let mut args = vec!["run", "-d", "--name", name];
    args.extend_from_slice(extra_args);
    args.extend_from_slice(&[PROBE_IMAGE, "sh", "-c", "sleep 120"]);
    let output = Command::new("docker")
        .args(&args)
        .output()
        .expect("docker run must spawn");
    assert!(
        output.status.success(),
        "docker run failed for {name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn docker_exec_ok(container: &str, cmd: &[&str]) -> bool {
    let mut args = vec!["exec", container];
    args.extend_from_slice(cmd);
    Command::new("docker")
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn docker_exec_output(container: &str, cmd: &[&str]) -> String {
    let mut args = vec!["exec", container];
    args.extend_from_slice(cmd);
    let output = Command::new("docker")
        .args(&args)
        .output()
        .expect("docker exec must spawn");
    String::from_utf8_lossy(&output.stdout).to_lowercase()
}

fn docker_rm(name: &str) {
    drop(
        Command::new("docker")
            .args(["rm", "-f", name])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output(),
    );
}

/// RAII guard that removes the container even if a test panics.
struct ContainerGuard(String);

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        docker_rm(&self.0);
    }
}

fn no_new_privileges_active(container: &str) -> bool {
    // `/proc/1/status` has `NoNewPrivs: 1` when the flag is active.
    let out = docker_exec_output(container, &["sh", "-c", "cat /proc/1/status"]);
    out.contains("nonewprivs:\t1") || out.contains("no_new_privs:\t1")
}

/// Read `CapEff` from `/proc/self/status` and test whether `bit` is set.
///
/// Linux capability bit numbers: `NET_ADMIN`=12, `NET_RAW`=13, `SETPCAP`=8.
fn cap_eff_has_bit(container: &str, bit: u32) -> bool {
    let out = docker_exec_output(container, &["sh", "-c", "cat /proc/self/status"]);
    for line in out.lines() {
        // The field is lowercase in `docker_exec_output` (it calls `.to_lowercase()`).
        if let Some(hex) = line.strip_prefix("capeff:\t")
            && let Ok(val) = u64::from_str_radix(hex.trim(), 16)
        {
            return (val >> bit) & 1 == 1;
        }
    }
    false
}

// ── Tier 1: mechanism probes ──────────────────────────────────────────────────

/// `locked` — read-only root, minimum caps + `NET_ADMIN`/`NET_RAW` (Allowlist implicit), `no-new-privileges`.
#[test]
fn tier1_locked_posture() {
    require_docker();
    let name = "jackin-profile-matrix-locked";
    let _guard = ContainerGuard(name.to_owned());
    docker_rm(name);

    docker_run_bg(
        name,
        &[
            "--read-only",
            "--tmpfs",
            "/tmp:mode=1777",
            "--tmpfs",
            "/run:exec",
            "--cap-drop=ALL",
            // MINIMUM_CAPABILITIES (docker_profile.rs)
            "--cap-add",
            "CHOWN",
            "--cap-add",
            "DAC_OVERRIDE",
            "--cap-add",
            "FOWNER",
            "--cap-add",
            "FSETID",
            "--cap-add",
            "SETUID",
            "--cap-add",
            "SETGID",
            "--cap-add",
            "SETFCAP",
            "--cap-add",
            "KILL",
            // Implicit from Allowlist network (apply_implicit_grants)
            "--cap-add",
            "NET_ADMIN",
            "--cap-add",
            "NET_RAW",
            "--security-opt",
            "no-new-privileges",
            "--memory",
            "4294967296", // 4 GiB
            "--network",
            "none",
        ],
    );

    // Root filesystem is read-only — writing to / must fail.
    assert!(
        !docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /test-write-probe 2>/dev/null"]
        ),
        "locked: / must be read-only"
    );

    // /tmp is writable via tmpfs.
    assert!(
        docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /tmp/test-write-probe"]
        ),
        "locked: /tmp must be writable"
    );

    // no-new-privileges active.
    assert!(
        no_new_privileges_active(container_ref(name)),
        "locked: no-new-privileges must be active"
    );

    // Capability set: NET_ADMIN (12) and NET_RAW (13) present (Allowlist implicit);
    // SETPCAP (8) absent (not in MINIMUM_CAPABILITIES — WP0 cap-set probe).
    assert!(
        cap_eff_has_bit(container_ref(name), 12),
        "locked: NET_ADMIN (bit 12) must be present (Allowlist network implicit cap)"
    );
    assert!(
        cap_eff_has_bit(container_ref(name), 13),
        "locked: NET_RAW (bit 13) must be present (Allowlist network implicit cap)"
    );
    assert!(
        !cap_eff_has_bit(container_ref(name), 8),
        "locked: SETPCAP (bit 8) must be absent (not in MINIMUM_CAPABILITIES)"
    );
}

/// `hardened` — read-only root, minimum caps + `NET_ADMIN`/`NET_RAW` (Allowlist implicit), `no-new-privileges`.
#[test]
fn tier1_hardened_posture() {
    require_docker();
    let name = "jackin-profile-matrix-hardened";
    let _guard = ContainerGuard(name.to_owned());
    docker_rm(name);

    docker_run_bg(
        name,
        &[
            "--read-only",
            "--tmpfs",
            "/tmp:mode=1777",
            "--tmpfs",
            "/run:exec",
            "--cap-drop=ALL",
            // MINIMUM_CAPABILITIES (docker_profile.rs)
            "--cap-add",
            "CHOWN",
            "--cap-add",
            "DAC_OVERRIDE",
            "--cap-add",
            "FOWNER",
            "--cap-add",
            "FSETID",
            "--cap-add",
            "SETUID",
            "--cap-add",
            "SETGID",
            "--cap-add",
            "SETFCAP",
            "--cap-add",
            "KILL",
            // Implicit from Allowlist network (apply_implicit_grants)
            "--cap-add",
            "NET_ADMIN",
            "--cap-add",
            "NET_RAW",
            "--security-opt",
            "no-new-privileges",
            "--memory",
            "17179869184", // 16 GiB
        ],
    );

    // Root filesystem is read-only.
    assert!(
        !docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /test-write-probe 2>/dev/null"]
        ),
        "hardened: / must be read-only"
    );

    // /tmp is writable.
    assert!(
        docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /tmp/test-write-probe"]
        ),
        "hardened: /tmp must be writable"
    );

    // no-new-privileges active.
    assert!(
        no_new_privileges_active(container_ref(name)),
        "hardened: no-new-privileges must be active"
    );

    // Capability set: NET_ADMIN (12) and NET_RAW (13) present (Allowlist implicit);
    // SETPCAP (8) absent (not in MINIMUM_CAPABILITIES — WP0 cap-set probe).
    assert!(
        cap_eff_has_bit(container_ref(name), 12),
        "hardened: NET_ADMIN (bit 12) must be present (Allowlist network implicit cap)"
    );
    assert!(
        cap_eff_has_bit(container_ref(name), 13),
        "hardened: NET_RAW (bit 13) must be present (Allowlist network implicit cap)"
    );
    assert!(
        !cap_eff_has_bit(container_ref(name), 8),
        "hardened: SETPCAP (bit 8) must be absent (not in MINIMUM_CAPABILITIES)"
    );
}

/// `standard` — writable root, no cap-drop, `no-new-privileges` (sudo off by default).
#[test]
fn tier1_standard_posture() {
    require_docker();
    let name = "jackin-profile-matrix-standard";
    let _guard = ContainerGuard(name.to_owned());
    docker_rm(name);

    docker_run_bg(
        name,
        &[
            // writable root, no --cap-drop
            "--security-opt",
            "no-new-privileges",
            "--memory",
            "17179869184", // 16 GiB
        ],
    );

    // Root filesystem is writable.
    assert!(
        docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /test-write-probe 2>/dev/null"]
        ),
        "standard: / must be writable"
    );

    // no-new-privileges active (sudo=false by default → no_new_privileges=true).
    assert!(
        no_new_privileges_active(container_ref(name)),
        "standard: no-new-privileges must be active (sudo=false by default)"
    );

    // Capability set (Docker default — no --cap-drop): NET_ADMIN (12) absent;
    // SETPCAP (8) present (Docker's 14-cap default — WP0 cap-set probe).
    assert!(
        !cap_eff_has_bit(container_ref(name), 12),
        "standard: NET_ADMIN (bit 12) must be absent (not in Docker default caps)"
    );
    assert!(
        cap_eff_has_bit(container_ref(name), 8),
        "standard: SETPCAP (bit 8) must be present (Docker default caps)"
    );
}

/// `compat` — writable root, no restrictions, no-new-privileges OFF (sudo=true).
#[test]
fn tier1_compat_posture() {
    require_docker();
    let name = "jackin-profile-matrix-compat";
    let _guard = ContainerGuard(name.to_owned());
    docker_rm(name);

    docker_run_bg(
        name,
        &[
            // no --cap-drop, no --read-only, no --security-opt no-new-privileges
            // no --memory (unlimited)
        ],
    );

    // Root filesystem is writable.
    assert!(
        docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /test-write-probe 2>/dev/null"]
        ),
        "compat: / must be writable"
    );

    // no-new-privileges NOT active (compat: sudo=true → no_new_privileges=false).
    assert!(
        !no_new_privileges_active(container_ref(name)),
        "compat: no-new-privileges must be inactive (sudo=true)"
    );

    // Capability set (Docker default — no --cap-drop): NET_ADMIN (12) absent;
    // SETPCAP (8) present (Docker's 14-cap default — WP0 cap-set probe).
    assert!(
        !cap_eff_has_bit(container_ref(name), 12),
        "compat: NET_ADMIN (bit 12) must be absent (not in Docker default caps)"
    );
    assert!(
        cap_eff_has_bit(container_ref(name), 8),
        "compat: SETPCAP (bit 8) must be present (Docker default caps)"
    );
}

fn container_ref(name: &str) -> &str {
    name
}

// ── Tier 2: real-workload stubs ───────────────────────────────────────────────
//
// These are placeholder stubs for the Tier 2 matrix (real workloads, expensive,
// nightly). Enable with `JACKIN_E2E_TIER2=1`. Each stub should:
//   1. Build the role image once (or reuse a pre-built one).
//   2. Launch jackin with the target profile.
//   3. Assert the named workload succeeds (or fails with the documented error).
//
// Workload × profile cells:
//   hardened: cargo build, git clone, gh pr create
//   locked:   read-only code analysis (no network), apt install fails with documented error
//   standard: apt install, docker compose (rootless), testcontainers
//   compat:   privileged dind, complex docker workflows
//
// The full matrix is not run in per-PR CI; it runs as its own nightly job.
//
// TODO(WP0-tier2): fill in the real workload cells once the Tier 1 matrix is green.
