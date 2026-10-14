// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    Arc, BranchName, GIT_BRANCH_CONTEXT_POLL_INTERVAL, GitContext, Instant, Multiplexer, Oid,
    PullRequestContextCacheEntry, PullRequestInfo, PullRequestLookupMode, PullRequestLookupOutcome,
    SessionEvent, gh_pull_request_info, git_current_context, resolve_default_branch,
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
            jackin_telemetry::schema::enums::BackgroundCycleName::BranchContext,
            move || git_current_context(&workdir),
            move |context| SessionEvent::GitBranchContextLoaded {
                request_id,
                context,
            },
            |_| jackin_telemetry::spawn::DetachedCompletion::success(),
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
            return false;
        }
        if !self.launch_env.workdir_context.gh_available
            && mode == PullRequestLookupMode::RespectCache
        {
            return false;
        }
        let Some(branch) = self.pr_watch.pull_request_context_branch.clone() else {
            return false;
        };
        if self.launch_env.workdir_context.is_default_branch(&branch) {
            return false;
        }
        if self.pull_request_cache_blocks_lookup(&branch, now, mode) {
            return false;
        }
        let request_id = self.pr_watch.pull_request_lookup.begin_spawn(now);
        let workdir = self.launch_env.workdir.clone();
        let branch_for_event = branch.clone();

        let head_for_event = self.pr_watch.pull_request_context_head.clone();
        self.spawn_context_lookup(
            jackin_telemetry::schema::enums::BackgroundCycleName::PrContext,
            move || match gh_pull_request_info(&workdir, branch.as_str()) {
                Ok(pr) => PullRequestLookupOutcome::Resolved(pr),
                Err(_) => PullRequestLookupOutcome::TransientFailure,
            },
            move |outcome| SessionEvent::PullRequestContextLoaded {
                request_id,
                branch: Some(branch_for_event),
                head: head_for_event,
                outcome,
            },
            |outcome| match outcome {
                PullRequestLookupOutcome::Resolved(_) => {
                    jackin_telemetry::spawn::DetachedCompletion::success()
                }
                PullRequestLookupOutcome::TransientFailure => {
                    jackin_telemetry::spawn::DetachedCompletion::recovered_degradation()
                }
            },
        );
        true
    }

    pub(super) fn spawn_context_lookup<F, T, E, C>(
        &self,
        cycle: jackin_telemetry::schema::enums::BackgroundCycleName,
        work: F,
        to_event: E,
        classify_work: C,
    ) where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
        E: FnOnce(T) -> SessionEvent + Send + 'static,
        C: FnOnce(&T) -> jackin_telemetry::spawn::DetachedCompletion + Send + 'static,
    {
        let event_tx = self.control.event_tx.clone();
        let emit = move || {
            let value = work();
            let completion = classify_work(&value);
            let delivered = event_tx.send(to_event(value)).is_ok();
            (delivered, completion)
        };
        let classify = |(delivered, completion): &(bool, _)| {
            if *delivered {
                *completion
            } else {
                jackin_telemetry::spawn::DetachedCompletion::error(
                    jackin_telemetry::schema::enums::ErrorType::RpcError,
                )
            }
        };

        match tokio::runtime::Handle::try_current() {
            Ok(_handle) => {
                drop(jackin_telemetry::spawn::autonomous_cycle_blocking(
                    cycle, emit, classify,
                ));
            }
            Err(_) => {
                drop(jackin_telemetry::spawn::thread_autonomous_cycle_named(
                    format!("capsule-blocking[{}]", cycle.as_str()),
                    cycle,
                    emit,
                    classify,
                ));
            }
        }
    }

    pub(super) fn apply_git_branch_context_loaded(
        &mut self,
        request_id: u64,
        context: GitContext,
        now: Instant,
    ) -> bool {
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

        if (branch.is_some() || head.is_some()) && !self.launch_env.workdir_context.is_git_repo {
            self.launch_env.workdir_context.is_git_repo = true;

            let workdir = self.launch_env.workdir.clone();
            if tokio::runtime::Handle::try_current().is_ok() {
                drop(jackin_telemetry::spawn::autonomous_cycle_blocking(
                    jackin_telemetry::schema::enums::BackgroundCycleName::BranchContext,
                    move || {
                        drop(resolve_default_branch(&workdir));
                    },
                    |()| jackin_telemetry::spawn::DetachedCompletion::success(),
                ));
            } else {
                self.launch_env.workdir_context.default_branch = resolve_default_branch(&workdir);
            }
        }
        self.pr_watch.pull_request_context = branch
            .as_ref()
            .and_then(|branch| self.cached_pull_request_for_branch(branch, now));

        self.pr_watch.pull_request_lookup.invalidate_in_flight();
        let changed = old_branch != self.pr_watch.pull_request_context_branch
            || old_head != self.pr_watch.pull_request_context_head
            || old_pull_request != self.pr_watch.pull_request_context;
        let resized = self.reconcile_content_rows();
        self.maybe_spawn_pull_request_context_lookup(now);
        resized || changed
    }

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
            return false;
        }
        let pre_loading = self.pull_request_context_loading();
        self.pr_watch.pull_request_lookup.in_flight = false;
        let post_loading = self.pull_request_context_loading();

        let loading_changed = pre_loading != post_loading;
        let Some(branch) = branch else {
            return loading_changed;
        };

        let pull_request = match outcome {
            PullRequestLookupOutcome::Resolved(pr) => {
                if !self.launch_env.workdir_context.gh_available {
                    self.launch_env.workdir_context.gh_available = true;
                }
                pr
            }
            PullRequestLookupOutcome::TransientFailure => {
                return loading_changed;
            }
        };

        if self.pr_watch.pull_request_context_branch.as_ref() != Some(&branch)
            || self.pr_watch.pull_request_context_head != head
        {
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

    pub(super) fn purge_expired_pull_request_cache_entries(&mut self, now: Instant) {
        self.pr_watch
            .pull_request_context_cache
            .retain(|_, entry| !entry.is_expired(now));
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

    pub(super) fn context_bar_branch(&self) -> Option<&str> {
        let branch = self.pr_watch.pull_request_context_branch.as_deref()?;
        crate::tui::components::branch_context_bar::visible_branch(
            Some(branch),
            self.launch_env.workdir_context.is_default_branch(branch),
        )
    }
}
