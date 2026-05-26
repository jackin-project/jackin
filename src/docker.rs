use std::path::Path;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Options that control how a command is executed.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub capture_stderr: bool,
    pub capture_stdout: bool,
    pub quiet: bool,
    pub extra_env: Vec<(String, String)>,
    pub null_stdin: bool,
    pub stream_captured_output: bool,
    /// The command needs the real terminal (an interactive `docker exec -it`
    /// multiplexer/shell client). Such commands must inherit stdio and are
    /// never captured — capturing denies the TTY and blocks forever on the
    /// long-lived session, even under `--debug` or while a rich surface was
    /// active.
    pub interactive: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            capture_stderr: false,
            capture_stdout: false,
            quiet: false,
            extra_env: Vec::new(),
            null_stdin: false,
            stream_captured_output: true,
            interactive: false,
        }
    }
}

pub trait CommandRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()>;
    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String>;
    /// Like `capture` but suppresses stdout from the debug stream and omits
    /// stderr from error messages. Use for commands whose output is a credential
    /// (e.g. `gh auth token`, `op read`) so the value never appears in debug logs.
    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String>;
}

#[derive(Debug, Default)]
pub struct ShellRunner {
    pub debug: bool,
}

impl ShellRunner {
    fn build_command(program: &str, args: &[&str], cwd: Option<&Path>) -> Command {
        let mut command = Command::new(program);
        command.args(args);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        command
    }

