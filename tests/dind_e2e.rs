//! End-to-end smoke that drives `jackin load` against a real Docker daemon
//! with proxy env declared in role config, then asserts the launched agent
//! container's environment carries the `DinD` hostname in both `NO_PROXY`
//! and `no_proxy`. Regression guard for the proxy-routed `DinD`-handshake
//! bug fixed in `src/runtime/launch.rs`.

#![cfg(feature = "e2e")]

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use jackin::derived_image::shell_quote;
use jackin::instance::naming::is_dns_label;
use tempfile::tempdir;

const ROLE_KEY: &str = "jackin-e2e/agent-smith";
const SENTINEL_ROLE_KEY: &str = "jackin-e2e/sentinel";

/// RAII cleanup so the test's Docker resources are removed even if an
/// assertion or `script(1)` invocation panics. Without this, a flaky run
/// leaks a container/network/volume and the next run fails on name
/// collision — turning a transient failure into a sticky red CI.
struct DockerCleanup;

impl Drop for DockerCleanup {
    fn drop(&mut self) {
        cleanup_role(ROLE_KEY, "jackin-jackin-e2e__agent-smith");
        cleanup_role(SENTINEL_ROLE_KEY, "jackin-jackin-e2e__sentinel");
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
    let extra_env = [("JACKIN_CONSTRUCT_IMAGE", "projectjackin/construct:trixie")];
    let output = run_in_pty(&jackin, &args, &home, &workspace_dir, &extra_env);

    assert!(
        output.status.success(),
        "jackin load smoke failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Agent prints its env + `docker ps` snapshot after a sentinel marker on
    // its stdout, which the PTY captures into `output.stdout`. Reading from
    // stdout instead of a `/workspace` bind-mount file keeps the test agnostic
    // to whether the Docker daemon shares the test process's filesystem (DinD
    // and remote daemons resolve bind-mount sources on the daemon side, where
    // the test cannot read them). The capture is a rendered terminal
    // transcript, so marker order and the closing marker's visibility can vary.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains(REPORT_BEGIN),
        "agent did not emit {REPORT_BEGIN} marker\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // REPORT_END proves the report block completed. Without this check a
    // partial transcript (agent crashed mid-print, PTY truncation) would
    // still satisfy the contains-substring asserts below on whatever
    // happened to land before the cut.
    assert!(
        stdout.contains(REPORT_END),
        "agent did not emit {REPORT_END} marker — report is truncated\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report = stdout.as_ref();

    let dind_hostname = find_report_value(report, "JACKIN_DIND_HOSTNAME=")
        .unwrap_or_else(|| panic!("report must include JACKIN_DIND_HOSTNAME\n{report}"));
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

#[test]
fn jackin_load_sentinel_role_resolves_rich_prompts_and_keeps_build_output_off_screen() {
    require_e2e_prereqs();
    let _cleanup = DockerCleanup;

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

    let jackin = std::env::var("CARGO_BIN_EXE_jackin").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .join("target/debug/jackin")
            .display()
            .to_string()
    });

    let target = format!("{}:/workspace", workspace_dir.display());
    let args = ["load", SENTINEL_ROLE_KEY, &target, "--agent", "codex"];
    let extra_env = [("JACKIN_CONSTRUCT_IMAGE", "projectjackin/construct:trixie")];
    let prompt_input = "\nrequired-value\n\n\n\n\n\n";
    let output = run_in_pty_with_input(
        &jackin,
        &args,
        &home,
        &workspace_dir,
        &extra_env,
        prompt_input,
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "jackin sentinel load failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    assert!(
        stdout.contains("JACKIN_SENTINEL_REPORT_BEGIN"),
        "sentinel report missing begin marker\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("JACKIN_SENTINEL_REPORT_END"),
        "sentinel report missing end marker\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("JACKIN=1"), "{stdout}");
    assert!(stdout.contains("JACKIN_AGENT=codex"), "{stdout}");
    assert!(stdout.contains("STATIC_DEFAULT=static-value"), "{stdout}");
    assert!(
        stdout.contains("LITERAL_TEMPLATE=preserve-${other.VALUE}"),
        "{stdout}"
    );
    assert!(stdout.contains("FREE_TEXT=typed-default"), "{stdout}");
    assert!(
        stdout.contains("FREE_TEXT_REQUIRED=required-value"),
        "{stdout}"
    );
    assert!(stdout.contains("SELECT_PROJECT=frontend"), "{stdout}");
    assert!(stdout.contains("SELECT_MODE=diagnostic"), "{stdout}");
    assert!(stdout.contains("BRANCH=feature/frontend"), "{stdout}");
    assert!(
        stdout.contains("COMBINED_LABEL=frontend-typed-default"),
        "{stdout}"
    );
    assert!(stdout.contains("OPTIONAL_API_KEY=unset"), "{stdout}");
    assert!(stdout.contains("OPTIONAL_DERIVED=unset"), "{stdout}");
    assert!(stdout.contains("JACKIN_SENTINEL_SOURCE_HOOK=1"), "{stdout}");
    assert!(
        stdout.contains("JACKIN_SENTINEL_PREFLIGHT_COUNT=1"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("jackin-sentinel build layer")
            && !stderr.contains("jackin-sentinel build layer"),
        "Docker build output leaked onto the rich screen\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

const REPORT_BEGIN: &str = "===JACKIN_E2E_REPORT_BEGIN===";
const REPORT_END: &str = "===JACKIN_E2E_REPORT_END===";

fn find_report_value<'a>(report: &'a str, key: &str) -> Option<&'a str> {
    report.lines().find_map(|line| {
        let (_, value) = line.split_once(key)?;
        value
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-'))
            .next()
            .filter(|value| !value.is_empty())
    })
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
    let mut command = Command::new("docker");
    command
        .arg("info")
        .env_remove("DOCKER_HOST")
        .env_remove("DOCKER_TLS_VERIFY")
        .env_remove("DOCKER_CERT_PATH")
        .env_remove("TESTCONTAINERS_HOST_OVERRIDE");
    command.output().is_ok_and(|output| output.status.success())
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

fn run_in_pty(
    jackin: &str,
    args: &[&str],
    home: &Path,
    cwd: &Path,
    extra_env: &[(&str, &str)],
) -> std::process::Output {
    run_in_pty_with_input(jackin, args, home, cwd, extra_env, "")
}

fn run_in_pty_with_input(
    jackin: &str,
    args: &[&str],
    home: &Path,
    cwd: &Path,
    extra_env: &[(&str, &str)],
    input: &str,
) -> std::process::Output {
    let mut command = Command::new("script");
    // BSD `script` (macOS) takes the command as positional args after the
    // typescript file. util-linux `script` (most Linux distros) takes it
    // via `-c <shell-string>`. BusyBox `script` is closer to BSD; if
    // encountered on Linux it will fall through to the util-linux branch
    // and fail loudly rather than silently misbehave.
    let invocation = std::iter::once(jackin)
        .chain(args.iter().copied())
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    let full = format!("stty cols 120 rows 40 >/dev/null 2>&1; exec {invocation}");
    if cfg!(target_os = "macos") {
        command
            .arg("-q")
            .arg("/dev/null")
            .arg("sh")
            .arg("-lc")
            .arg(&full);
    } else {
        command.args(["-q", "-e", "-c", &full, "/dev/null"]);
    }
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("TERM", "xterm-256color")
        .env_remove("JACKIN_DEBUG")
        .env_remove("DOCKER_HOST")
        .env_remove("DOCKER_TLS_VERIFY")
        .env_remove("DOCKER_CERT_PATH")
        .env_remove("TESTCONTAINERS_HOST_OVERRIDE");
    for (k, v) in extra_env {
        command.env(k, v);
    }
    command.current_dir(cwd);
    if input.is_empty() {
        return command.output().expect("script must spawn");
    }

    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("script must spawn");
    let mut stdin = child.stdin.take().expect("script stdin must be piped");
    stdin
        .write_all(input.as_bytes())
        .expect("script stdin write must succeed");
    drop(stdin);
    child.wait_with_output().expect("script must finish")
}

fn seed_agent_smith_role_repo(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    std::fs::write(path.join("Dockerfile"), role_dockerfile()).unwrap();
    std::fs::write(
        path.join("jackin.role.toml"),
        r#"version = "v1alpha3"
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

fn seed_sentinel_role_repo(path: &Path) {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/roles/jackin-sentinel");
    copy_dir(&fixture, path);
    run("git", &["init"], Some(path));
    run("git", &["add", "."], Some(path));
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
            "Seed sentinel e2e role",
        ],
        Some(path),
    );
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&src_path, &dst_path);
        } else {
            std::fs::copy(&src_path, &dst_path).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let mode = std::fs::metadata(&src_path).unwrap().permissions().mode();
                let mut perms = std::fs::metadata(&dst_path).unwrap().permissions();
                perms.set_mode(mode);
                std::fs::set_permissions(&dst_path, perms).unwrap();
            }
        }
    }
}

fn write_sentinel_config(path: &Path, role_source: &Path) {
    std::fs::write(
        path,
        format!(
            r#"[roles."{SENTINEL_ROLE_KEY}"]
git = "{}"
trusted = true
"#,
            role_source.display()
        ),
    )
    .unwrap();
}

const fn role_dockerfile() -> &'static str {
    r"FROM projectjackin/construct:0.1-trixie
USER root
RUN apt-get update && \
    apt-get install -y --no-install-recommends default-jdk-headless maven && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/* \
           /var/cache/apt/* \
           /tmp/*
USER agent
"
}

fn seed_claude_installer_stub(home: &Path) {
    let stub = home
        .join(".jackin")
        .join("cache")
        .join("agent-binaries-test-stub")
        .join("claude");
    std::fs::create_dir_all(stub.parent().unwrap()).unwrap();
    std::fs::write(&stub, fake_claude_installer()).unwrap();
    chmod_executable(&stub);
}

fn seed_all_agent_stubs(home: &Path) {
    for slug in ["claude", "amp", "kimi", "opencode"] {
        seed_agent_stub(home, slug, &format!("{slug} 0.0.0-e2e\n"));
    }
    seed_agent_stub(
        home,
        "codex",
        concat!(
            r#"if [ "$"#,
            r#"{1:-}" = "--version" ]; then
  echo "codex 0.0.0-e2e"
  exit 0
fi
jackin-sentinel-report
"#,
        ),
    );
}

fn seed_agent_stub(home: &Path, slug: &str, body: &str) {
    let stub = home
        .join(".jackin")
        .join("cache")
        .join("agent-binaries-test-stub")
        .join(slug);
    std::fs::create_dir_all(stub.parent().unwrap()).unwrap();
    std::fs::write(&stub, format!("#!/bin/sh\nset -eu\n{body}")).unwrap();
    chmod_executable(&stub);
}

#[cfg(unix)]
fn chmod_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

#[cfg(not(unix))]
fn chmod_executable(_path: &Path) {}

/// The agent emits its env + `docker ps` snapshot after a sentinel marker on
/// stdout. The test parses that block from the PTY-captured stdout, so the
/// report channel works identically whether the daemon shares the test
/// process's filesystem or runs in `DinD`. `REPORT_BEGIN`/`REPORT_END` are
/// interpolated via `format!` so the Rust consts remain the single source of
/// truth; `${{...}}` in the body escapes the format string back to `${...}` for
/// the embedded shell.
fn fake_claude_installer() -> String {
    format!(
        r#"#!/bin/sh
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
# Emit REPORT_END before the Maven smoke so the host's `output.stdout`
# parse can succeed even when mvn's network reach to Maven Central
# (testcontainers pull, JDK plugin downloads) is slow or fails. The
# TESTCONTAINERS_SMOKE=ok assertion later in the test catches a real
# smoke regression independently.
echo "{REPORT_END}"
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
CLAUDE
chmod +x "$HOME/.local/bin/claude"
"#
    )
}

fn cleanup_role(role_key: &str, image: &str) {
    let output = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("label=jackin.class={role_key}"),
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
    let _ = Command::new("docker").args(["rmi", image]).output();
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
