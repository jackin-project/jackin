// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Git repo context inside the container: branch, ahead/behind counts, dirty
//! state, and PR metadata for the status bar.
//!
//! Not responsible for: rendering the status bar (see `tui`) or host-side git
//! operations.
//!
//! Key invariant: all `git` and `gh` calls are bounded by
//! `GIT_CONTEXT_COMMAND_TIMEOUT` / `GH_PULL_REQUEST_COMMAND_TIMEOUT` so a
//! slow repo cannot stall the daemon tick.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime};

#[cfg(target_os = "linux")]
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use tokio::sync::mpsc;

use crate::session::{BranchName, GitContext, Oid, SessionEvent};
use crate::util::{WaitOutcome, command_stdout_trimmed_with_timeout, wait_child_with_timeout};

pub(crate) const GIT_CONTEXT_COMMAND_TIMEOUT: Duration = Duration::from_millis(1500);
pub(crate) const GH_PULL_REQUEST_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);

/// One-shot resolution of workdir + tool facts. `gh_available` may
/// flip from false to true when a background PR lookup succeeds (so a
/// startup PATH race doesn't freeze the feature for the daemon
/// lifetime); the other fields are never re-probed.
pub(crate) struct WorkdirContext {
    pub(crate) is_git_repo: bool,
    pub(crate) git_available: bool,
    pub(crate) gh_available: bool,
    /// `origin/HEAD` resolved to a short branch name (`main`, `master`,
    /// `trunk`, `develop`, …). `None` when the workdir is not a git
    /// checkout, when `origin/HEAD` is not set, or when `git
    /// symbolic-ref` fails. Falls back to a `main`/`master` literal
    /// match for branches that look like defaults when this is `None`.
    pub(crate) default_branch: Option<String>,
}

impl WorkdirContext {
    pub(crate) fn resolve(workdir: &Path) -> Self {
        let git_available = command_in_path("git");
        let gh_available = command_in_path("gh");
        // `.git` may be a regular directory (normal checkout) or a
        // file containing `gitdir: …` (worktree / submodule).
        // `try_exists` covers both. Keep this independent of the
        // startup `git --version` probe: the hot branch path can read
        // `.git/HEAD` directly, so a normal checkout can still update
        // chrome even if the subprocess probe fails or runs before the
        // shell has expanded PATH.
        let git_metadata = workdir.join(".git");
        let has_git_metadata = match git_metadata.try_exists() {
            Ok(present) => present,
            Err(_error) => {
                record_recovered_degradation();
                false
            }
        };
        let is_git_repo =
            has_git_metadata || (git_available && workdir_is_inside_git_tree(workdir));
        let default_branch = if is_git_repo {
            resolve_default_branch(workdir)
        } else {
            None
        };
        Self {
            is_git_repo,
            git_available,
            gh_available,
            default_branch,
        }
    }

    /// True when `branch` is the repo's default branch (the chrome bar
    /// stays hidden in that case). Falls back to a literal
    /// `main`/`master`/empty match when `origin/HEAD` is not set so
    /// freshly-cloned-but-not-`gh-repo-set-default`ed repos still
    /// suppress the bar.
    pub(crate) fn is_default_branch(&self, branch: &str) -> bool {
        if branch.is_empty() {
            return true;
        }
        if let Some(default) = self.default_branch.as_deref() {
            return branch == default;
        }
        matches!(branch, "main" | "master")
    }
}

/// Probe `name --version` once at construction. Stdin/stdout/stderr
/// are nulled so a misbehaving subprocess cannot leak output into the
/// daemon's logs and cannot block on stdin. As PID 1, Capsule has a
/// SIGCHLD zombie reaper that can win the race against Rust's
/// `Child::try_wait`; `ECHILD` after a successful spawn still proves
/// the executable exists, so treat it as available instead of freezing
/// the feature off for the daemon lifetime.
pub(crate) fn command_in_path(name: &str) -> bool {
    let request = jackin_process::ExecRequest::new(name, ["--version"])
        .stdout_mode(jackin_process::StdioMode::Null)
        .stderr_mode(jackin_process::StdioMode::Null);
    let Ok((operation, mut child)) = crate::process_telemetry::spawn_sync(&request) else {
        return false;
    };
    match wait_child_with_timeout(&mut child, GIT_CONTEXT_COMMAND_TIMEOUT) {
        WaitOutcome::Exited(status) if status.success() => {
            operation.complete_status(status, &[0]);
            true
        }
        WaitOutcome::Exited(status) => {
            operation.complete_status(status, &[0]);
            false
        }
        WaitOutcome::Reaped => {
            operation.complete_reaped();
            true
        }
        WaitOutcome::Failed => {
            operation.complete_io_failure();
            false
        }
        WaitOutcome::TimedOut => {
            operation.complete_timeout();
            false
        }
    }
}

