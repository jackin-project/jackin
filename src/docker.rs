use owo_colors::OwoColorize;
use std::path::Path;
use std::time::Duration;

/// Default timeout for commands that capture output (git, docker inspect, etc.).
const DEFAULT_CAPTURE_TIMEOUT: Duration = Duration::from_secs(120);

pub trait CommandRunner {
    fn run(&mut self, program: &str, args: &[&str], cwd: Option<&Path>) -> anyhow::Result<()>;
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
    /// Override the default timeout for `capture` calls. `None` uses
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
                eprintln!("{}", format!("[debug] cd {} && {}", dir.display(), cmd).dimmed());
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
        let status = Self::build_command(program, args, cwd).status()?;
        anyhow::ensure!(
            status.success(),
            "command failed: {} {}",
            program,
            args.join(" ")
        );
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
        let status = Self::wait_with_timeout(&mut child, timeout, program, args)?;
        let output = child.wait_with_output()?;
        if !status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                anyhow::bail!("command failed: {} {}", program, args.join(" "));
            }
            anyhow::bail!("command failed: {} {}: {}", program, args.join(" "), stderr);
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if self.debug && !stdout.is_empty() {
            let first_line = stdout.lines().next().unwrap_or("");
            eprintln!("{}", format!("[debug] -> {first_line}").dimmed());
        }
        Ok(stdout)
    }
}
