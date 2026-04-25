use crate::docker::CommandRunner;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachOutcome {
    pub exit_code: Option<i32>,
    pub oom_killed: bool,
}

impl AttachOutcome {
    pub const fn still_running() -> Self {
        Self {
            exit_code: None,
            oom_killed: false,
        }
    }
    pub const fn stopped(code: i32) -> Self {
        Self {
            exit_code: Some(code),
            oom_killed: false,
        }
    }
    pub const fn oom_killed() -> Self {
        Self {
            exit_code: None,
            oom_killed: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeDecision {
    Preserved,
    Cleaned,
    ReturnToAgent,
}

pub trait FinalizerPrompt {
    fn ask_unsafe_cleanup(&mut self, container: &str, worktree_path: &str)
    -> anyhow::Result<usize>;
}

pub struct StdinPrompt;
impl FinalizerPrompt for StdinPrompt {
    fn ask_unsafe_cleanup(
        &mut self,
        container: &str,
        worktree_path: &str,
    ) -> anyhow::Result<usize> {
        let msg = format!(
            "Isolated worktree for {container} still has uncommitted changes:\n  {worktree_path}\n\nWhat do you want to do?"
        );
        crate::tui::prompt::prompt_choice(
            &msg,
            &[
                "Return to agent to address it",
                "Preserve worktree and exit",
                "Force delete worktree and discard changes",
            ],
        )
    }
}

pub fn finalize_foreground_session(
    container_name: &str,
    container_state_dir: &Path,
    outcome: AttachOutcome,
    is_interactive: bool,
    prompt: &mut impl FinalizerPrompt,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<FinalizeDecision> {
    if outcome.exit_code.is_none() || outcome.oom_killed || outcome.exit_code != Some(0) {
        return Ok(FinalizeDecision::Preserved);
    }
    finalize_clean_exit(
        container_name,
        container_state_dir,
        is_interactive,
        prompt,
        runner,
    )
}

#[allow(clippy::missing_const_for_fn, clippy::unnecessary_wraps)]
fn finalize_clean_exit(
    _container_name: &str,
    _container_state_dir: &Path,
    _is_interactive: bool,
    _prompt: &mut impl FinalizerPrompt,
    _runner: &mut impl CommandRunner,
) -> anyhow::Result<FinalizeDecision> {
    // Implemented in 7.2-7.4
    Ok(FinalizeDecision::Cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct NoPrompt;
    impl FinalizerPrompt for NoPrompt {
        fn ask_unsafe_cleanup(&mut self, _c: &str, _w: &str) -> anyhow::Result<usize> {
            panic!("prompt should not be called in this test");
        }
    }

    use crate::runtime::test_support::FakeRunner;

    #[test]
    fn still_running_preserves_records() {
        let dir = TempDir::new().unwrap();
        let mut p = NoPrompt;
        let mut r = FakeRunner::default();
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::still_running(),
            false,
            &mut p,
            &mut r,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Preserved);
    }

    #[test]
    fn stopped_non_zero_preserves_records() {
        let dir = TempDir::new().unwrap();
        let mut p = NoPrompt;
        let mut r = FakeRunner::default();
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(137),
            false,
            &mut p,
            &mut r,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Preserved);
    }

    #[test]
    fn oom_killed_preserves_records() {
        let dir = TempDir::new().unwrap();
        let mut p = NoPrompt;
        let mut r = FakeRunner::default();
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::oom_killed(),
            false,
            &mut p,
            &mut r,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Preserved);
    }
}