/// Bounded by `GIT_CONTEXT_COMMAND_TIMEOUT` so a stalled `git`
/// subprocess against a network-mounted `.git` cannot block the daemon.
pub(crate) fn git_capture_at_workdir(workdir: &Path, args: &[&str]) -> Option<String> {
    let command_args = std::iter::once(OsStr::new("-C"))
        .chain(std::iter::once(workdir.as_os_str()))
        .chain(args.iter().map(OsStr::new));
    let request = jackin_process::ExecRequest::new("git", command_args)
        .stdout_mode(jackin_process::StdioMode::Capture)
        .stderr_mode(jackin_process::StdioMode::Null);
    command_stdout_trimmed_with_timeout(&request, GIT_CONTEXT_COMMAND_TIMEOUT)
}

/// `git symbolic-ref --short refs/remotes/origin/HEAD` returns
/// `origin/main` (or whatever the default branch is). Strip the
/// `origin/` so it can compare directly against `git branch
/// --show-current` output.
pub(crate) fn resolve_default_branch(workdir: &Path) -> Option<String> {
    let raw = git_capture_at_workdir(
        workdir,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )?;
    raw.strip_prefix("origin/").map(ToOwned::to_owned)
}

pub(crate) fn workdir_is_inside_git_tree(workdir: &Path) -> bool {
    git_capture_at_workdir(workdir, &["rev-parse", "--is-inside-work-tree"])
        .is_some_and(|value| value == "true")
}

#[cfg(target_os = "linux")]
const GIT_CONTEXT_WATCH_MASK: AddWatchFlags = AddWatchFlags::IN_CLOSE_WRITE
    .union(AddWatchFlags::IN_MOVED_TO)
    .union(AddWatchFlags::IN_CREATE)
    .union(AddWatchFlags::IN_ATTRIB)
    .union(AddWatchFlags::IN_DELETE_SELF)
    .union(AddWatchFlags::IN_MOVE_SELF);

