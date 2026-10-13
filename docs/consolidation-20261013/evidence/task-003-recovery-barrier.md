# TASK-003 recovery barrier

Tree: `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` at `7a05f3aebda4a73e183b665702f00bb7a38ce36f`. Read-only. No product edit, no `cargo` / `mbx` / `nextest`, no commit.

**Not approved.**

## Symptom

`usage_broker_killed_owner_recovers_once_without_a_herd` (`crates/apps/jackin/tests/usage_broker_e2e/recovery.rs:18`) kills the owner, backdates both persisted timestamps to `now-301`, then waits on eight `recovery-*` children (`recovery.rs:73-74`). Each child panics in `wait_until` with `timed out waiting for barrier` (`crates/apps/jackin/tests/usage_broker_e2e.rs:242-246`). The parent then fails `assert!(child.wait()?.success())` at `recovery.rs:74`.

The firing wait is the recovery retry wait at `usage_broker_e2e.rs:181`. The done barrier at `usage_broker_e2e.rs:195-197` is never entered. All eight children panic on the same predicate, so none write `done-*`.

Raising that retry wait and the done wait from 10s to 30s cannot pass. `retry_at` is about five minutes after the successor loads state. A ~61s failure of the 30s trial is the 30s broker lease plus that 30s retry wait expiring. It is not evidence that the deadline falls inside 30s, and it is not the done barrier.

## Causal chain

1. The owner process calls `refresh` and blocks inside `FileCountingProvider::probe` on `release-owner`, which nothing creates (`usage_broker_e2e.rs:119-139`, `64-68`). `mark_updating` has already persisted `UsageRefreshPhase::Updating` and `provider_invoked_at_epoch` (`finish.rs:36-44`). The owner then writes `owner-active` and parks (`usage_broker_e2e.rs:133-139`). The parent sees that file and kills the process (`recovery.rs:25-28`). The worker never returns, so `finish_failure` (`finish.rs:118-221`, including the second floor at `finish.rs:202-209`) does not run in the killed process.

2. The parent loads that envelope, asserts `phase == Updating` and a fresh invocation timestamp, then writes both `started_at_epoch` and `provider_invoked_at_epoch` as `recovery_now.saturating_sub(301)` (`recovery.rs:36-61`). `FileAccountStateStore::store` persists that envelope (`state/stores.rs:385-394`). Load only rejects future timestamps (`state/validate.rs:54-73`), so the backdate survives on disk.

3. Each recovery child writes `ready-*`, waits until the parent creates `go` (`usage_broker_e2e.rs:141-142`, `recovery.rs:69-72`), then calls `ensure_usage_broker_with_executor`. A dead owner's lease stays claimable only after `renewed_at` is at least `BROKER_LEASE_DURATION` old (`leader.rs:127-137`). That duration is 30s (`jackin-usage-broker-wire/src/consts.rs:15`). Until then connect returns `Unavailable`, and the child retries for up to 90s (`usage_broker_e2e.rs:148-168`). That loop does not call `wait_until`.

4. The leader that wins after the lease loads the backdated `Updating` envelope in `ensure_loaded` (`join.rs:137-190`). `Updating` is active (`jackin-protocol/src/usage_broker.rs:184-186`), so `recovery_pending` is true. The envelope becomes generation 1, `Failed`, `OwnerLost` (`join.rs:155-160`). `retry_deadline` is then:

   - the recovery-mode backoff, which `child_broker_config` caps at 1s (`usage_broker_e2e.rs:250-258`, `join.rs:162-169`);
   - `max` with `minimum_attempt_deadline(capability, provider_invoked_at_epoch)` (`join.rs:170-178`). For `surface_id == "claude"` that is `invoked_at + 300` (`policy.rs:11`, `policy.rs:122-131`). The backdate makes this `recovery_now - 1`, already due;
   - `max` again with `recovery_floor` (`join.rs:179-190`). `recovering_invoked_attempt` is true for every `Updating` record. The floor calls `minimum_attempt_deadline(capability, Some(recovery_epoch))`, and `recovery_epoch` is `SystemMonotonicClock::sample(...).ceil_epoch()`. That clock ignores the fallback epoch and reads wall time (`jobs.rs:123-130`). Result: `ceil(now) + 300`.

   Because reload time is later than the backdated invocation, `ceil(now) + 300` wins. The 1s retry cap and the expired persisted floor do not change `retry_deadline_epoch`. `generation_view` publishes that value as `retry_at_epoch` (`upkeep.rs:202-219`).

