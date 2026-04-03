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
pub struct ShellRunner;

impl ShellRunner {
    fn build_command(program: &str, args: &[&str], cwd: Option<&Path>) -> std::process::Command {
        let mut command = std::process::Command::new(program);
        command.args(args);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        command
    }
}

impl CommandRunner for ShellRunner {
    fn run(&mut self, program: &str, args: &[&str], cwd: Option<&Path>) -> anyhow::Result<()> {
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
        let output = Self::build_command(program, args, cwd).output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                anyhow::bail!("command failed: {} {}", program, args.join(" "));
            }
            anyhow::bail!("command failed: {} {}: {}", program, args.join(" "), stderr);
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
