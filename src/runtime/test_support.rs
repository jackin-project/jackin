use crate::docker::{CommandRunner, RunOptions};
use std::collections::VecDeque;

#[derive(Default)]
pub struct FakeRunner {
    pub recorded: Vec<String>,
    pub run_recorded: Vec<String>,
    pub fail_on: Vec<String>,
    pub fail_with: Vec<(String, String)>,
    pub capture_queue: VecDeque<String>,
    /// Optional callbacks keyed by a substring of the command.  When a
    /// captured command matches the key, the callback is invoked before the
    /// output is returned.  This is useful for simulating filesystem
    /// side-effects (e.g. `git clone` creating repo files on disk).
    pub side_effects: Vec<(String, Box<dyn FnOnce()>)>,
}

impl FakeRunner {
    pub(super) fn with_capture_queue<const N: usize>(outputs: [String; N]) -> Self {
        Self {
            capture_queue: VecDeque::from(outputs),
            ..Default::default()
        }
    }

    /// Number of capture calls `load_role` makes before reaching role-
    /// specific logic: 2 GC queries (orphaned `DinD` scan + orphaned network
    /// scan) + 4 identity lookups (`git config user.name`, `git config
    /// user.email`, `id -u`, `id -g`).
    const LOAD_PREAMBLE_CAPTURES: usize = 6;

    /// Prefixes the capture queue with empty responses for the `load_role`
    /// preamble queries so tests can focus on the role-specific output.
    pub(super) fn for_load_agent<const N: usize>(outputs: [String; N]) -> Self {
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
    fn run(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&std::path::Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<()> {
        let command = format!("{} {}", program, args.join(" "));
        self.run_recorded.push(command.clone());
        self.recorded.push(command.clone());
        self.check_command(&command)
    }

    fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<String> {
        let command = format!("{} {}", program, args.join(" "));
        self.recorded.push(command.clone());
        self.check_command(&command)?;
        // `unwrap_or_default()` returns `Ok("")` when the queue is exhausted.
        // Two commands in `assess_cleanup` are dangerous when they silently
        // receive a phantom `Ok("")`:
        //   - `rev-list`: empty output = "no commits ahead" → SafeToDelete
        //   - `symbolic-ref HEAD`: any Ok (including "") = "HEAD on a branch",
        //     silently skipping the detached-HEAD guard
        // Always provide one queue entry per expected capture call and
        // document each in a comment above the `fake_with_outputs` call.
        Ok(self.capture_queue.pop_front().unwrap_or_default())
    }
}
