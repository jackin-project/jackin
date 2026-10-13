# TASK-004 source-unit map

Read-only map of the policy worktree. This file does not approve the architecture. No build, test, commit, or edit was made in either worktree.

| Tree | Path | HEAD |
| --- | --- | --- |
| P (policy) | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| M (main) | `/Users/donbeave/Projects/jackin-project/wt-baseline-M` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |

Both SHAs were confirmed with `git rev-parse HEAD` in those worktrees.

Package paths below are relative to the P worktree root. Symbol paths are relative to the named tree.

## 1. P workspace members by root

Source: `Cargo.toml` `[workspace].members`. Count: **150**. Packages outside `adapters`, `apps`, `core`, `services`, `testing`, `tools`, `ui`: **none**.

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

One Cargo package sits under `tools` and is not a member: `crates/tools/jackin-lints` (`exclude = ["crates/tools/jackin-lints"]`). `[patch.crates-io] arrayref` points at `vendor/arrayref`; that path is not a workspace member.

### adapters (5)

- `crates/adapters/jackin-docker`
- `crates/adapters/jackin-image`
- `crates/adapters/jackin-omp-store`
- `crates/adapters/jackin-oppicker`
- `crates/adapters/jackin-usage-ffi`

### apps (2)

- `crates/apps/jackin`
- `crates/apps/jackin-capsule`

### core (9)

- `crates/core/jackin-brand`
- `crates/core/jackin-build-meta`
- `crates/core/jackin-config`
- `crates/core/jackin-core`
- `crates/core/jackin-env`
- `crates/core/jackin-manifest`
- `crates/core/jackin-process`
- `crates/core/jackin-process-directory`
- `crates/core/jackin-protocol`

### services (126)

