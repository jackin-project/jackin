//! `FakeRunner`: an in-memory `jackin_core::CommandRunner` fake for subprocess
//! injection in tests.

use jackin_core::{CommandRunner, RunOptions};
use std::collections::VecDeque;

#[expect(
    missing_debug_implementations,
    reason = "FakeRunner stores one-shot side-effect closures that cannot be formatted."
)]
#[derive(Default)]
pub struct FakeRunner {
    pub recorded: Vec<String>,
    pub run_recorded: Vec<String>,
    pub run_options: Vec<RunOptions>,
    pub fail_on: Vec<String>,
    pub fail_with: Vec<(String, String)>,
    pub capture_queue: VecDeque<String>,
    pub side_effects: Vec<(String, Box<dyn FnOnce()>)>,
    /// Optional test-only observation hook invoked for every command.
    pub command_hook: Option<fn(&str)>,
}

impl FakeRunner {
    pub fn with_capture_queue<const N: usize>(outputs: [String; N]) -> Self {
        Self {
            capture_queue: VecDeque::from(outputs),
            ..Default::default()
        }
    }

    /// Number of capture calls `load_role` makes before reaching role-
    /// specific logic: 1 identity lookup (`git config --get-regexp ...`).
    /// GC now uses `DockerApi`, not `CommandRunner`, so it no longer counts.
    const LOAD_PREAMBLE_CAPTURES: usize = 1;

    pub fn for_load_agent<const N: usize>(outputs: [String; N]) -> Self {
        let mut queue = VecDeque::with_capacity(Self::LOAD_PREAMBLE_CAPTURES + N);
        for _ in 0..Self::LOAD_PREAMBLE_CAPTURES {
            queue.push_back(String::new());
        }
        queue.extend(outputs);
        Self {
            capture_queue: queue,
            ..Default::default()
        }
    }
}

impl FakeRunner {
    fn check_command(&mut self, command: &str) -> anyhow::Result<()> {
        if let Some(hook) = self.command_hook {
            hook(command);
        }
        if let Some((_, message)) = self
            .fail_with
            .iter()
            .find(|(pattern, _)| command.contains(pattern))
        {
            let message = message.clone();
            anyhow::bail!("{message}");
        }
        if self.fail_on.iter().any(|pattern| command.contains(pattern)) {
            anyhow::bail!("command failed: {command}");
        }
        if let Some(pos) = self
            .side_effects
            .iter()
            .position(|(pattern, _)| command.contains(pattern))
        {
            let (_, callback) = self.side_effects.remove(pos);
            callback();
        }
        Ok(())
    }
}

impl CommandRunner for FakeRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&std::path::Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()> {
        let command = format!("{} {}", program, args.join(" "));
        self.run_options.push(opts.clone());
        self.run_recorded.push(command.clone());
        self.recorded.push(command.clone());
        self.check_command(&command)
    }

    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<String> {
        let command = format!("{} {}", program, args.join(" "));
        self.recorded.push(command.clone());
        self.check_command(&command)?;
        // Empty queue returns "" — safe for most captures (git SHA, id outputs), but
        // dangerous for assess_cleanup captures: `rev-list` returning "" maps to
        // "0 commits ahead, safe to delete" and `symbolic-ref HEAD` returning ""
        // silently skips the detached-HEAD guard. Pre-fill the queue in tests that
        // exercise those code paths.
        Ok(self.capture_queue.pop_front().unwrap_or_default())
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<String> {
        self.capture(program, args, cwd).await
    }
}