#[cfg(target_os = "linux")]
pub(crate) fn start_git_context_watcher(
    workdir: PathBuf,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
) {
    let Some(git_dir) = git_dir_for_watch(&workdir) else {
        return;
    };
    if let Err(_error) =
        jackin_telemetry::spawn::thread_stream_named("git-context-watch".to_owned(), move || {
            watch_git_head_changes(git_dir, event_tx);
        })
    {
        record_recovered_degradation();
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn start_git_context_watcher(
    _workdir: PathBuf,
    _event_tx: mpsc::UnboundedSender<SessionEvent>,
) {
}

#[cfg(target_os = "linux")]
fn git_dir_for_watch(workdir: &Path) -> Option<PathBuf> {
    git_metadata_dirs(workdir)
        .map(|metadata| metadata.git_dir)
        .or_else(|| {
            let raw = git_capture_at_workdir(workdir, &["rev-parse", "--git-dir"])?;
            let path = PathBuf::from(raw);
            Some(if path.is_absolute() {
                path
            } else {
                workdir.join(path)
            })
        })
}

#[cfg(target_os = "linux")]
fn watch_git_head_changes(git_dir: PathBuf, event_tx: mpsc::UnboundedSender<SessionEvent>) {
    let open =
        jackin_telemetry::stream::phase(jackin_telemetry::schema::enums::StreamOperation::Open);
    let instance = match Inotify::init(InitFlags::IN_CLOEXEC) {
        Ok(instance) => instance,
        Err(_error) => {
            record_io_error();
            jackin_telemetry::stream::complete_error(
                open,
                jackin_telemetry::schema::enums::ErrorType::IoError,
            );
            return;
        }
    };
    if let Err(_error) = instance.add_watch(git_dir.as_path(), GIT_CONTEXT_WATCH_MASK) {
        record_io_error();
        jackin_telemetry::stream::complete_error(
            open,
            jackin_telemetry::schema::enums::ErrorType::IoError,
        );
        return;
    }
    jackin_telemetry::stream::complete_success(open);
    loop {
        let events = match instance.read_events() {
            Ok(events) => events,
            Err(_error) => {
                record_io_error();
                jackin_telemetry::stream::complete_error(
                    jackin_telemetry::stream::phase(
                        jackin_telemetry::schema::enums::StreamOperation::Close,
                    ),
                    jackin_telemetry::schema::enums::ErrorType::IoError,
                );
                return;
            }
        };
        let changed = events.iter().any(|event| {
            event.mask.intersects(
                AddWatchFlags::IN_Q_OVERFLOW
                    | AddWatchFlags::IN_DELETE_SELF
                    | AddWatchFlags::IN_MOVE_SELF,
            ) || event.name.as_deref() == Some(OsStr::new("HEAD"))
        });
        if changed
            && event_tx
                .send(SessionEvent::GitBranchContextRefreshRequested)
                .is_err()
        {
            jackin_telemetry::stream::complete_success(jackin_telemetry::stream::phase(
                jackin_telemetry::schema::enums::StreamOperation::Close,
            ));
            return;
        }
    }
}

pub(crate) fn git_current_context(workdir: &Path) -> GitContext {
    // Try the cheap path first: read `.git/HEAD` and parse the symref.
    // For a normal checkout on a branch the file is one line of
    // `ref: refs/heads/<name>\n` (no subprocess fork, ~50µs vs ~3-15ms
    // for `git branch --show-current`). Detached HEAD writes the raw
    // SHA which we treat as "no branch" — the bar slot stays hidden,
    // matching `git branch --show-current` which prints empty.
    //
    // Falls back to the subprocess path for worktrees (where `.git`
    // is a file, not a directory) and for any other unusual layout
    // the file-read approach cannot handle.
    if let Some(context) = read_context_from_git_metadata(workdir) {
        return match context {
            // `Branch` with no head means the loose+packed lookup
            // missed (unborn, race with `pack-refs`, etc.). Try the
            // subprocess as a last-resort recovery for that single
            // case rather than ship a head-less context.
            GitContext::Branch { name, head: None } => {
                let head = git_capture_at_workdir(workdir, &["rev-parse", "--verify", "HEAD"])
                    .as_deref()
                    .and_then(Oid::parse);
                GitContext::Branch { name, head }
            }
            other => other,
        };
    }
    git_context_from_subprocess(workdir)
}

#[cfg(test)]
pub(crate) fn read_branch_from_git_head(workdir: &Path) -> Option<BranchName> {
    match read_context_from_git_metadata(workdir)? {
        GitContext::Branch { name, .. } => Some(name),
        _ => None,
    }
}

fn git_context_from_subprocess(workdir: &Path) -> GitContext {
    let branch = git_capture_at_workdir(workdir, &["branch", "--show-current"])
        .as_deref()
        .and_then(BranchName::parse);
    let head = git_capture_at_workdir(workdir, &["rev-parse", "--verify", "HEAD"])
        .as_deref()
        .and_then(Oid::parse);
    match (branch, head) {
        (Some(name), head) => GitContext::Branch { name, head },
        (None, Some(head)) => GitContext::Detached { head },
        (None, None) => GitContext::Absent,
    }
}

pub(crate) fn read_context_from_git_metadata(workdir: &Path) -> Option<GitContext> {
    let metadata = git_metadata_dirs(workdir)?;
    let head_path = metadata.git_dir.join("HEAD");
    let head = crate::util::read_text_bounded(&head_path, GIT_METADATA_FILE_MAX_BYTES)?;
    let trimmed = head.trim();
    if let Some(ref_name) = trimmed.strip_prefix("ref: ") {
        let oid = read_git_ref_oid(
            &metadata.git_dir,
            metadata.common_git_dir.as_deref(),
            ref_name,
        );
        return Some(match BranchName::parse(ref_name) {
            // `ref:` pointing outside `refs/heads/` (e.g. refs/remotes/origin/HEAD)
            // is treated as detached for our chrome purposes — we have no branch
            // to show and the resolved tip (if any) is the head OID.
            Some(name) => GitContext::Branch { name, head: oid },
            None => oid.map_or(GitContext::Absent, |head| GitContext::Detached { head }),
        });
    }
    Some(if let Some(head) = Oid::parse(trimmed) {
        GitContext::Detached { head }
    } else {
        GitContext::Absent
    })
}

struct GitMetadataDirs {
    git_dir: PathBuf,
    common_git_dir: Option<PathBuf>,
}

fn git_metadata_dirs(workdir: &Path) -> Option<GitMetadataDirs> {
    let git_path = workdir.join(".git");
    if git_path.is_dir() {
        return Some(GitMetadataDirs {
            git_dir: git_path,
            common_git_dir: None,
        });
    }
    let git_file = crate::util::read_text_bounded(&git_path, GIT_METADATA_FILE_MAX_BYTES)?;
    let suffix = git_file.trim().strip_prefix("gitdir:")?;
    let git_dir = PathBuf::from(suffix.trim());
    let git_dir = if git_dir.is_absolute() {
        git_dir
    } else {
        workdir.join(git_dir)
    };
    let common_git_dir = common_git_dir(&git_dir, GIT_METADATA_FILE_MAX_BYTES);
    Some(GitMetadataDirs {
        git_dir,
        common_git_dir,
    })
}

fn common_git_dir(git_dir: &Path, max_bytes: u64) -> Option<PathBuf> {
    let raw = crate::util::read_text_bounded(&git_dir.join("commondir"), max_bytes)?;
    let path = PathBuf::from(raw.trim());
    Some(if path.is_absolute() {
        path
    } else {
        git_dir.join(path)
    })
}

pub(crate) fn read_git_ref_oid(
    git_dir: &Path,
    common_git_dir: Option<&Path>,
    ref_name: &str,
) -> Option<Oid> {
    // common_git_dir first when distinct: in a worktree (`git_dir` is
    // `.git/worktrees/<name>/`) branch refs (`refs/heads/*`) live in
    // common_git_dir; the per-worktree dir only holds per-worktree
    // refs (`HEAD`, `bisect/`, `rewritten/`). Probing common_git_dir
    // first saves one stat per poll on the worktree path and matches
    // git's own lookup order.
    let bases: [Option<&Path>; 2] = match common_git_dir {
        Some(common) if common != git_dir => [Some(common), Some(git_dir)],
        _ => [Some(git_dir), None],
    };
    for base in bases.into_iter().flatten() {
        if let Some(oid) = read_loose_git_ref_oid(&base.join(ref_name)) {
            return Some(oid);
        }
    }
    let packed_base = common_git_dir.unwrap_or(git_dir);
    read_packed_git_ref_oid(&packed_base.join("packed-refs"), ref_name)
}

fn read_loose_git_ref_oid(path: &Path) -> Option<Oid> {
    let raw = crate::util::read_text_bounded(path, GIT_LOOSE_REF_MAX_BYTES)?;
    let trimmed = raw.trim();
    if trimmed.starts_with("ref: ") {
        // Legitimate symref content (`git symbolic-ref refs/heads/foo
        // refs/heads/bar`). Not corruption; chaining is rare for branch
        // refs and we don't need to resolve it here — the upstream
        // caller can fall through to packed-refs. Stay silent to avoid
        // per-poll cdebug spam on a symref branch.
        return None;
    }
    let Some(oid) = Oid::parse(trimmed) else {
        // File present, content unexpected: corruption, mid-write, or
        // a hash format jackin❯ doesn't recognise. Distinguish from
        // the file-missing case (logged by `read_text_bounded` itself)
        // so triage can localise.
        return None;
    };
    Some(oid)
}

pub(crate) fn read_packed_git_ref_oid(path: &Path, ref_name: &str) -> Option<Oid> {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_error) => {
            record_recovered_degradation();
            return None;
        }
    };
    let Some(signature) = PackedRefsCacheSignature::for_metadata(&metadata) else {
        // Fail-closed: without mtime the (len-only) signature would
        // silently miss same-length rewrites. Parse fresh every call
        // on this workdir; log once per path so an operator on an
        // exotic filesystem sees why the cache is not engaging without
        // a per-poll telemetry firehose.
        log_mtime_unavailable_once(path);
        return parse_packed_refs_for_ref(path, &metadata, ref_name);
    };
    // Hot-path cache hit: lookup the requested ref inside the locked
    // section so only the Oid (~40-64 bytes) escapes, not the whole
    // PackedRefsCacheEntry clone of every ref in the repo.
    if let Some(oid) = with_packed_refs_cache(|cache| {
        cache
            .get(path)
            .filter(|entry| entry.signature == signature)
            .and_then(|entry| entry.refs.get(ref_name).cloned())
    }) {
        return Some(oid);
    }
    let (refs, truncated) = load_packed_refs(path, &metadata)?;
    let oid = refs.get(ref_name).cloned();
    if truncated {
        // A truncated read can only produce a partial ref map; caching
        // it would poison every future lookup with a wrong "absent"
        // answer until the file's (len, mtime) signature changes.
        return oid;
    }
    insert_packed_refs_cache_entry(path, PackedRefsCacheEntry { signature, refs });
    oid
}