- `crates/services/jackin-agent-status`
- `crates/services/jackin-diagnostics`
- `crates/services/jackin-host`
- `crates/services/jackin-instance`
- `crates/services/jackin-instance-agents`
- `crates/services/jackin-instance-credentials`
- `crates/services/jackin-instance-roles`
- `crates/services/jackin-isolation`
- `crates/services/jackin-runtime`
- `crates/services/jackin-runtime-apple-container-activate-entry`
- `crates/services/jackin-runtime-apple-container-attach`
- `crates/services/jackin-runtime-apple-container-attach-outcome`
- `crates/services/jackin-runtime-apple-container-check-dns`
- `crates/services/jackin-runtime-apple-container-client`
- `crates/services/jackin-runtime-apple-container-probe-version`
- `crates/services/jackin-runtime-apple-container-running`
- `crates/services/jackin-runtime-apple-container-session-contract`
- `crates/services/jackin-runtime-apple-container-stop`
- `crates/services/jackin-runtime-apple-container-supervisor-env`
- `crates/services/jackin-runtime-apple-container-wait`
- `crates/services/jackin-runtime-attach-admission`
- `crates/services/jackin-runtime-attach-capsule-ready`
- `crates/services/jackin-runtime-attach-exec-args`
- `crates/services/jackin-runtime-attach-inspect`
- `crates/services/jackin-runtime-attach-running-gate`
- `crates/services/jackin-runtime-attach-sessions`
- `crates/services/jackin-runtime-attach-transport`
- `crates/services/jackin-runtime-backend-selection`
- `crates/services/jackin-runtime-cleanup-absent-guards`
- `crates/services/jackin-runtime-cleanup-dind-gc`
- `crates/services/jackin-runtime-cleanup-eject-resources`
- `crates/services/jackin-runtime-cleanup-prune-cache`
- `crates/services/jackin-runtime-cleanup-prune-dir`
- `crates/services/jackin-runtime-cleanup-prune-home`
- `crates/services/jackin-runtime-cleanup-prune-images`
- `crates/services/jackin-runtime-cleanup-prune-roles`
- `crates/services/jackin-runtime-cleanup-resolve`
- `crates/services/jackin-runtime-cleanup-socket-dir`
- `crates/services/jackin-runtime-cleanup-timing`
- `crates/services/jackin-runtime-coordination`
- `crates/services/jackin-runtime-discovery`
- `crates/services/jackin-runtime-docker-profile`
- `crates/services/jackin-runtime-drift`
- `crates/services/jackin-runtime-exec-host`
- `crates/services/jackin-runtime-exit-summary`
- `crates/services/jackin-runtime-host-attach`
- `crates/services/jackin-runtime-host-daemon`
- `crates/services/jackin-runtime-identity`
- `crates/services/jackin-runtime-image`
- `crates/services/jackin-runtime-isolation`
- `crates/services/jackin-runtime-launch-account-config`
- `crates/services/jackin-runtime-launch-account-identity`
- `crates/services/jackin-runtime-launch-attach-outcome`
- `crates/services/jackin-runtime-launch-auth-error`
- `crates/services/jackin-runtime-launch-capsule-setup`
- `crates/services/jackin-runtime-launch-debug-envs`
- `crates/services/jackin-runtime-launch-dind`
- `crates/services/jackin-runtime-launch-dry-run`
- `crates/services/jackin-runtime-launch-exit-diagnosis`
- `crates/services/jackin-runtime-launch-exit-outro`
- `crates/services/jackin-runtime-launch-failure`
- `crates/services/jackin-runtime-launch-git-pull`
- `crates/services/jackin-runtime-launch-image-plan`
- `crates/services/jackin-runtime-launch-load-cleanup`
- `crates/services/jackin-runtime-launch-load-options`
- `crates/services/jackin-runtime-launch-mounts`
- `crates/services/jackin-runtime-launch-plan`
- `crates/services/jackin-runtime-launch-post-run-failure`
- `crates/services/jackin-runtime-launch-programmatic-selection`
- `crates/services/jackin-runtime-launch-progress-helpers`
- `crates/services/jackin-runtime-launch-restore`
- `crates/services/jackin-runtime-launch-run-envs`
- `crates/services/jackin-runtime-launch-sibling-auth-prewarm`
- `crates/services/jackin-runtime-launch-slot`
- `crates/services/jackin-runtime-launch-supported-agents`
- `crates/services/jackin-runtime-launch-trust`
- `crates/services/jackin-runtime-naming`
- `crates/services/jackin-runtime-prewarm-trigger`
- `crates/services/jackin-runtime-process-telemetry`
- `crates/services/jackin-runtime-progress`
- `crates/services/jackin-runtime-prune-output`
- `crates/services/jackin-runtime-reactive-daemon`
- `crates/services/jackin-runtime-repo-cache`
- `crates/services/jackin-runtime-session-control`
- `crates/services/jackin-runtime-shared-runner`
- `crates/services/jackin-runtime-snapshot`
- `crates/services/jackin-runtime-spin-wait`
- `crates/services/jackin-runtime-universe`
- `crates/services/jackin-runtime-universe-claims`
- `crates/services/jackin-runtime-usage-relay`
- `crates/services/jackin-telemetry`
- `crates/services/jackin-usage`
- `crates/services/jackin-usage-provider-core`
- `crates/services/jackin-usage-provider-zai`
- `crates/services/jackin-usage-provider-kimi`
- `crates/services/jackin-usage-provider-amp`
- `crates/services/jackin-usage-provider-minimax`
- `crates/services/jackin-usage-provider-openrouter`
- `crates/services/jackin-usage-provider-grok`
- `crates/services/jackin-usage-provider-antigravity`
- `crates/services/jackin-usage-provider-cursor`
- `crates/services/jackin-usage-provider-codex`
- `crates/services/jackin-usage-provider-hermes`
- `crates/services/jackin-usage-provider-gemini`
- `crates/services/jackin-usage-provider-muse`
- `crates/services/jackin-usage-provider-opencode`
- `crates/services/jackin-usage-provider-claude`
- `crates/services/jackin-usage-output`
- `crates/services/jackin-usage-credential-snapshots`
- `crates/services/jackin-usage-coordinator`
- `crates/services/jackin-usage-store-backend`
- `crates/services/jackin-usage-snapshot-store`
- `crates/services/jackin-usage-token-monitor`
- `crates/services/jackin-usage-host-presentation`
- `crates/services/jackin-usage-host-credentials`
- `crates/services/jackin-usage-host-accounts`
- `crates/services/jackin-usage-capsule-telemetry`
- `crates/services/jackin-usage-broker-publish`
- `crates/services/jackin-usage-broker-wire`
- `crates/services/jackin-usage-destination`
- `crates/services/jackin-usage-discovery`
- `crates/services/jackin-usage-broker`
- `crates/services/jackin-usage-projection`
- `crates/services/jackin-usage-host-glance`
- `crates/services/jackin-usage-credential-resolver`
- `crates/services/jackin-usage-host-runtime`

### testing (2)

