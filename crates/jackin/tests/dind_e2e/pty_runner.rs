//! PTY-based runner family: spawn `script(1)` wrapping `jackin load`, drive
//! stdin with either a sentinel file watch, a transcript script, or a quick
//! exit probe, then collect stdout / stderr into `Arc<Mutex<Vec<u8>>>`
//! buffers.

#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use jackin_image::derived_image::shell_quote;

use super::common::apply_host_docker_config;
use super::diagnostics::{diagnostics_snapshot, transcript_excerpt};
use super::transcript::{
    MACOS_TYPESCRIPT_NAME, buffer_bytes, spawn_logged_pipe_collector, spawn_pipe_collector,
    spawn_stdout_collector, transcript_contains, wait_for_transcript_text,
};

fn wait_for_file_exists(path: &str, done: &AtomicBool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline && !done.load(Ordering::Relaxed) {
        if Path::new(path).exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Sets `done` when dropped so the macOS typescript follower always
/// terminates, including on panic-unwind past the explicit stores.
struct DoneGuard {
    done: Arc<AtomicBool>,
}

impl Drop for DoneGuard {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
    }
}

pub(super) fn pty_command(
    jackin: &str,
    args: &[&str],
    home: &Path,
    cwd: &Path,
    extra_env: &[(&str, &str)],
) -> Command {
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
    // `quit undef`: the palette hotkey (`^\`, 0x1c) doubles as the tty
    // VQUIT char. A script step gated on a boot-marker file can land
    // while the child is still in cooked mode, and the line discipline
    // would SIGQUIT the child instead of delivering a keypress — a
    // coin-flip crash. Undefining VQUIT keeps every other discipline
    // behavior (canonical mode, ICRNL for `\r` prompt answers) intact.
    let full = format!("stty cols 120 rows 40 quit undef >/dev/null 2>&1; exec {invocation}");
    if cfg!(target_os = "macos") {
        // BSD `script` block-buffers pipe stdout (nothing arrives live), so
        // the transcript is followed through this file instead; see
        // `spawn_stdout_collector`. Remove any stale typescript BEFORE
        // spawn: after spawn the file may already be the live one, and a
        // reused `cwd` would otherwise pollute matching with old bytes.
        drop(std::fs::remove_file(cwd.join(MACOS_TYPESCRIPT_NAME)));
        command
            .arg("-q")
            .arg(cwd.join(MACOS_TYPESCRIPT_NAME))
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
        // stays stripped so a host value can't bleed past jackin❯'s reserved
        // per-container override into the in-container testcontainers smoke.
        .env_remove("TESTCONTAINERS_HOST_OVERRIDE");
    apply_host_docker_config(&mut command);
    for (k, v) in extra_env {
        command.env(k, v);
    }
    command.current_dir(cwd);
    command
}

pub(super) fn run_in_pty_until_file(
    jackin: &str,
    args: &[&str],
    home: &Path,
    cwd: &Path,
    extra_env: &[(&str, &str)],
    script: &[PtyScriptStep],
    sentinel: PtyFileSentinel<'_>,
) -> std::process::Output {
    let mut child = pty_command(jackin, args, home, cwd, extra_env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("script must spawn");
    let mut stdin = child.stdin.take().expect("script stdin must be piped");
    let stdout = child.stdout.take().expect("script stdout must be piped");
    let stderr = child.stderr.take().expect("script stderr must be piped");
    let done = Arc::new(AtomicBool::new(false));
    let _done_guard = DoneGuard {
        done: Arc::clone(&done),
    };
    let (stdout_buf, stdout_reader) = spawn_stdout_collector(stdout, cwd, &done);
    let (stderr_buf, stderr_reader) =
        spawn_logged_pipe_collector(stderr, &cwd.join("e2e-launch-stderr.log"));
    let stdout_for_writer = Arc::clone(&stdout_buf);
    let done_for_writer = Arc::clone(&done);
    let script = script.to_vec();
    let stdin_writer = std::thread::spawn(move || {
        for step in script {
            // File gates observe container-side progress (agent boot
            // markers) that never reaches the transcript; they budget
            // for image build + boot, unlike the fast UI transcript waits.
            if !step.wait_for_file.is_empty()
                && !wait_for_file_exists(
                    step.wait_for_file,
                    &done_for_writer,
                    Duration::from_mins(12),
                )
            {
                return;
            }
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
            drop(stdin.write_all(step.input.as_bytes()));
            std::thread::sleep(Duration::from_millis(500));
        }
        while !done_for_writer.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    let deadline = Instant::now() + sentinel.timeout;
    while Instant::now() < deadline {
        let stop_requested = sentinel
            .stop_after
            .is_some_and(|completed| completed.load(Ordering::Acquire));
        if stop_requested
            || std::fs::read_to_string(sentinel.path)
                .is_ok_and(|contents| contents.contains(sentinel.text))
        {
            drop(child.kill());
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
            let fault_expected = sentinel
                .accept_early_exit_after
                .is_some_and(|started| started.load(Ordering::Acquire));
            assert!(
                status.success() || fault_expected,
                "script exited before sentinel file appeared\nstdout:\n{}\nstderr:\n{}",
                transcript_excerpt(&String::from_utf8_lossy(&output.stdout)),
                transcript_excerpt(&String::from_utf8_lossy(&output.stderr)),
            );
            return output;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    drop(child.kill());
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
        transcript_excerpt(&String::from_utf8_lossy(&output.stdout)),
        transcript_excerpt(&String::from_utf8_lossy(&output.stderr)),
    );
}

pub(super) fn run_in_pty_until_quick_exit_after_input(
    jackin: &str,
    args: &[&str],
    home: &Path,
    cwd: &Path,
    extra_env: &[(&str, &str)],
    exit: PtyQuickExit<'_>,
) -> std::process::Output {
    let mut child = pty_command(jackin, args, home, cwd, extra_env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("script must spawn");
    let mut stdin = child.stdin.take().expect("script stdin must be piped");
    let stdout = child.stdout.take().expect("script stdout must be piped");
    let stderr = child.stderr.take().expect("script stderr must be piped");
    let done = Arc::new(AtomicBool::new(false));
    let _done_guard = DoneGuard {
        done: Arc::clone(&done),
    };
    // macOS follows the live typescript; other platforms keep the exact
    // previous pipe collector (no new files, no new failure modes).
    #[cfg(target_os = "macos")]
    let (stdout_buf, stdout_reader) = spawn_stdout_collector(stdout, cwd, &done);
    #[cfg(not(target_os = "macos"))]
    let (stdout_buf, stdout_reader) = spawn_pipe_collector(stdout);
    let (stderr_buf, stderr_reader) = spawn_pipe_collector(stderr);
    let wait_deadline = Instant::now() + Duration::from_mins(3);
    while !transcript_contains(&stdout_buf, exit.wait_for) {
        if let Some(status) = child.try_wait().expect("script status must be readable") {
            done.store(true, Ordering::Relaxed);
            stdout_reader.join().expect("stdout reader must finish");
            stderr_reader.join().expect("stderr reader must finish");
            panic!(
                "PTY command exited before transcript reached {:?} with status {status}\ndiagnostics:\n{}\nstdout tail:\n{}\nstderr tail:\n{}",
                exit.wait_for,
                diagnostics_snapshot(home),
                transcript_excerpt(&String::from_utf8_lossy(&buffer_bytes(&stdout_buf))),
                transcript_excerpt(&String::from_utf8_lossy(&buffer_bytes(&stderr_buf))),
            );
        }
        if Instant::now() >= wait_deadline {
            drop(child.kill());
            let _status = child.wait().expect("script must finish");
            done.store(true, Ordering::Relaxed);
            stdout_reader.join().expect("stdout reader must finish");
            stderr_reader.join().expect("stderr reader must finish");
            panic!(
                "PTY transcript never reached {:?}\ndiagnostics:\n{}\nstdout tail:\n{}\nstderr tail:\n{}",
                exit.wait_for,
                diagnostics_snapshot(home),
                transcript_excerpt(&String::from_utf8_lossy(&buffer_bytes(&stdout_buf))),
                transcript_excerpt(&String::from_utf8_lossy(&buffer_bytes(&stderr_buf))),
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    stdin
        .write_all(exit.input.as_bytes())
        .expect("exit input must write");
    stdin.flush().expect("exit input must flush");

    let deadline = Instant::now() + exit.max_exit_after_input;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("script status must be readable") {
            done.store(true, Ordering::Relaxed);
            stdout_reader.join().expect("stdout reader must finish");
            stderr_reader.join().expect("stderr reader must finish");
            let output = std::process::Output {
                status,
                stdout: buffer_bytes(&stdout_buf),
                stderr: buffer_bytes(&stderr_buf),
            };
            assert_restored_terminal(&output);
            return output;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    drop(child.kill());
    let status = child.wait().expect("script must finish");
    done.store(true, Ordering::Relaxed);
    stdout_reader.join().expect("stdout reader must finish");
    stderr_reader.join().expect("stderr reader must finish");
    let output = std::process::Output {
        status,
        stdout: buffer_bytes(&stdout_buf),
        stderr: buffer_bytes(&stderr_buf),
    };
    panic!(
        "PTY command did not exit within {}ms after input\nstdout tail:\n{}\nstderr tail:\n{}",
        exit.max_exit_after_input.as_millis(),
        transcript_excerpt(&String::from_utf8_lossy(&output.stdout)),
        transcript_excerpt(&String::from_utf8_lossy(&output.stderr)),
    );
}

#[derive(Clone, Copy)]
pub(super) struct PtyQuickExit<'a> {
    pub(super) wait_for: &'a str,
    pub(super) input: &'a str,
    pub(super) max_exit_after_input: Duration,
}

pub(super) fn assert_restored_terminal(output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("\x1b[?1049l") && stdout.contains("\x1b[?25h"),
        "hard exit did not visibly restore the terminal\nstdout tail:\n{}\nstderr tail:\n{}",
        transcript_excerpt(stdout.as_ref()),
        transcript_excerpt(stderr.as_ref()),
    );
}

#[derive(Clone, Copy)]
pub(super) struct PtyScriptStep {
    pub(super) wait_for: &'static str,
    pub(super) input: &'static str,
    /// Optional filesystem gate (empty = none): the step waits for this
    /// path to exist before the transcript wait. Only the focused tab's
    /// pane content reaches the transcript, so boot markers from
    /// background tabs are unmatchable there; file gates observe them.
    pub(super) wait_for_file: &'static str,
}

#[derive(Clone, Copy)]
pub(super) struct PtyFileSentinel<'a> {
    pub(super) path: &'a Path,
    pub(super) text: &'a str,
    pub(super) timeout: Duration,
    pub(super) accept_early_exit_after: Option<&'a AtomicBool>,
    pub(super) stop_after: Option<&'a AtomicBool>,
}

pub(super) const fn scripted_sentinel_launch_input() -> [PtyScriptStep; 8] {
    [
        PtyScriptStep {
            wait_for: "Choose launch agent",
            input: "\x1b[B\r",
            wait_for_file: "",
        },
        PtyScriptStep {
            wait_for: "Sentinel free text:",
            input: "\r",
            wait_for_file: "",
        },
        PtyScriptStep {
            wait_for: "",
            input: "required-value\r",
            wait_for_file: "",
        },
        PtyScriptStep {
            wait_for: "",
            input: "\r",
            wait_for_file: "",
        },
        PtyScriptStep {
            wait_for: "",
            input: "\r",
            wait_for_file: "",
        },
        PtyScriptStep {
            wait_for: "",
            input: "\r",
            wait_for_file: "",
        },
        PtyScriptStep {
            wait_for: "",
            input: "\r",
            wait_for_file: "",
        },
        PtyScriptStep {
            wait_for: "",
            input: "\r",
            wait_for_file: "",
        },
    ]
}