/// Shared read+parse path for the cached and uncached call sites.
/// Truncation is detected via `metadata.len() > cap` rather than
/// `read.len() == cap`, which distinguishes a real cap-hit from a
/// legitimately exact-cap-sized file. When truncated, the partial
/// final line (no trailing `\n`) is dropped from the parse to avoid
/// inserting an entry under a half-cut ref name.
fn load_packed_refs(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Option<(HashMap<String, Oid>, bool)> {
    let truncated = metadata.len() > PACKED_REFS_MAX_BYTES;
    let raw = crate::util::read_text_bounded(path, PACKED_REFS_MAX_BYTES)?;
    Some((parse_packed_git_refs(&raw, truncated), truncated))
}

fn parse_packed_refs_for_ref(
    path: &Path,
    metadata: &std::fs::Metadata,
    ref_name: &str,
) -> Option<Oid> {
    let (refs, _truncated) = load_packed_refs(path, metadata)?;
    refs.get(ref_name).cloned()
}

fn insert_packed_refs_cache_entry(path: &Path, entry: PackedRefsCacheEntry) {
    with_packed_refs_cache(|cache| {
        if cache.len() >= PACKED_REFS_CACHE_MAX_ENTRIES && !cache.contains_key(path) {
            // Bounded eviction: visiting >CAP distinct workdirs over a
            // long-running daemon lifetime would otherwise grow the
            // map without bound. Drop one entry (HashMap iteration
            // order is implementation-defined but cheap); the hot
            // workdir is re-inserted on its next poll.
            if let Some(victim) = cache.keys().next().cloned() {
                cache.remove(&victim);
            }
        }
        cache.insert(path.to_path_buf(), entry);
    });
}

fn log_mtime_unavailable_once(path: &Path) {
    let new_entry = {
        let mut guard = PACKED_REFS_MTIME_UNAVAILABLE_LOGGED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.insert(path.to_path_buf())
    };
    if new_entry {
        record_recovered_degradation();
    }
}

/// Recover from a poisoned `PACKED_REFS_CACHE` mutex instead of silently
/// disabling the cache for the daemon lifetime. The cached values are
/// plain `HashMap<String, Oid>` entries with no torn invariants, so
/// `PoisonError::into_inner()` is safe to use after a panic.
pub(crate) fn with_packed_refs_cache<R>(
    f: impl FnOnce(&mut HashMap<PathBuf, PackedRefsCacheEntry>) -> R,
) -> R {
    let mut guard = PACKED_REFS_CACHE.lock().unwrap_or_else(|poisoned| {
        record_recovered_degradation();
        poisoned.into_inner()
    });
    f(&mut guard)
}

pub(crate) fn parse_packed_git_refs(raw: &str, truncated: bool) -> HashMap<String, Oid> {
    let mut refs = HashMap::new();
    let mut lines: Vec<&str> = raw.lines().collect();
    if truncated && !raw.ends_with('\n') {
        // Last line missing its terminator means the cap fell mid-line;
        // its second token (ref name) is a half-cut string that would
        // poison the map. Drop it.
        lines.pop();
    }
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('^') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(oid_str) = parts.next() else {
            continue;
        };
        if let Some(ref_name) = parts.next()
            && ref_name.starts_with("refs/")
            && let Some(oid) = Oid::parse(oid_str)
        {
            refs.insert(ref_name.to_owned(), oid);
        }
    }
    refs
}

