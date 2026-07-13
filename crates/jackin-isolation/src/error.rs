//! Typed isolation materialize/cleanup/state errors.

use jackin_core::MountIsolation;
use std::path::PathBuf;

/// Failures from mount materialization, isolation state, and purge cleanup.
#[derive(Debug, thiserror::Error)]
pub enum IsolationError {
    #[error("isolated mount `{dst}` cannot be readonly (isolation = {isolation})")]
    ReadonlyIsolated {
        dst: String,
        isolation: MountIsolation,
    },
    #[error(
        "isolated mount `{dst}` overlaps sensitive path `{src}` ({reason}) (isolation = {isolation})"
    )]
    SensitiveOverlap {
        dst: String,
        src: String,
        reason: String,
        isolation: MountIsolation,
    },
    #[error("isolated mount `{dst}`: host repo `{src}` is mid-{marker}; resolve before launching")]
    MidOperation {
        dst: String,
        src: String,
        marker: String,
    },
    #[error("isolated mount `{dst}`: src `{src}` is inside repo `{toplevel}` but not its root")]
    NotRepoRoot {
        dst: String,
        src: String,
        toplevel: String,
    },
    #[error(
        "isolated mount `{dst}`: host tree at `{src}` is dirty (staged/unstaged/untracked); \
         pass --force to acknowledge, or commit/stash before launching"
    )]
    DirtyTree { dst: String, src: String },
    #[error("internal mount materialization error: missing mount slot")]
    MissingMountSlot,
    #[error(
        "source drift on container `{container}`, mount `{mount}`: recorded src `{recorded}` \
         differs from configured src `{configured}`; preserved {isolation} at `{worktree}`. \
         Restore the previous src, inspect the path above, or `jackin purge {container}` to discard."
    )]
    SourceDrift {
        container: String,
        mount: String,
        recorded: String,
        configured: String,
        isolation: MountIsolation,
        worktree: String,
    },
    #[error(
        "isolation mode drift on container `{container}`, mount `{mount}`: recorded mode \
         `{recorded}` differs from configured mode `{configured}`; preserved {recorded} at \
         `{worktree}`. Run `jackin purge {container}` to discard the old isolated state before \
         switching modes."
    )]
    ModeDrift {
        container: String,
        mount: String,
        recorded: MountIsolation,
        configured: MountIsolation,
        worktree: String,
    },
    #[error(
        "scratch branch `{branch}` still present after `git branch -D` on host repo `{repo}`; \
         record retained at `{state_dir}` so re-running `jackin purge` is possible after \
         resolving the underlying issue (branch may be checked out in another worktree, \
         or you may lack permission to delete it)."
    )]
    ScratchBranchRemains {
        branch: String,
        repo: String,
        state_dir: PathBuf,
    },
    #[error(
        "could not remove worktree directory `{path}`: {source}; \
         record retained at `{state_dir}` so re-running `jackin purge` is possible \
         after resolving the underlying issue (file in use, permission \
         denied, or filesystem error)."
    )]
    WorktreeRemove {
        path: String,
        state_dir: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "worktree directory `{path}` still present after cleanup; \
         record retained at `{state_dir}` so re-running `jackin purge` is possible."
    )]
    WorktreeStillPresent { path: String, state_dir: PathBuf },
    #[error(
        "could not remove clone directory `{path}`: {source}; record retained at `{state_dir}` \
         so re-running `jackin purge` is possible after resolving the underlying issue"
    )]
    CloneRemove {
        path: String,
        state_dir: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "clone directory `{path}` still present after cleanup; record retained at `{state_dir}` \
         so re-running `jackin purge` is possible."
    )]
    CloneStillPresent { path: String, state_dir: PathBuf },
    #[error(
        "purge of isolated mounts had {n} failure(s): {list}; \
         record(s) retained at `{state_dir}` so re-running `jackin purge` is possible \
         after resolving the underlying issue(s) (see warnings above for details)"
    )]
    PurgePartialFailure {
        n: usize,
        list: String,
        state_dir: PathBuf,
    },
    #[error("unsupported isolation.json version {got} at {path}; expected {expected}")]
    UnsupportedStateVersion {
        got: u32,
        path: PathBuf,
        expected: u32,
    },
}
