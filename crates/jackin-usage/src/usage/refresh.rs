// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Refresh orchestration: scheduling, cooldown, shared filesystem sync.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.

#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;
use serde::Deserialize;

pub(crate) static MATERIALIZED_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn usage_refresh_error_type(error: &str) -> &'static str {
    if usage_error_is_rate_limited(error) {
        "usage_http_status"
    } else if error.to_ascii_lowercase().contains("http") {
        "usage_http_request_failed"
    } else {
        "usage_provider_failed"
    }
}

pub(crate) fn collect_usage_refresh_results<F>(
    due_targets: Vec<UsageRefreshTarget>,
    probe: F,
) -> Vec<UsageRefreshResult>
where
    F: Fn(UsageRefreshTarget) -> UsageRefreshResult + Send + Sync + 'static,
{
    collect_usage_refresh_results_with_timeout(due_targets, probe, PROVIDER_PROBE_TIMEOUT)
}

pub(crate) fn collect_usage_refresh_results_with_timeout<F>(
    due_targets: Vec<UsageRefreshTarget>,
    probe: F,
    timeout: Duration,
) -> Vec<UsageRefreshResult>
where
    F: Fn(UsageRefreshTarget) -> UsageRefreshResult + Send + Sync + 'static,
{
    let probe = Arc::new(probe);
    let (tx, rx) = mpsc::channel();
    let mut pending = due_targets
        .iter()
        .map(UsageRefreshTarget::cache_key)
        .collect::<std::collections::HashSet<_>>();
    let fallback_targets = due_targets
        .iter()
        .map(|target| (target.cache_key(), target.clone()))
        .collect::<HashMap<_, _>>();
    let expected = due_targets.len();
    for target in due_targets {
        let tx = tx.clone();
        let probe = Arc::clone(&probe);
        jackin_telemetry::spawn::thread_joined(move || {
            // One span per provider probe so the refresh lifecycle is visible in
            // telemetry — each provider's fetch duration (e.g. the slow Amp CLI
            // fallback) shows directly instead of being lost in the render
            // firehose (Class VI: the usage path had no spans).
            let agent = target.agent.clone();
            let provider = target.provider.clone().unwrap_or_default();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let span = tracing::info_span!(
                    "usage.provider_probe",
                    otel.name = "usage:provider_probe",
                    agent = %agent,
                    provider = %provider,
                );
                span.in_scope(|| probe(target))
            }));
            match result {
                Ok(result) => {
                    if let Some(error) = result.view.last_error.as_deref() {
                        jackin_diagnostics::operation_error(
                            "usage.refresh",
                            usage_refresh_error_type(error),
                            "usage provider refresh failed",
                            &[],
                        );
                    }
                    drop(tx.send(result));
                }
                Err(_) => {
                    jackin_diagnostics::telemetry_info!(
                        "capsule",
                        "usage-refresh: provider probe panicked"
                    );
                }
            }
        });
    }
    drop(tx);

    let started = Instant::now();
    let mut results = Vec::new();
    while results.len() < expected {
        let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
            break;
        };
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok(result) => {
                pending.remove(&result.target.cache_key());
                results.push(result);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    if !pending.is_empty() {
        let now = now_epoch();
        for key in pending {
            let Some(target) = fallback_targets.get(&key).cloned() else {
                continue;
            };
            jackin_diagnostics::telemetry_info!(
                "capsule",
                "usage-refresh: provider probe timed out for {}",
                target.cache_key()
            );
            let span = jackin_diagnostics::operation_span("usage.refresh", &[]);
            span.in_scope(|| {
                jackin_diagnostics::operation_error(
                    "usage.refresh",
                    "usage_provider_timeout",
                    "usage provider refresh timed out",
                    &[],
                );
            });
            let mut view = cached_unavailable_view(&target.agent, target.provider.as_deref(), now);
            view.last_error = Some("usage provider probe timed out".to_owned());
            results.push(UsageRefreshResult {
                target,
                view,
                codex_rpc_gate: ManagedCliLaunchGate::default(),
                grok_rpc_gate: ManagedCliLaunchGate::default(),
            });
        }
    }
    results
}

