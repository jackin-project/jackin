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
        self.run_recorded
            .push(format!("{program} {}", args.join(" ")));
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
    let mut runner =
        ScriptedRunner::new(&[&repo.path().to_string_lossy(), "", "", "0", "deadbeef\n"]);
    let mat = materialize_workspace(
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

    // Override files were written alongside the materialized worktree
    // and the MaterializedMount carries the auxiliary mount metadata
    // for all five extra bind mounts.
    let m = &mat.mounts[0];
    let aux = m
        .worktree_aux
        .as_ref()
        .expect("worktree mount must carry aux mount metadata");

    // Container-side targets follow the new /jackin/{host,admin}/<dst-stripped>/ scheme.
    assert_eq!(
        aux.host_git_target, "/jackin/host/workspace/jackin/.git",
        "host .git mount mirrors host topology and ends in .git",
    );
    assert_eq!(
        aux.admin_target, "/jackin/admin/workspace/jackin",
        "admin mount lives at /jackin/admin/, separate from /jackin/host/",
    );
    assert_eq!(aux.git_file_target, "/workspace/jackin/.git");
    assert_eq!(
        aux.commondir_target, "/jackin/admin/workspace/jackin/commondir",
        "commondir override lives under /jackin/admin/, not nested in /jackin/host/",
    );
    assert_eq!(
        aux.gitdir_back_target, "/jackin/admin/workspace/jackin/gitdir",
        "gitdir back-pointer override lives under /jackin/admin/, not nested in /jackin/host/",
    );
    assert_eq!(aux.host_git_dir, format!("{}/.git", repo.path().display()));
    assert_eq!(
        aux.admin_dir,
        format!(
            "{}/.git/worktrees/jackin-the-architect",
            repo.path().display()
        ),
        "admin entry name = container name → globally unique per (host_repo, container)",
    );

    // Override file contents.
    let git_file_content = std::fs::read_to_string(&aux.git_file_override).unwrap();
    assert_eq!(
        git_file_content, "gitdir: /jackin/admin/workspace/jackin\n",
        "replacement .git pointer redirects gitdir to /jackin/admin/, away from /jackin/host/",
    );
    let commondir_content = std::fs::read_to_string(&aux.commondir_override).unwrap();
    assert_eq!(
        commondir_content, "/jackin/host/workspace/jackin/.git\n",
        "commondir override resolves the shared .git mount at /jackin/host/.../.git",
    );
    let gitdir_back_content = std::fs::read_to_string(&aux.gitdir_back_override).unwrap();
    assert_eq!(
        gitdir_back_content, "/workspace/jackin/.git\n",
        "back-pointer matches the worktree's <dst>/.git location inside the container",
    );

    // Host layout: all git artifacts grouped under <state>/git/<dst-tree>/.
    // The fake runner doesn't actually run `git worktree add` so the
    // worktree subdir itself isn't materialized; assert via the
    // recorded `bind_src` instead. Override files DO land on disk
    // because `write_git_overrides` writes them via std::fs.
    let mount_root = cdir.join("git/workspace/jackin");
    assert!(
        mount_root.is_dir(),
        "mount root created at <state>/git/<dst-tree>/"
    );
    assert!(
        m.bind_src
            .ends_with("/git/workspace/jackin/jackin-the-architect"),
        "worktree subdir basename = container name; got {}",
        m.bind_src
    );
    let overrides_dir = mount_root.join("overrides");
    assert!(overrides_dir.is_dir());
    assert!(overrides_dir.join(".git").is_file());
    assert!(overrides_dir.join("commondir").is_file());
    assert!(overrides_dir.join("gitdir").is_file());

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