    fn apply_run_opts(cmd: &mut Command, opts: &RunOptions) {
        // Destructure so a new RunOptions field forces a maintainer to
        // decide whether it belongs here (applied to every `run` arm)
        // or stays the responsibility of an arm-specific branch.
        let RunOptions {
            capture_stderr: _,
            capture_stdout: _,
            quiet: _,
            extra_env,
            null_stdin,
            stream_captured_output: _,
            interactive: _,
        } = opts;
        if *null_stdin {
            cmd.stdin(std::process::Stdio::null());
        }
        if !extra_env.is_empty() {
            cmd.envs(extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        }
    }

    fn log_command(&self, program: &str, args: &[&str], cwd: Option<&Path>) {
        if self.debug {
            let redacted = redact_env_args(args);
            let cmd = format!("{} {}", program, redacted.join(" "));
            if let Some(dir) = cwd {
                crate::tui::emit_debug_line("cmd", &format!("cd {} && {cmd}", dir.display()));
            } else {
                crate::tui::emit_debug_line("cmd", &cmd);
            }
        }
    }
}

/// Mask the value portion of `-e KEY=VALUE` / `--env KEY=VALUE` args.
pub(crate) fn redact_env_args(args: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        out.push(arg.to_string());
        if (arg == "-e" || arg == "--env") && i + 1 < args.len() {
            let next = args[i + 1];
            match next.find('=') {
                Some(eq) => out.push(format!("{}=<redacted>", &next[..eq])),
                None => out.push(next.to_string()),
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

async fn read_process_pipe<R, W>(
    pipe: &mut R,
    stream: bool,
    mut output: W,
) -> std::io::Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
    W: std::io::Write,
{
    let mut captured = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = pipe.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        if stream {
            output.write_all(&buf[..n])?;
        }
        captured.extend_from_slice(&buf[..n]);
    }
    Ok(captured)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaptureMode {
    Normal,
    Secret,
}

impl CommandRunner for ShellRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()> {
        self.log_command(program, args, cwd);

        if opts.interactive {
            // Interactive commands (the `docker exec -it` multiplexer / shell
            // client) must inherit the real terminal. The --debug and
            // rich-surface arms below would otherwise capture this output,
            // denying the client its TTY and blocking forever on the
            // long-lived session — so inherit stdio directly and never capture.
            let mut cmd = Self::build_command(program, args, cwd);
            Self::apply_run_opts(&mut cmd, opts);
            let status = cmd.status().await?;
            anyhow::ensure!(
                status.success(),
                "command failed: {} {}",
                program,
                args.join(" ")
            );
        } else if opts.quiet {
            let mut cmd = Self::build_command(program, args, cwd);
            Self::apply_run_opts(&mut cmd, opts);
            let status = cmd
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await?;
            anyhow::ensure!(
                status.success(),
                "command failed: {} {}",
                program,
                args.join(" ")
            );
        } else if opts.capture_stderr || opts.capture_stdout {
            Box::pin(self.run_captured(program, args, cwd, opts)).await?;
        } else if self.debug || crate::tui::rich_surface_active() {
            // This arm would otherwise inherit the terminal and stream raw
            // command output straight to the screen — which floods a rich TUI
            // and a --debug run. Capture both streams instead so the output
            // lands in the diagnostics file (under --debug) and never on the
            // screen.
            let captured = RunOptions {
                capture_stdout: true,
                capture_stderr: true,
                ..opts.clone()
            };
            Box::pin(self.run_captured(program, args, cwd, &captured)).await?;
        } else {
            let mut cmd = Self::build_command(program, args, cwd);
            Self::apply_run_opts(&mut cmd, opts);
            let status = cmd.status().await?;
            anyhow::ensure!(
                status.success(),
                "command failed: {} {}",
                program,
                args.join(" ")
            );
        }
        Ok(())
    }

    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.do_capture(program, args, cwd, CaptureMode::Normal)
            .await
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.do_capture(program, args, cwd, CaptureMode::Secret)
            .await
    }
}

impl ShellRunner {
    async fn run_captured(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()> {
        let mut cmd = Self::build_command(program, args, cwd);
        Self::apply_run_opts(&mut cmd, opts);
        if opts.capture_stdout {
            cmd.stdout(std::process::Stdio::piped());
        }
        if opts.capture_stderr {
            cmd.stderr(std::process::Stdio::piped());
        }
        let mut child = cmd.spawn()?;
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        // Never stream child output to the terminal while a debug run is
        // capturing (it belongs in the diagnostics file, not the screen) or
        // while a rich full-screen TUI owns the terminal (it would corrupt
        // the frame). In both cases the output is captured and, under
        // --debug, written to the run's JSONL by `log_captured_output`.
        let stream = opts.stream_captured_output
            && !self.debug
            && !crate::tui::rich_surface_active();
        let read_stdout = async move {
            let Some(mut stdout_pipe) = stdout_pipe else {
                return Ok::<Vec<u8>, std::io::Error>(Vec::new());
            };
            read_process_pipe(&mut stdout_pipe, stream, std::io::stdout()).await
        };
        let read_stderr = async move {
            let Some(mut stderr_pipe) = stderr_pipe else {
                return Ok::<Vec<u8>, std::io::Error>(Vec::new());
            };
            read_process_pipe(&mut stderr_pipe, stream, std::io::stderr()).await
        };
        let (status, stdout_result, stderr_result) =
            tokio::join!(child.wait(), read_stdout, read_stderr);
        let stdout_buf = stdout_result?;
        let stderr_buf = stderr_result?;
        let status = status?;
        self.log_captured_output(program, args, &stdout_buf, &stderr_buf);
        if !status.success() {
            if String::from_utf8_lossy(&stderr_buf).trim().is_empty() {
                anyhow::bail!("command failed: {} {}", program, args.join(" "));
            }
            if !opts.stream_captured_output {
                if let Some(run) = crate::diagnostics::active_run().filter(|_| self.debug) {
                    anyhow::bail!(
                        "command failed: {} {} (captured output in diagnostics run {})",
                        program,
                        args.join(" "),
                        run.run_id()
                    );
                }
                if let Some(run) = crate::diagnostics::active_run() {
                    anyhow::bail!(
                        "command failed: {} {} (output suppressed; rerun with --debug to capture it in diagnostics run {})",
                        program,
                        args.join(" "),
                        run.run_id()
                    );
                }
                anyhow::bail!(
                    "command failed: {} {} (captured output suppressed)",
                    program,
                    args.join(" ")
                );
            }
            anyhow::bail!(
                "command failed: {} {} (see stderr above)",
                program,
                args.join(" ")
            );
        }
        Ok(())
    }

    fn log_captured_output(&self, program: &str, args: &[&str], stdout: &[u8], stderr: &[u8]) {
        if !self.debug {
            return;
        }
        let command = format!("{} {}", program, redact_env_args(args).join(" "));
        for line in String::from_utf8_lossy(stdout).lines() {
            crate::diagnostics::active_debug("cmd.stdout", &format!("{command}: {line}"));
        }
        for line in String::from_utf8_lossy(stderr).lines() {
            crate::diagnostics::active_debug("cmd.stderr", &format!("{command}: {line}"));
        }
    }

    async fn do_capture(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        mode: CaptureMode,
    ) -> anyhow::Result<String> {
        self.log_command(program, args, cwd);
        let output = Self::build_command(program, args, cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await?;
        if !output.status.success() {
            match mode {
                CaptureMode::Secret => {
                    anyhow::bail!("command failed: {} {}", program, args.join(" "));
                }
                CaptureMode::Normal => {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    if stderr.is_empty() {
                        anyhow::bail!("command failed: {} {}", program, args.join(" "));
                    }
                    anyhow::bail!("command failed: {} {}: {}", program, args.join(" "), stderr);
                }
            }
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if self.debug && !stdout.is_empty() {
            match mode {
                CaptureMode::Normal => {
                    let first_line = stdout.lines().next().unwrap_or("");
                    crate::tui::emit_debug_line("cmd", &format!("-> {first_line}"));
                }
                CaptureMode::Secret => {}
            }
        }
        Ok(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn run_capture_stderr_returns_hint_after_streaming_stderr() {
        let mut runner = ShellRunner::default();
        let opts = RunOptions {
            capture_stderr: true,
            ..RunOptions::default()
        };

        let error = runner
            .run(
                "sh",
                &["-c", "printf 'region blocked\\n' >&2; exit 2"],
                None,
                &opts,
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("see stderr above"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn capture_handles_large_stdout() {
        let mut runner = ShellRunner::default();

        let output = runner
            .capture("sh", &["-c", "yes x | head -c 200000"], None)
            .await
            .unwrap();

        assert!(output.len() >= 190_000);
        assert!(output.starts_with('x'));
    }

    #[test]
    fn redact_env_args_masks_dash_e_value() {
        let args = &[
            "run",
            "-e",
            "CLAUDE_CODE_OAUTH_TOKEN=sk-ant-secretvalue",
            "image:tag",
        ];
        let redacted = redact_env_args(args);
        assert_eq!(
            redacted,
            vec![
                "run",
                "-e",
                "CLAUDE_CODE_OAUTH_TOKEN=<redacted>",
                "image:tag",
            ],
        );
    }

    #[test]
    fn redact_env_args_masks_long_env_form() {
        let args = &["run", "--env", "GITHUB_TOKEN=ghp_secret", "image:tag"];
        let redacted = redact_env_args(args);
        assert_eq!(
            redacted,
            vec!["run", "--env", "GITHUB_TOKEN=<redacted>", "image:tag"],
        );
    }

    #[test]
    fn redact_env_args_leaves_host_passthrough_form_unchanged() {
        let args = &["run", "-e", "GITHUB_TOKEN", "image:tag"];
        let redacted = redact_env_args(args);
        assert_eq!(redacted, vec!["run", "-e", "GITHUB_TOKEN", "image:tag"]);
    }

    #[test]
    fn redact_env_args_redacts_multiple_dash_e_values() {
        let args = &[
            "run",
            "-e",
            "TOKEN=secret-a",
            "--name",
            "my-container",
            "-e",
            "API_KEY=secret-b",
            "image:tag",
        ];
        let redacted = redact_env_args(args);
        assert_eq!(
            redacted,
            vec![
                "run",
                "-e",
                "TOKEN=<redacted>",
                "--name",
                "my-container",
                "-e",
                "API_KEY=<redacted>",
                "image:tag",
            ],
        );
    }

    #[test]
    fn redact_env_args_passes_non_env_args_through() {
        let args = &["build", "-t", "image:tag", "--no-cache", "."];
        let redacted = redact_env_args(args);
        assert_eq!(
            redacted,
            vec!["build", "-t", "image:tag", "--no-cache", "."],
        );
    }

    #[test]
    fn redact_env_args_handles_empty_value() {
        let args = &["run", "-e", "EMPTY=", "image:tag"];
        let redacted = redact_env_args(args);
        assert_eq!(redacted, vec!["run", "-e", "EMPTY=<redacted>", "image:tag"]);
    }

    #[test]
    fn redact_env_args_handles_value_containing_equals() {
        let args = &[
            "run",
            "-e",
            "DATABASE_URL=postgres://user:pass@host:5432/db?sslmode=require",
            "image:tag",
        ];
        let redacted = redact_env_args(args);
        assert_eq!(
            redacted,
            vec!["run", "-e", "DATABASE_URL=<redacted>", "image:tag",],
        );
    }

    #[test]
    fn redact_env_args_handles_dash_e_at_end_with_no_value() {
        let args = &["run", "-e"];
        let redacted = redact_env_args(args);
        assert_eq!(redacted, vec!["run", "-e"]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn capture_secret_omits_stderr_from_error_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let secret_file = dir.path().join("s.txt");
        std::fs::write(&secret_file, "xSECRET_STDERR_CONTENTx").unwrap();
        let script = format!("cat '{}' >&2; exit 1", secret_file.display());
        let mut runner = ShellRunner::default();
        let err = runner
            .capture_secret("sh", &["-c", &script], None)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.contains("xSECRET_STDERR_CONTENTx"),
            "stderr must not appear in error message: {msg}"
        );
        assert!(msg.contains("sh"), "program name must appear: {msg}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn debug_run_captures_noncapturing_command_into_diagnostics() {
        // A non-quiet, non-capturing `run` would inherit the terminal and
        // stream straight to the screen. Under --debug it must capture both
        // streams and route them to the diagnostics run file instead — never
        // to the terminal (which would flood a rich TUI).
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(dir.path());
        let run = crate::diagnostics::RunDiagnostics::start(&paths, true, "test").unwrap();
        let _active = run.activate();
        let mut runner = ShellRunner { debug: true };
        runner
            .run(
                "sh",
                &["-c", "echo hello-from-cmd"],
                None,
                &RunOptions::default(),
            )
            .await
            .unwrap();
        let contents = std::fs::read_to_string(run.path()).unwrap();
        assert!(
            contents.contains("hello-from-cmd"),
            "non-capturing command stdout must be captured into the run file under --debug: {contents}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn capture_secret_suppresses_stdout_debug_echo() {
        use std::sync::Mutex;
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let dir = tempfile::tempdir().unwrap();
        let token_file = dir.path().join("t.txt");
        std::fs::write(&token_file, "gho_token_value\n").unwrap();
        let script = format!("cat '{}'", token_file.display());

        crate::tui::set_debug_mode(true);
        crate::tui::begin_debug_buffering();
        let mut runner = ShellRunner { debug: true };
        let output = runner
            .capture_secret("sh", &["-c", &script], None)
            .await
            .unwrap();
        let lines = crate::tui::drain_debug_buffer_for_test();
        crate::tui::set_debug_mode(false);

        assert_eq!(
            output, "gho_token_value",
            "secret value must still be returned"
        );
        for line in &lines {
            assert!(
                !line.contains("gho_token_value"),
                "secret must not appear in debug output: {line}"
            );
        }
    }
}
