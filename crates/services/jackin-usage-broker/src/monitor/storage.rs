// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
use std::cell::Cell;
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
use serde::Deserialize;

use super::resetless::{PairSource, valid_optional_source_receipt};
use super::{Observed, ObservedPercentage, StoreState, valid_bounded_text};

const STORE_FILE: &str = "state.json";
const STORE_DIR: &str = "monitor";
const STORE_MODE: u32 = 0o600;
const STORE_DIR_MODE: u32 = 0o700;
pub(super) const MAX_STORE_BYTES: usize = 32 * 1024 * 1024;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveFailurePoint {
    BeforeRename,
    AfterRename,
}

#[cfg(test)]
thread_local! {
    static SAVE_FAILURE_POINT: Cell<Option<SaveFailurePoint>> = const { Cell::new(None) };
}

#[cfg(test)]
pub(super) fn set_test_save_failure_point(point: SaveFailurePoint) {
    SAVE_FAILURE_POINT.with(|failure| failure.set(Some(point)));
}

#[cfg(test)]
fn fail_at_test_point(point: SaveFailurePoint) -> bool {
    SAVE_FAILURE_POINT.with(|failure| {
        if failure.get() == Some(point) {
            failure.set(None);
            true
        } else {
            false
        }
    })
}

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

    match mkdirat(&broker_dir, STORE_DIR, Mode::from_bits_truncate(0o700)) {
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
    fchmod(&store_dir, Mode::from_bits_truncate(0o700)).map_err(|_| unavailable())?;
    validate_owned_dir(&store_dir, STORE_DIR_MODE)?;
    Ok(store_dir)
}

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
        4 => migrate_v4(&bytes)?,
        USAGE_MONITOR_SCHEMA_VERSION => {
            serde_json::from_slice(&bytes).map_err(|_| unavailable())?
        }
        _ => return Err(unavailable()),
    };
    super::validate_store_state(&state)?;
    if version != USAGE_MONITOR_SCHEMA_VERSION {
        save(dir, &state)?;
    }
    Ok(Some(state))
}