- `crates/testing/jackin-otlp-testbed`
- `crates/testing/jackin-test-support`

### tools (3)

- `crates/tools/jackin-dev`
- `crates/tools/jackin-pr-trailers`
- `crates/tools/jackin-xtask`

### ui (3)

- `crates/ui/jackin-console`
- `crates/ui/jackin-launch`
- `crates/ui/jackin-tui`

M is a flat `crates/<package>` workspace (32 members in `Cargo.toml`). The symbol table in section 4 is the migration map. M is not re-grouped under the seven P roots.

## 2. Placement checks

| Check | Result |
| --- | --- |
| `jackin-diagnostics` under `services` | FOUND — `crates/services/jackin-diagnostics` |
| `jackin-telemetry` under `services` | FOUND — `crates/services/jackin-telemetry` |
| `jackin-usage` under `services` | FOUND — `crates/services/jackin-usage` |
| OMP store under `adapters` | FOUND — `crates/adapters/jackin-omp-store` |
| usage FFI under `adapters` | FOUND — `crates/adapters/jackin-usage-ffi` |
| `crates/services/jackin-runtime-cleanup-absent-guards` exists | FOUND — directory and `Cargo.toml` present |
| that crate is a workspace member | FOUND — member and `[workspace.dependencies]` path both name it |

## 3–4. Symbol map

Status is per tree. P line numbers are definitions unless noted as tests.

### `should_exit_idle`

| Tree | Status | Site |
| --- | --- | --- |
| P | FOUND | `fn should_exit_idle` — `crates/services/jackin-usage-broker/src/serve.rs:518`. Call site `serve.rs:190`. |
| P test | FOUND | `spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch` — `crates/services/jackin-usage-broker/src/serve/tests.rs:376` (calls at 396, 406, 415). |
| M | FOUND | `fn should_exit_idle` — `crates/jackin-usage/src/host/broker/serve_loop.rs:496`. Call site `serve_loop.rs:198`. |
| M test | FOUND | `spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch` — `crates/jackin-usage/src/host/broker/serve_loop/tests.rs` (calls at 406, 416, 425). |

P’s sixth parameter is `has_authenticated_foreground_guard: bool` (`serve.rs:524`). M’s sixth parameter is `has_foreground_liveness: bool` (`serve_loop.rs:502`).

### `has_foreground_liveness`

| Tree | Status | Site |
| --- | --- | --- |
| P | **NOT_FOUND** | No definition, parameter, or test uses this name. The idle-exit guard in P is `has_authenticated_foreground_guard` (`ServeConfig` field `serve.rs:95`; parameter `serve.rs:524`). |
| M | FOUND | Parameter of `should_exit_idle`, `crates/jackin-usage/src/host/broker/serve_loop.rs:502`. It is not a standalone function. The holding assertion text is in `serve_loop/tests.rs` (“foreground collector liveness…”). |

### Typed provider status

No symbol is named `ProviderStatus`, `TypedProviderStatus`, or `typed provider status`. The typed status surfaces that exist are below.

| Tree | Status | Site |
| --- | --- | --- |
| P snapshot status | FOUND | `pub enum UsageSnapshotStatus` — `crates/core/jackin-protocol/src/control.rs:961`. Producer `pub fn provider_outcome` — `crates/services/jackin-usage-provider-core/src/outcome.rs:94`. Test `provider_outcome_maps_presence_states` — `crates/services/jackin-usage-provider-core/src/tests/case_05.rs:67`. |
| P HTTP status kind | FOUND | `pub enum ProviderErrorKind` — `crates/services/jackin-usage-provider-core/src/refresh.rs:13`. `ProviderError::status` — `refresh.rs:115`. `ProviderError::kind` — `refresh.rs:91`. Test `provider_error_keeps_timeout_transport_and_decode_distinct_from_status_text` — `crates/services/jackin-usage-provider-core/src/tests/case_04.rs:355`. |
| M snapshot status | FOUND | `pub enum UsageSnapshotStatus` — `crates/jackin-protocol/src/control.rs:961`. `provider_outcome` — `crates/jackin-usage/src/usage.rs:1014`. Test `provider_outcome_maps_presence_states` — `crates/jackin-usage/src/usage/tests.rs:4004`. |
| M HTTP status kind | FOUND | `pub enum ProviderErrorKind` — `crates/jackin-usage/src/usage/refresh.rs:14`. `ProviderError::status` — `refresh.rs:106`. |
| M test `provider_error_keeps_timeout_transport_and_decode_distinct_from_status_text` | **NOT_FOUND** | The type and `status()` exist; that test function name does not. |

