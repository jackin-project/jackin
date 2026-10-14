#![cfg(feature = "e2e")]

const PROBE_IMAGE: &str = "busybox:1.36";

fn docker_available() -> bool {
    let request = jackin_process::ExecRequest::new("docker", ["info"])
        .stdout_mode(jackin_process::StdioMode::Null)
        .stderr_mode(jackin_process::StdioMode::Null);
    jackin_process::exec_sync(&request).is_ok_and(|result| result.success)
}

fn require_docker() -> bool {
    docker_available()
}

fn docker_run_bg(name: &str, extra_args: &[&str]) -> String {
    let mut args = vec!["run", "-d", "--name", name];
    args.extend_from_slice(extra_args);
    args.extend_from_slice(&[PROBE_IMAGE, "sh", "-c", "sleep 120"]);
    let output = jackin_process::exec_sync(&jackin_process::ExecRequest::new("docker", &args));
    assert!(output.is_ok(), "docker run must spawn: {output:?}");
    let Ok(output) = output else {
        return String::new();
    };
    assert!(
        output.success,
        "docker run failed for {name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn docker_exec_ok(container: &str, cmd: &[&str]) -> bool {
    let mut args = vec!["exec", container];
    args.extend_from_slice(cmd);
    let request = jackin_process::ExecRequest::new("docker", &args)
        .stdout_mode(jackin_process::StdioMode::Null)
        .stderr_mode(jackin_process::StdioMode::Null);
    jackin_process::exec_sync(&request).is_ok_and(|result| result.success)
}

fn docker_exec_output(container: &str, cmd: &[&str]) -> String {
    let mut args = vec!["exec", container];
    args.extend_from_slice(cmd);
    let output = jackin_process::exec_sync(&jackin_process::ExecRequest::new("docker", &args));
    assert!(output.is_ok(), "docker exec must spawn: {output:?}");
    let Ok(output) = output else {
        return String::new();
    };
    String::from_utf8_lossy(&output.stdout).to_lowercase()
}

fn docker_rm(name: &str) {
    let request = jackin_process::ExecRequest::new("docker", ["rm", "-f", name])
        .stdout_mode(jackin_process::StdioMode::Null)
        .stderr_mode(jackin_process::StdioMode::Null);
    drop(jackin_process::exec_sync(&request));
}

struct ContainerGuard(String);

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        docker_rm(&self.0);
    }
}

fn no_new_privileges_active(container: &str) -> bool {
    let out = docker_exec_output(container, &["sh", "-c", "cat /proc/1/status"]);
    out.contains("nonewprivs:\t1") || out.contains("no_new_privs:\t1")
}

fn cap_eff_has_bit(container: &str, bit: u32) -> bool {
    let out = docker_exec_output(container, &["sh", "-c", "cat /proc/self/status"]);
    for line in out.lines() {
        if let Some(hex) = line.strip_prefix("capeff:\t")
            && let Ok(val) = u64::from_str_radix(hex.trim(), 16)
        {
            return (val >> bit) & 1 == 1;
        }
    }
    false
}

#[test]
fn tier1_locked_posture() {
    if !require_docker() {
        return;
    }
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
            "--cap-add",
            "NET_ADMIN",
            "--cap-add",
            "NET_RAW",
            "--security-opt",
            "no-new-privileges",
            "--memory",
            "4294967296",
            "--network",
            "none",
        ],
    );

    assert!(
        !docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /test-write-probe 2>/dev/null"]
        ),
        "locked: / must be read-only"
    );

    assert!(
        docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /tmp/test-write-probe"]
        ),
        "locked: /tmp must be writable"
    );

    assert!(
        no_new_privileges_active(container_ref(name)),
        "locked: no-new-privileges must be active"
    );

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

#[test]
fn tier1_hardened_posture() {
    if !require_docker() {
        return;
    }
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
            "--cap-add",
            "NET_ADMIN",
            "--cap-add",
            "NET_RAW",
            "--security-opt",
            "no-new-privileges",
            "--memory",
            "17179869184",
        ],
    );

    assert!(
        !docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /test-write-probe 2>/dev/null"]
        ),
        "hardened: / must be read-only"
    );

    assert!(
        docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /tmp/test-write-probe"]
        ),
        "hardened: /tmp must be writable"
    );

    assert!(
        no_new_privileges_active(container_ref(name)),
        "hardened: no-new-privileges must be active"
    );

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

#[test]
fn tier1_standard_posture() {
    if !require_docker() {
        return;
    }
    let name = "jackin-profile-matrix-standard";
    let _guard = ContainerGuard(name.to_owned());
    docker_rm(name);

    docker_run_bg(
        name,
        &[
            "--security-opt",
            "no-new-privileges",
            "--memory",
            "17179869184",
        ],
    );

    assert!(
        docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /test-write-probe 2>/dev/null"]
        ),
        "standard: / must be writable"
    );

    assert!(
        no_new_privileges_active(container_ref(name)),
        "standard: no-new-privileges must be active (sudo=false by default)"
    );

    assert!(
        !cap_eff_has_bit(container_ref(name), 12),
        "standard: NET_ADMIN (bit 12) must be absent (not in Docker default caps)"
    );
    assert!(
        cap_eff_has_bit(container_ref(name), 8),
        "standard: SETPCAP (bit 8) must be present (Docker default caps)"
    );
}

#[test]
fn tier1_compat_posture() {
    if !require_docker() {
        return;
    }
    let name = "jackin-profile-matrix-compat";
    let _guard = ContainerGuard(name.to_owned());
    docker_rm(name);

    docker_run_bg(name, &[]);

    assert!(
        docker_exec_ok(
            container_ref(name),
            &["sh", "-c", "touch /test-write-probe 2>/dev/null"]
        ),
        "compat: / must be writable"
    );

    assert!(
        !no_new_privileges_active(container_ref(name)),
        "compat: no-new-privileges must be inactive (sudo=true)"
    );

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