fn migrate_v2_or_v3(bytes: &[u8], version: u16) -> Result<StoreState, MonitorIssue> {
    let prepared = insert_resetless_metadata(bytes, version)?;
    let mut state: StoreState = serde_json::from_slice(&prepared).map_err(|_| unavailable())?;
    state.schema_version = USAGE_MONITOR_SCHEMA_VERSION;
    migrate_event_versions(&mut state, version)?;

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

fn migrate_v4(bytes: &[u8]) -> Result<StoreState, MonitorIssue> {
    let prepared = insert_resetless_metadata(bytes, 4)?;
    let mut state: StoreState = serde_json::from_slice(&prepared).map_err(|_| unavailable())?;
    state.schema_version = USAGE_MONITOR_SCHEMA_VERSION;
    migrate_event_versions(&mut state, 4)?;
    super::validate_store_state(&state)?;
    Ok(state)
}

fn insert_resetless_metadata(bytes: &[u8], expected_version: u16) -> Result<Vec<u8>, MonitorIssue> {
    let mut value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| unavailable())?;
    if value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(u64::from(expected_version))
    {
        return Err(unavailable());
    }
    let last_now_epoch = value
        .get("last_now_epoch")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(unavailable)?;
    let next_input_sequence = value
        .get("next_input_sequence")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(unavailable)?;
    normalize_legacy_pair_metadata(&mut value, next_input_sequence, last_now_epoch)?;
    let accounts = value
        .get_mut("accounts")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(unavailable)?;
    for account in accounts.values_mut() {
        let account = account.as_object_mut().ok_or_else(unavailable)?;
        if account.contains_key("reset_admission_fences")
            || account.contains_key("used_only_admission_sequences")
        {
            return Err(unavailable());
        }
        let unknown = serde_json::json!({
            "state": "unknown",
            "after_epoch": last_now_epoch,
        });
        account.insert(
            "reset_admission_fences".to_owned(),
            serde_json::Value::Array(vec![unknown.clone(), unknown]),
        );
        account.insert(
            "used_only_admission_sequences".to_owned(),
            serde_json::json!([null, null]),
        );
    }
    serde_json::to_vec(&value).map_err(|_| unavailable())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyObservedWindow {
    used: Option<ObservedPercentage>,
    reset: Option<Observed<i64>>,
    paired: Option<LegacyObservedQuotaPair>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyObservedQuotaPair {
    used_percentage_basis_points: i32,
    reset_at_epoch: i64,
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
    claude_code_version: Option<String>,
}

fn normalize_legacy_pair_metadata(
    value: &mut serde_json::Value,
    max_sequence: u64,
    now_epoch: i64,
) -> Result<(), MonitorIssue> {
    let accounts = value
        .get_mut("accounts")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(unavailable)?;
    for account_value in accounts.values_mut() {
        let account = account_value.as_object_mut().ok_or_else(unavailable)?;
        let account_sequence = account
            .get("input_sequence")
            .and_then(serde_json::Value::as_u64)
            .filter(|sequence| *sequence <= max_sequence)
            .ok_or_else(unavailable)?;
        let broker_windows = account
            .get_mut("broker_windows")
            .and_then(serde_json::Value::as_array_mut)
            .filter(|windows| windows.len() == 2)
            .ok_or_else(unavailable)?;
        for window in broker_windows {
            normalize_legacy_window(
                window,
                account_sequence,
                now_epoch,
                PairSource::BrokerProjection,
            )?;
        }
        let sessions = account
            .get_mut("sessions")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(unavailable)?;
        for session in sessions.values_mut() {
            let windows = session
                .get_mut("windows")
                .and_then(serde_json::Value::as_array_mut)
                .filter(|windows| windows.len() == 2)
                .ok_or_else(unavailable)?;
            for window in windows {
                normalize_legacy_window(
                    window,
                    account_sequence,
                    now_epoch,
                    PairSource::Statusline,
                )?;
            }
        }
    }
    if let Some(unbound_sessions) = value.get_mut("unbound_sessions") {
        let sessions = unbound_sessions.as_object_mut().ok_or_else(unavailable)?;
        for session in sessions.values_mut() {
            let windows = session
                .get_mut("windows")
                .and_then(serde_json::Value::as_array_mut)
                .filter(|windows| windows.len() == 2)
                .ok_or_else(unavailable)?;
            for window in windows {
                normalize_legacy_window(window, max_sequence, now_epoch, PairSource::Statusline)?;
            }
        }
    }
    Ok(())
}

fn normalize_legacy_window(
    value: &mut serde_json::Value,
    max_sequence: u64,
    now_epoch: i64,
    pair_source: PairSource,
) -> Result<(), MonitorIssue> {
    let legacy: LegacyObservedWindow =
        serde_json::from_value(value.clone()).map_err(|_| unavailable())?;
    let Some(pair) = legacy.paired.as_ref() else {
        return Ok(());
    };
    if !valid_legacy_pair(pair, max_sequence, now_epoch) {
        return Err(unavailable());
    }
    let coherent = legacy_pair_matches_fields(pair, legacy.used.as_ref(), legacy.reset.as_ref())
        && legacy_pair_source_shape_matches(
            legacy.used.as_ref(),
            legacy.reset.as_ref(),
            pair_source,
        );
    let window = value.as_object_mut().ok_or_else(unavailable)?;
    if coherent {
        let used = legacy.used.as_ref().ok_or_else(unavailable)?;
        let reset = legacy.reset.as_ref().ok_or_else(unavailable)?;
        let pair_value = window
            .get_mut("paired")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(unavailable)?;
        pair_value.insert(
            "source_times".to_owned(),
            serde_json::json!([used.evidence_at_epoch, reset.evidence_at_epoch]),
        );
    } else {
        window.insert("paired".to_owned(), serde_json::Value::Null);
    }
    Ok(())
}

fn legacy_pair_source_shape_matches(
    used: Option<&ObservedPercentage>,
    reset: Option<&Observed<i64>>,
    pair_source: PairSource,
) -> bool {
    let (Some(used), Some(reset)) = (used, reset) else {
        return false;
    };
    match pair_source {
        PairSource::BrokerProjection => {
            used.evidence_at_epoch.is_some() && reset.evidence_at_epoch.is_some()
        }
        PairSource::Statusline => {
            used.evidence_at_epoch.is_none() && reset.evidence_at_epoch.is_none()
        }
    }
}

fn valid_legacy_pair(pair: &LegacyObservedQuotaPair, max_sequence: u64, now_epoch: i64) -> bool {
    (0..=10_000).contains(&pair.used_percentage_basis_points)
        && pair.reset_at_epoch >= 0
        && pair.input_sequence > 0
        && valid_optional_source_receipt(
            pair.evidence_at_epoch,
            pair.received_at_epoch,
            pair.input_sequence,
            max_sequence,
            now_epoch,
        )
        && pair
            .claude_code_version
            .as_deref()
            .is_none_or(|version| valid_bounded_text(version, 64))
}

fn legacy_pair_matches_fields(
    pair: &LegacyObservedQuotaPair,
    used: Option<&ObservedPercentage>,
    reset: Option<&Observed<i64>>,
) -> bool {
    let (Some(used), Some(reset)) = (used, reset) else {
        return false;
    };
    let pair_evidence = match (used.evidence_at_epoch, reset.evidence_at_epoch) {
        (Some(used_source), Some(reset_source)) => Some(used_source.min(reset_source)),
        (None, None) => None,
        _ => return false,
    };
    used.reset_at_epoch == Some(reset.value)
        && pair.used_percentage_basis_points == used.value
        && pair.reset_at_epoch == reset.value
        && pair.input_sequence == used.input_sequence
        && pair.input_sequence == reset.input_sequence
        && pair.received_at_epoch == used.received_at_epoch
        && pair.received_at_epoch == reset.received_at_epoch
        && pair.evidence_at_epoch == pair_evidence
        && pair.claude_code_version == used.claude_code_version
        && pair.claude_code_version == reset.claude_code_version
}

fn migrate_event_versions(
    state: &mut StoreState,
    expected_version: u16,
) -> Result<(), MonitorIssue> {
    for monitor in state.monitors.values_mut() {
        for event in &mut monitor.events {
            if event.status.schema_version != expected_version {
                return Err(unavailable());
            }
            event.status.schema_version = USAGE_MONITOR_SCHEMA_VERSION;
        }
    }
    Ok(())
}

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

pub(super) fn save(dir: &File, state: &StoreState) -> Result<(), MonitorIssue> {
    let bytes = serde_json::to_vec(state).map_err(|_| unavailable())?;
    if bytes.len() > MAX_STORE_BYTES {
        return Err(unavailable());
    }

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
        Mode::from_bits_truncate(0o600),
    )
    .map_err(|_| unavailable())?;
    let mut file = File::from(fd);
    let result = (|| {
        fchmod(&file, Mode::from_bits_truncate(0o600)).map_err(|_| unavailable())?;
        validate_owned_file(&file)?;
        file.write_all(&bytes).map_err(|_| unavailable())?;
        file.sync_all().map_err(|_| unavailable())?;
        #[cfg(test)]
        if fail_at_test_point(SaveFailurePoint::BeforeRename) {
            return Err(unavailable());
        }
        renameat(dir, temporary.as_str(), dir, STORE_FILE).map_err(|_| unavailable())?;
        #[cfg(test)]
        if fail_at_test_point(SaveFailurePoint::AfterRename) {
            return Err(unavailable());
        }
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
mod tests;