### Quota window provenance

| Tree | Status | Site |
| --- | --- | --- |
| P | FOUND | `pub struct MonitorFieldEvidence` — `crates/core/jackin-protocol/src/usage_monitor.rs:433`. `pub struct MonitorQuotaWindowStatus` — `usage_monitor.rs:449` (“independent used-percent and reset provenance”). `pub enum MonitorEvidenceValue::QuotaWindow` — `usage_monitor.rs:564`. `pub enum MonitorQuotaWindow` — `usage_monitor.rs:612`. |
| P test | FOUND | `quota_window_status_keeps_used_and_reset_evidence_independent` — `crates/core/jackin-protocol/src/usage_monitor.rs:1310`. |
| M | FOUND | `MonitorFieldEvidence` — `crates/jackin-protocol/src/usage_monitor.rs:430`. `MonitorQuotaWindowStatus` — `usage_monitor.rs:446`. |
| M test | FOUND | `quota_window_status_keeps_used_and_reset_evidence_independent` — `crates/jackin-protocol/src/usage_monitor/tests.rs:354`. |

### Monitor wire and schema version constants

Quoted from the definitions. Neither tree defines these constants as `6` or `"v9"`.

| Constant | Value | P | M |
| --- | --- | --- | --- |
| `USAGE_MONITOR_SCHEMA_VERSION` | `4` (`u16`) | FOUND `crates/core/jackin-protocol/src/usage_monitor.rs:11` | FOUND `crates/jackin-protocol/src/usage_monitor.rs:11` |
| `USAGE_STATUSLINE_INPUT_SCHEMA_VERSION` | `2` (`u16`) | FOUND `usage_monitor.rs:14` | FOUND `usage_monitor.rs:14` |
| `USAGE_BROKER_PROTOCOL_VERSION` | `"v8"` | FOUND `crates/core/jackin-protocol/src/usage_broker.rs:14` | FOUND `crates/jackin-protocol/src/usage_broker.rs:14` |

Tests that assert the numbers:

| Tree | Status | Site |
| --- | --- | --- |
| P | FOUND | `v4_monitor_control_shapes_are_tagged_and_secret_free` asserts schema `4`, statusline input `2`, and broker protocol `"v8"` — `crates/core/jackin-protocol/src/usage_monitor.rs:1043` (asserts at 1154–1156). `broker_conflict_has_a_stable_sanitized_wire_name` asserts `"v8"` — `crates/core/jackin-protocol/src/usage_broker/tests/case_01.rs:45`. |
| M | FOUND | `v4_monitor_control_shapes_are_tagged_and_secret_free` asserts schema `4` and broker protocol `"v8"` — `crates/jackin-protocol/src/usage_monitor/tests.rs:119` (asserts at 196–197). `local_source_identity_kind_has_an_explicit_v8_wire_name` asserts `"v8"` — `crates/jackin-protocol/src/usage_broker/tests.rs:61`. |
| M assert of statusline schema `== 2` | **NOT_FOUND** | The constant is defined as `2` and is used. No `assert_eq!(USAGE_STATUSLINE_INPUT_SCHEMA_VERSION, 2)` was found. |

### Coordination inode / lock owner

| Tree | Status | Site |
| --- | --- | --- |
| P inode + flock owner | FOUND | `pub fn universe_dir` — `crates/services/jackin-runtime-coordination/src/coordination.rs:143` (pathname hash, “never its transient inode”). `pub fn open_lock` — `coordination.rs:154` (“File ownership is only flock ownership”). `pub fn open_in_namespace` — `coordination.rs:175` (“Lock inodes are created once and never truncated or removed”). Module `pub mod coordination` — `crates/services/jackin-runtime-coordination/src/lib.rs:13`. |
| P tests | FOUND | `concurrent_first_lock_opens_all_share_one_inode` — `crates/services/jackin-runtime-coordination/src/coordination/tests/case_01.rs:62`. `three_process_contenders_preserve_lock_inode_until_and_after_owner_exit` — `crates/services/jackin-runtime/src/runtime/launch/tests/case_29.rs:57`. |
| M inode + flock owner | FOUND | Same function names in `crates/jackin-runtime/src/runtime/coordination.rs`: `universe_dir` at 143, `open_lock` at 154, `open_in_namespace` at 175. Declared `pub(crate) mod coordination` — `crates/jackin-runtime/src/runtime.rs:13`. |
| M tests | FOUND | `concurrent_first_lock_opens_all_share_one_inode` — `crates/jackin-runtime/src/runtime/coordination.rs:444`. `three_process_contenders_preserve_lock_inode_until_and_after_owner_exit` — `crates/jackin-runtime/src/runtime/launch/launch_slot.rs:486`. |