/// Fail-closed signature: `modified` is mandatory because a
/// length-only signature silently misses same-length rewrites on
/// filesystems with coarse mtime resolution. Construction returns
/// `None` when `metadata.modified()` is unavailable so the caller
/// bypasses the cache rather than caching against a weak key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PackedRefsCacheSignature {
    len: u64,
    modified: SystemTime,
}

impl PackedRefsCacheSignature {
    fn for_metadata(metadata: &std::fs::Metadata) -> Option<Self> {
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok()?,
        })
    }
}

#[derive(Clone)]
pub(crate) struct PackedRefsCacheEntry {
    pub(crate) signature: PackedRefsCacheSignature,
    pub(crate) refs: HashMap<String, Oid>,
}

pub(crate) const PACKED_REFS_MAX_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const PACKED_REFS_CACHE_MAX_ENTRIES: usize = 32;
const GIT_METADATA_FILE_MAX_BYTES: u64 = 64 * 1024;
const GIT_LOOSE_REF_MAX_BYTES: u64 = 64 * 1024;

static PACKED_REFS_CACHE: LazyLock<Mutex<HashMap<PathBuf, PackedRefsCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Paths whose mtime is unavailable have emitted one governed recovery.
/// Prevents a poll-rate firehose on exotic filesystems.
static PACKED_REFS_MTIME_UNAVAILABLE_LOGGED: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

fn record_recovered_degradation() {
    let _warning = jackin_telemetry::record_recovered_degradation();
}

#[cfg(target_os = "linux")]
fn record_io_error() {
    let _error =
        jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::IoError);
}
