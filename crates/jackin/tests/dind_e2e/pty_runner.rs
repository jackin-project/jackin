//! PTY-based runner family: spawn `script(1)` wrapping `jackin load`, drive
//! stdin with either a sentinel file watch, a transcript script, or a quick
//! exit probe, then collect stdout / stderr into `Arc<Mutex<Vec<u8>>>`
//! buffers.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use jackin_image::derived_image::shell_quote;

use super::common::apply_host_docker_config;
use super::diagnostics::{diagnostics_snapshot, tail_text};
use super::transcript::{
    buffer_bytes, spawn_pipe_collector, transcript_contains, transcript_contains_all,
    wait_for_transcript_text,
};
use super::{
    BUILD_FAILED_MODAL_TEXT, CAPSULE_DETACH_KEYS, FAILURE_DIAGNOSTICS_LABEL, FAILURE_DISMISS_HINT,
    REPORT_BEGIN, REPORT_END, TESTCONTAINERS_SMOKE_OK,
};

pub(super) fn run_in_pty_until_agent_report(
    jackin: &str,
    args: &[&str],
    home: &Path,
    cwd: &Path,
    extra_env: &[(&str, &str)],
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
    let (stdout_buf, stdout_reader) = spawn_pipe_collector(stdout);
    let (stderr_buf, stderr_reader) = spawn_pipe_collector(stderr);
    let stdout_for_writer = Arc::clone(&stdout_buf);
    let done_for_writer = Arc::clone(&done);
    let stdin_writer = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_mins(6);
        while Instant::now() < deadline && !done_for_writer.load(Ordering::Relaxed) {
            if transcript_contains(&stdout_for_writer, BUILD_FAILED_MODAL_TEXT) {
                drop(stdin.write_all(b"\r"));
                return;
            }
            if transcript_contains_all(
                &stdout_for_writer,
                &[FAILURE_DIAGNOSTICS_LABEL, FAILURE_DISMISS_HINT],
            ) {
                drop(stdin.write_all(b"\r"));
                return;
            }
            if transcript_contains_all(
                &stdout_for_writer,
                &[REPORT_BEGIN, REPORT_END, TESTCONTAINERS_SMOKE_OK],
            ) {
                drop(stdin.write_all(CAPSULE_DETACH_KEYS.as_bytes()));
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    let output = wait_for_collected_pty_output(
        child,
        home,
        Duration::from_mins(6),
        &stdout_buf,
        stdout_reader,
        &stderr_buf,
        stderr_reader,
    );
    done.store(true, Ordering::Relaxed);
    stdin_writer.join().expect("stdin writer must finish");
    output
}

pub(super) fn wait_for_collected_pty_output(
    mut child: std::process::Child,
    home: &Path,
    timeout: Duration,
    stdout_buf: &Arc<Mutex<Vec<u8>>>,
    stdout_reader: std::thread::JoinHandle<()>,
    stderr_buf: &Arc<Mutex<Vec<u8>>>,
    stderr_reader: std::thread::JoinHandle<()>,
) -> std::process::Output {
    let deadline = Instant::now() + timeout;

    loop {
        if let Some(status) = child.try_wait().expect("script status must be readable") {
            stdout_reader.join().expect("stdout reader must finish");
            stderr_reader.join().expect("stderr reader must finish");
            return std::process::Output {
                status,
                stdout: buffer_bytes(stdout_buf),
                stderr: buffer_bytes(stderr_buf),
            };
        }

        if Instant::now() >= deadline {
            drop(child.kill());
            let status = child.wait().expect("script must finish");
            stdout_reader.join().expect("stdout reader must finish");
            stderr_reader.join().expect("stderr reader must finish");
            let output = std::process::Output {
                status,
                stdout: buffer_bytes(stdout_buf),
                stderr: buffer_bytes(stderr_buf),
            };
            panic!(
                "timed out waiting for PTY command after {}s\ndiagnostics:\n{}\nstdout tail:\n{}\nstderr tail:\n{}",
                timeout.as_secs(),
                diagnostics_snapshot(home),
                tail_text(&String::from_utf8_lossy(&output.stdout)),
                tail_text(&String::from_utf8_lossy(&output.stderr)),
            );
        }

        std::thread::sleep(Duration::from_millis(100));
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
    let (stdout_buf, stdout_reader) = spawn_pipe_collector(stdout);
    let (stderr_buf, stderr_reader) = spawn_pipe_collector(stderr);
    let stdout_for_writer = Arc::clone(&stdout_buf);
    let done_for_writer = Arc::clone(&done);
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
            drop(stdin.write_all(step.input.as_bytes()));
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
        tail_text(&String::from_utf8_lossy(&output.stdout)),
        tail_text(&String::from_utf8_lossy(&output.stderr)),
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
    let (stdout_buf, stdout_reader) = spawn_pipe_collector(stdout);
    let (stderr_buf, stderr_reader) = spawn_pipe_collector(stderr);
    let wait_deadline = Instant::now() + Duration::from_mins(3);
    while !transcript_contains(&stdout_buf, exit.wait_for) {
        if let Some(status) = child.try_wait().expect("script status must be readable") {
            stdout_reader.join().expect("stdout reader must finish");
            stderr_reader.join().expect("stderr reader must finish");
            panic!(
                "PTY command exited before transcript reached {:?} with status {status}\ndiagnostics:\n{}\nstdout tail:\n{}\nstderr tail:\n{}",
                exit.wait_for,
                diagnostics_snapshot(home),
                tail_text(&String::from_utf8_lossy(&buffer_bytes(&stdout_buf))),
                tail_text(&String::from_utf8_lossy(&buffer_bytes(&stderr_buf))),
            );
        }
        if Instant::now() >= wait_deadline {
            drop(child.kill());
            let _status = child.wait().expect("script must finish");
            stdout_reader.join().expect("stdout reader must finish");
            stderr_reader.join().expect("stderr reader must finish");
            panic!(
                "PTY transcript never reached {:?}\ndiagnostics:\n{}\nstdout tail:\n{}\nstderr tail:\n{}",
                exit.wait_for,
                diagnostics_snapshot(home),
                tail_text(&String::from_utf8_lossy(&buffer_bytes(&stdout_buf))),
                tail_text(&String::from_utf8_lossy(&buffer_bytes(&stderr_buf))),
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
        tail_text(&String::from_utf8_lossy(&output.stdout)),
        tail_text(&String::from_utf8_lossy(&output.stderr)),
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
        tail_text(stdout.as_ref()),
        tail_text(stderr.as_ref()),
    );
}

#[derive(Clone, Copy)]
pub(super) struct PtyScriptStep {
    pub(super) wait_for: &'static str,
    pub(super) input: &'static str,
}

#[derive(Clone, Copy)]
pub(super) struct PtyFileSentinel<'a> {
    pub(super) path: &'a Path,
    pub(super) text: &'a str,
    pub(super) timeout: Duration,
}

pub(super) const fn scripted_sentinel_launch_input() -> [PtyScriptStep; 8] {
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
