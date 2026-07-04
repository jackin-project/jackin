// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Fixture seeders + `Dockerfile` + fake-claude scripts that build the
//! three role repos the e2e harness runs against (`agent-smith`,
//! `sentinel`, `slow-exit`), plus the agent-binary stub installers used in
//! place of the real CLIs so the `DinD` lane runs hermetically.

use std::path::Path;
use std::time::Instant;

use super::util::run;

pub(super) fn seed_existing_construct_entry(home: &Path) {
    let pending = home.join(".jackin/data/universe-pending");
    std::fs::create_dir_all(&pending).unwrap();
    std::fs::write(pending.join("e2e-existing-entry"), b"already entering").unwrap();
}

pub(super) fn seed_agent_smith_role_repo(path: &Path) {
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
plugins = ["caveman@jackin-e2e", "rtk@jackin-e2e"]

[[claude.marketplaces]]
source = "jackin/e2e-marketplace"
sparse = ["plugins"]
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

pub(super) fn write_config(path: &Path, role_source: &Path) {
    let role_key = super::ROLE_KEY;
    std::fs::write(
        path,
        format!(
            r#"version = "v1alpha5"

[roles."{role_key}"]
git = "{}"
trusted = true

[roles."{role_key}".env]
HTTPS_PROXY = "http://127.0.0.1:9"
https_proxy = "http://127.0.0.1:9"
NO_PROXY = "localhost,127.0.0.1"
"#,
            role_source.display()
        ),
    )
    .unwrap();
}

pub(super) fn seed_sentinel_role_repo(path: &Path) {
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

pub(super) fn write_sentinel_config(path: &Path, role_source: &Path) {
    let sentinel_key = super::SENTINEL_ROLE_KEY;
    std::fs::write(
        path,
        format!(
            r#"version = "v1alpha5"

[roles."{sentinel_key}"]
git = "{}"
trusted = true
"#,
            role_source.display()
        ),
    )
    .unwrap();
}

pub(super) fn seed_slow_exit_role_repo(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    std::fs::write(path.join("Dockerfile"), slow_exit_role_dockerfile()).unwrap();
    std::fs::write(path.join("slow-marker"), format!("{:?}", Instant::now())).unwrap();
    std::fs::write(
        path.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude"]

[identity]
name = "Slow Exit"

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
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Seed slow exit e2e role",
        ],
        Some(path),
    );
}

pub(super) fn write_slow_exit_config(path: &Path, role_source: &Path) {
    let slow_exit_key = super::SLOW_EXIT_ROLE_KEY;
    std::fs::write(
        path,
        format!(
            r#"version = "v1alpha5"

[roles."{slow_exit_key}"]
git = "{}"
trusted = true
"#,
            role_source.display()
        ),
    )
    .unwrap();
}

pub(super) const fn slow_exit_role_dockerfile() -> &'static str {
    r"FROM projectjackin/construct:0.1-trixie
USER root
COPY slow-marker /tmp/jackin-slow-exit-marker
RUN sleep 20
USER agent
"
}

pub(super) const fn role_dockerfile() -> &'static str {
    // The baked Claude seed propagates into /jackin/default-home/.claude/backups
    // via the derived default-home snapshot. runtime-setup copies default-home
    // into the agent's home on first launch; when the container runs as an
    // arbitrary host UID/GID with supplementary group 0, the seed is readable
    // only if role-baked files are born group-0 + group-readable. Keep the file
    // non-world-readable so the test still exercises the group-0 contract after
    // the recursive derived chmod pass was removed.
    r"FROM projectjackin/construct:0.1-trixie