/// Log a persistence outcome at the right tier: always-on governed INFO event once when a
/// fault starts and once when it clears, plus a per-cycle governed DEBUG events firehose
/// line while it persists. Returns the new "failed" latch for the caller to store.
pub(crate) fn log_persist_transition(
    what: &str,
    was_failed: bool,
    result: Result<(), String>,
) -> bool {
    match result {
        Ok(()) => {
            if was_failed {
                jackin_diagnostics::telemetry_info!("capsule", "{what} recovered");
            }
            false
        }
        Err(error) => {
            if !was_failed {
                jackin_diagnostics::telemetry_info!(
                    "capsule",
                    "{what} failed (suppressing repeats until recovery): {error}"
                );
            }
            jackin_diagnostics::telemetry_debug!("capsule", "{what} failed: {error}");
            true
        }
    }
}

pub(crate) fn ordered_refresh_targets(
    active_targets: &[UsageRefreshTarget],
    focused: Option<UsageRefreshTarget>,
) -> Vec<UsageRefreshTarget> {
    let mut seen = std::collections::HashSet::new();
    let mut targets = Vec::new();
    if let Some(target) = focused
        && seen.insert(target.cache_key())
    {
        targets.push(target);
    }
    for target in active_targets {
        if seen.insert(target.cache_key()) {
            targets.push(target.clone());
        }
    }
    targets
}

pub(crate) fn refresh_interval_for_key(key: &str) -> Duration {
    let jitter_span = USAGE_REFRESH_JITTER.as_secs().saturating_mul(2);
    let hash = stable_usage_hash(key);
    let offset = hash % (jitter_span.saturating_add(1));
    let min = USAGE_REFRESH_BASE_INTERVAL.saturating_sub(USAGE_REFRESH_JITTER);
    min + Duration::from_secs(offset)
}

pub(crate) fn shared_usage_cooldown_dir() -> PathBuf {
    env_dir_or_home(
        "JACKIN_USAGE_COOLDOWN_DIR",
        ".jackin/data/daemon/usage-cooldowns",
    )
}

pub(crate) fn shared_usage_snapshots_dir() -> PathBuf {
    env_dir_or_home(
        "JACKIN_USAGE_SNAPSHOTS_DIR",
        ".jackin/data/daemon/usage-snapshots",
    )
}

pub(crate) fn shared_usage_lock_dir() -> PathBuf {
    env_dir_or_home("JACKIN_USAGE_LOCK_DIR", ".jackin/data/daemon/usage-locks")
}

/// Outcome of trying to take the cross-container per-account refresh lock.
#[derive(Debug)]
pub(crate) enum RefreshLockOutcome {
    /// Won the lock — hold the handle for the refresh + shared-write window.
    Acquired(fs::File),
    /// Locking infra is absent (no shared volume / lock dir): proceed without a
    /// lock so single-container/dev runs still refresh. Best-effort, never gates.
    Unavailable,
    /// Another instance holds the lock (it is refreshing this account now): skip
    /// the network and rely on the shared snapshot already seeded into memory.
    Held,
}

/// Try to take the account's exclusive refresh lock (non-blocking). `flock` on
/// the bind-mounted shared volume shares the inode across same-kernel containers,
/// so exactly one instance refreshes a given account while the rest read the
/// shared snapshot — ending the N×-instances rate-limit storm (Class III-D). A
/// stale lock self-heals: `flock` releases when the holding process exits.
pub(crate) fn acquire_account_refresh_lock(account_key: &str) -> RefreshLockOutcome {
    acquire_account_refresh_lock_in(&shared_usage_lock_dir(), account_key)
}

