// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Git branch and pull-request context management for the Multiplexer.

use super::{
    Arc, BranchName, GIT_BRANCH_CONTEXT_POLL_INTERVAL, GitContext, Instant, Multiplexer, Oid,
    PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL, PullRequestContextCacheEntry, PullRequestInfo,
    PullRequestLookupMode, PullRequestLookupOutcome, SessionEvent, gh_pull_request_info,
    git_current_context, resolve_default_branch,
};

impl Multiplexer {
    pub(super) fn pull_request_context_loading(&self) -> bool {
        let Some(branch) = self.pr_watch.pull_request_context_branch.as_deref() else {
            return false;
        };
        self.pr_watch.pull_request_lookup.in_flight
            && !self.launch_env.workdir_context.is_default_branch(branch)
            && self.pr_watch.pull_request_context.is_none()
    }

    pub(super) fn maybe_spawn_git_branch_context_lookup(&mut self, now: Instant) {
        self.spawn_git_branch_context_lookup(now, true);
    }

    pub(super) fn force_spawn_git_branch_context_lookup(&mut self, now: Instant) {
        self.spawn_git_branch_context_lookup(now, false);
    }

    fn spawn_git_branch_context_lookup(&mut self, now: Instant, respect_cooldown: bool) {
        if !self.launch_env.workdir_context.git_available
            && !self.launch_env.workdir_context.is_git_repo
        {
            return;
        }
        if self.pr_watch.git_branch_lookup.in_flight {
            return;
        }
        if respect_cooldown
            && self
                .pr_watch
                .git_branch_lookup
                .cooldown_active(now, GIT_BRANCH_CONTEXT_POLL_INTERVAL)
        {
            return;
        }
        let request_id = self.pr_watch.git_branch_lookup.begin_spawn(now);
        let workdir = self.launch_env.workdir.clone();
        self.spawn_context_lookup(
            "git-branch-context",
            move || git_current_context(&workdir),
            move |context| SessionEvent::GitBranchContextLoaded {
                request_id,
                context,
            },
        );
    }

    pub(super) fn maybe_spawn_pull_request_context_lookup(&mut self, now: Instant) -> bool {
        self.spawn_pull_request_context_lookup(now, PullRequestLookupMode::RespectCache)
    }

    pub(super) fn force_spawn_pull_request_context_lookup(&mut self, now: Instant) -> bool {
        self.spawn_pull_request_context_lookup(now, PullRequestLookupMode::ForceRefresh)
    }

    fn spawn_pull_request_context_lookup(
        &mut self,
        now: Instant,
        mode: PullRequestLookupMode,
    ) -> bool {
        if self.pr_watch.pull_request_lookup.in_flight {
            if mode == PullRequestLookupMode::ForceRefresh {
                crate::cdebug!(
                    "pull-request-context: force-refresh skipped: in-flight lookup request_id={} will satisfy",
                    self.pr_watch.pull_request_lookup.request_id
                );
            }
            return false;
        }
        if !self.launch_env.workdir_context.gh_available {
            if mode == PullRequestLookupMode::RespectCache {
                return false;
            }
            crate::clog!(
                "pull-request-context: force-refresh scheduling lookup despite startup gh unavailable"
            );
        }
        let Some(branch) = self.pr_watch.pull_request_context_branch.clone() else {
            if mode == PullRequestLookupMode::ForceRefresh {
                crate::cdebug!("pull-request-context: force-refresh skipped: no branch");
            }
            return false;
        };
        if self.launch_env.workdir_context.is_default_branch(&branch) {
            if mode == PullRequestLookupMode::ForceRefresh {
                crate::cdebug!(
                    "pull-request-context: force-refresh skipped: branch {branch} is default"
                );
            }
            return false;
        }
        if self.pull_request_cache_blocks_lookup(&branch, now, mode) {
            return false;
        }
        let request_id = self.pr_watch.pull_request_lookup.begin_spawn(now);
        let workdir = self.launch_env.workdir.clone();
        let branch_for_event = branch.clone();
        // Snapshot HEAD at spawn time so the cache entry the result
        // populates is keyed on the head the worker actually queried,
        // not whatever `self.pr_watch.pull_request_context_head` happens to be
        // at apply time.
        let head_for_event = self.pr_watch.pull_request_context_head.clone();
        self.spawn_context_lookup(
            "pull-request-context",
            move || match gh_pull_request_info(&workdir, branch.as_str()) {
                Ok(pr) => PullRequestLookupOutcome::Resolved(pr),
                Err(err) => {
                    crate::clog!(
                        "pull-request-context: gh lookup failed for branch {branch}: {err}"
                    );
                    PullRequestLookupOutcome::TransientFailure
                }
            },
            move |outcome| SessionEvent::PullRequestContextLoaded {
                request_id,
                branch: Some(branch_for_event),
                head: head_for_event,
                outcome,
            },
        );
        true
    }

