use owo_colors::OwoColorize;
use std::io::Read;
use std::path::Path;

/// Options that control how a command is executed.
#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    /// When `true`, stderr is piped, streamed to the terminal in real time,
    /// and captured so it can be included in error messages.  When `false`
    /// (the default), stderr is inherited directly from the parent process.
    pub capture_stderr: bool,
    /// When `true`, both stdout and stderr are sent to `/dev/null`.
    /// Useful for suppressing noisy output (e.g. git fetch) in non-debug mode.
    pub quiet: bool,
}

pub trait CommandRunner {
    fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()>;
    fn capture(
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
    fn build_command(program: &str, args: &[&str], cwd: Option<&Path>) -> std::process::Command {
        let mut command = std::process::Command::new(program);
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
                eprintln!(
                    "{}",
                    format!("[debug] cd {} && {}", dir.display(), cmd).dimmed()
                );
            } else {
                eprintln!("{}", format!("[debug] {cmd}").dimmed());
            }
        }
    }
}

/// Mask the value portion of `-e KEY=VALUE` / `--env KEY=VALUE` args so that
/// secrets resolved into the env (1Password references, host-env passthroughs
/// like `$GITHUB_TOKEN`, plain literals that may carry tokens) never appear in
/// `--debug` output. The env var *name* is preserved so the operator can still
/// see which keys are being injected.
///
/// Forms handled:
/// - `-e KEY=VALUE` → `-e KEY=<redacted>`
/// - `--env KEY=VALUE` → `--env KEY=<redacted>`
/// - `-e KEY` (host-env passthrough; the docker CLI inherits the value from
///   the launching shell — no value present in the argv) → left unchanged.
///
/// Other args are passed through unchanged.
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

impl CommandRunner for ShellRunner {
    fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()> {
        self.log_command(program, args, cwd);

        if opts.quiet {
            let mut child = Self::build_command(program, args, cwd)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            let status = child.wait()?;
            anyhow::ensure!(
                status.success(),
                "command failed: {} {}",
                program,
                args.join(" ")
            );
        } else if opts.capture_stderr {
            let mut child = Self::build_command(program, args, cwd)
                .stderr(std::process::Stdio::piped())
                .spawn()?;
            let stderr = child.stderr.take().ok_or_else(|| {
                anyhow::anyhow!(
                    "failed to capture stderr for {} {}",
                    program,
                    args.join(" ")
                )
            })?;
            let stderr_handle = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
                let mut reader = std::io::BufReader::new(stderr);
                let mut output = Vec::new();
                let mut buf = [0_u8; 8192];
                let mut writer = std::io::stderr().lock();
                loop {
                    let read = reader.read(&mut buf)?;
                    if read == 0 {
                        break;
                    }
                    std::io::Write::write_all(&mut writer, &buf[..read])?;
                    output.extend_from_slice(&buf[..read]);
                }
                Ok(output)
            });
            let status = child.wait()?;
            let stderr = stderr_handle
                .join()
                .map_err(|_| anyhow::anyhow!("stderr reader thread panicked"))??;
            if !status.success() {
                if String::from_utf8_lossy(&stderr).trim().is_empty() {
                    anyhow::bail!("command failed: {} {}", program, args.join(" "));
                }
                anyhow::bail!(
                    "command failed: {} {} (see stderr above)",
                    program,
                    args.join(" ")
                );
            }
        } else {
            let mut child = Self::build_command(program, args, cwd).spawn()?;
            let status = child.wait()?;
            anyhow::ensure!(
                status.success(),
                "command failed: {} {}",
                program,
                args.join(" ")
            );
        }
        Ok(())
    }

    fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.log_command(program, args, cwd);
        let mut child = Self::build_command(program, args, cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().ok_or_else(|| {
            anyhow::anyhow!(
                "failed to capture stdout for {} {}",
                program,
                args.join(" ")
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            anyhow::anyhow!(
                "failed to capture stderr for {} {}",
                program,
                args.join(" ")
            )
        })?;
        let stdout_handle = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
            let mut reader = std::io::BufReader::new(stdout);
            let mut output = Vec::new();
            reader.read_to_end(&mut output)?;
            Ok(output)
        });
        let stderr_handle = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
            let mut reader = std::io::BufReader::new(stderr);
            let mut output = Vec::new();
            reader.read_to_end(&mut output)?;
            Ok(output)
        });
        let status = child.wait()?;
        let stdout = stdout_handle
            .join()
            .map_err(|_| anyhow::anyhow!("stdout reader thread panicked"))??;
        let stderr = stderr_handle
            .join()
            .map_err(|_| anyhow::anyhow!("stderr reader thread panicked"))??;
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
            if stderr.is_empty() {
                anyhow::bail!("command failed: {} {}", program, args.join(" "));
            }
            anyhow::bail!("command failed: {} {}: {}", program, args.join(" "), stderr);
        }
        let stdout = String::from_utf8_lossy(&stdout).trim().to_string();
        if self.debug && !stdout.is_empty() {
            let first_line = stdout.lines().next().unwrap_or("");
            eprintln!("{}", format!("[debug] -> {first_line}").dimmed());
        }
        Ok(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn run_capture_stderr_returns_hint_after_streaming_stderr() {
        let mut runner = ShellRunner::default();
        let opts = RunOptions {
            capture_stderr: true,
            ..RunOptions::default()
        };

        let error = runner
            .run(
                "sh",
                &["-c", "printf 'region blocked\n' >&2; exit 2"],
                None,
                &opts,
            )
            .unwrap_err();

        assert!(error.to_string().contains("see stderr above"));
    }

    #[cfg(unix)]
    #[test]
    fn capture_handles_large_stdout() {
        let mut runner = ShellRunner::default();

        let output = runner
            .capture("sh", &["-c", "yes x | head -c 200000"], None)
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
        // `-e KEY` (no `=`) means docker inherits from the host shell.
        // There's no value in argv to redact.
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
        // No -e / --env anywhere — args round-trip unchanged.
        let args = &["build", "-t", "image:tag", "--no-cache", "."];
        let redacted = redact_env_args(args);
        assert_eq!(
            redacted,
            vec!["build", "-t", "image:tag", "--no-cache", "."],
        );
    }

    #[test]
    fn redact_env_args_handles_empty_value() {
        // `-e KEY=` (empty value) should still mask, never pass through.
        let args = &["run", "-e", "EMPTY=", "image:tag"];
        let redacted = redact_env_args(args);
        assert_eq!(redacted, vec!["run", "-e", "EMPTY=<redacted>", "image:tag"]);
    }

    #[test]
    fn redact_env_args_handles_value_containing_equals() {
        // Values can contain `=` (e.g. URLs with query strings). Only the
        // first `=` separates KEY from VALUE; everything after the first
        // `=` is the value and must be redacted.
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
        // Defensive: malformed `-e` at end of args (no following arg).
        // Should not panic; the lone `-e` passes through.
        let args = &["run", "-e"];
        let redacted = redact_env_args(args);
        assert_eq!(redacted, vec!["run", "-e"]);
    }
}
