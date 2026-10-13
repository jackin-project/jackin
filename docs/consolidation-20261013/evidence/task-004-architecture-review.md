# TASK-004 architecture review

Independent of the author of `task-004-source-unit-map.md`. That map was read as a claim and checked against the trees. No build, commit, or source edit was made. This verdict selects source units only. It leaves the 366-commit policy history unmerged and authorizes no integration branch.

Reviewed: 2026-10-13

| Tree | Worktree | HEAD checked with `git rev-parse HEAD` |
| --- | --- | --- |
| M | `/Users/donbeave/Projects/jackin-project/wt-baseline-M` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| P | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| U | `/Users/donbeave/Projects/jackin-project/wt-baseline-U` | `2e4a62da4bc4406764162a267163389a5fb513a7` |

`mbx` metadata, `xtask health`, and `xtask ci --only policy` were left **NOT_RUN**. This review was required to stay read-only.

## Verdict

SOURCE_UNITS_ACCEPTED

P's seven layered roots are the integration destination: `crates/adapters`, `crates/apps`, `crates/core`, `crates/services`, `crates/testing`, `crates/tools`, `crates/ui`. Every required symbol cited by the map sits at the path the map names. No required-symbol destination was wrong.

Blocking destination gaps: none.

Copying U monitor schema 6 or broker wire v9 onto P is refused. The monitor/wire/account-state migration below stays an explicit open decision.

## 1. Package placement

P `Cargo.toml` `[workspace].members` contains **150** paths. Root counts match the map and sum to 150. Every member is under one of the seven roots. The member set equals the map's package list (0 missing, 0 extra).

| Root | Count |
| --- | ---: |
| adapters | 5 |
| apps | 2 |
| core | 9 |
| services | 126 |
| testing | 2 |
| tools | 3 |
| ui | 3 |
| **members** | **150** |

`exclude = ["crates/tools/jackin-lints"]`. `[patch.crates-io] arrayref` points at `vendor/arrayref`, which is outside the member list.

M `Cargo.toml` has **32** flat `crates/<package>` members. M stays flat. It is not regrouped under P's roots by this verdict.

| Obligation | Evidence |
| --- | --- |
| `jackin-diagnostics` is a service | Member `crates/services/jackin-diagnostics`; package name `jackin-diagnostics` |
| `jackin-telemetry` is a service | Member `crates/services/jackin-telemetry`; package name `jackin-telemetry` |
| `jackin-usage` is a service | Member `crates/services/jackin-usage`; package name `jackin-usage`. The other usage crates in the member list are also under `crates/services` |
| OMP store is an adapter | Member `crates/adapters/jackin-omp-store`; package name `jackin-omp-store` |
| usage FFI is an adapter | Member `crates/adapters/jackin-usage-ffi`; package name `jackin-usage-ffi` |
| purge member | Member `crates/services/jackin-runtime-cleanup-absent-guards`, also named in `[workspace.dependencies]` at `Cargo.toml:235`. Crate entry is `absent_guards::ensure_role_resources_absent_for_purge`. `jackin-runtime-apple-container-ensure-absent` is absent as a member and as a directory |

Backend `ensure_absent_for_purge` remains in `crates/services/jackin-runtime`. The selected purge member is the role-resource absence guard extracted into `jackin-runtime-cleanup-absent-guards`.

## 2. Foreground-guard finding

P preserves all three idle-exit outcomes. P contains zero definitions, parameters, or tests named `has_foreground_liveness`. The sixth parameter is `has_authenticated_foreground_guard: bool` (`crates/services/jackin-usage-broker/src/serve.rs:524`).

P `should_exit_idle` (`serve.rs:518-530`) returns true only when every conjunct holds:

```rust
now_monotonic.saturating_sub(last_activity_monotonic) >= idle_exit
    && coordinator_idle
    && !has_active_monitor
    && !has_authenticated_foreground_guard
```

M `should_exit_idle` (`crates/jackin-usage/src/host/broker/serve_loop.rs:496-508`) is the same conjunction with the sixth name `has_foreground_liveness`:

```rust
now_monotonic.saturating_sub(last_activity_monotonic) >= idle_exit
    && coordinator_idle
    && !has_active_monitor
    && !has_foreground_liveness
```

| Required outcome | How the condition produces it |
| --- | --- |
| Active monitors keep the service alive | `has_active_monitor` true makes `!has_active_monitor` false, so the function returns false. Both call sites pass `monitor_store.has_active()` (P `serve.rs:195`, M `serve_loop.rs:203`) |
| Passive idle brokers expire | Sixth flag false, coordinator idle, no active monitor, and the idle window elapsed make the function return true. P's passive entry `run_usage_broker_service_with_executor_and_metadata` passes `false` (`service.rs:361`). `ensure_usage_broker_with_executor` also sets the flag false (`service.rs:537`) |
| Foreground liveness blocks idle exit | Sixth flag true makes `!has_authenticated_foreground_guard` false, so the function returns false. P's foreground entry `run_usage_broker_foreground_bootstrap` passes `true` (`service.rs:124`). M's call site passes `collector_liveness.is_some()` (`serve_loop.rs:204`), which is `Some` only on M's foreground bootstrap |

P's regression `spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch` (`serve/tests.rs:376`) asserts the same three rows: active monitor with the flag false stays (`396-404`), passive idle exits (`406-413`), and the flag true blocks exit (`414-423`, message "an authenticated foreground guard prevents idle exit while held"). M's same test name asserts the third row as "foreground collector liveness…" (`serve_loop/tests.rs:424-434`).

