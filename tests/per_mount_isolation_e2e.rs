use jackin::docker::{CommandRunner, RunOptions};
use jackin::isolation::MountIsolation;
use jackin::isolation::finalize::{
    AttachOutcome, FinalizeDecision, FinalizerPrompt, finalize_foreground_session,
};
use jackin::isolation::materialize::{PreflightContext, materialize_workspace};
use jackin::isolation::state::{CleanupStatus, read_records};
use jackin::workspace::{MountConfig, ResolvedWorkspace};
use std::collections::VecDeque;
use std::path::Path;
use tempfile::TempDir;

struct NoPrompt;
impl FinalizerPrompt for NoPrompt {
    fn ask_unsafe_cleanup(&mut self, _c: &str, _w: &str) -> anyhow::Result<usize> {
        panic!("prompt should not be called");
    }
}

struct ScriptedRunner {
    capture_queue: VecDeque<String>,
    run_recorded: Vec<String>,
}

impl ScriptedRunner {
    fn new(outputs: &[&str]) -> Self {
        Self {
            capture_queue: outputs.iter().map(|s| (*s).to_string()).collect(),
            run_recorded: Vec::new(),
        }
    }
}

impl CommandRunner for ScriptedRunner {
    fn run(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<()> {
        self.run_recorded.push(format!("{program} {}", args.join(" ")));
        Ok(())
    }

    fn capture(
        &mut self,
        _program: &str,
        _args: &[&str],
        _cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        Ok(self.capture_queue.pop_front().unwrap_or_default())
    }
}

#[test]
fn materialize_then_clean_exit_removes_record_and_branch() {
    let repo = TempDir::new().unwrap();
    std::fs::create_dir_all(repo.path().join(".git")).unwrap();
    let data = TempDir::new().unwrap();
    let cdir = data.path().join("jackin-the-architect");
    std::fs::create_dir_all(&cdir).unwrap();

    let resolved = ResolvedWorkspace {
        label: "jackin".into(),
        workdir: "/workspace/jackin".into(),
        mounts: vec![MountConfig {
            src: repo.path().to_string_lossy().into(),
            dst: "/workspace/jackin".into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        }],
    };

    // materialize_workspace capture queue:
    //   rev-parse --show-toplevel (preflight)
    //   status --porcelain (clean)
    //   ext.worktreeConfig --get
    //   format --get
    //   rev-parse HEAD
    let mut runner = ScriptedRunner::new(&[
        &repo.path().to_string_lossy(),
        "",
        "",
        "0",
        "deadbeef\n",
    ]);
    let _mat = materialize_workspace(
        &resolved,
        &cdir,
        "the-architect",
        "jackin-the-architect",
        "jackin",
        &PreflightContext {
            workspace_name: "jackin".into(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .unwrap();

    let recs = read_records(&cdir).unwrap();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].cleanup_status, CleanupStatus::Active);

    // Now finalize a clean exit with HEAD == base.
    // Capture queue: status --porcelain (clean), rev-parse HEAD (== base)
    let mut finalize_runner = ScriptedRunner::new(&["", "deadbeef\n"]);
    let mut prompt = NoPrompt;
    let dec = finalize_foreground_session(
        "jackin-the-architect",
        &cdir,
        AttachOutcome::stopped(0),
        false,
        &mut prompt,
        &mut finalize_runner,
    )
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(&cdir).unwrap().is_empty());
    assert!(
        finalize_runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree remove --force"))
    );
    assert!(
        finalize_runner
            .run_recorded
            .iter()
            .any(|c| c.contains("branch -D"))
    );
}
