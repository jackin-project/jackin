# Task invariant check

Read-only. No files in the tree were edited. Cargo was not run. This check does not approve a merge.

| Field | Value |
| --- | --- |
| Tree | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` |
| HEAD | `7a05f3aebda4a73e183b665702f00bb7a38ce36f` |
| Required commit | `7a05f3ae` or a descendant |
| Ancestry | PASS. `git merge-base --is-ancestor 7a05f3aebda4a73e183b665702f00bb7a38ce36f HEAD` exited 0. HEAD is that commit. |
| Worktree | `git status --porcelain` was empty. |
| Result | 7 PASS, 0 FAIL |
| Merge | Not approved. |

## 1. `should_exit_idle` — PASS

`crates/services/jackin-usage-broker/src/serve.rs:521`

Returns true only when idle time has elapsed, the coordinator is idle, there is no active monitor, and there is no authenticated foreground guard.

```rust
fn should_exit_idle(
    now_monotonic: Duration,
    last_activity_monotonic: Duration,
    idle_exit: Duration,
    coordinator_idle: bool,
    has_active_monitor: bool,
    has_authenticated_foreground_guard: bool,
) -> bool {
    now_monotonic.saturating_sub(last_activity_monotonic) >= idle_exit
        && coordinator_idle
        && !has_active_monitor
        && !has_authenticated_foreground_guard
}
```

## 2. Monitor schema version and unknown newer schemas — PASS

`crates/core/jackin-protocol/src/usage_monitor.rs:11`

```rust
pub const USAGE_MONITOR_SCHEMA_VERSION: u16 = 4;
```

`crates/services/jackin-usage-broker/src/monitor/storage.rs:98`

The `load()` match accepts 1, 2, 3, and schema 4. Every other version, including a newer schema, returns `unavailable()` from the `_` arm. That return happens before the later `save()` at lines 107–110, so the file is not rewritten.

```rust
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
```

## 3. Broker protocol version — PASS

`crates/core/jackin-protocol/src/usage_broker.rs:14`

```rust
pub const USAGE_BROKER_PROTOCOL_VERSION: &str = "v8";
```

`crates/services/jackin-usage-broker/src/dispatch.rs:86`

A `protocol_version` other than `v8` makes the conjunction false, so `request_header_matches_current_protocol` fails.

```rust
fn request_header_matches_current_protocol(
    protocol_version: &str,
    request_build_id: &str,
    build_id: &str,
) -> bool {
    protocol_version == USAGE_BROKER_PROTOCOL_VERSION && request_build_id == build_id
}
```

## 4. `lease_path_matches_open_file` — PASS

`crates/services/jackin-usage-broker/src/leader.rs:237`

Requires `nlink != 0`, both metadata objects to be regular files, the path not to be a symlink, and equal device and inode.

```rust
fn lease_path_matches_open_file(path: &Path, file: &File) -> bool {
    let Ok(descriptor_metadata) = file.metadata() else {
        return false;
    };
    let Ok(path_metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    descriptor_metadata.nlink() != 0
        && descriptor_metadata.is_file()
        && path_metadata.is_file()
        && !path_metadata.file_type().is_symlink()
        && path_metadata.dev() == descriptor_metadata.dev()
        && path_metadata.ino() == descriptor_metadata.ino()
}
```

## 5. `used_percent_clamped` — PASS

`crates/services/jackin-usage-provider-codex/src/types.rs:96`

A negative rounded Codex percent returns `None`.

```rust
pub fn used_percent_clamped(&self) -> Option<u8> {
    let used = self.used_percent_raw()?;
    if used < 0.0 {
        return None;
    }
    #[expect(
        clippy::cast_sign_loss,
        reason = "non-negative values clamped into 0.0..=100.0"
    )]
    Some(used.min(100.0) as u8)
}
```

## 6. `ServiceStopped` clears the experimental collector source — PASS

`crates/services/jackin-usage-broker/src/monitor/operations.rs:87` returns the reply. `crates/services/jackin-usage-broker/src/dispatch.rs:169` clears the source when that reply is `ServiceStopped`.

```rust
MonitorOperation::ServiceStop => Ok(MonitorReply::ServiceStopped),
```

```rust
let stopping = matches!(request, MonitorOperation::ServiceStop);
let reply = match monitor_store.operate(request.clone(), now) {
    Ok(reply) => reply,
    Err(issue) => return Some(UsageBrokerResponse::MonitorError { issue }),
};
if stopping && matches!(reply, MonitorReply::ServiceStopped) {
    shutdown.store(true, Ordering::Release);
    monitor_store.set_experimental_collector_source(None);
}
```

`crates/services/jackin-usage-broker/src/monitor/operations.rs:258` stores that `Option`, so `None` clears the field:

```rust
pub(crate) fn set_experimental_collector_source(&self, source: Option<String>) {
    let _admission = self.lock_collector_admission();
    *self
        .inner
        .experimental_collector_source
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = source;
}
```

## 7. `reclaim_abandoned_lock` directory sync order — PASS

`crates/services/jackin-isolation/src/ref_transaction.rs:401`

The parent directory is synced after the lock unlink and before the journal-candidate unlink.

```rust
unlinkat(parent, leaf, UnlinkatFlags::NoRemoveDir)?;
// Persist disappearance of the conventional lock before removing its
// journaled hard-link witness, matching `Lock::cleanup`.
sync_directory(parent)?;
unlinkat(&journal, *candidate_name, UnlinkatFlags::NoRemoveDir)?;
sync_directory(journal.as_fd())?;
```

`crates/services/jackin-isolation/src/ref_transaction.rs:827` defines that sync as `sync_all` on the directory descriptor:

```rust
fn sync_directory(fd: BorrowedFd<'_>) -> anyhow::Result<()> {
    File::from(fd.try_clone_to_owned()?)
        .sync_all()
        .context("cannot synchronize ref transaction directory")
}
```

## Decision

Merge is not approved.
