//! End-to-end smoke that drives `jackin load` against a real Docker daemon
//! with proxy env declared in role config, then asserts the launched agent
//! container's environment carries the `DinD` hostname in both `NO_PROXY`
//! and `no_proxy`. Regression guard for the proxy-routed `DinD`-handshake
//! bug fixed in `src/runtime/launch.rs`.

#![cfg(feature = "e2e")]

use std::path::{Path, PathBuf};
use std::process::Command;

use jackin::derived_image::shell_quote;
use tempfile::tempdir;

const ROLE_KEY: &str = "jackin-e2e/agent-smith";
const CONTAINER_NAME: &str = "jackin-jackin-e2e__agent-smith";
const DIND_HOSTNAME: &str = "jackin-jackin-e2e__agent-smith-dind";

/// RAII cleanup so the test's Docker resources are removed even if an
/// assertion or `script(1)` invocation panics. Without this, a flaky run
/// leaks a container/network/volume and the next run fails on name
/// collision — turning a transient failure into a sticky red CI.
struct DockerCleanup;

impl Drop for DockerCleanup {
    fn drop(&mut self) {
        cleanup_role();
    }
}

struct WorkspaceMount {
    local_dir: PathBuf,
    mount_src: String,
    daemon_backed: bool,
}

impl WorkspaceMount {
    fn new(local_dir: PathBuf) -> Self {
        let daemon_backed = docker_uses_remote_daemon_paths();
        let mount_src = if daemon_backed {
            // With DinD, bind-mount sources are resolved on the daemon
            // container's filesystem, not this test process's filesystem.
            let src = format!(
                "/tmp/jackin-e2e-workspace-{}-{}",
                std::process::id(),
                local_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("workspace")
            );
            std::fs::create_dir_all(&src).unwrap();
            prepare_daemon_workspace(&src);
            src
        } else {
            local_dir.display().to_string()
        };

        Self {
            local_dir,
            mount_src,
            daemon_backed,
        }
    }

    fn target(&self) -> String {
        format!("{}:/workspace", self.mount_src)
    }

    fn read_file(&self, name: &str) -> String {
        if !self.daemon_backed {
            return std::fs::read_to_string(self.local_dir.join(name)).unwrap();
        }

        let mount = format!("{}:/workspace:ro", self.mount_src);
        let path = format!("/workspace/{name}");
        let output = Command::new("docker")
            .args(["run", "--rm", "-v", &mount, "alpine:3.20", "cat", &path])
            .output()
            .expect("docker must read daemon-backed workspace file");
        assert!(
            output.status.success(),
            "failed to read daemon-backed workspace file {name}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for WorkspaceMount {
    fn drop(&mut self) {
        if !self.daemon_backed {
            return;
        }

        let mount = format!("{}:/workspace", self.mount_src);
        let _ = Command::new("docker")
            .args([
                "run",
                "--rm",
                "-v",
                &mount,
                "alpine:3.20",
                "sh",
                "-lc",
                "rm -rf /workspace/* /workspace/.[!.]* /workspace/..?* 2>/dev/null || true",
            ])
            .status();
        let _ = std::fs::remove_dir_all(&self.mount_src);
    }
}

#[test]
fn jackin_load_agent_smith_can_reach_its_dind_daemon_with_proxy_env() {
    require_e2e_prereqs();
    let _cleanup = DockerCleanup;

    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let config_dir = home.join(".config/jackin");
    let role_source = temp.path().join("agent-smith-source");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&workspace_dir).unwrap();
    let workspace_mount = WorkspaceMount::new(workspace_dir);

    seed_agent_smith_role_repo(&role_source);
    write_config(&config_dir.join("config.toml"), &role_source);

    let jackin = std::env::var("CARGO_BIN_EXE_jackin").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .join("target/debug/jackin")
            .display()
            .to_string()
    });

    let target = workspace_mount.target();
    let args = ["load", ROLE_KEY, &target, "--agent", "claude", "--no-intro"];
    let output = run_in_pty(&jackin, &args, &home, &workspace_mount.local_dir);

    assert!(
        output.status.success(),
        "jackin load smoke failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let env_report = workspace_mount.read_file("jackin-e2e-env.txt");
    assert!(env_report.contains(&format!("DOCKER_HOST=tcp://{DIND_HOSTNAME}:2376")));
    assert!(env_report.contains("DOCKER_TLS_VERIFY=1"));
    assert!(env_report.contains("DOCKER_CERT_PATH=/certs/client"));
    assert!(env_report.contains(&format!("JACKIN_DIND_HOSTNAME={DIND_HOSTNAME}")));
    // Both casings carry the merged list — operator's localhost,127.0.0.1
    // must reach tools that read either uppercase NO_PROXY (Go runtime) or
    // lowercase no_proxy (curl, Python requests, wget).
    let merged = format!("NO_PROXY=localhost,127.0.0.1,{DIND_HOSTNAME}");
    let merged_lower = format!("no_proxy=localhost,127.0.0.1,{DIND_HOSTNAME}");
    assert!(
        env_report.contains(&merged),
        "missing {merged}\n{env_report}"
    );
    assert!(
        env_report.contains(&merged_lower),
        "missing {merged_lower}\n{env_report}"
    );

    let docker_ps = workspace_mount.read_file("jackin-e2e-docker-ps.txt");
    assert!(docker_ps.contains("CONTAINER ID"));
}