USER root
RUN apt-get update && \
    apt-get install -y --no-install-recommends default-jdk-headless maven && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/* \
           /var/cache/apt/* \
           /tmp/*
USER agent
RUN install -d -m 0750 /home/agent/.claude/backups && \
    printf 'seed' > /home/agent/.claude/backups/.claude.json.backup.e2e && \
    chmod 0640 /home/agent/.claude/backups/.claude.json.backup.e2e
"
}

pub(super) fn seed_claude_installer_stub(home: &Path) {
    let stub = home
        .join(".jackin")
        .join("cache")
        .join("agent-binaries-test-stub")
        .join("claude");
    std::fs::create_dir_all(stub.parent().unwrap()).unwrap();
    std::fs::write(&stub, fake_claude_installer()).unwrap();
    chmod_executable(&stub);
}

pub(super) fn seed_all_agent_stubs(home: &Path) {
    for slug in ["claude", "amp", "kimi", "opencode", "grok"] {
        seed_agent_stub(home, slug, &agent_installer(slug, ""));
    }
    seed_agent_stub(
        home,
        "codex",
        &agent_installer(
            "codex",
            "jackin-sentinel-report | tee /workspace/jackin-sentinel-report.txt",
        ),
    );
}

pub(super) fn agent_installer(slug: &str, run_body: &str) -> String {
    let fallback = format!("echo \"{slug} 0.0.0-e2e\"");
    let run_body = if run_body.trim().is_empty() {
        fallback.as_str()
    } else {
        run_body
    };
    format!(
        r#"if [ "${{1:-}}" = "install" ]; then
  mkdir -p "$HOME/.local/bin"
  cat > "$HOME/.local/bin/{slug}" <<'AGENT'
#!/bin/sh
set -eu
if [ "${{1:-}}" = "--version" ]; then
  echo "{slug} 0.0.0-e2e"
  exit 0
fi
{run_body}
AGENT
  chmod 0755 "$HOME/.local/bin/{slug}"
  exit 0
fi
if [ "${{1:-}}" = "--version" ]; then
  echo "{slug} 0.0.0-e2e"
  exit 0
fi
{run_body}
"#
    )
}

pub(super) fn seed_agent_stub(home: &Path, slug: &str, body: &str) {
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
pub(super) fn fake_claude_installer() -> String {
    let runtime = fake_claude_runtime_script();
    format!(
        r#"#!/bin/sh
set -eu
if [ "${{1:-}}" = "install" ]; then
  mkdir -p "$HOME/.local/bin"
  cat > "$HOME/.local/bin/claude" <<'CLAUDE'
{runtime}
CLAUDE
  chmod 0755 "$HOME/.local/bin/claude"
  exit 0
fi
{runtime}
"#
    )
}

pub(super) fn fake_claude_runtime_script() -> String {
    format!(
        r#"#!/bin/sh
set -eu
if [ "${{1:-}}" = "--version" ]; then
  echo "claude 0.0.0-e2e"
  exit 0
fi
if [ "${{1:-}}" = "plugin" ]; then
  exit 0
fi
echo "{begin}"
echo "DOCKER_HOST=$DOCKER_HOST"
echo "DOCKER_TLS_VERIFY=$DOCKER_TLS_VERIFY"
echo "DOCKER_CERT_PATH=$DOCKER_CERT_PATH"
echo "JACKIN_DIND_HOSTNAME=$JACKIN_DIND_HOSTNAME"
echo "TESTCONTAINERS_HOST_OVERRIDE=$TESTCONTAINERS_HOST_OVERRIDE"
echo "NO_PROXY=${{NO_PROXY:-}}"
echo "no_proxy=${{no_proxy:-}}"
smoke_image="jackin-dind-e2e-smoke:local"
if ! docker image inspect "$smoke_image" >/dev/null 2>&1; then
  smoke_root="$(mktemp -d)"
  rootfs="$smoke_root/rootfs"
  mkdir -p "$rootfs/bin" "$rootfs/usr/bin"

  copy_binary() {{
    src="$(readlink -f "$1")"
    dest="$2"
    mkdir -p "$rootfs$(dirname "$dest")"
    cp "$src" "$rootfs$dest"
    ldd "$src" | awk '{{ for (i = 1; i <= NF; i++) if ($i ~ /^\//) print $i }}' | while IFS= read -r lib; do
      mkdir -p "$rootfs$(dirname "$lib")"
      cp "$lib" "$rootfs$lib"
    done
  }}

  copy_binary /bin/sh /bin/sh
  copy_binary /usr/bin/sleep /usr/bin/sleep
  cp "$rootfs/usr/bin/sleep" "$rootfs/bin/sleep"
  tar -C "$rootfs" -cf "$smoke_root/rootfs.tar" .
  docker import "$smoke_root/rootfs.tar" "$smoke_image" >/dev/null
  rm -rf "$smoke_root"
fi
docker rm -f jackin-dind-e2e-docker-ps-smoke >/dev/null 2>&1 || true
child_id="$(docker run -d --name jackin-dind-e2e-docker-ps-smoke "$smoke_image" /bin/sh -c 'sleep 30')"
echo "DIND_DOCKER_RUN_CHILD=$child_id"
docker inspect --format 'DIND_DOCKER_RUN_STATE={{{{.State.Status}}}}' "$child_id"
docker ps --no-trunc --filter "id=$child_id"
docker rm -f "$child_id" >/dev/null 2>&1 || true
# Emit REPORT_END before the Maven smoke so the host's `output.stdout`
# parse can succeed even when mvn's network reach to Maven Central
# (testcontainers pull, JDK plugin downloads) is slow or fails. The
# TESTCONTAINERS_SMOKE=ok assertion later in the test catches a real
# smoke regression independently.
echo "{end}"
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
        GenericContainer<?> container = new GenericContainer<>(DockerImageName.parse("jackin-dind-e2e-smoke:local"))
                .withImagePullPolicy(imageName -> false)
                .withCommand("/bin/sh", "-c", "echo jackin-testcontainers-child-ok && sleep 1");
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
"#,
        begin = super::util::REPORT_BEGIN,
        end = super::util::REPORT_END,
    )
}