pub(crate) fn acquire_account_refresh_lock_in(dir: &Path, account_key: &str) -> RefreshLockOutcome {
    use fs4::FileExt;
    if fs::create_dir_all(dir).is_err() {
        return RefreshLockOutcome::Unavailable;
    }
    let path = shared_usage_file_path(dir, account_key, "lock");
    #[expect(
        clippy::disallowed_methods,
        reason = "advisory lock file; the usage refresh runs on the blocking pool (spawn_blocking), not the render thread"
    )]
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path);
    match file {
        Ok(file) => match FileExt::try_lock(&file) {
            Ok(()) => RefreshLockOutcome::Acquired(file),
            Err(_) => RefreshLockOutcome::Held,
        },
        Err(_) => RefreshLockOutcome::Unavailable,
    }
}

/// `<dir>/usage-<account-hash>.<ext>` — the per-account shared-file naming scheme
/// shared by the snapshot, cooldown marker, and refresh lock. Centralized so all
/// three hash the account key the same way; cross-container files for one account
/// must collide on name for the coordination to work (Class III).
pub(crate) fn shared_usage_file_path(dir: &Path, key: &str, ext: &str) -> PathBuf {
    dir.join(format!("usage-{:016x}.{ext}", stable_usage_hash(key)))
}

pub(crate) fn shared_usage_snapshot_path(snapshots_dir: &Path, key: &str) -> PathBuf {
    shared_usage_file_path(snapshots_dir, key, "snapshot.json")
}

pub(crate) fn write_shared_usage_snapshot(
    snapshots_dir: &Path,
    key: &str,
    view: &FocusedUsageView,
) {
    let Ok(json) = serde_json::to_string(view) else {
        return;
    };
    if let Err(error) = fs::create_dir_all(snapshots_dir) {
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "usage snapshot dir create failed for {key}: {error}"
        );
        return;
    }
    let path = shared_usage_snapshot_path(snapshots_dir, key);
    if let Err(error) = fs::write(path, json) {
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "usage snapshot write failed for {key}: {error}"
        );
    }
}

pub(crate) fn read_shared_usage_snapshot(
    snapshots_dir: &Path,
    key: &str,
) -> Option<FocusedUsageView> {
    let path = shared_usage_snapshot_path(snapshots_dir, key);
    let json = fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

pub(crate) fn shared_usage_cooldown_marker_path(cooldown_dir: &Path, key: &str) -> PathBuf {
    shared_usage_file_path(cooldown_dir, key, "cooldown")
}

pub(crate) fn shared_usage_cooldown_active(cooldown_dir: &Path, key: &str, now_epoch: i64) -> bool {
    let path = shared_usage_cooldown_marker_path(cooldown_dir, key);
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let Some(first) = text.lines().next() else {
        return false;
    };
    first
        .trim()
        .parse::<i64>()
        .is_ok_and(|until_epoch| until_epoch > now_epoch)
}

/// Like `shared_usage_cooldown_active`, but returns `true` only when the marker
/// represents a mandatory rate-limit (429) backoff, not an advisory success
/// cooldown.  The file format is `{until_epoch}\n{reason}\n`; reason `"ok"`
/// denotes a success marker, any other reason (e.g. the 429 response body) is
/// a rate-limit marker.
pub(crate) fn shared_usage_rate_limit_cooldown_active(
    cooldown_dir: &Path,
    key: &str,
    now_epoch: i64,
) -> bool {
    let path = shared_usage_cooldown_marker_path(cooldown_dir, key);
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let mut lines = text.lines();
    let until = lines
        .next()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(0);
    if until <= now_epoch {
        return false;
    }
    lines.next().map_or("", str::trim) != "ok"
}

pub(crate) fn write_shared_usage_cooldown_marker(
    cooldown_dir: &Path,
    key: &str,
    until_epoch: i64,
    reason: &str,
) {
    if let Err(error) = fs::create_dir_all(cooldown_dir) {
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "usage cooldown marker dir create failed for {key}: {error}"
        );
        return;
    }
    let path = shared_usage_cooldown_marker_path(cooldown_dir, key);
    let reason = reason.replace('\n', " ");
    // A dropped marker means the provider gets re-probed inside its backoff
    // window, so surface the failure rather than silently defeating the 429
    // cooldown.
    if let Err(error) = fs::write(path, format!("{until_epoch}\n{reason}\n")) {
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "usage cooldown marker write failed for {key}: {error}"
        );
    }
}