    /// Generic worker spawn for the two background context lookups.
    /// `work` runs the actual `git`/`gh` subprocess (off the daemon's
    /// main thread); `to_event` maps the worker's return value into
    /// the `SessionEvent` variant the main loop dispatches. The
    /// channel-closed `clog!` is uniform across callers so a future
    /// triage of "why didn't the bar refresh?" has the same shape
    /// regardless of which lookup misbehaved.
    pub(super) fn spawn_context_lookup<F, T, E>(&self, label: &'static str, work: F, to_event: E)
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
        E: FnOnce(T) -> SessionEvent + Send + 'static,
    {
        let event_tx = self.control.event_tx.clone();
        let emit = move || {
            let value = work();
            if event_tx.send(to_event(value)).is_err() {
                crate::clog!("{label}: event channel closed before result reached main loop");
            }
        };
        // Fire-and-forget worker — no `await`, no tokio context needed.
        // Inside the daemon's `#[tokio::main]` we still route through
        // `spawn_blocking` so the runtime accounts for blocking work;
        // outside one (unit tests, ad-hoc tools) a plain OS thread
        // avoids spinning up a second runtime.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn_blocking(emit);
            }
            Err(_) => {
                if let Err(e) = std::thread::Builder::new()
                    .name(format!("capsule-blocking[{label}]"))
                    .spawn(emit)
                {
                    crate::clog!("{label}: failed to spawn blocking worker thread: {e}");
                }
            }
        }
    }

    pub(super) fn apply_git_branch_context_loaded(
        &mut self,
        request_id: u64,
        context: GitContext,
        now: Instant,
    ) -> bool {
        crate::cdebug!(
            "git-branch-context: lookup loaded request_id={} current_request_id={} context={:?}",
            request_id,
            self.pr_watch.git_branch_lookup.request_id,
            context,
        );
        if request_id != self.pr_watch.git_branch_lookup.request_id {
            return false;
        }
        self.pr_watch.git_branch_lookup.in_flight = false;
        self.apply_git_context(context, now)
    }

    pub(super) fn apply_git_context(&mut self, context: GitContext, now: Instant) -> bool {
        let (branch, head) = match context {
            GitContext::Absent => (None, None),
            GitContext::Detached { head } => (None, Some(head)),
            GitContext::Branch { name, head } => (Some(name), head),
        };
        // Steady-state polling path: (branch, head) unchanged, no chrome
        // update needed, but the spawn-gate may still admit a refresh if
        // the cache aged out.
        if self.pr_watch.pull_request_context_branch == branch
            && self.pr_watch.pull_request_context_head == head
        {
            return self.maybe_spawn_pull_request_context_lookup(now);
        }
        let old_branch = self.pr_watch.pull_request_context_branch.take();
        let old_head = self.pr_watch.pull_request_context_head.take();
        let old_pull_request = self.pr_watch.pull_request_context.clone();
        self.pr_watch.pull_request_context_branch = branch.clone();
        self.pr_watch.pull_request_context_head = head.clone();
        // Detached HEAD (head Some, branch None) is still a git repo; a
        // bare `branch.is_some()` would miss the `git checkout <sha>` case.
        if (branch.is_some() || head.is_some()) && !self.launch_env.workdir_context.is_git_repo {
            self.launch_env.workdir_context.is_git_repo = true;
            // Offload the synchronous `git` subprocess to the blocking pool so
            // it doesn't stall the daemon's render thread (Defect 43).
            let workdir = self.launch_env.workdir.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn_blocking(move || {
                    // Result discarded: the next git-branch watcher tick will
                    // pick up the default branch via inotify or periodic poll.
                    drop(resolve_default_branch(&workdir));
                });
            } else {
                self.launch_env.workdir_context.default_branch = resolve_default_branch(&workdir);
            }
        }
        self.pr_watch.pull_request_context = branch
            .as_ref()
            .and_then(|branch| self.cached_pull_request_for_branch(branch, now));

        // Branch/HEAD flips invalidate the in-flight `gh pr list --head <old>`
        // worker; bumping the id makes its response fail the request_id
        // guard at the top of `apply_pull_request_context_loaded`. The
        // apply path also runs a second (branch, head) equality check as
        // defense-in-depth for any future call site that bypasses this
        // path.
        let in_flight_before = self.pr_watch.pull_request_lookup.in_flight;
        self.pr_watch.pull_request_lookup.invalidate_in_flight();
        crate::cdebug!(
            "git-branch-context: context flip old_branch={:?} old_head={:?} new_branch={:?} new_head={:?} invalidated_in_flight={}",
            old_branch,
            old_head,
            self.pr_watch.pull_request_context_branch,
            self.pr_watch.pull_request_context_head,
            in_flight_before
        );
        let changed = old_branch != self.pr_watch.pull_request_context_branch
            || old_head != self.pr_watch.pull_request_context_head
            || old_pull_request != self.pr_watch.pull_request_context;
        let resized = self.reconcile_content_rows();
        self.maybe_spawn_pull_request_context_lookup(now);
        resized || changed
    }

    /// Test-only ergonomic shim: wrap a short-name branch into a
    /// `GitContext::Branch { head: None }` so existing tests that
    /// don't care about head behaviour stay readable. Production code
    /// calls `apply_git_context` directly with a fully-built
    /// `GitContext`.
    #[cfg(test)]
    pub(super) fn apply_git_branch_context(
        &mut self,
        branch_name: Option<&str>,
        now: Instant,
    ) -> bool {
        let context = match branch_name.and_then(BranchName::parse) {
            Some(name) => GitContext::Branch { name, head: None },
            None => GitContext::Absent,
        };
        self.apply_git_context(context, now)
    }

    pub(super) fn apply_pull_request_context_loaded(
        &mut self,
        request_id: u64,
        branch: Option<BranchName>,
        head: Option<Oid>,
        outcome: PullRequestLookupOutcome,
        now: Instant,
    ) -> bool {
        if request_id != self.pr_watch.pull_request_lookup.request_id {
            crate::cdebug!(
                "pull-request-context: dropping stale result request_id={request_id} (current={})",
                self.pr_watch.pull_request_lookup.request_id
            );
            // `in_flight` belongs to the NEW lookup spawned during the
            // branch flip — clearing it here lets the spawn-gate admit
            // a third concurrent worker.
            return false;
        }
        let pre_loading = self.pull_request_context_loading();
        self.pr_watch.pull_request_lookup.in_flight = false;
        let post_loading = self.pull_request_context_loading();
        // `in_flight` just flipped from true → false, which changes the
        // `Resolving PR · …` ↔ `Branch · …` slot in the bottom bar even
        // when the resolved value matches the prior cache. Track the
        // transition explicitly so a non-`changed` exit still requests a
        // redraw on the loading flip.
        let loading_changed = pre_loading != post_loading;
        let Some(branch) = branch else {
            return loading_changed;
        };
        // Transient gh failures (binary missing, auth not configured,
        // JSON parse, timeout) MUST NOT poison the 60s cache with a
        // synthetic "no PR" answer — operators would lose a real PR
        // for a full minute after every blip. Preserve the previous
        // cached value; the next state-ticker tick retries.
        let pull_request = match outcome {
            PullRequestLookupOutcome::Resolved(pr) => {
                if !self.launch_env.workdir_context.gh_available {
                    crate::clog!("pull-request-context: gh lookup succeeded after startup miss");
                    self.launch_env.workdir_context.gh_available = true;
                }
                pr
            }
            PullRequestLookupOutcome::TransientFailure => {
                return loading_changed;
            }
        };
        // Defense in depth on top of the request_id discriminator: if
        // mux's (branch, head) drifted between spawn and apply (e.g. a
        // future call site that mutates these fields without routing
        // through `apply_git_context`, which bumps `request_id` via
        // `pull_request_lookup.invalidate_in_flight`), refuse to assign
        // or cache so we cannot stamp data against the wrong key.
        if self.pr_watch.pull_request_context_branch.as_ref() != Some(&branch)
            || self.pr_watch.pull_request_context_head != head
        {
            crate::cdebug!(
                "pull-request-context: (branch, head) drift between spawn and apply — \
                 spawn=({:?}, {:?}) apply=({:?}, {:?}); refusing to assign or cache",
                branch,
                head,
                self.pr_watch.pull_request_context_branch,
                self.pr_watch.pull_request_context_head,
            );
            // We just cleared in_flight a few lines above; schedule a
            // fresh lookup for the current (branch, head) so the bar
            // doesn't sit stale until the next git-branch poll happens
            // to differ from the active mux state.
            self.maybe_spawn_pull_request_context_lookup(now);
            return loading_changed;
        }
        self.purge_expired_pull_request_cache_entries(now);
        self.pr_watch.pull_request_context_cache.insert(
            branch.clone(),
            PullRequestContextCacheEntry {
                checked_at: now,
                head,
                pull_request: pull_request.clone(),
            },
        );
        let changed = self.pr_watch.pull_request_context != pull_request;
        self.pr_watch.pull_request_context = pull_request;
        if self.reconcile_content_rows() {
            return true;
        }
        changed || loading_changed
    }

    /// Drop cache entries older than `2 * PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL`
    /// so a session that visits many feature branches does not grow the
    /// cache without bound. Two intervals = enough that an "I'm flipping
    /// between two PRs" workflow keeps both warm, while monotonic growth
    /// across hundreds of branches gets pruned.
    pub(super) fn purge_expired_pull_request_cache_entries(&mut self, now: Instant) {
        let before = self.pr_watch.pull_request_context_cache.len();
        self.pr_watch
            .pull_request_context_cache
            .retain(|_, entry| !entry.is_expired(now));
        let dropped = before - self.pr_watch.pull_request_context_cache.len();
        if dropped > 0 {
            crate::cdebug!(
                "pull-request-context: purged {dropped} expired cache entries (ttl=2x{:?})",
                PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL
            );
        }
    }

    pub(super) fn cached_pull_request_for_branch(
        &self,
        branch: &str,
        now: Instant,
    ) -> Option<Arc<PullRequestInfo>> {
        self.pr_watch
            .pull_request_context_cache
            .get(branch)
            .filter(|entry| entry.is_fresh(self.pr_watch.pull_request_context_head.as_ref(), now))
            .and_then(|entry| entry.pull_request.clone())
    }

    pub(super) fn pull_request_cache_is_fresh(&self, branch: &str, now: Instant) -> bool {
        self.pr_watch
            .pull_request_context_cache
            .get(branch)
            .is_some_and(|entry| {
                entry.is_fresh(self.pr_watch.pull_request_context_head.as_ref(), now)
            })
    }

    pub(super) fn pull_request_cache_blocks_lookup(
        &self,
        branch: &str,
        now: Instant,
        mode: PullRequestLookupMode,
    ) -> bool {
        mode == PullRequestLookupMode::RespectCache && self.pull_request_cache_is_fresh(branch, now)
    }

    /// Current branch for the chrome bar. Daemon supplies Git/default-branch
    /// facts; the TUI component owns the visible filtering rule.
    pub(super) fn context_bar_branch(&self) -> Option<&str> {
        let branch = self.pr_watch.pull_request_context_branch.as_deref()?;
        crate::tui::components::branch_context_bar::visible_branch(
            Some(branch),
            self.launch_env.workdir_context.is_default_branch(branch),
        )
    }
}