The idle predicate survives under the renamed static bool. M's `ClaudeCollectorLiveness` object, including `shutdown_flag()` used before the idle check (`serve_loop.rs:132-135`), is a separate serve-loop input. P's `serve` allocates its own `AtomicBool` (`serve.rs:130`) and sets it from monitor `ServiceStop` (`dispatch.rs:135-136`). That shutdown wiring is outside `should_exit_idle` and stays for TASK-007 to trace. It does not move the idle-exit destination.

## 3. Open migration decision

MIGRATION_DECISION_OPEN

This selection keeps P's current monitor schema and broker wire. It refuses a silent overwrite that would stamp U's schema 6 / wire v9 onto those constants.

| Contract | M `1c3dd1be` | U `2e4a62da` | P `fba69d60` |
| --- | --- | --- | --- |
| `USAGE_MONITOR_SCHEMA_VERSION` | `4` (`crates/jackin-protocol/src/usage_monitor.rs:11`) | `6` (`crates/jackin-protocol/src/usage_monitor.rs:11`) | `4` (`crates/core/jackin-protocol/src/usage_monitor.rs:11`) |
| `USAGE_BROKER_PROTOCOL_VERSION` | `"v8"` (`crates/jackin-protocol/src/usage_broker.rs:14`) | `"v9"` (`crates/jackin-protocol/src/usage_broker.rs:14`) | `"v8"` (`crates/core/jackin-protocol/src/usage_broker.rs:14`) |
| `ACCOUNT_STATE_SCHEMA_VERSION` | `2` (`crates/jackin-usage/src/coordinator/state/envelope.rs:14`) | `2` (same path, line 14) | `3` (`crates/services/jackin-usage-coordinator/src/state/envelope.rs:14`) |

P load (`crates/services/jackin-usage-broker/src/monitor/storage.rs:95-101`) accepts schema 1, migrates 2 and 3, loads current schema 4, and rejects every other version, including 5 and 6, with `_ => return Err(unavailable())`.

U load (`crates/jackin-usage/src/host/broker/monitor/storage.rs:95-103`) migrates schema 5 through `migrate_v5` and loads schema 6 as current `USAGE_MONITOR_SCHEMA_VERSION`. Schema 6 is the current arm, with no `migrate_v6`. U also defines `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION = "v8"` (`crates/jackin-usage/src/host/broker.rs:48`). That previous-wire constant is absent from M and P.

P account-state current is 3, with `PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION = 2` and `LEGACY_ACCOUNT_STATE_SCHEMA_VERSION = 1` (`envelope.rs:14-16`). M and U current account-state schema is 2, with previous 1. Adopting P's account-state 3 therefore migrates the schema that M and U treat as current. P already has a version-2 arm. That arm is a candidate implementation, and this review leaves the choice of migration, refusal, and disposable predecessor tests undecided.

Downstream work must record an explicit decision before any of these numbers change. Selected destinations for the constants, until that decision, are P's paths above.

## 4. Symbol destinations checked

86 map citations were read at the named file and line. All 86 contain the named symbol. Destinations that matter for this selection:

| Symbol | Selected P destination |
| --- | --- |
| `should_exit_idle` | `crates/services/jackin-usage-broker/src/serve.rs:518` |
| `UsageSnapshotStatus` | `crates/core/jackin-protocol/src/control.rs:961` |
| `provider_outcome` | `crates/services/jackin-usage-provider-core/src/outcome.rs:94` |
| `ProviderErrorKind` | `crates/services/jackin-usage-provider-core/src/refresh.rs:13` |
| `MonitorFieldEvidence` / `MonitorQuotaWindowStatus` | `crates/core/jackin-protocol/src/usage_monitor.rs:433` and `:449` |
| `universe_dir` / `open_lock` / `open_in_namespace` | `crates/services/jackin-runtime-coordination/src/coordination.rs:143`, `:154`, `:175` |
| `LoadArgs` model and effort | `crates/apps/jackin/src/cli/role.rs:67` and `:70` |
| `redact_text` | `crates/services/jackin-diagnostics/src/redact.rs:526` |

No type or function is named `ProviderStatus`, `TypedProviderStatus`, or `typed provider status` in P or M. Typed status at these heads is `UsageSnapshotStatus`, `provider_outcome`, `ProviderErrorKind`, and `ProviderError::status`.

Rust floor, read from the files: P channel `1.99.0`, `rust-version = "1.99"`, mise pin `rust = { version = "1.99.0", mr_boxington = true }` at `mise.toml:28`. M channel `1.97.1`, `rust-version = "1.97"`, with rust taken from `rust-toolchain.toml` and `idiomatic_version_file_enable_tools = ["rust"]` at `mise.toml:51`.

## Limits

This acceptance covers the layered package roots and the symbol destinations checked above. It leaves monitor schema 6, broker wire v9, and account-state schema 3 as MIGRATION_DECISION_OPEN. It authorizes no merge of P's 366 commits, no integration branch, and no source edit.

Coordinator addendum after this verdict: two defects inside the accepted units are not waived. Policy `leader.rs` has no `lease_path_matches_open_file` check that main and the usage line have. Policy `boltffi.toml` relatives `../../target` and `../../native` resolve under `crates/` because the crate moved to `crates/adapters/`. Both must be fixed in any later port of these units. See `integration-constraints.md`.