A different “lock owner” string exists in both trees and is not the coordination lock: `fn reclaim_abandoned_lock` parses a “Jackin lock owner record” that includes `recorded_inode`. P and M: `crates/services/jackin-isolation/src/ref_transaction.rs:309` in P, `crates/jackin-isolation/src/ref_transaction.rs:309` in M (field check at line 324 in both).

### `load --model` and `--effort`

| Tree | Status | Site |
| --- | --- | --- |
| P | FOUND | `LoadArgs.model` — `crates/apps/jackin/src/cli/role.rs:67` (`long`, `value_parser = parse_model`). `LoadArgs.effort` — `role.rs:70` (`long`, `value_parser = parse_reasoning_effort`). Parsers `parse_model` `role.rs:99`, `parse_reasoning_effort` `role.rs:107`. |
| P tests | FOUND | `load_args_parses_task_scoped_model_and_effort` `role/tests/case_01.rs:51`; `load_args_parses_model_and_effort_overrides` `case_01.rs:83`; `load_args_rejects_empty_model_and_unknown_effort` `case_01.rs:107`; `load_help_documents_model_and_effort_overrides` `case_01.rs:322`. |
| M | FOUND | `LoadArgs.model` — `crates/jackin/src/cli/role.rs:67`. `LoadArgs.effort` — `role.rs:70`. `parse_model` `role.rs:99`. `parse_reasoning_effort` `role.rs:107`. |
| M tests | FOUND | `load_args_parses_task_scoped_model_and_effort` `crates/jackin/src/cli/role/tests.rs:77`; `load_args_parses_model_and_effort_overrides` `tests.rs:109`; `load_args_rejects_empty_model_and_unknown_effort` `tests.rs:133`; `load_help_documents_model_and_effort_overrides` `tests.rs:350`. |

### Diagnostic redaction

| Tree | Status | Site |
| --- | --- | --- |
| P | FOUND | `pub mod redact` — `crates/services/jackin-diagnostics/src/lib.rs:12`. `pub fn redact_text` — `src/redact.rs:526`. `pub fn redact_and_cap` — `src/redact.rs:543`. |
| P test | FOUND | `redacts_named_secret_values` — `crates/services/jackin-diagnostics/src/redact/tests/case_01.rs:7`. |
| M | FOUND | `pub mod redact` — `crates/jackin-diagnostics/src/lib.rs:14`. `pub fn redact_text` — `src/redact.rs:526`. |
| M test | FOUND | `redacts_named_secret_values` — `crates/jackin-diagnostics/src/redact/tests.rs:7`. |

## 5. Rust floor

| Source | P | M |
| --- | --- | --- |
| `rust-toolchain.toml` `[toolchain].channel` | `1.99.0` | `1.97.1` |
| `Cargo.toml` `[workspace.package].rust-version` | `1.99` | `1.97` |
| `mise.toml` rust pin | FOUND `rust = { version = "1.99.0", mr_boxington = true }` (line 28). Also `idiomatic_version_file_enable_tools = ["rust"]` (line 46). | **NOT_FOUND** as a version pin. Comment at lines 46–47: rust is read from `rust-toolchain.toml` via `idiomatic_version_file`. Setting: `idiomatic_version_file_enable_tools = ["rust"]` (line 51). |

## NOT_FOUND required names

- `has_foreground_liveness` in P.
- A type or function literally named `ProviderStatus`, `TypedProviderStatus`, or `typed provider status` in P and M. Typed status is carried by `UsageSnapshotStatus`, `provider_outcome`, `ProviderErrorKind`, and `ProviderError::status`.
- M test function `provider_error_keeps_timeout_transport_and_decode_distinct_from_status_text`.
- M `assert_eq!(USAGE_STATUSLINE_INPUT_SCHEMA_VERSION, 2)`. The constant itself is FOUND and equals `2`.

## Verdict

Source-unit map only. Architecture is not approved.
