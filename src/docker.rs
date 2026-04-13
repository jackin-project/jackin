use owo_colors::OwoColorize;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

/// Default timeout for commands that capture output (git, docker inspect, etc.).
const DEFAULT_CAPTURE_TIMEOUT: Duration = Duration::from_secs(120);

/// Timeout for long-running commands such as `docker build`.
pub const DOCKER_BUILD_TIMEOUT: Duration = Duration::from_secs(900);

pub trait CommandRunner {
    fn run(&mut self, program: &str, args: &[&str], cwd: Option<&Path>) -> anyhow::Result<()>;
    fn run_capture_stderr(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<()> {
        self.run(program, args, cwd)
    }
    /// Like [`run_capture_stderr`](Self::run_capture_stderr) but with an
    /// explicit timeout. The default implementation ignores the timeout and
    /// delegates to `run_capture_stderr`.
    fn run_capture_stderr_with_timeout(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        _timeout: Duration,
    ) -> anyhow::Result<()> {
        self.run_capture_stderr(program, args, cwd)
    }
    fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String>;
}

#[derive(Default)]
pub struct ShellRunner {
    pub debug: bool,
    /// Override the default timeout for child-process calls. `None` uses
    /// [`DEFAULT_CAPTURE_TIMEOUT`].
    pub capture_timeout: Option<Duration>,
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
            let cmd = format!("{} {}", program, args.join(" "));
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

    fn wait_with_timeout(
        child: &mut std::process::Child,
        timeout: Duration,
        program: &str,
        args: &[&str],
    ) -> anyhow::Result<std::process::ExitStatus> {
        let start = std::time::Instant::now();
        loop {
            match child.try_wait()? {
                Some(status) => return Ok(status),
                None if start.elapsed() >= timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!(
                        "command timed out after {}s: {} {}",
                        timeout.as_secs(),
                        program,
                        args.join(" ")
                    );
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }
}

impl CommandRunner for ShellRunner {
    fn run(&mut self, program: &str, args: &[&str], cwd: Option<&Path>) -> anyhow::Result<()> {
        self.log_command(program, args, cwd);
        let timeout = self.capture_timeout.unwrap_or(DEFAULT_CAPTURE_TIMEOUT);
        let mut child = Self::build_command(program, args, cwd).spawn()?;
        let status = Self::wait_with_timeout(&mut child, timeout, program, args)?;
        anyhow::ensure!(
            status.success(),
            "command failed: {} {}",
            program,
            args.join(" ")
        );
        Ok(())
    }

    fn run_capture_stderr(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<()> {
        self.log_command(program, args, cwd);
        let timeout = self.capture_timeout.unwrap_or(DEFAULT_CAPTURE_TIMEOUT);
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
        let status = Self::wait_with_timeout(&mut child, timeout, program, args)?;
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
        Ok(())
    }

    fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.log_command(program, args, cwd);
        let timeout = self.capture_timeout.unwrap_or(DEFAULT_CAPTURE_TIMEOUT);
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
        let status = Self::wait_with_timeout(&mut child, timeout, program, args)?;
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

    fn run_capture_stderr_with_timeout(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        self.log_command(program, args, cwd);
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
        let status = Self::wait_with_timeout(&mut child, timeout, program, args)?;
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn run_capture_stderr_returns_hint_after_streaming_stderr() {
        let mut runner = ShellRunner::default();

        let error = runner
            .run_capture_stderr("sh", &["-c", "printf 'region blocked\n' >&2; exit 2"], None)
            .unwrap_err();

        assert!(error.to_string().contains("see stderr above"));
    }

    #[cfg(unix)]
    #[test]
    fn capture_handles_large_stdout_without_timing_out() {
        let mut runner = ShellRunner {
            capture_timeout: Some(Duration::from_secs(2)),
            ..Default::default()
        };

        let output = runner
            .capture("sh", &["-c", "yes x | head -c 200000"], None)
            .unwrap();

        assert!(output.len() >= 190000);
        assert!(output.starts_with('x'));
    }

    #[cfg(unix)]
    #[test]
    fn run_respects_timeout() {
        let mut runner = ShellRunner {
            capture_timeout: Some(Duration::from_millis(50)),
            ..Default::default()
        };

        let error = runner.run("sh", &["-c", "sleep 1"], None).unwrap_err();

        assert!(error.to_string().contains("command timed out after"));
    }
}
