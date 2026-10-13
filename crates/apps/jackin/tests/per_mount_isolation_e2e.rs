#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]
mod common;

use jackin::workspace::{MountConfig, ResolvedWorkspace};
use jackin_core::MountIsolation;
use jackin_docker::{CommandRunner, RunOptions};
use jackin_runtime::isolation::finalize::{
    AttachOutcome, ExitDialogChoice, FinalizeContext, FinalizeDecision, FinalizerPrompt,
    PreservedReason, finalize_foreground_session,
};
use jackin_runtime::isolation::materialize::{PreflightContext, materialize_workspace};
use jackin_runtime::isolation::state::IsolationRecord;
use jackin_runtime::isolation::state::{CleanupStatus, read_records};
use std::collections::VecDeque;
use std::path::Path;
use tempfile::TempDir;

struct NoPrompt;
impl FinalizerPrompt for NoPrompt {
    fn ask_exit_dialog(
        &mut self,
        _c: &str,
        _records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice> {
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
            capture_queue: outputs.iter().map(|s| (*s).to_owned()).collect(),
            run_recorded: Vec::new(),
        }
    }
}

impl CommandRunner for ScriptedRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<()> {
        std::future::ready(()).await;
        self.run_recorded
            .push(format!("{program} {}", args.join(" ")));
        Ok(())
    }

    async fn capture(
        &mut self,
        _program: &str,
        _args: &[&str],
        _cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        std::future::ready(()).await;
        Ok(self.capture_queue.pop_front().unwrap_or_default())
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.capture(program, args, cwd).await
    }

    async fn capture_with_options(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<String> {
        // Scripted outputs are pre-pinned by construction; descriptor pinning
        // is a no-op for the queue.
        self.capture(program, args, cwd).await
    }
}

/// Turn the fixture repo into a real repository with a real worktree plus
/// registration for `branch`, returning the branch tip. Journaled cleanup
/// pins these host structures, so the e2e exercises the real flow.
#[expect(
    clippy::disallowed_methods,
    reason = "test fixture shells real git to build journaled-cleanup structures"
)]
fn init_repo_with_scratch_worktree(
    repo: &Path,
    worktree: &Path,
    branch: &str,
) -> anyhow::Result<String> {
    let git = |args: &[&str]| -> anyhow::Result<()> {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()?;
        anyhow::ensure!(status.success(), "git {args:?} failed");
        Ok(())
    };
    git(&["init", "--initial-branch=main"])?;
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "--allow-empty",
        "-m",
        "fixture",
    ])?;
    let worktree = worktree
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("worktree path is not UTF-8"))?;
    git(&["worktree", "add", "-b", branch, worktree])?;
    let tip_output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", branch])
        .output()?;
    anyhow::ensure!(tip_output.status.success(), "git rev-parse failed");
    let tip = String::from_utf8(tip_output.stdout)?;
    let tip = tip.trim().to_owned();
    anyhow::ensure!(tip.len() == 40, "scratch tip must be a full object id");
    Ok(tip)
}

