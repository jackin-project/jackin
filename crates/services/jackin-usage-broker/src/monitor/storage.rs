// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded, owner-only atomic persistence for monitor state.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use jackin_protocol::usage_monitor::{
    MonitorIssue, MonitorIssueCode, USAGE_MONITOR_SCHEMA_VERSION,
};
use nix::fcntl::{OFlag, open, openat, renameat};
use nix::sys::stat::{Mode, fchmod, mkdirat};
use nix::unistd::{UnlinkatFlags, fsync, geteuid, unlinkat};

use super::StoreState;

const STORE_FILE: &str = "state.json";
const STORE_DIR: &str = "monitor";
const STORE_MODE: u32 = 0o600;
const STORE_DIR_MODE: u32 = 0o700;
pub(super) const MAX_STORE_BYTES: usize = 32 * 1024 * 1024;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Open or create the private monitor directory below the host broker state.
pub(super) fn open_store_dir(data_dir: &Path) -> Result<File, MonitorIssue> {
    crate::secure_run_directory(data_dir).map_err(|_| unavailable())?;
    let broker_path = data_dir.join(crate::BROKER_DIR);
    let broker_fd = open(
        &broker_path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| unavailable())?;
    let broker_dir = File::from(broker_fd);
    validate_owned_dir(&broker_dir, STORE_DIR_MODE)?;

    match mkdirat(
        &broker_dir,
        STORE_DIR,
        Mode::from_bits_truncate(STORE_DIR_MODE as u16),
    ) {
        Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
        Err(_) => return Err(unavailable()),
    }
    let store_fd = openat(
        &broker_dir,
        STORE_DIR,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| unavailable())?;
    let store_dir = File::from(store_fd);
    fchmod(&store_dir, Mode::from_bits_truncate(STORE_DIR_MODE as u16))
        .map_err(|_| unavailable())?;
    validate_owned_dir(&store_dir, STORE_DIR_MODE)?;
    Ok(store_dir)
}

/// Load one validated store. Missing state is returned as `None`.
pub(super) fn load(dir: &File) -> Result<Option<StoreState>, MonitorIssue> {
    let fd = match openat(
        dir,
        STORE_FILE,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(nix::errno::Errno::ENOENT) => return Ok(None),
        Err(_) => return Err(unavailable()),
    };
    let file = File::from(fd);
    validate_owned_file(&file)?;
    let size = usize::try_from(file.metadata().map_err(|_| unavailable())?.len())
        .map_err(|_| unavailable())?;
    if size > MAX_STORE_BYTES {
        return Err(unavailable());
    }
    let mut bytes = Vec::with_capacity(size);
    file.take(u64::try_from(MAX_STORE_BYTES + 1).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() > MAX_STORE_BYTES {
        return Err(unavailable());
    }
    let version = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|value| value.get("schema_version")?.as_u64())
        .and_then(|version| u16::try_from(version).ok())
        .ok_or_else(unavailable)?;
    let state = match version {
        1 => super::legacy::migrate_v1(&bytes)?,
        2 | 3 => migrate_v2_or_v3(&bytes, version)?,
        USAGE_MONITOR_SCHEMA_VERSION => {
            serde_json::from_slice(&bytes).map_err(|_| unavailable())?
        }
        _ => return Err(unavailable()),
    };
    super::validate_store_state(&state)?;
    if version != USAGE_MONITOR_SCHEMA_VERSION {
        // The converted snapshot replaces its source schema only after full
        // semantic validation; `save` publishes it through the existing fsync+rename path.
        save(dir, &state)?;
    }
    Ok(Some(state))
}

