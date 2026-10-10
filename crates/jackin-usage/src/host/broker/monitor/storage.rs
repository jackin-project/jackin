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

use super::super::{BROKER_DIR, secure_run_directory};
use super::StoreState;

const STORE_FILE: &str = "state.json";
const STORE_DIR: &str = "monitor";
const STORE_MODE: u32 = 0o600;
const STORE_DIR_MODE: u32 = 0o700;
pub(super) const MAX_STORE_BYTES: usize = 32 * 1024 * 1024;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Open or create the private monitor directory below the host broker state.
pub(super) fn open_store_dir(data_dir: &Path) -> Result<File, MonitorIssue> {
    secure_run_directory(data_dir).map_err(|_| unavailable())?;
    let broker_path = data_dir.join(BROKER_DIR);
    let broker_fd = open(
        &broker_path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| unavailable())?;
    let broker_dir = File::from(broker_fd);
    validate_owned_dir(&broker_dir, STORE_DIR_MODE)?;

    match mkdirat(&broker_dir, STORE_DIR, mode_from_bits(STORE_DIR_MODE)?) {
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
    fchmod(&store_dir, mode_from_bits(STORE_DIR_MODE)?).map_err(|_| unavailable())?;
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
        // semantic validation; `save` uses the existing fsync+rename path.
        save(dir, &state)?;
    }
    Ok(Some(state))
}

/// Upgrade pre-V4 snapshots that already use the current durable state shape.
/// New fields are serde-defaulted. V3 migration preserves known estimates,
/// baselines, and counters while latching uncertainty for rolled strict goals.
/// Event status values carry the monitor schema version too, so restamp those
/// envelopes after upgrading the enclosing store.
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

    // Validate the normalized source before applying migration semantics.
    // This prevents V3 uncertainty latching from repairing a persisted
    // goal/monitor mismatch that should make the source unavailable.
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

/// V3 did not persist an uncertainty horizon for verified receipts that fell
/// outside the retained current/previous-period slots. A rolled strict goal
/// therefore may contain an undercounted but still apparently complete total.
/// Mark only goals whose retained account or goal anchors prove they crossed
/// their baseline period, preserve their estimate, and write an account
/// horizon at the newest retained period start so later receipts cannot clear
/// the uncertainty. This is an uncertainty boundary, not evidence that a
/// historical correction was actually received. A goal beginning in the
/// latest retained period has an equal boundary and remains unaffected.
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
        mode_from_bits(STORE_MODE)?,
    )
    .map_err(|_| unavailable())?;
    let mut file = File::from(fd);
    let result = (|| {
        fchmod(&file, mode_from_bits(STORE_MODE)?).map_err(|_| unavailable())?;
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

fn mode_from_bits(bits: u32) -> Result<Mode, MonitorIssue> {
    let platform_bits = nix::sys::stat::mode_t::try_from(bits).map_err(|_| unavailable())?;
    Mode::from_bits(platform_bits).ok_or_else(unavailable)
}