#[tokio::test]
async fn materialize_then_clean_exit_removes_record_and_branch() {
    let repo = TempDir::new().unwrap();
    std::fs::create_dir_all(repo.path().join(".git")).unwrap();
    // Journaled cleanup pins and validates the host HEAD on disk.
    std::fs::write(repo.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    let data = TempDir::new().unwrap();
    let cdir = data.path().join("jackin-the-architect");
    std::fs::create_dir_all(&cdir).unwrap();

    let resolved = ResolvedWorkspace {
        name: String::new(),
        label: "jackin".into(),
        workdir: "/workspace/jackin".into(),
        mounts: vec![MountConfig {
            src: repo.path().to_string_lossy().into(),
            dst: "/workspace/jackin".into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    };

    // materialize_workspace capture queue:
    //   rev-parse --show-toplevel (preflight)
    //   status --porcelain (clean)
    //   ext.worktreeConfig --get
    //   format --get
    //   rev-parse HEAD
    let mut runner =
        ScriptedRunner::new(&[&repo.path().to_string_lossy(), "", "", "0", "deadbeef\n"]);
    let mat = materialize_workspace(
        &resolved,
        &cdir,
        "the-architect",
        "jackin-the-architect",
        Some(&jackin_core::WorkspaceName::parse("jackin").unwrap()),
        &PreflightContext {
            workspace_label: jackin_core::WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();

    let recs = read_records(&cdir).unwrap();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].cleanup_status, CleanupStatus::Active);

    // Override files were written alongside the materialized worktree
    // and the MaterializedMount carries the auxiliary mount metadata
    // for the three extra bind mounts (host .git/, .git pointer
    // override, gitdir back-pointer override). No commondir override:
    // the admin entry lives natively inside the host .git/ mount, so
    // git's on-disk default `commondir = ../..` resolves correctly.
    let m = &mat.mounts[0];
    let aux = m
        .worktree_aux
        .as_ref()
        .expect("worktree mount must carry aux mount metadata");

    // Container-side targets all live under a single /jackin/host/<dst-tree>/ root.
    assert_eq!(
        aux.host_git_target, "/jackin/host/workspace/jackin/.git",
        "host .git mount mirrors host topology and ends in .git",
    );
    assert_eq!(aux.git_file_target, "/workspace/jackin/.git");
    assert_eq!(
        aux.gitdir_back_target,
        "/jackin/host/workspace/jackin/.git/worktrees/jackin-the-architect/gitdir",
        "gitdir back-pointer override lives natively inside the host .git/ mount",
    );
    assert_eq!(aux.host_git_dir, format!("{}/.git", repo.path().display()));

    // Override file contents.
    let git_file_content = std::fs::read_to_string(&aux.git_file_override).unwrap();
    assert_eq!(
        git_file_content,
        "gitdir: /jackin/host/workspace/jackin/.git/worktrees/jackin-the-architect\n",
        "replacement .git pointer redirects gitdir to the admin entry inside the host .git/ mount",
    );
    let gitdir_back_content = std::fs::read_to_string(&aux.gitdir_back_override).unwrap();
    assert_eq!(
        gitdir_back_content, "/workspace/jackin/.git\n",
        "back-pointer matches the worktree's <dst>/.git location inside the container",
    );

    // Host layout: worktree under <state>/git/worktree/repo/<dst-tree>/<container>/,
    // overrides under <state>/git/overrides/<dst-tree>/. The fake
    // runner doesn't actually run `git worktree add` so the worktree
    // subdir itself isn't materialized; assert via the recorded
    // `bind_src` instead. Override files DO land on disk because
    // `write_git_overrides` writes them via std::fs.
    assert!(
        m.bind_src
            .ends_with("/git/worktree/repo/workspace/jackin/jackin-the-architect"),
        "worktree subdir basename = container name; got {}",
        m.bind_src
    );
    let overrides_dir = cdir.join("git/overrides/workspace/jackin");
    assert!(overrides_dir.is_dir());
    assert!(overrides_dir.join(".git").is_file());
    assert!(overrides_dir.join("gitdir").is_file());
    assert!(
        !overrides_dir.join("commondir").exists(),
        "commondir override removed in V1 final design",
    );

    // Finalize a clean exit. Capture queue: status --porcelain (clean),
    // for-each-ref refs/heads/ (single scratch branch parked at base).
    let branches = "jackin/scratch/jackin-the-architect\tdeadbeef\t\t\n";
    let wt_path = cdir.join("git/worktree/repo/workspace/jackin/jackin-the-architect");
    let tip = init_repo_with_scratch_worktree(
        repo.path(),
        &wt_path,
        "jackin/scratch/jackin-the-architect",
    )
    .unwrap();
    let full_ref = format!("refs/heads/jackin/scratch/jackin-the-architect\t{tip}\t\tEND\n");

    // Capture queue: status (clean), short for-each-ref (scratch parked at
    // base), symbolic-ref (attached), then the journaled inventory: full
    // for-each-ref, ref format, reinspect, and three post-deletion absence
    // proofs.
    let mut finalize_runner =
        ScriptedRunner::new(&["", branches, "", &full_ref, "files", &full_ref, "", "", ""]);
    let mut prompt = NoPrompt;
    let docker = common::FakeDockerClient::default();
    let dec = finalize_foreground_session(FinalizeContext {
        container_name: "jackin-the-architect",
        container_state_dir: &cdir,
        outcome: AttachOutcome::stopped(0),
        is_interactive: false,
        dirty_exit_policy: jackin::workspace::DirtyExitPolicy::Ask,
        prompt: &mut prompt,
        docker: &docker,
        runner: &mut finalize_runner,
        container: jackin_core::ContainerHandle::new("jackin-the-architect", "test-finalizer-id")
            .unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(dec, FinalizeDecision::Cleaned);
    assert!(read_records(&cdir).unwrap().is_empty());
    // Journaled cleanup removes via pinned descriptors, not CLI: prove the
    // worktree, its registration, and the scratch ref are gone from disk.
    assert!(!wt_path.exists(), "isolated worktree removed");
    assert!(
        !repo
            .path()
            .join(".git/refs/heads/jackin/scratch/jackin-the-architect")
            .exists(),
        "scratch branch deleted"
    );
    let worktrees_dir = repo.path().join(".git/worktrees");
    assert!(
        !worktrees_dir.exists() || std::fs::read_dir(&worktrees_dir).unwrap().next().is_none(),
        "worktree registration removed"
    );
}
