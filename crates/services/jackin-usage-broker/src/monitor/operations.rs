// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::path::Path;
use std::sync::MutexGuard;
#[cfg(test)]
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

impl MonitorStore {
    /// Open the private host-broker monitor store below `data_dir`.
    pub(crate) fn open(data_dir: &Path) -> Result<Self, MonitorIssue> {
        let directory = storage::open_store_dir(data_dir)?;
        let state = if let Some(state) = storage::load(&directory)? {
            validate_store_state(&state)?;
            state
        } else {
            let state = StoreState::default();
            storage::save(&directory, &state)?;
            state
        };
        Ok(Self {
            inner: std::sync::Arc::new(MonitorStoreInner {
                directory,
                state: Mutex::new(state),
                collector_admission: Mutex::new(()),
                #[cfg(test)]
                collector_admission_waiters: std::sync::atomic::AtomicUsize::new(0),
                experimental_collector_source: Mutex::new(None),
                changed: Condvar::new(),
            }),
        })
    }

    /// Apply one monitor operation using the caller's clock value.
    pub(crate) fn operate(
        &self,
        operation: MonitorOperation,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        match operation {
            MonitorOperation::Start {
                config,
                idempotency_key,
            } => self.start(config, &idempotency_key, now_epoch),
            MonitorOperation::BindAccount { binding } => self.bind_account(binding, now_epoch),
            MonitorOperation::ApprovePolicy { approval } => {
                self.approve_policy(approval, now_epoch)
            }
            MonitorOperation::Stop { monitor_id } => self.stop(&monitor_id, now_epoch),
            MonitorOperation::Status { monitor_id } => self.status(&monitor_id, now_epoch),
            MonitorOperation::Doctor { provider } => Ok(MonitorReply::Doctor {
                report: MonitorDoctorReport {
                    provider,
                    broker_available: true,
                    statusline_ingress_supported: true,
                    auth_state: MonitorAuthState::Unknown,
                    issues: vec![issue(
                        MonitorIssueCode::AuthStatusUnknown,
                        "authentication was not inspected by this passive check",
                        None,
                    )],
                },
            }),
            MonitorOperation::Ingest { scope, observation } => {
                self.ingest_statusline(scope, observation, now_epoch)
            }
            MonitorOperation::RecordSpend { record } => self.record_spend(record, now_epoch),
            MonitorOperation::Refresh { monitor_id } => self.refresh(&monitor_id, now_epoch),
            MonitorOperation::ServiceStatus => {
                self.tick(now_epoch)?;
                let experimental_collector_source = self.experimental_collector_source();
                let state = self.lock();
                let active_monitors = active_monitor_count(&state);
                Ok(MonitorReply::ServiceStatus {
                    status: MonitorServiceStatus {
                        running: true,
                        active_monitors,
                        next_wake_epoch: next_wake_for(&state),
                        experimental_collector_source,
                    },
                })
            }
            MonitorOperation::ServiceStop => Ok(MonitorReply::ServiceStopped),
            MonitorOperation::Watch {
                monitor_id,
                after_sequence,
                timeout_ms,
            } => self.watch(&monitor_id, after_sequence, timeout_ms, now_epoch),
        }
    }

    /// Apply one canonical provider projection without performing a refresh.
    pub(crate) fn observe_projection(
        &self,
        projection: &UsageProjectionV1,
        now_epoch: i64,
    ) -> Result<(), MonitorIssue> {
        projection.validate().map_err(|_| {
            issue(
                MonitorIssueCode::MonitorStoreUnavailable,
                "canonical usage projection is invalid",
                None,
            )
        })?;
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        let clock_advanced = now_epoch > guard.last_now_epoch;
        staged.last_now_epoch = now_epoch;
        let mut observations_changed = false;
        for provider in &projection.providers {
            if provider.provider_id == "claude" {
                for account in &provider.accounts {
                    observations_changed |=
                        observe_projection_account(&mut staged, account, now_epoch)?;
                }
            }
        }
        let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        if observations_changed || monitors_changed || clock_advanced {
            self.commit(&mut guard, staged)?;
        }
        if monitors_changed {
            self.inner.changed.notify_all();
        }
        Ok(())
    }