/// Hard-fail with an actionable message when the e2e prerequisites are
/// missing. The `e2e` feature is opt-in (CI runs `cargo nextest run
/// --all-features` on a Docker-equipped runner); silently skipping would
/// turn a missing prereq into a green check.
fn require_e2e_prereqs() {
    assert!(
        docker_available(),
        "e2e tests require a running Docker daemon (`docker info` failed). \
         Disable the `e2e` feature or start Docker."
    );
    assert!(
        script_available(),
        "e2e tests require `script(1)` on PATH for PTY emulation. \
         Install bsdmainutils (Debian/Ubuntu) or util-linux (most distros), \
         or disable the `e2e` feature."
    );
}

fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Probe `script(1)` via the canonical PATH lookup. The previous
/// `script --help` / `script -q /dev/null` fallback chain was unsound:
/// the fallback only fired on spawn failure, and on the only platforms
/// that lack `--help` it would invoke `script` with side effects (start a
/// real PTY recording session against `/dev/null`).
fn script_available() -> bool {
    Command::new("which")
        .arg("script")
        .output()
        .is_ok_and(|out| out.status.success())
}

fn docker_uses_remote_daemon_paths() -> bool {
    std::env::var("DOCKER_HOST").is_ok_and(|host| {
        let host = host.trim();
        host.starts_with("tcp://") || host.starts_with("ssh://")
    })
}

fn prepare_daemon_workspace(src: &str) {
    let uid = capture_trimmed("id", &["-u"]).unwrap_or_else(|| "1000".to_string());
    let gid = capture_trimmed("id", &["-g"]).unwrap_or_else(|| "1000".to_string());
    let mount = format!("{src}:/workspace");
    let ownership = format!("chown -R {uid}:{gid} /workspace && chmod 700 /workspace");
    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &mount,
            "alpine:3.20",
            "sh",
            "-lc",
            &ownership,
        ])
        .output()
        .expect("docker must prepare daemon-backed workspace");
    assert!(
        output.status.success(),
        "failed to prepare daemon-backed workspace {src}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn capture_trimmed(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn run_in_pty(jackin: &str, args: &[&str], home: &Path, cwd: &Path) -> std::process::Output {
    let mut command = Command::new("script");
    // BSD `script` (macOS) takes the command as positional args after the
    // typescript file. util-linux `script` (most Linux distros) takes it
    // via `-c <shell-string>`. BusyBox `script` is closer to BSD; if
    // encountered on Linux it will fall through to the util-linux branch
    // and fail loudly rather than silently misbehave.
    if cfg!(target_os = "macos") {
        command.arg("-q").arg("/dev/null").arg(jackin).args(args);
    } else {
        let full = std::iter::once(jackin)
            .chain(args.iter().copied())
            .map(shell_quote)
            .collect::<Vec<_>>()
            .join(" ");
        command.args(["-q", "-e", "-c", &full, "/dev/null"]);
    }
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env_remove("JACKIN_DEBUG")
        .current_dir(cwd)
        .output()
        .expect("script must spawn")
}

