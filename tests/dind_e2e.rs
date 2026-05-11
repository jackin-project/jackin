//! End-to-end smoke that drives `jackin load` against a real Docker daemon
//! with proxy env declared in role config, then asserts the launched agent
//! container's environment carries the `DinD` hostname in both `NO_PROXY`
//! and `no_proxy`. Regression guard for the proxy-routed `DinD`-handshake
//! bug fixed in `src/runtime/launch.rs`.

#![cfg(feature = "e2e")]

use std::path::Path;
use std::process::Command;

use jackin::derived_image::shell_quote;
use tempfile::tempdir;

const ROLE_KEY: &str = "jackin-e2e/agent-smith";

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
    let args = ["load", ROLE_KEY, &target, "--agent", "claude", "--no-intro"];
    let output = run_in_pty(&jackin, &args, &home, &workspace_dir);

    assert!(
        output.status.success(),
        "jackin load smoke failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Agent prints its env + `docker ps` snapshot between sentinel markers on
    // its stdout, which the PTY captures into `output.stdout`. Reading from
    // stdout instead of a `/workspace` bind-mount file keeps the test agnostic
    // to whether the Docker daemon shares the test process's filesystem (DinD
    // and remote daemons resolve bind-mount sources on the daemon side, where
    // the test cannot read them).
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let Some((_, after_begin)) = stdout.split_once(REPORT_BEGIN) else {
        panic!("agent did not emit {REPORT_BEGIN} marker\nstdout:\n{stdout}\nstderr:\n{stderr}");
    };
    let Some((report, _)) = after_begin.split_once(REPORT_END) else {
        panic!("agent did not emit {REPORT_END} marker\nstdout:\n{stdout}\nstderr:\n{stderr}");
    };

    let dind_hostname = report
        .lines()
        .find_map(|line| line.strip_prefix("JACKIN_DIND_HOSTNAME="))
        .expect("report must include JACKIN_DIND_HOSTNAME");
    assert!(is_dns_label(dind_hostname), "{dind_hostname}");
    assert!(!dind_hostname.contains("__"));
    assert!(!dind_hostname.contains("clone-"));

    assert!(report.contains(&format!("DOCKER_HOST=tcp://{dind_hostname}:2376")));
    assert!(report.contains("DOCKER_TLS_VERIFY=1"));
    assert!(report.contains("DOCKER_CERT_PATH=/certs/client"));
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
        report.contains("CONTAINER ID"),
        "agent's `docker ps` did not list any containers\n{report}"
    );
    assert!(
        report.contains("TESTCONTAINERS_SMOKE=ok"),
        "agent's Java Testcontainers smoke did not pass\n{report}"
    );
}

const REPORT_BEGIN: &str = "===JACKIN_E2E_REPORT_BEGIN===";
const REPORT_END: &str = "===JACKIN_E2E_REPORT_END===";

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

