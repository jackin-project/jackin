use std::path::Path;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Options that control how a command is executed.
#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    pub capture_stderr: bool,
    pub quiet: bool,
    pub extra_env: Vec<(String, String)>,
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

        if opts.quiet {
            let mut cmd = Self::build_command(program, args, cwd);
            if !opts.extra_env.is_empty() {
                cmd.envs(opts.extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            }
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
        } else if opts.capture_stderr {
            let mut cmd = Self::build_command(program, args, cwd);
            if !opts.extra_env.is_empty() {
                cmd.envs(opts.extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            }
            let mut child = cmd.stderr(std::process::Stdio::piped()).spawn()?;
            let mut stderr_pipe = child.stderr.take().ok_or_else(|| {
                anyhow::anyhow!(
                    "failed to capture stderr for {} {}",
                    program,
                    args.join(" ")
                )
            })?;
            let mut stderr_buf = Vec::new();
            let read_fut = async {
                let mut buf = [0u8; 8192];
                loop {
                    let n = stderr_pipe.read(&mut buf).await?;
                    if n == 0 {
                        break;
                    }
                    use std::io::Write;
                    std::io::stderr().write_all(&buf[..n])?;
                    stderr_buf.extend_from_slice(&buf[..n]);
                }
                Ok::<(), std::io::Error>(())
            };
            let (status, read_result) = tokio::join!(child.wait(), read_fut);
            read_result?;
            let status = status?;
            if !status.success() {
                if String::from_utf8_lossy(&stderr_buf).trim().is_empty() {
                    anyhow::bail!("command failed: {} {}", program, args.join(" "));
                }
                anyhow::bail!(
                    "command failed: {} {} (see stderr above)",
                    program,
                    args.join(" ")
                );
            }
        } else {
            let mut cmd = Self::build_command(program, args, cwd);
            if !opts.extra_env.is_empty() {
                cmd.envs(opts.extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            }
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
        self.do_capture(program, args, cwd, CaptureMode::Normal).await
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.do_capture(program, args, cwd, CaptureMode::Secret).await
    }
}

impl ShellRunner {
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
