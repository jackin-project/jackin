//! Shared e2e prereq checks: `docker` daemon + `buildx` + `script(1)` probe,
//! capsule-binary ELF + executable assertions, and the e2e serial lock that
//! keeps `cargo nextest run -p jackin --features e2e` from spawning multiple
//! concurrent `DinD` harnesses against the same `docker` daemon.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use fs4::FileExt;

pub(super) fn require_e2e_prereqs() {
    require_capsule_binary_override();
    assert!(
        docker_available(),
        "e2e tests require a running Docker daemon (`docker info` failed). \
         Disable the `e2e` feature or start Docker."
    );
    assert!(
        docker_buildx_available(),
        "e2e tests require Docker Buildx (`docker buildx version` failed). \
         Install the buildx CLI plugin or set DOCKER_CONFIG to a Docker config \
         directory that contains cli-plugins/docker-buildx."
    );
    assert!(
        script_available(),
        "e2e tests require `script(1)` on PATH for PTY emulation. \
         Install bsdmainutils (Debian/Ubuntu) or util-linux (most distros), \
         or disable the `e2e` feature."
    );
}

pub(super) fn require_capsule_binary_override() {
    let Some(path) = std::env::var_os("JACKIN_CAPSULE_BIN") else {
        panic!(
            "e2e tests require JACKIN_CAPSULE_BIN to point at a locally built \
             Linux jackin-capsule binary. In PR checkouts, run \
             `jackin-dev pr sync <PR_NUMBER>` and source \
             `$(jackin-dev pr path <PR_NUMBER>)/env.sh` first. Outside that \
             flow, run \
             `eval \"$(cargo run --bin build-jackin-capsule -- --export)\"`. \
             The e2e harness must not fall back to the preview-release \
             download verifier."
        );
    };
    let path = PathBuf::from(path);
    assert!(
        path.is_file(),
        "JACKIN_CAPSULE_BIN must point at a file, got {}",
        path.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path)
            .unwrap_or_else(|error| panic!("failed to stat {}: {error}", path.display()))
            .permissions()
            .mode();
        assert!(
            mode & 0o111 != 0,
            "JACKIN_CAPSULE_BIN must be executable, got {}",
            path.display()
        );
    }
    assert!(
        is_elf_binary(&path),
        "JACKIN_CAPSULE_BIN must point at a Linux jackin-capsule binary, got {}. \
         Build/export a Linux capsule with \
         `eval \"$(cargo run --bin build-jackin-capsule -- --export)\"` or \
         `jackin-dev pr sync <PR_NUMBER>` plus the generated env.sh.",
        path.display()
    );
}

pub(super) fn is_elf_binary(path: &Path) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0_u8; 4];
    file.read_exact(&mut magic).is_ok() && magic == [0x7f, b'E', b'L', b'F']
}

pub(super) fn docker_available() -> bool {
    // Probe the same daemon jackin will drive: honor DOCKER_HOST (and the
    // active docker context when unset), exactly as the host-side client
    // does. Stripping DOCKER_HOST here would gate on a default-socket daemon
    // that jackin itself would bypass whenever the operator set one.
    let mut command = docker_command();
    command
        .arg("info")
        .output()
        .is_ok_and(|output| output.status.success())
}

pub(super) fn docker_buildx_available() -> bool {
    let mut command = docker_command();
    command
        .args(["buildx", "version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

pub(super) fn docker_command() -> Command {
    let mut command = Command::new("docker");
    apply_host_docker_config(&mut command);
    command
}

pub(super) fn apply_host_docker_config(command: &mut Command) {
    if let Some(config) = host_docker_config() {
        command.env("DOCKER_CONFIG", config);
    }
}

pub(super) fn host_docker_config() -> Option<PathBuf> {
    std::env::var_os("DOCKER_CONFIG")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".docker"))
        })
}

/// Probe `script(1)` via the canonical PATH lookup. The previous
/// `script --help` / `script -q /dev/null` fallback chain was unsound:
/// the fallback only fired on spawn failure, and on the only platforms
/// that lack `--help` it would invoke `script` with side effects (start a
/// real PTY recording session against `/dev/null`).
pub(super) fn script_available() -> bool {
    Command::new("which")
        .arg("script")
        .output()
        .is_ok_and(|out| out.status.success())
}

pub(super) fn e2e_serial_lock() -> std::fs::File {
    let path = std::env::temp_dir().join("jackin-dind-e2e.lock");
    let lock = std::fs::File::create(path).expect("e2e lock file must be creatable");
    FileExt::lock(&lock).expect("e2e lock file must be lockable");
    lock
}

pub(super) fn e2e_construct_image() -> String {
    std::env::var("JACKIN_E2E_CONSTRUCT_IMAGE")
        .unwrap_or_else(|_| "projectjackin/construct:trixie".to_owned())
}