pub(crate) fn usage_error_is_rate_limited(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("retry-after")
        || lower.contains("retry after")
}

/// True when a provider fetch failed because the token was rejected (expired or
/// revoked), as opposed to a transient/network error. Drives the honest
/// `NeedsLogin` status so a stale on-disk token reads as "login", not "stale".
pub(crate) fn usage_error_is_unauthorized(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("http 401") || lower.contains("http 403") || lower.contains("unauthorized")
}

pub(crate) fn usage_rate_limit_delay(error: &str, failures: u32) -> Duration {
    let lower = error.to_ascii_lowercase();
    parse_retry_after_seconds(&lower)
        .map_or_else(
            || usage_backoff_delay(USAGE_REFRESH_BASE_INTERVAL, failures),
            Duration::from_secs,
        )
        .min(USAGE_REFRESH_BACKOFF_CAP)
}

pub(crate) fn parse_retry_after_seconds(error: &str) -> Option<u64> {
    for marker in ["retry-after", "retry after"] {
        let Some((_, tail)) = error.split_once(marker) else {
            continue;
        };
        let digits = tail
            .chars()
            .skip_while(|ch| !ch.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        if let Ok(seconds) = digits.parse::<u64>() {
            return Some(seconds);
        }
    }
    None
}

pub(crate) fn usage_backoff_delay(base: Duration, failures: u32) -> Duration {
    let shift = failures.saturating_sub(1).min(8);
    let multiplier = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    Duration::from_secs(base.as_secs().saturating_mul(multiplier)).min(USAGE_REFRESH_BACKOFF_CAP)
}

/// Owned document shape for reading materialized accounts JSON (tests + any
/// future consumers). Write path serializes via `MaterializedUsageAccountsRef`.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "owned Deserialize twin; write path uses MaterializedUsageAccountsRef"
    )
)]
pub(crate) struct MaterializedUsageAccounts {
    pub(crate) generated_at_epoch: i64,
    pub(crate) snapshots: Vec<FocusedUsageView>,
}

#[derive(Serialize)]
struct MaterializedUsageAccountsRef<'a> {
    generated_at_epoch: i64,
    snapshots: &'a [&'a FocusedUsageView],
}

pub(crate) fn write_materialized_usage_accounts(
    path: &Path,
    generated_at_epoch: i64,
    snapshots: &[&FocusedUsageView],
) -> Result<(), String> {
    let document = MaterializedUsageAccountsRef {
        generated_at_epoch,
        snapshots,
    };
    let contents = serde_json::to_string_pretty(&document)
        .map_err(|err| format!("usage accounts encode failed: {err}"))?;
    atomic_write_usage_json(path, &contents)
}

#[expect(
    clippy::disallowed_methods,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn atomic_write_usage_json(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create usage materialization dir failed: {err}"))?;
    }
    let counter = MATERIALIZED_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut staged_name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    staged_name.push(format!(".tmp.{}.{counter}", std::process::id()));
    let tmp = path.with_file_name(staged_name);
    let staged = (|| -> Result<(), String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o644)
                .open(&tmp)
                .map_err(|err| format!("open staged usage accounts failed: {err}"))?;
            file.write_all(contents.as_bytes())
                .map_err(|err| format!("write staged usage accounts failed: {err}"))?;
            file.sync_all()
                .map_err(|err| format!("sync staged usage accounts failed: {err}"))?;
        }

        #[cfg(not(unix))]
        fs::write(&tmp, contents)
            .map_err(|err| format!("write staged usage accounts failed: {err}"))?;

        Ok(())
    })();
    if let Err(error) = staged {
        drop(fs::remove_file(&tmp));
        return Err(error);
    }
    if let Err(error) = fs::rename(&tmp, path) {
        drop(fs::remove_file(&tmp));
        return Err(format!("rename usage accounts into place failed: {error}"));
    }
    Ok(())
}