    /// Reconcile active monitor decisions against current local evidence.
    pub(crate) fn tick(&self, now_epoch: i64) -> Result<(), MonitorIssue> {
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        let clock_advanced = now_epoch > guard.last_now_epoch;
        staged.last_now_epoch = now_epoch;
        let changed = reconcile_all_monitors(&mut staged, now_epoch);
        if !changed && !clock_advanced {
            return Ok(());
        }
        self.commit(&mut guard, staged)?;
        if changed {
            self.inner.changed.notify_all();
        }
        Ok(())
    }

    /// Whether the service still owns any non-stopped monitor.
    #[must_use]
    pub(crate) fn has_active(&self) -> bool {
        active_monitor_count(&self.lock()) > 0
    }

    /// Source capability IDs authorized for the experimental Claude collector.
    /// Only current, confirmed bindings attached to active opted-in monitors
    /// whose source matches this process's foreground source grant collection.
    #[must_use]
    pub(crate) fn collection_accounts(&self) -> Vec<String> {
        let Some(configured_source) = self.experimental_collector_source() else {
            return Vec::new();
        };
        let state = self.lock();
        state
            .monitors
            .values()
            .filter(|monitor| {
                monitor.stopped_at_epoch.is_none()
                    && monitor.config.experimental_collector
                    && monitor.config.purpose == MonitorPurpose::ObserveOnly
                    && monitor.config.provider
                        == jackin_protocol::usage_monitor::MonitorProvider::Claude
            })
            .filter_map(|monitor| {
                let MonitorScope::BoundAccount {
                    binding_id,
                    binding_revision,
                    ..
                } = &monitor.config.scope
                else {
                    return None;
                };
                let binding = current_binding(&state, binding_id)?;
                (binding.revision == *binding_revision
                    && binding.provider == monitor.config.provider
                    && binding.operator_confirmed
                    && binding.experimental_collector_approved
                    && Some(binding.account_id.as_str()) == monitor.account_id.as_deref()
                    && binding.provider_account_id.as_deref() == Some(configured_source.as_str()))
                .then(|| binding.provider_account_id.clone())
                .flatten()
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Serialize a collector snapshot and its coordinator admission against
    /// Stop and binding-source revocation. The callback must only perform
    /// bounded admission work; it must not call another method that acquires
    /// this gate or wait for provider completion.
    pub(crate) fn with_collection_admission<T>(&self, admit: impl FnOnce(&[String]) -> T) -> T {
        let _admission = self.lock_collector_admission();
        let source_ids = self.collection_accounts();
        admit(&source_ids)
    }

    fn lock_collector_admission(&self) -> MutexGuard<'_, ()> {
        #[cfg(test)]
        match self.inner.collector_admission.try_lock() {
            Ok(guard) => return guard,
            Err(std::sync::TryLockError::WouldBlock) => {
                self.inner
                    .collector_admission_waiters
                    .fetch_add(1, Ordering::SeqCst);
                let guard = self
                    .inner
                    .collector_admission
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                self.inner
                    .collector_admission_waiters
                    .fetch_sub(1, Ordering::SeqCst);
                return guard;
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {}
        }
        self.inner
            .collector_admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(test)]
    pub(crate) fn collector_admission_waiters(&self) -> usize {
        self.inner
            .collector_admission_waiters
            .load(Ordering::SeqCst)
    }

    /// Configure the source capability selected by this foreground service.
    /// The value is ephemeral and does not prove credential or provider health.
    pub(crate) fn set_experimental_collector_source(&self, source: Option<String>) {
        let _admission = self.lock_collector_admission();
        *self
            .inner
            .experimental_collector_source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = source;
    }

    fn experimental_collector_source(&self) -> Option<String> {
        self.inner
            .experimental_collector_source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Earliest persisted evidence-expiry or reset-grace wake.
    #[must_use]
    pub(crate) fn next_wake(&self) -> Option<i64> {
        next_wake_for(&self.lock())
    }

    fn bind_account(
        &self,
        input: MonitorAccountBindingInput,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        validate_identifier(&input.account_id)?;
        if input
            .provider_account_id
            .as_deref()
            .is_some_and(|source_id| !valid_source_capability_id(source_id))
        {
            return Err(issue(
                MonitorIssueCode::StatuslineInvalid,
                "source capability ID must be 64 lowercase hexadecimal characters",
                None,
            ));
        }
        if !valid_bounded_text(&input.operator_label, MAX_OPERATOR_LABEL_LENGTH) {
            return Err(invalid_operator_label());
        }
        if !input.operator_confirmed {
            return Err(operator_confirmation_required());
        }
        if input.experimental_collector_approved && input.provider_account_id.is_none() {
            return Err(binding_required());
        }

        let _collector_admission = self.lock_collector_admission();
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;

        let existing = staged.bindings.iter().find_map(|(binding_id, history)| {
            history.last().and_then(|latest| {
                (latest.provider == input.provider && latest.account_id == input.account_id)
                    .then_some((binding_id.clone(), latest.clone()))
            })
        });
        let binding = if let Some((binding_id, previous)) = existing {
            if previous.operator_confirmed
                && previous.operator_label == input.operator_label
                && previous.provider_account_id == input.provider_account_id
                && previous.experimental_collector_approved == input.experimental_collector_approved
            {
                previous
            } else {
                let source_mapping_changed =
                    previous.provider_account_id != input.provider_account_id;
                let has_goal_history =
                    staged
                        .goals
                        .values()
                        .any(|goal| goal.binding_id == binding_id)
                        || staged.policy_records.values().flatten().any(|policy| {
                            policy.binding_id.as_deref() == Some(binding_id.as_str())
                        });
                if source_mapping_changed && has_goal_history {
                    return Err(issue(
                        MonitorIssueCode::AccountMismatch,
                        "a source capability mapping cannot change after goal history exists",
                        None,
                    ));
                }
                let revision = previous
                    .revision
                    .checked_add(1)
                    .ok_or_else(store_unavailable)?;
                let next = MonitorAccountBinding {
                    binding_id: binding_id.clone(),
                    provider: input.provider,
                    account_id: input.account_id,
                    provider_account_id: input.provider_account_id,
                    experimental_collector_approved: input.experimental_collector_approved,
                    operator_label: input.operator_label,
                    revision,
                    operator_confirmed: true,
                    confirmed_at_epoch: Some(now_epoch),
                };
                staged
                    .bindings
                    .get_mut(&binding_id)
                    .ok_or_else(store_unavailable)?
                    .push(next.clone());
                next
            }
        } else {
            if staged.bindings.len() >= MAX_BINDINGS {
                return Err(store_unavailable());
            }
            let next_binding_id = staged
                .next_binding_id
                .checked_add(1)
                .ok_or_else(store_unavailable)?;
            let binding_id = format!("binding-{:08}", staged.next_binding_id);
            staged.next_binding_id = next_binding_id;
            let binding = MonitorAccountBinding {
                binding_id: binding_id.clone(),
                provider: input.provider,
                account_id: input.account_id,
                provider_account_id: input.provider_account_id,
                experimental_collector_approved: input.experimental_collector_approved,
                operator_label: input.operator_label,
                revision: 1,
                operator_confirmed: true,
                confirmed_at_epoch: Some(now_epoch),
            };
            staged.bindings.insert(binding_id, vec![binding.clone()]);
            binding
        };

        self.commit(&mut guard, staged)?;
        self.inner.changed.notify_all();
        Ok(MonitorReply::AccountBound { binding })
    }

    fn approve_policy(
        &self,
        approval: MonitorPolicyApprovalInput,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        validate_identifier(&approval.binding_id)?;
        validate_goal_id(&approval.goal_id)?;
        if !valid_bounded_text(&approval.operator_label, MAX_OPERATOR_LABEL_LENGTH) {
            return Err(invalid_operator_label());
        }
        if !approval.operator_confirmed {
            return Err(operator_confirmation_required());
        }
        validate_policy_input(
            approval.new_policy,
            approval.budget.as_ref(),
            approval.acknowledge_no_sgd_cap,
        )?;

        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        let binding = current_binding(&staged, &approval.binding_id)
            .filter(|binding| binding.revision == approval.binding_revision)
            .cloned()
            .ok_or_else(binding_mismatch)?;
        if !binding.operator_confirmed {
            return Err(operator_confirmation_required());
        }
        if staged.policy_records.values().map(Vec::len).sum::<usize>() >= MAX_POLICY_REVISIONS {
            return Err(store_unavailable());
        }

        let previous = current_policy(&staged, &approval.goal_id).cloned();
        if previous.as_ref().is_some_and(|record| {
            record.provider != binding.provider || record.account_id != binding.account_id
        }) {
            return Err(issue(
                MonitorIssueCode::AccountMismatch,
                "a goal policy cannot move to a different provider account",
                None,
            ));
        }
        if let Some(expected) = approval.expected_revision
            && previous.as_ref().map(|record| record.revision) != Some(expected)
        {
            return Err(policy_conflict());
        }
        if let (Some(previous), Some(_goal)) =
            (previous.as_ref(), staged.goals.get(&approval.goal_id))
            && previous.new_policy != approval.new_policy
        {
            return Err(policy_conflict());
        }
        if previous.as_ref().is_some_and(|record| {
            record.new_policy == MonitorPolicy::StrictSgd
                && approval.new_policy == MonitorPolicy::QuotaOnly
        }) {
            return Err(policy_conflict());
        }
        if previous.as_ref().is_some_and(|record| {
            record.new_policy == MonitorPolicy::StrictSgd
                && approval.new_policy == MonitorPolicy::StrictSgd
                && !budget_is_same_or_tighter(record.budget.as_ref(), approval.budget.as_ref())
                && !is_migrated_zero_sgd_budget_repair(record)
        }) {
            return Err(policy_conflict());
        }

        let revision = previous
            .as_ref()
            .map(|record| record.revision.checked_add(1).ok_or_else(store_unavailable))
            .transpose()?
            .unwrap_or(1);
        let record = MonitorPolicyRecord {
            provider: binding.provider,
            account_id: binding.account_id.clone(),
            binding_id: Some(binding.binding_id.clone()),
            binding_revision: Some(binding.revision),
            goal_id: approval.goal_id.clone(),
            previous_policy: previous.as_ref().map(|record| record.new_policy),
            new_policy: approval.new_policy,
            budget: approval.budget,
            operator_label: Some(approval.operator_label),
            operator_confirmed: true,
            acknowledge_no_sgd_cap: approval.acknowledge_no_sgd_cap,
            recorded_at_epoch: Some(now_epoch),
            revision,
            origin: MonitorPolicyOrigin::Operator,
        };
        staged
            .policy_records
            .entry(record.goal_id.clone())
            .or_default()
            .push(record.clone());
        if let Some(goal) = staged.goals.get_mut(&record.goal_id) {
            goal.binding_id = binding.binding_id.clone();
            goal.binding_revision = binding.revision;
            goal.policy_revision = record.revision;
            goal.policy = record.new_policy;
            goal.budget = record.budget.clone();
        }
        refresh_all_goal_spend(&mut staged, now_epoch);
        let changed = reconcile_all_monitors(&mut staged, now_epoch);
        self.commit(&mut guard, staged)?;
        if changed {
            self.inner.changed.notify_all();
        }
        Ok(MonitorReply::PolicyApproved { policy: record })
    }

    fn start(
        &self,
        config: MonitorConfig,
        idempotency_key: &str,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        validate_config(&config)?;
        if !valid_bounded_text(idempotency_key, MAX_IDEMPOTENCY_KEY_LENGTH)
            || idempotency_key.starts_with(MIGRATED_IDEMPOTENCY_KEY_PREFIX)
        {
            return Err(issue(
                MonitorIssueCode::StatuslineInvalid,
                "idempotency key is empty, reserved, or outside its accepted bounds",
                None,
            ));
        }
        // Keep lock ordering consistent with service-status and collector
        // account reads, which acquire the ephemeral source before state.
        let configured_source = config
            .experimental_collector
            .then(|| self.experimental_collector_source())
            .flatten();
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);

        // Revalidate the source mapping, consent, and foreground lease before
        // returning even an idempotent replay. A reused start key cannot
        // restore authority that a later binding revision revoked.
        if config.experimental_collector {
            let binding = resolve_scope_binding(&staged, &config.scope, config.provider)?.clone();
            if !binding.operator_confirmed {
                return Err(operator_confirmation_required());
            }
            validate_collection_binding(&config, &binding)?;
            if configured_source
                .as_deref()
                .is_none_or(|source| binding.provider_account_id.as_deref() != Some(source))
            {
                return Err(issue(
                    MonitorIssueCode::CollectorAuthRequired,
                    "experimental collection requires a foreground lease for the approved source capability",
                    None,
                ));
            }
        }

        if let Some((monitor_id, existing_config)) = staged
            .monitors
            .iter()
            .find(|(_, monitor)| monitor.idempotency_key == idempotency_key)
            .map(|(monitor_id, monitor)| (monitor_id.clone(), monitor.config.clone()))
        {
            if existing_config != config {
                return Err(issue(
                    MonitorIssueCode::IdempotencyConflict,
                    "the start key is already associated with a different monitor configuration",
                    None,
                ));
            }
            staged.last_now_epoch = now_epoch;
            let changed = reconcile_all_monitors(&mut staged, now_epoch);
            let monitor = staged
                .monitors
                .get(&monitor_id)
                .ok_or_else(store_unavailable)?;
            let status = status_for(&staged, &monitor_id, monitor, now_epoch);
            if changed || now_epoch > guard.last_now_epoch {
                self.commit(&mut guard, staged)?;
            }
            return Ok(MonitorReply::Started {
                status: Box::new(status),
            });
        }

        if staged.monitors.len() >= MAX_MONITORS {
            return Err(issue(
                MonitorIssueCode::MonitorStoreUnavailable,
                "monitor store reached its configured monitor limit",
                None,
            ));
        }
        staged.last_now_epoch = now_epoch;

        let StartAuthority {
            account_id,
            policy,
            goal_id,
        } = prepare_start_authority(&mut staged, &config, now_epoch)?;

        refresh_all_goal_spend(&mut staged, now_epoch);
        let next_monitor_id = staged
            .next_monitor_id
            .checked_add(1)
            .ok_or_else(store_unavailable)?;
        let monitor_id = format!("monitor-{:08}", staged.next_monitor_id);
        staged.next_monitor_id = next_monitor_id;
        let spend_state = goal_id
            .as_ref()
            .and_then(|goal_id| staged.goals.get(goal_id))
            .and_then(|goal| goal.spend_state.clone());
        let monitor = DurableMonitor {
            config: config.clone(),
            account_id,
            idempotency_key: idempotency_key.to_owned(),
            policy,
            created_at_epoch: now_epoch,
            stopped_at_epoch: None,
            updated_at_epoch: now_epoch,
            last_reconciled_at_epoch: now_epoch,
            next_evidence_sequence: 0,
            next_decision_sequence: 0,
            next_event_sequence: 0,
            evidence: Vec::new(),
            evidence_fingerprints: BTreeMap::new(),
            reset_barriers: [None, None],
            latest_decision: None,
            decision_fingerprint: None,
            events: Vec::new(),
            spend_state,
        };
        staged.monitors.insert(monitor_id.clone(), monitor);
        let _monitor_decisions_changed = reconcile_all_monitors(&mut staged, now_epoch);
        let monitor = staged
            .monitors
            .get(&monitor_id)
            .ok_or_else(store_unavailable)?;
        let status = status_for(&staged, &monitor_id, monitor, now_epoch);
        self.commit(&mut guard, staged)?;
        self.inner.changed.notify_all();
        Ok(MonitorReply::Started {
            status: Box::new(status),
        })
    }

    fn stop(&self, monitor_id: &str, now_epoch: i64) -> Result<MonitorReply, MonitorIssue> {
        let _collector_admission = self.lock_collector_admission();
        let mut guard = self.lock();
        if let Some(monitor) = guard.monitors.get(monitor_id)
            && monitor.stopped_at_epoch.is_some()
        {
            let status = status_for(&guard, monitor_id, monitor, guard.last_now_epoch);
            return Ok(MonitorReply::Stopped {
                status: Box::new(status),
            });
        }
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        if !staged.monitors.contains_key(monitor_id) {
            return Err(monitor_not_found());
        }
        let monitor = staged
            .monitors
            .get_mut(monitor_id)
            .ok_or_else(monitor_not_found)?;
        monitor.stopped_at_epoch = Some(now_epoch);
        monitor.updated_at_epoch = now_epoch;
        monitor.latest_decision = None;
        monitor.decision_fingerprint = None;
        let _other_monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        let status = {
            let monitor = staged
                .monitors
                .get(monitor_id)
                .ok_or_else(monitor_not_found)?;
            status_for(&staged, monitor_id, monitor, now_epoch)
        };
        append_event(
            staged
                .monitors
                .get_mut(monitor_id)
                .ok_or_else(monitor_not_found)?,
            status.clone(),
            now_epoch,
        );
        self.commit(&mut guard, staged)?;
        self.inner.changed.notify_all();
        Ok(MonitorReply::Stopped {
            status: Box::new(status),
        })
    }

    fn ingest_statusline(
        &self,
        scope: MonitorScope,
        observation: StatuslineObservation,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        let mut guard = self.lock();
        let account_id = match &scope {
            MonitorScope::Session { session_id } => {
                validate_identifier(session_id)?;
                if session_id != &observation.session_id {
                    return Err(issue(
                        MonitorIssueCode::BindingMismatch,
                        "session evidence does not match the selected session scope",
                        None,
                    ));
                }
                None
            }
            MonitorScope::BoundAccount {
                binding_id,
                binding_revision,
                session_id,
            } => {
                validate_identifier(binding_id)?;
                let binding = current_binding(&guard, binding_id)
                    .filter(|binding| binding.revision == *binding_revision)
                    .cloned()
                    .ok_or_else(binding_mismatch)?;
                if !binding.operator_confirmed {
                    return Err(operator_confirmation_required());
                }
                if session_id
                    .as_deref()
                    .is_some_and(|expected| expected != observation.session_id)
                {
                    return Err(binding_mismatch());
                }
                Some(binding.account_id)
            }
        };
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        validate_observation(&observation, now_epoch)?;
        if account_id.is_none() {
            prune_inactive_unbound_sessions(&mut staged, &observation.session_id, now_epoch);
        }
        let proposed_sequence = staged.next_input_sequence.saturating_add(1);
        let evidence_sequence = if let Some(account_id) = account_id.as_deref() {
            prune_inactive_sessions(
                &mut staged,
                account_id,
                Some(&observation.session_id),
                now_epoch,
            );
            if !staged.accounts.contains_key(account_id) && staged.accounts.len() >= MAX_ACCOUNTS {
                return Err(store_unavailable());
            }
            let session_exists = staged
                .accounts
                .get(account_id)
                .is_some_and(|account| account.sessions.contains_key(&observation.session_id));
            if !session_exists
                && staged
                    .accounts
                    .get(account_id)
                    .is_some_and(|account| account.sessions.len() >= MAX_SESSIONS_PER_ACCOUNT)
            {
                return Err(store_unavailable());
            }
            let (account_sequence, fields_changed) = {
                let account = staged.accounts.entry(account_id.to_owned()).or_default();
                let (changed, fields_changed) =
                    apply_statusline(account, &observation, now_epoch, proposed_sequence);
                let _ = changed;
                if fields_changed {
                    account.input_sequence = proposed_sequence;
                }
                (account.input_sequence, fields_changed)
            };
            if fields_changed {
                staged.next_input_sequence = proposed_sequence;
            }
            account_sequence
        } else {
            let session_exists = staged
                .unbound_sessions
                .contains_key(&observation.session_id);
            if !session_exists && staged.unbound_sessions.len() >= MAX_UNBOUND_SESSIONS {
                return Err(store_unavailable());
            }
            let session = staged
                .unbound_sessions
                .entry(observation.session_id.clone())
                .or_default();
            let (changed, fields_changed) = apply_unbound_statusline(
                session,
                &observation,
                now_epoch,
                proposed_sequence,
                !session_exists,
            );
            let _ = changed;
            if fields_changed {
                staged.next_input_sequence = proposed_sequence;
                proposed_sequence
            } else {
                staged.next_input_sequence
            }
        };
        let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        self.commit(&mut guard, staged)?;
        if monitors_changed {
            self.inner.changed.notify_all();
        }
        Ok(MonitorReply::Ingested {
            scope,
            account_id,
            session_id: observation.session_id,
            evidence_sequence,
        })
    }

    fn record_spend(
        &self,
        record: SpendRecordInput,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        validate_identifier(&record.account_id)?;
        if record.amount.currency.is_empty()
            || record.amount.currency.len() > 16
            || !record.amount.currency.is_ascii()
            || record.amount.exponent > 9
        {
            return Err(issue(
                MonitorIssueCode::StatuslineInvalid,
                "spend currency or exponent is outside its accepted bounds",
                None,
            ));
        }
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        if !staged.accounts.contains_key(&record.account_id)
            && staged.accounts.len() >= MAX_ACCOUNTS
        {
            return Err(issue(
                MonitorIssueCode::MonitorStoreUnavailable,
                "monitor store reached its configured account limit",
                None,
            ));
        }
        let account_id = record.account_id.clone();
        if let Some(current) = staged
            .accounts
            .get(&account_id)
            .and_then(|account| account.spend.latest_record.as_ref())
            .filter(|current| spend_input_matches_record(&record, current))
            .cloned()
        {
            let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
            self.commit(&mut guard, staged)?;
            if monitors_changed {
                self.inner.changed.notify_all();
            }
            return Ok(MonitorReply::SpendRecorded { record: current });
        }
        let accepted = {
            let account = staged.accounts.entry(account_id.clone()).or_default();
            let (next, accepted) =
                record_account_spend(&account.spend, &account_id, record, now_epoch).map_err(
                    |error| match error {
                        spend::SpendRecordReject::AccountMismatch => issue(
                            MonitorIssueCode::AccountMismatch,
                            "spend record account does not match the selected account",
                            None,
                        ),
                        _ => issue(
                            MonitorIssueCode::StatuslineInvalid,
                            "spend record failed account, period, amount, or source validation",
                            None,
                        ),
                    },
                )?;
            account.spend = next;
            account.spend.latest_record.clone().unwrap_or(accepted)
        };
        let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        self.commit(&mut guard, staged)?;
        if monitors_changed {
            self.inner.changed.notify_all();
        }
        Ok(MonitorReply::SpendRecorded { record: accepted })
    }

    fn refresh(&self, monitor_id: &str, now_epoch: i64) -> Result<MonitorReply, MonitorIssue> {
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        if !staged.monitors.contains_key(monitor_id) {
            return Err(monitor_not_found());
        }
        let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        let monitor = staged
            .monitors
            .get(monitor_id)
            .ok_or_else(monitor_not_found)?;
        let status = status_for(&staged, monitor_id, monitor, now_epoch);
        self.commit(&mut guard, staged)?;
        if monitors_changed {
            self.inner.changed.notify_all();
        }
        Ok(MonitorReply::Refreshed {
            status: Box::new(status),
        })
    }

    fn status(&self, monitor_id: &str, now_epoch: i64) -> Result<MonitorReply, MonitorIssue> {
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        if !staged.monitors.contains_key(monitor_id) {
            return Err(monitor_not_found());
        }
        let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        let monitor = staged
            .monitors
            .get(monitor_id)
            .ok_or_else(monitor_not_found)?;
        let status = status_for(&staged, monitor_id, monitor, now_epoch);
        self.commit(&mut guard, staged)?;
        if monitors_changed {
            self.inner.changed.notify_all();
        }
        Ok(MonitorReply::Status {
            status: Box::new(status),
        })
    }

    fn watch(
        &self,
        monitor_id: &str,
        after_sequence: u64,
        timeout_ms: u64,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        // Reconcile durable evidence ages before a fresh attach can observe the
        // event log. In particular, a broker restart past the evidence TTL must
        // persist a blocked decision before returning the prior runnable event.
        self.tick(now_epoch)?;
        let timeout_ms = timeout_ms.min(MONITOR_WATCH_TIMEOUT_CAP_MS);
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut state = self.lock();
        if !state.monitors.contains_key(monitor_id) {
            return Err(monitor_not_found());
        }
        loop {
            let (events, next_sequence) = watch_snapshot(&state, monitor_id, after_sequence)?;
            if !events.is_empty() {
                return Ok(MonitorReply::Watch {
                    events,
                    next_sequence,
                    timed_out: false,
                });
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(MonitorReply::Watch {
                    events,
                    next_sequence,
                    timed_out: true,
                });
            }
            let result = self.inner.changed.wait_timeout(state, remaining);
            let (next_state, wait) = result.unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next_state;
            if wait.timed_out() {
                let (events, next_sequence) = watch_snapshot(&state, monitor_id, after_sequence)?;
                return Ok(MonitorReply::Watch {
                    timed_out: events.is_empty(),
                    events,
                    next_sequence,
                });
            }
        }
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, StoreState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn commit(
        &self,
        guard: &mut MutexGuard<'_, StoreState>,
        staged: StoreState,
    ) -> Result<(), MonitorIssue> {
        validate_store_state(&staged)?;
        storage::save(&self.inner.directory, &staged)?;
        **guard = staged;
        Ok(())
    }
}
