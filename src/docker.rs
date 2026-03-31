use std::path::Path;

pub trait CommandRunner {
    fn run(&mut self, program: &str, args: &[String], cwd: Option<&Path>) -> anyhow::Result<()>;
    fn capture(
        &mut self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String>;
}

#[derive(Default)]
pub struct ShellRunner;

impl CommandRunner for ShellRunner {
    fn run(&mut self, program: &str, args: &[String], cwd: Option<&Path>) -> anyhow::Result<()> {
        let mut command = std::process::Command::new(program);
        command.args(args);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        let status = command.status()?;
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
        args: &[String],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        let mut command = std::process::Command::new(program);
        command.args(args);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        let output = command.output()?;
        anyhow::ensure!(
            output.status.success(),
            "command failed: {} {}",
            program,
            args.join(" ")
        );
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
