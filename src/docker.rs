use owo_colors::OwoColorize;
use std::path::Path;

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
        let output = Self::build_command(program, args, cwd).output()?;
        if !output.status.success() {
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