fn is_dns_label(input: &str) -> bool {
    !input.is_empty()
        && input.len() <= 63
        && input
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && input
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && input
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
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
RUN apt-get update && \
    apt-get install -y --no-install-recommends default-jdk maven && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/* \
           /var/cache/apt/* \
           /tmp/*
COPY fake-curl /usr/local/bin/curl
RUN chmod +x /usr/local/bin/curl
USER agent
"
}

/// The agent emits its env + `docker ps` snapshot between sentinel markers on
/// stdout. The test parses that block from the PTY-captured stdout, so the
/// report channel works identically whether the daemon shares the test
/// process's filesystem or runs in `DinD`. `REPORT_BEGIN`/`REPORT_END` are
/// interpolated via `format!` so the Rust consts remain the single source of
/// truth — `${{…}}` in the body escapes the format string back to `${…}` for
/// the embedded shell.
fn fake_curl() -> String {
    format!(
        r#"#!/bin/sh
cat <<'INSTALL'
#!/bin/sh
set -eu
mkdir -p "$HOME/.local/bin"
cat > "$HOME/.local/bin/claude" <<'CLAUDE'
#!/bin/sh
set -eu
if [ "${{1:-}}" = "--version" ]; then
  echo "claude 0.0.0-e2e"
  exit 0
fi
echo "{REPORT_BEGIN}"
echo "DOCKER_HOST=$DOCKER_HOST"
echo "DOCKER_TLS_VERIFY=$DOCKER_TLS_VERIFY"
echo "DOCKER_CERT_PATH=$DOCKER_CERT_PATH"
echo "JACKIN_DIND_HOSTNAME=$JACKIN_DIND_HOSTNAME"
echo "TESTCONTAINERS_HOST_OVERRIDE=$TESTCONTAINERS_HOST_OVERRIDE"
echo "NO_PROXY=${{NO_PROXY:-}}"
echo "no_proxy=${{no_proxy:-}}"
docker ps
tmpdir="$(mktemp -d)"
cat > "$tmpdir/pom.xml" <<'POM'
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <groupId>dev.jackin</groupId>
  <artifactId>dind-testcontainers-smoke</artifactId>
  <version>1.0.0</version>
  <properties>
    <maven.compiler.source>17</maven.compiler.source>
    <maven.compiler.target>17</maven.compiler.target>
    <exec-maven-plugin.version>3.5.0</exec-maven-plugin.version>
  </properties>
  <dependencies>
    <dependency>
      <groupId>org.testcontainers</groupId>
      <artifactId>testcontainers</artifactId>
      <version>2.0.5</version>
    </dependency>
  </dependencies>
  <build>
    <plugins>
      <plugin>
        <groupId>org.codehaus.mojo</groupId>
        <artifactId>exec-maven-plugin</artifactId>
        <version>${{exec-maven-plugin.version}}</version>
      </plugin>
    </plugins>
  </build>
</project>
POM
mkdir -p "$tmpdir/src/main/java"
cat > "$tmpdir/src/main/java/JackinTestcontainersSmoke.java" <<'JAVA'
import org.testcontainers.containers.GenericContainer;
import org.testcontainers.utility.DockerImageName;

public final class JackinTestcontainersSmoke {{
    public static void main(String[] args) {{
        GenericContainer<?> container = new GenericContainer<>(DockerImageName.parse("alpine:3.20"))
                .withCommand("sh", "-c", "echo jackin-testcontainers-child-ok && sleep 1");
        container.start();
        String logs = container.getLogs();
        if (!logs.contains("jackin-testcontainers-child-ok")) {{
            throw new IllegalStateException("child container logs missing marker: " + logs);
        }}
        System.out.println("TESTCONTAINERS_SMOKE=ok");
        System.exit(0);
    }}
}}
JAVA
(
  unset HTTP_PROXY HTTPS_PROXY http_proxy https_proxy ALL_PROXY all_proxy
  cd "$tmpdir"
  mvn -q -DskipTests compile exec:java -Dexec.mainClass=JackinTestcontainersSmoke
)
rm -rf "$tmpdir"
echo "{REPORT_END}"
CLAUDE
chmod +x "$HOME/.local/bin/claude"
INSTALL
"#
    )
}

fn cleanup_role() {
    let output = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("label=jackin.class={ROLE_KEY}"),
            "--format",
            "{{.Names}}",
        ])
        .output();
    if let Ok(output) = output {
        for name in String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
        {
            let _ = Command::new("docker").args(["rm", "-f", name]).output();
            let _ = Command::new("docker")
                .args(["rm", "-f", &format!("{name}-dind")])
                .output();
            let _ = Command::new("docker")
                .args(["network", "rm", &format!("{name}-net")])
                .output();
            let _ = Command::new("docker")
                .args(["volume", "rm", &format!("{name}-dind-certs")])
                .output();
        }
    }
    let _ = Command::new("docker")
        .args(["rmi", "jackin-jackin-e2e__agent-smith"])
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