5. `request_refresh(..., force: true)` still returns that view. `retry_deadline_epoch > admission_epoch` is true, and a Claude surface does not skip a future retry deadline on `force` (`refresh.rs:110-132`). Generation stays 1, phase stays `Failed`, error stays `OwnerLost`. The child assertions immediately above the wait therefore pass, and `wait_until(Duration::from_secs(10), || epoch_now() >= retry_at)` at `usage_broker_e2e.rs:178-181` starts with `retry_at` about 300 seconds ahead. It panics at `usage_broker_e2e.rs:245`. The same view is served to every client of that leader, so all eight children take this branch.

6. The second `refresh`, `join`, `done-*` write, and done barrier (`usage_broker_e2e.rs:182-197`) are after that panic. `entries_with_prefix(root, "done-")` stays 0. The parent is already blocked in `child.wait()` and fails at `recovery.rs:74`. It never reaches `assert_eq!(entries_with_prefix(root, "provider-call-"), 2)` (`recovery.rs:76`).

`finish.rs:195-209` is the same two-floor shape for a probe that actually returned: persisted invocation, then `finished_at` when `attempted_provider` is true. This failure uses the reload copy of that second floor, with `recovery_epoch` standing in for `finished_at` (`join.rs:179-186`). Editing `finish.rs` does not move the deadline these children wait on.

The unit test `coordinator_recovers_persisted_owner_loss_once_without_a_herd` pins the rebase. Invocation `1_000`, refresh at `1_001` on `FakeMonotonicClock` (wall time is the fallback epoch; `support_01.rs:23-29`), expected `retry_at_epoch == Some(1_301)` (`case_02.rs:1065-1084`). `1_000 + 300 = 1_300` would be the persisted floor. `1_301` is `recovery_epoch + 300`.

## Why a 30s bump still fails around 61s

| Interval | Where | Role in the ~61s run |
| --- | --- | --- |
| 30s | `consts.rs:15`, `leader.rs:127-130` | Children spin on `Unavailable` until the killed lease is old enough. No `wait_until`. |
| ~300s | `join.rs:179-186`, `policy.rs:11` | `retry_at` after load. Still in the future after 10s and after 30s. |
| 10s / 30s | `usage_broker_e2e.rs:181` | The wait that panics. |
| 10s / 30s | `usage_broker_e2e.rs:195-197` | Not reached. |

`~30s` lease plus a full `30s` retry-wait timeout is about `61s`. The done barrier cannot be that timeout: reaching it requires `epoch_now() >= retry_at` already, and that predicate stays false for about five minutes after load. The deadline is not inside 30s. Keep the waits at 10s.

## Smallest edit

1. `crates/services/jackin-usage-coordinator/src/join.rs:179-186`. For an `Updating` recovery that already has `provider_invoked_at_epoch`, do not pass `Some(recovery_epoch)` into `minimum_attempt_deadline`. Keep the persisted-invocation floor at `join.rs:170-178`. Pass `Some(recovery_epoch)` only when `provider_invoked_at_epoch` is `None`, so an unresolved attempt still gets `recovery_epoch + 300`.
2. `crates/services/jackin-usage-coordinator/src/tests/case_02.rs:1084`. Expect `Some(1_300)` (`1_000 + 300`). The refresh at `1_301` (`case_02.rs:1087-1099`) still sits after that floor. `case_02.rs:2096` already expects `Some(1_300)`.

After that, the e2e backdate leaves the Claude floor at `now - 1`. The visible retry is the 1s recovery cap (`usage_broker_e2e.rs:256-257`), which fits the existing 10s wait at `usage_broker_e2e.rs:181`. The first `refresh` still returns generation 1 `OwnerLost` because the 1s deadline is slightly in the future (`refresh.rs:117-120`), the child waits it out, and the second `refresh` is the single recovery probe.

No wait change. No `finish.rs` change.
