//! End-to-end smoke that drives `jackin load` against a real Docker daemon
//! with proxy env declared in role config, then asserts the launched agent
//! container's environment carries the `DinD` hostname in both `NO_PROXY`
//! and `no_proxy`. Regression guard for the proxy-routed `DinD`-handshake
//! bug fixed in `src/runtime/launch.rs`.

#![cfg(feature = "e2e")]

use std::io::{Read, Write as _};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use fs2::FileExt as _;
use jackin::derived_image::shell_quote;
use jackin::instance::naming::is_dns_label;
use tempfile::tempdir;

const ROLE_KEY: &str = "jackin-e2e/agent-smith";
const ROLE_CONTAINER_PREFIX: &str = "jackin-jackin-e2e__agent-smith";
const SENTINEL_ROLE_KEY: &str = "jackin-e2e/sentinel";
const SENTINEL_CONTAINER_PREFIX: &str = "jackin-jackin-e2e__sentinel";
const CAPSULE_DETACH_KEYS: &str = "\u{2}d";

#[derive(Clone, Copy)]
enum PtyInputMode {
    OnceAfter(Duration),
}

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
    let extra_env = [("JACKIN_CONSTRUCT_IMAGE", "projectjackin/construct:trixie")];
    let output = run_in_pty_with_input(
        &jackin,
        &args,
        &home,
        &workspace_dir,
        &extra_env,
        CAPSULE_DETACH_KEYS,
        PtyInputMode::OnceAfter(Duration::from_secs(2)),
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
    let extra_env = [("JACKIN_CONSTRUCT_IMAGE", "projectjackin/construct:trixie")];
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

fn assert_sentinel_report(report: &str, stdout: &str, stderr: &str) {
    assert!(
        report.contains("JACKIN_SENTINEL_REPORT_BEGIN"),
        "sentinel report missing begin marker\nreport:\n{report}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        report.contains("JACKIN_SENTINEL_REPORT_END"),
        "sentinel report missing end marker\nreport:\n{report}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(report.contains("JACKIN=1"), "{report}");
    assert!(report.contains("JACKIN_AGENT=codex"), "{report}");
    assert!(report.contains("STATIC_DEFAULT=static-value"), "{report}");
    assert!(
        report.contains("LITERAL_TEMPLATE=preserve-${other.VALUE}"),
        "{report}"
    );
    assert!(report.contains("FREE_TEXT=typed-default"), "{report}");
    assert!(
        report.contains("FREE_TEXT_REQUIRED=required-value"),
        "{report}"
    );
    assert!(report.contains("SELECT_PROJECT=frontend"), "{report}");
    assert!(report.contains("SELECT_MODE=diagnostic"), "{report}");
    assert!(report.contains("BRANCH=feature/frontend"), "{report}");
    assert!(
        report.contains("COMBINED_LABEL=frontend-typed-default"),
        "{report}"
    );
    assert!(report.contains("OPTIONAL_API_KEY=unset"), "{report}");
    assert!(report.contains("OPTIONAL_DERIVED=unset"), "{report}");
    assert!(report.contains("JACKIN_SENTINEL_SOURCE_HOOK=1"), "{report}");
    assert!(
        report.contains("JACKIN_SENTINEL_PREFLIGHT_COUNT=1"),
        "{report}"
    );
}

fn assert_sentinel_build_output_routed_to_log(home: &Path, stdout: &str, stderr: &str) {
    assert!(
        !stdout.contains("jackin-sentinel build layer")
            && !stderr.contains("jackin-sentinel build layer"),
        "Docker build output leaked onto the rich screen\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Choose launch agent")
            && stdout.contains("Sentinel free text:")
            && stdout.contains("↵")
            && stdout.contains("save"),
        "PTY transcript should prove the rich launch dialogs rendered\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let build_log = latest_docker_build_log(home).unwrap_or_else(|| {
        panic!(
            "expected docker build log artifact under diagnostics\n{}",
            diagnostics_snapshot(home)
        )
    });
    let build_log_contents = std::fs::read_to_string(&build_log).unwrap_or_else(|error| {
        panic!(
            "failed to read docker build log {}: {error}",
            build_log.display()
        )
    });
    assert!(
        build_log_contents.contains("jackin-sentinel build layer"),
        "Docker build output should be captured in the build log artifact {}\n{}",
        build_log.display(),
        build_log_contents
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
    // Probe the same daemon jackin will drive: honor DOCKER_HOST (and the
    // active docker context when unset), exactly as the host-side client
    // does. Stripping DOCKER_HOST here would gate on a default-socket daemon
    // that jackin itself would bypass whenever the operator set one.
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

fn e2e_serial_lock() -> std::fs::File {
    let path = std::env::temp_dir().join("jackin-dind-e2e.lock");
    let lock = std::fs::File::create(path).expect("e2e lock file must be creatable");
    lock.lock_exclusive()
        .expect("e2e lock file must be lockable");
    lock
}

fn run_in_pty_with_input(
    jackin: &str,
    args: &[&str],
    home: &Path,
    cwd: &Path,
    extra_env: &[(&str, &str)],
    input: &str,
    input_mode: PtyInputMode,
) -> std::process::Output {
    let mut command = pty_command(jackin, args, home, cwd, extra_env, true);
    if input.is_empty() {
        command.stdin(Stdio::null());
        return command.output().expect("script must spawn");
    }

    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("script must spawn");
    let mut stdin = child.stdin.take().expect("script stdin must be piped");
    match input_mode {
        PtyInputMode::OnceAfter(delay) => {
            std::thread::sleep(delay);
            stdin
                .write_all(input.as_bytes())
                .expect("script stdin write must succeed");
            drop(stdin);
            child.wait_with_output().expect("script must finish")
        }
    }
}

fn pty_command(
    jackin: &str,
    args: &[&str],
    home: &Path,
    cwd: &Path,
    extra_env: &[(&str, &str)],
    linux_timeout: bool,
) -> Command {
    let mut command = if cfg!(target_os = "linux") && linux_timeout {
        let mut command = Command::new("timeout");
        command.args(["--kill-after=5s", "360s", "script"]);
        command
    } else {
        Command::new("script")
    };
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
        .env_remove("CI")
        .env_remove("JACKIN_DEBUG")
        // Inherit DOCKER_HOST/DOCKER_TLS_VERIFY/DOCKER_CERT_PATH so the launch
        // drives whatever daemon the operator points at — the DOCKER_HOST
        // behavior architecture.mdx documents. TESTCONTAINERS_HOST_OVERRIDE
        // stays stripped so a host value can't bleed past jackin's reserved
        // per-container override into the in-container testcontainers smoke.
        .env_remove("TESTCONTAINERS_HOST_OVERRIDE");
    for (k, v) in extra_env {
        command.env(k, v);
    }
    command.current_dir(cwd);
    command
}

fn run_in_pty_until_file(
    jackin: &str,
    args: &[&str],
    home: &Path,
    cwd: &Path,
    extra_env: &[(&str, &str)],
    script: &[PtyScriptStep],
    sentinel: PtyFileSentinel<'_>,
) -> std::process::Output {
    let mut child = pty_command(jackin, args, home, cwd, extra_env, false)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("script must spawn");
    let mut stdin = child.stdin.take().expect("script stdin must be piped");
    let stdout = child.stdout.take().expect("script stdout must be piped");
    let stderr = child.stderr.take().expect("script stderr must be piped");
    let done = Arc::new(AtomicBool::new(false));
    let (stdout_buf, stdout_reader) = spawn_pipe_collector(stdout);
    let (stderr_buf, stderr_reader) = spawn_pipe_collector(stderr);
    let stdout_for_writer = stdout_buf.clone();
    let done_for_writer = done.clone();
    let script = script.to_vec();
    let stdin_writer = std::thread::spawn(move || {
        for step in script {
            if !step.wait_for.is_empty()
                && !wait_for_transcript_text(
                    &stdout_for_writer,
                    step.wait_for,
                    &done_for_writer,
                    Duration::from_mins(2),
                )
            {
                return;
            }
            let _ = stdin.write_all(step.input.as_bytes());
            std::thread::sleep(Duration::from_millis(500));
        }
        while !done_for_writer.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    let deadline = Instant::now() + sentinel.timeout;
    while Instant::now() < deadline {
        if std::fs::read_to_string(sentinel.path)
            .is_ok_and(|contents| contents.contains(sentinel.text))
        {
            let _ = child.kill();
            let status = child.wait().expect("script must finish");
            done.store(true, Ordering::Relaxed);
            stdin_writer.join().expect("stdin writer must finish");
            stdout_reader.join().expect("stdout reader must finish");
            stderr_reader.join().expect("stderr reader must finish");
            return std::process::Output {
                status,
                stdout: buffer_bytes(&stdout_buf),
                stderr: buffer_bytes(&stderr_buf),
            };
        }
        if let Some(status) = child.try_wait().expect("script status must be readable") {
            done.store(true, Ordering::Relaxed);
            stdin_writer.join().expect("stdin writer must finish");
            stdout_reader.join().expect("stdout reader must finish");
            stderr_reader.join().expect("stderr reader must finish");
            let output = std::process::Output {
                status,
                stdout: buffer_bytes(&stdout_buf),
                stderr: buffer_bytes(&stderr_buf),
            };
            assert!(
                status.success(),
                "script exited before sentinel file appeared\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            return output;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    let _ = child.kill();
    let status = child.wait().expect("script must finish");
    done.store(true, Ordering::Relaxed);
    stdin_writer.join().expect("stdin writer must finish");
    stdout_reader.join().expect("stdout reader must finish");
    stderr_reader.join().expect("stderr reader must finish");
    let output = std::process::Output {
        status,
        stdout: buffer_bytes(&stdout_buf),
        stderr: buffer_bytes(&stderr_buf),
    };
    let diagnostics = diagnostics_snapshot(home);
    panic!(
        "timed out waiting for sentinel file {}\ndiagnostics:\n{}\nstdout tail:\n{}\nstderr tail:\n{}",
        sentinel.path.display(),
        diagnostics,
        tail_text(&String::from_utf8_lossy(&output.stdout)),
        tail_text(&String::from_utf8_lossy(&output.stderr)),
    );
}

#[derive(Clone, Copy)]
struct PtyScriptStep {
    wait_for: &'static str,
    input: &'static str,
}

#[derive(Clone, Copy)]
struct PtyFileSentinel<'a> {
    path: &'a Path,
    text: &'a str,
    timeout: Duration,
}

const fn scripted_sentinel_launch_input() -> [PtyScriptStep; 8] {
    [
        PtyScriptStep {
            wait_for: "Choose launch agent",
            input: "\x1b[B\r",
        },
        PtyScriptStep {
            wait_for: "Sentinel free text:",
            input: "\r",
        },
        PtyScriptStep {
            wait_for: "",
            input: "required-value\r",
        },
        PtyScriptStep {
            wait_for: "",
            input: "\r",
        },
        PtyScriptStep {
            wait_for: "",
            input: "\r",
        },
        PtyScriptStep {
            wait_for: "",
            input: "\r",
        },
        PtyScriptStep {
            wait_for: "",
            input: "\r",
        },
        PtyScriptStep {
            wait_for: "",
            input: "\r",
        },
    ]
}

fn spawn_pipe_collector<R>(mut reader: R) -> (Arc<Mutex<Vec<u8>>>, std::thread::JoinHandle<()>)
where
    R: Read + Send + 'static,
{
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let thread_buffer = buffer.clone();
    let handle = std::thread::spawn(move || {
        let mut chunk = [0_u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => thread_buffer
                    .lock()
                    .expect("pty output buffer mutex must not be poisoned")
                    .extend_from_slice(&chunk[..n]),
            }
        }
    });
    (buffer, handle)
}

fn wait_for_transcript_text(
    buffer: &Arc<Mutex<Vec<u8>>>,
    needle: &str,
    done: &AtomicBool,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline && !done.load(Ordering::Relaxed) {
        if transcript_contains(buffer, needle) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn transcript_contains(buffer: &Arc<Mutex<Vec<u8>>>, needle: &str) -> bool {
    String::from_utf8_lossy(
        &buffer
            .lock()
            .expect("pty output buffer mutex must not be poisoned"),
    )
    .contains(needle)
}

fn buffer_bytes(buffer: &Arc<Mutex<Vec<u8>>>) -> Vec<u8> {
    buffer
        .lock()
        .expect("pty output buffer mutex must not be poisoned")
        .clone()
}

fn diagnostics_snapshot(home: &Path) -> String {
    let dir = home.join(".jackin/data/diagnostics/runs");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return format!("no diagnostics directory at {}", dir.display());
    };
    let mut files = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata
                .modified()
                .ok()
                .map(|modified| (modified, entry.path()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(modified, _)| *modified);
    let Some((_, latest)) = files.last() else {
        return format!("no diagnostics files in {}", dir.display());
    };

    let mut out = format!("latest diagnostics: {}\n", latest.display());
    match std::fs::read_to_string(latest) {
        Ok(contents) => {
            append_tail_lines(&mut out, &contents);
        }
        Err(error) => {
            out.push_str("failed to read diagnostics file: ");
            out.push_str(&error.to_string());
            out.push('\n');
        }
    }

    let Some(stem) = latest.file_stem().and_then(|stem| stem.to_str()) else {
        return out;
    };
    let build_log = latest.with_file_name(format!("{stem}.docker-build.log"));
    if let Ok(contents) = std::fs::read_to_string(&build_log) {
        out.push_str("latest docker build log: ");
        out.push_str(&build_log.display().to_string());
        out.push('\n');
        append_tail_lines(&mut out, &contents);
    }
    out
}

fn latest_docker_build_log(home: &Path) -> Option<std::path::PathBuf> {
    let dir = home.join(".jackin/data/diagnostics/runs");
    let mut files = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".docker-build.log"))
        })
        .filter_map(|path| {
            std::fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .ok()
                .map(|modified| (modified, path))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(modified, _)| *modified);
    files.pop().map(|(_, path)| path)
}

fn append_tail_lines(out: &mut String, contents: &str) {
    let mut lines = std::collections::VecDeque::with_capacity(80);
    for line in contents.lines() {
        if lines.len() == 80 {
            lines.pop_front();
        }
        lines.push_back(line);
    }
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
}

fn tail_text(contents: &str) -> String {
    let mut lines = std::collections::VecDeque::with_capacity(80);
    for line in contents.lines() {
        if lines.len() == 80 {
            lines.pop_front();
        }
        lines.push_back(line);
    }
    lines.into_iter().collect::<Vec<_>>().join("\n")
}

fn seed_existing_construct_entry(home: &Path) {
    let pending = home.join(".jackin/data/universe-pending");
    std::fs::create_dir_all(&pending).unwrap();
    std::fs::write(pending.join("e2e-existing-entry"), b"already entering").unwrap();
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
            r#"version = "v1alpha5"

[roles."{ROLE_KEY}"]
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
            r#"version = "v1alpha5"

[roles."{SENTINEL_ROLE_KEY}"]
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

fn agent_installer(slug: &str, run_body: &str) -> String {
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
