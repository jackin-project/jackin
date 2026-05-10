#![cfg(feature = "e2e")]

use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn jackin_load_agent_smith_can_reach_its_dind_daemon_with_proxy_env() {
    if !docker_available() || !script_available() {
        return;
    }

    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let config_dir = home.join(".config/jackin");
    let role_source = temp.path().join("agent-smith-source");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&workspace_dir).unwrap();

    seed_agent_smith_role_repo(&role_source);
    write_config(&config_dir.join("config.toml"), &role_source);

    let jackin = std::env::var("CARGO_BIN_EXE_jackin").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .join("target/debug/jackin")
            .display()
            .to_string()
    });

    let target = format!("{}:/workspace", workspace_dir.display());
    let args = [
        "load",
        "jackin-e2e/agent-smith",
        &target,
        "--agent",
        "claude",
        "--no-intro",
    ];
    let output = run_in_pty(&jackin, &args, &home, &workspace_dir);
    cleanup_role();

    assert!(
        output.status.success(),
        "jackin load smoke failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let env_report = std::fs::read_to_string(workspace_dir.join("jackin-e2e-env.txt")).unwrap();
    assert!(env_report.contains("DOCKER_HOST=tcp://jackin-jackin-e2e__agent-smith-dind:2376"));
    assert!(env_report.contains("DOCKER_TLS_VERIFY=1"));
    assert!(env_report.contains("DOCKER_CERT_PATH=/certs/client"));
    assert!(env_report.contains("JACKIN_DIND_HOSTNAME=jackin-jackin-e2e__agent-smith-dind"));
    assert!(
        env_report.contains("NO_PROXY=localhost,127.0.0.1,jackin-jackin-e2e__agent-smith-dind")
    );
    assert!(env_report.contains("no_proxy=jackin-jackin-e2e__agent-smith-dind"));

    let docker_ps =
        std::fs::read_to_string(workspace_dir.join("jackin-e2e-docker-ps.txt")).unwrap();
    assert!(docker_ps.contains("CONTAINER ID"));
}

fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn script_available() -> bool {
    Command::new("script")
        .arg("--help")
        .output()
        .or_else(|_| Command::new("script").arg("-q").arg("/dev/null").output())
        .is_ok()
}

fn run_in_pty(jackin: &str, args: &[&str], home: &Path, cwd: &Path) -> std::process::Output {
    let mut command = Command::new("script");
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
        r#"dockerfile = "Dockerfile"
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
    run(
        "git",
        &[
            "-c",
            "user.name=Jackin E2E",
            "-c",
            "user.email=e2e@example.invalid",
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
            r#"[roles."jackin-e2e/agent-smith"]
git = "{}"
trusted = true

[roles."jackin-e2e/agent-smith".env]
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

#[allow(clippy::literal_string_with_formatting_args)]
const fn fake_curl() -> &'static str {
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
    let container = "jackin-jackin-e2e__agent-smith";
    let _ = Command::new("docker")
        .args(["rm", "-f", container])
        .output();
    let _ = Command::new("docker")
        .args(["rm", "-f", &format!("{container}-dind")])
        .output();
    let _ = Command::new("docker")
        .args(["network", "rm", &format!("{container}-net")])
        .output();
    let _ = Command::new("docker")
        .args(["volume", "rm", &format!("{container}-dind-certs")])
        .output();
    let _ = Command::new("docker").args(["rmi", container]).output();
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

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push_str("'\"'\"'");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}