/// Upgrade pre-V4 snapshots that already use the current durable state shape.
/// New consent and source fields default to disabled/absent. V3 migration also
/// latches uncertainty for strict goals whose retained spend anchors prove a
/// rollover that the old schema could not represent safely.
fn migrate_v2_or_v3(bytes: &[u8], version: u16) -> Result<StoreState, MonitorIssue> {
    let mut state: StoreState = serde_json::from_slice(bytes).map_err(|_| unavailable())?;
    state.schema_version = USAGE_MONITOR_SCHEMA_VERSION;
    for monitor in state.monitors.values_mut() {
        for event in &mut monitor.events {
            if event.status.schema_version != version {
                return Err(unavailable());
            }
            event.status.schema_version = USAGE_MONITOR_SCHEMA_VERSION;
        }
    }

    // Validate the normalized source before migration semantics can alter it.
    // This prevents uncertainty latching from masking corrupt persisted state.
    super::validate_store_state(&state)?;

    let changed_goals = if version == 3 {
        latch_ambiguous_v3_rollovers(&mut state)
    } else {
        BTreeSet::new()
    };
    if !changed_goals.is_empty() {
        let current_goal_spend = state
            .goals
            .iter()
            .map(|(goal_id, goal)| {
                (
                    goal_id.clone(),
                    (goal.policy_revision, goal.spend_state.clone()),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for monitor in state.monitors.values_mut() {
            let Some(goal_id) = monitor.config.goal_id.as_deref() else {
                continue;
            };
            if changed_goals.contains(goal_id)
                && let Some((policy_revision, spend_state)) = current_goal_spend.get(goal_id)
                && monitor.config.policy_revision == Some(*policy_revision)
            {
                monitor.spend_state = spend_state.clone();
            }
        }
    }
    Ok(state)
}

/// V3 could lose verified receipts outside its retained current/previous
/// period slots while still claiming a complete cumulative total. Preserve the
/// stored estimate, but mark only rolled strict goals uncertain and persist a
/// horizon that later receipts cannot clear.
fn latch_ambiguous_v3_rollovers(state: &mut StoreState) -> BTreeSet<String> {
    let mut account_horizons = BTreeMap::<String, i64>::new();
    let mut changed_goals = BTreeSet::new();
    let last_now_epoch = state.last_now_epoch;
    let accounts = &state.accounts;
    for (goal_id, goal) in &mut state.goals {
        if goal.policy != jackin_protocol::usage_monitor::MonitorPolicy::StrictSgd {
            continue;
        }
        let Some(spend) = goal.spend_state.as_mut() else {
            continue;
        };
        let Some(baseline) = spend.baseline.as_ref() else {
            continue;
        };
        let baseline_period = (
            baseline.billing_period_start_epoch,
            baseline.billing_period_end_epoch,
        );
        let current = accounts
            .get(&goal.account_id)
            .and_then(|account| account.spend.current_period_record.as_ref());
        let current_period = current.map(|record| {
            (
                record.billing_period_start_epoch,
                record.billing_period_end_epoch,
            )
        });
        let anchor = spend.period_anchor.as_ref();
        let anchor_period = anchor.map(|record| {
            (
                record.billing_period_start_epoch,
                record.billing_period_end_epoch,
            )
        });
        let rolled_current = current_period.is_some_and(|period| {
            period != baseline_period && period.0 > baseline.billing_period_start_epoch
        });
        let rolled_anchor = anchor_period.is_some_and(|period| {
            period != baseline_period && period.0 > baseline.billing_period_start_epoch
        });
        let rolled_closed = spend.closed_period_anchor.as_ref().is_some_and(|closed| {
            closed.billing_period_end_epoch > baseline.billing_period_start_epoch
                && anchor_period
                    .is_some_and(|period| period.0 > baseline.billing_period_start_epoch)
        });
        if !(rolled_current || rolled_anchor || rolled_closed) {
            continue;
        }

        if !spend.rollover_unknown || spend.cumulative_complete {
            spend.rollover_unknown = true;
            spend.cumulative_complete = false;
            changed_goals.insert(goal_id.clone());
        }
        let horizon = current
            .filter(|record| {
                record.billing_period_start_epoch > baseline.billing_period_start_epoch
            })
            .map(|record| record.billing_period_start_epoch)
            .into_iter()
            .chain(
                anchor
                    .filter(|record| {
                        record.billing_period_start_epoch > baseline.billing_period_start_epoch
                    })
                    .map(|record| record.billing_period_start_epoch),
            )
            .chain(
                spend
                    .closed_period_anchor
                    .as_ref()
                    .filter(|record| {
                        record.billing_period_end_epoch > baseline.billing_period_start_epoch
                    })
                    .map(|record| record.billing_period_end_epoch),
            )
            .filter(|epoch| *epoch <= last_now_epoch)
            .max();
        if let Some(horizon) = horizon {
            account_horizons
                .entry(goal.account_id.clone())
                .and_modify(|latest| *latest = (*latest).max(horizon))
                .or_insert(horizon);
        }
    }

    for (account_id, horizon) in account_horizons {
        if let Some(account) = state.accounts.get_mut(&account_id) {
            account.spend.historical_correction_horizon_epoch = Some(
                account
                    .spend
                    .historical_correction_horizon_epoch
                    .unwrap_or(0)
                    .max(horizon),
            );
        }
    }
    changed_goals
}

/// Atomically publish one bounded state snapshot with mode `0600`.
pub(super) fn save(dir: &File, state: &StoreState) -> Result<(), MonitorIssue> {
    let bytes = serde_json::to_vec(state).map_err(|_| unavailable())?;
    if bytes.len() > MAX_STORE_BYTES {
        return Err(unavailable());
    }

    // Refuse a symlink or foreign-mode existing target before replacing it.
    match openat(
        dir,
        STORE_FILE,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    ) {
        Ok(fd) => validate_owned_file(&File::from(fd))?,
        Err(nix::errno::Errno::ENOENT) => {}
        Err(_) => return Err(unavailable()),
    }

    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = format!(".state.{}.{}.tmp", std::process::id(), counter);
    let fd = openat(
        dir,
        temporary.as_str(),
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW,
        Mode::from_bits_truncate(STORE_MODE as u16),
    )
    .map_err(|_| unavailable())?;
    let mut file = File::from(fd);
    let result = (|| {
        fchmod(&file, Mode::from_bits_truncate(STORE_MODE as u16)).map_err(|_| unavailable())?;
        validate_owned_file(&file)?;
        file.write_all(&bytes).map_err(|_| unavailable())?;
        file.sync_all().map_err(|_| unavailable())?;
        renameat(dir, temporary.as_str(), dir, STORE_FILE).map_err(|_| unavailable())?;
        fsync(dir).map_err(|_| unavailable())
    })();
    if result.is_err() {
        let _ignored = unlinkat(dir, temporary.as_str(), UnlinkatFlags::NoRemoveDir);
    }
    result
}

fn validate_owned_dir(dir: &File, expected_mode: u32) -> Result<(), MonitorIssue> {
    let metadata = dir.metadata().map_err(|_| unavailable())?;
    if !metadata.is_dir()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != expected_mode
    {
        return Err(unavailable());
    }
    Ok(())
}

fn validate_owned_file(file: &File) -> Result<(), MonitorIssue> {
    let metadata = file.metadata().map_err(|_| unavailable())?;
    if !metadata.is_file()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != STORE_MODE
        || metadata.nlink() != 1
    {
        return Err(unavailable());
    }
    Ok(())
}

fn unavailable() -> MonitorIssue {
    MonitorIssue {
        code: MonitorIssueCode::MonitorStoreUnavailable,
        message: "monitor state is unavailable or invalid".to_owned(),
        retry_at_epoch: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::{
        AccountObservations, DurableGoalSpend, SpendAccountState, SpendState, StoreState,
    };
    use jackin_protocol::control::Money;
    use jackin_protocol::usage_monitor::{
        MonitorAccountBinding, MonitorPolicy, MonitorPolicyOrigin, MonitorPolicyRecord,
        MonitorProvider, SpendRecord, SpendRecordSource, SpendVerification,
    };
    use std::os::unix::fs::OpenOptionsExt as _;

    fn legacy_snapshot(version: u16) -> Vec<u8> {
        let mut state = StoreState::default();
        state.schema_version = version;
        state.last_now_epoch = 10;
        state.next_binding_id = 2;
        state
            .accounts
            .insert("account-1".to_owned(), AccountObservations::default());
        state.bindings.insert(
            "binding-00000001".to_owned(),
            vec![MonitorAccountBinding {
                binding_id: "binding-00000001".to_owned(),
                provider: MonitorProvider::Claude,
                account_id: "account-1".to_owned(),
                provider_account_id: None,
                experimental_collector_approved: false,
                operator_label: "migrated".to_owned(),
                revision: 1,
                operator_confirmed: true,
                confirmed_at_epoch: Some(10),
            }],
        );

        let mut value = serde_json::to_value(state).expect("serialize current-shaped fixture");
        let binding = &mut value["bindings"]["binding-00000001"][0];
        binding
            .as_object_mut()
            .expect("binding object")
            .remove("provider_account_id");
        binding
            .as_object_mut()
            .expect("binding object")
            .remove("experimental_collector_approved");
        value["accounts"]["account-1"]["spend"]
            .as_object_mut()
            .expect("spend account object")
            .remove("historical_correction_horizon_epoch");
        serde_json::to_vec(&value).expect("serialize old snapshot")
    }

    fn write_snapshot(data_dir: &Path, bytes: &[u8]) -> (File, std::path::PathBuf) {
        let directory = open_store_dir(data_dir).expect("open private store directory");
        let path = data_dir
            .join(crate::BROKER_DIR)
            .join(STORE_DIR)
            .join(STORE_FILE);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(STORE_MODE)
            .open(&path)
            .expect("create owner-only snapshot");
        file.write_all(bytes).expect("write snapshot fixture");
        file.sync_all().expect("sync snapshot fixture");
        (directory, path)
    }

    #[test]
    fn v2_and_v3_default_new_source_and_horizon_fields_off() {
        for version in [2, 3] {
            let state = migrate_v2_or_v3(&legacy_snapshot(version), version)
                .expect("old schema should migrate safely");
            assert_eq!(state.schema_version, USAGE_MONITOR_SCHEMA_VERSION);
            let binding = &state.bindings["binding-00000001"][0];
            assert_eq!(binding.provider_account_id, None);
            assert!(!binding.experimental_collector_approved);
            assert_eq!(
                state.accounts["account-1"]
                    .spend
                    .historical_correction_horizon_epoch,
                None
            );
        }
    }

    #[test]
    fn v3_latches_only_rolled_strict_spend_and_preserves_estimate() {
        let baseline = SpendRecord {
            account_id: "account-1".to_owned(),
            billing_period_start_epoch: 100,
            billing_period_end_epoch: 200,
            amount: Money::new(7_000, "SGD", 2),
            evidence_at_epoch: Some(150),
            evidence_received_at_epoch: 150,
            source: SpendRecordSource::OperatorReceipt,
            verification: SpendVerification::Verified,
        };
        let current = SpendRecord {
            account_id: "account-1".to_owned(),
            billing_period_start_epoch: 200,
            billing_period_end_epoch: 300,
            amount: Money::new(200, "SGD", 2),
            evidence_at_epoch: Some(240),
            evidence_received_at_epoch: 240,
            source: SpendRecordSource::OperatorReceipt,
            verification: SpendVerification::Verified,
        };
        let mut state = StoreState {
            last_now_epoch: 250,
            ..StoreState::default()
        };
        state.accounts.insert(
            "account-1".to_owned(),
            AccountObservations {
                spend: SpendAccountState {
                    latest_record: Some(current.clone()),
                    current_period_record: Some(current.clone()),
                    previous_period_record: None,
                    historical_correction_horizon_epoch: None,
                },
                ..AccountObservations::default()
            },
        );
        state.goals.insert(
            "goal-1".to_owned(),
            DurableGoalSpend {
                account_id: "account-1".to_owned(),
                binding_id: "binding-00000001".to_owned(),
                binding_revision: 1,
                policy_revision: 1,
                policy: MonitorPolicy::StrictSgd,
                budget: Some(Money::new(10_000, "SGD", 2)),
                spend_state: Some(SpendState {
                    baseline: Some(baseline),
                    period_anchor: Some(current.clone()),
                    closed_period_anchor: None,
                    cumulative_goal_spend: Some(Money::new(7_200, "SGD", 2)),
                    rollover_unknown: false,
                    cumulative_complete: true,
                }),
            },
        );

        let changed = latch_ambiguous_v3_rollovers(&mut state);
        assert_eq!(changed, BTreeSet::from(["goal-1".to_owned()]));
        let spend = state.goals["goal-1"]
            .spend_state
            .as_ref()
            .expect("goal spend state");
        assert_eq!(
            spend.cumulative_goal_spend,
            Some(Money::new(7_200, "SGD", 2))
        );
        assert!(spend.rollover_unknown);
        assert!(!spend.cumulative_complete);
        assert_eq!(
            state.accounts["account-1"]
                .spend
                .historical_correction_horizon_epoch,
            Some(200)
        );
    }

    #[test]
    fn load_atomically_rewrites_v3_and_fails_closed_without_overwriting_invalid_state() {
        let temp = tempfile::tempdir().expect("temporary fixture root");
        let data_dir = temp.path().join("data");
        let (directory, path) = write_snapshot(&data_dir, &legacy_snapshot(3));

        let migrated = load(&directory)
            .expect("load migrated v3 state")
            .expect("snapshot exists");
        assert_eq!(migrated.schema_version, USAGE_MONITOR_SCHEMA_VERSION);
        let rewritten = std::fs::read(&path).expect("read published v4 state");
        let value: serde_json::Value =
            serde_json::from_slice(&rewritten).expect("valid published JSON");
        assert_eq!(value["schema_version"], USAGE_MONITOR_SCHEMA_VERSION);
        assert_eq!(
            std::fs::metadata(&path).expect("state metadata").mode() & 0o777,
            STORE_MODE
        );
        assert_eq!(
            std::fs::read_dir(path.parent().expect("store directory"))
                .expect("read store directory")
                .count(),
            1,
            "atomic rename leaves no staging file"
        );

        let bad_temp = tempfile::tempdir().expect("invalid fixture root");
        let bad_data_dir = bad_temp.path().join("data");
        let mut malformed: serde_json::Value =
            serde_json::from_slice(&legacy_snapshot(3)).expect("parse legacy fixture");
        malformed["accounts"]["account-1"]["spend"]["historical_correction_horizon_epoch"] =
            serde_json::json!(-1);
        let malformed = serde_json::to_vec(&malformed).expect("serialize malformed fixture");
        let (bad_directory, bad_path) = write_snapshot(&bad_data_dir, &malformed);
        assert!(load(&bad_directory).is_err());
        assert_eq!(
            std::fs::read(&bad_path).expect("invalid source remains inspectable"),
            malformed,
            "invalid source is not replaced by a migrated snapshot"
        );

        let future_temp = tempfile::tempdir().expect("future fixture root");
        let future_data_dir = future_temp.path().join("data");
        let mut future: serde_json::Value =
            serde_json::from_slice(&legacy_snapshot(3)).expect("parse legacy fixture");
        future["schema_version"] = serde_json::json!(5);
        let future = serde_json::to_vec(&future).expect("serialize future fixture");
        let (future_directory, future_path) = write_snapshot(&future_data_dir, &future);
        assert!(load(&future_directory).is_err());
        assert_eq!(
            std::fs::read(&future_path).expect("future source remains untouched"),
            future
        );
    }

    #[test]
    fn load_rejects_and_preserves_v3_policy_history_that_loosens_a_strict_budget() {
        let mut state: StoreState =
            serde_json::from_slice(&legacy_snapshot(3)).expect("parse v3 snapshot fixture");
        state.policy_records.insert(
            "goal-history-test".to_owned(),
            vec![
                MonitorPolicyRecord {
                    provider: MonitorProvider::Claude,
                    account_id: "account-1".to_owned(),
                    binding_id: Some("binding-00000001".to_owned()),
                    binding_revision: Some(1),
                    goal_id: "goal-history-test".to_owned(),
                    previous_policy: None,
                    new_policy: MonitorPolicy::StrictSgd,
                    budget: Some(Money::new(5_000, "SGD", 2)),
                    operator_label: Some("operator".to_owned()),
                    operator_confirmed: true,
                    acknowledge_no_sgd_cap: false,
                    recorded_at_epoch: Some(2),
                    revision: 1,
                    origin: MonitorPolicyOrigin::Operator,
                },
                MonitorPolicyRecord {
                    provider: MonitorProvider::Claude,
                    account_id: "account-1".to_owned(),
                    binding_id: Some("binding-00000001".to_owned()),
                    binding_revision: Some(1),
                    goal_id: "goal-history-test".to_owned(),
                    previous_policy: Some(MonitorPolicy::StrictSgd),
                    new_policy: MonitorPolicy::StrictSgd,
                    budget: Some(Money::new(5_001, "SGD", 2)),
                    operator_label: Some("operator".to_owned()),
                    operator_confirmed: true,
                    acknowledge_no_sgd_cap: false,
                    recorded_at_epoch: Some(3),
                    revision: 2,
                    origin: MonitorPolicyOrigin::Operator,
                },
            ],
        );
        let bytes = serde_json::to_vec(&state).expect("serialize invalid old-schema fixture");
        let temp = tempfile::tempdir().expect("temporary fixture root");
        let data_dir = temp.path().join("data");
        let (directory, path) = write_snapshot(&data_dir, &bytes);

        assert!(
            load(&directory).is_err(),
            "invalid history must fail closed"
        );
        assert_eq!(
            std::fs::read(&path).expect("invalid v3 source remains inspectable"),
            bytes,
            "invalid policy history must not be restamped as schema 4"
        );
    }
}