fn seed_agent_smith_role_repo(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    std::fs::write(path.join("Dockerfile"), role_dockerfile()).unwrap();
    std::fs::write(path.join("fake-curl"), fake_curl()).unwrap();
    std::fs::write(
        path.join("jackin.role.toml"),
        r#"version = "v1alpha2"
dockerfile = "Dockerfile"
agents = ["claude"]

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
    )
    .unwrap();

    run("git", &["init"], Some(path));
    run("git", &["add", "."], Some(path));
    // `commit.gpgsign=false` defends against developers with global
    // gpgsign enabled but no signing key configured for this repo —
    // otherwise the seed commit fails and the test bails before exercising
    // anything jackin-related.
    run(
        "git",
        &[
            "-c",
            "user.name=Jackin E2E",
            "-c",
            "user.email=e2e@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Seed agent smith e2e role",
        ],
        Some(path),
    );
}

fn write_config(path: &Path, role_source: &Path) {
    std::fs::write(
        path,
        format!(
            r#"[roles."{ROLE_KEY}"]
git = "{}"
trusted = true

[roles."{ROLE_KEY}".env]
HTTPS_PROXY = "http://127.0.0.1:9"
https_proxy = "http://127.0.0.1:9"
NO_PROXY = "localhost,127.0.0.1"
"#,
            role_source.display()
        ),
    )
    .unwrap();
}

const fn role_dockerfile() -> &'static str {
    r"FROM projectjackin/construct:trixie
USER root
COPY fake-curl /usr/local/bin/curl
RUN chmod +x /usr/local/bin/curl
USER agent
"
}

// `$VAR` expansions inside the heredoc trigger
// `clippy::literal_string_with_formatting_args` because the body looks like
// a Rust format string; the script is `cat`'d verbatim into the container,
// so the lint is a false positive.
#[allow(clippy::literal_string_with_formatting_args)]
const fn fake_curl() -> &'static str {
    // `sleep 5` keeps the agent container alive long enough for jackin's
    // post-launch reads (env report, docker ps snapshot) to land in the
    // workspace before the entrypoint exits and the container is torn
    // down. Tuned empirically; bump if the test races on slow CI.
    r#"#!/bin/sh
cat <<'INSTALL'
#!/bin/sh
set -eu
mkdir -p "$HOME/.local/bin"
cat > "$HOME/.local/bin/claude" <<'CLAUDE'
#!/bin/sh
set -eu
if [ "${1:-}" = "--version" ]; then
  echo "claude 0.0.0-e2e"
  exit 0
fi
docker ps > /workspace/jackin-e2e-docker-ps.txt
{
  echo "DOCKER_HOST=$DOCKER_HOST"
  echo "DOCKER_TLS_VERIFY=$DOCKER_TLS_VERIFY"
  echo "DOCKER_CERT_PATH=$DOCKER_CERT_PATH"
  echo "JACKIN_DIND_HOSTNAME=$JACKIN_DIND_HOSTNAME"
  echo "NO_PROXY=${NO_PROXY:-}"
  echo "no_proxy=${no_proxy:-}"
} > /workspace/jackin-e2e-env.txt
sleep 5
CLAUDE
chmod +x "$HOME/.local/bin/claude"
INSTALL
"#
}

fn cleanup_role() {
    let _ = Command::new("docker")
        .args(["rm", "-f", CONTAINER_NAME])
        .output();
    let _ = Command::new("docker")
        .args(["rm", "-f", &format!("{CONTAINER_NAME}-dind")])
        .output();
    let _ = Command::new("docker")
        .args(["network", "rm", &format!("{CONTAINER_NAME}-net")])
        .output();
    let _ = Command::new("docker")
        .args(["volume", "rm", &format!("{CONTAINER_NAME}-dind-certs")])
        .output();
    let _ = Command::new("docker")
        .args(["rmi", CONTAINER_NAME])
        .output();
}

fn run(program: &str, args: &[&str], cwd: Option<&Path>) {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("{program} {} failed to spawn: {e}", args.join(" ")));
    assert!(
        output.status.success(),
        "{program} {} failed\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
