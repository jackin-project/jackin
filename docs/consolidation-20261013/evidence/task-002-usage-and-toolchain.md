# TASK-002 — Usage line U and toolchain line T

Read-only inspection of `/Users/donbeave/Projects/jackin-project/jackin`. HEAD was `1c3dd1be4a3b1ed518133ec3899c084c792370ff` with a clean work tree. Commands were `git log`, `git diff`, `git show`, `git grep`, `git merge-base`, and `git ls-tree` only. No checkout, commit, merge, reset, provider call, or Keychain read.

| Pin | SHA | Relation to M |
| --- | --- | --- |
| M | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` | target |
| U | `2e4a62da4bc4406764162a267163389a5fb513a7` | `fix/claude-usage-live-repair`; merge-base is M; 25 ahead, 0 behind |
| T | `9ef617d014df15c6d967501bcbbdcb3689ed453c` | `chore/rust-mise-mbx-modernize`; merge-base `9b6e1d92608fed65579b0ab5bb4b04d2a8dd602b`; 2 ahead, 4 behind |
| P | `fba69d60c2373156dc0dcecee8e085c83776966f` | descendant of M (`merge-base --is-ancestor` exit 0). Context only; the 366-commit range was not reviewed |

Neither T commit is an ancestor of M or of P.

## U — 25 commits reachable from U and not from M

Oldest first:

| SHA | Subject |
| --- | --- |
| `dba3a1811a06436a5e087f127b19d7828fa343a5` | fix(usage): publish completed collector evidence |
| `ef49789843d703b948d85902b7d7ceb38444ae1e` | docs(usage): record live publication repair evidence |
| `fbe514005ef5f275054034d8da0414f8f9c748cc` | docs(usage): record v8 installation evidence |
| `cf85e92d07b3ad0632eaf6cca90075864b86f0d9` | docs(usage): record live rate-limit findings |
| `43631225fa62a6ff97100da86d4dc96911f70c3d` | docs(usage): record v8 live verification |
| `93500698bd25baf26e9425041af1dd2d91510fe4` | fix(usage): honor monitor watch deadline |
| `b46f9a854330e744b572de76723cedb5d8d5ab4f` | test(usage): fix watch deadline lint warnings |
| `f5163e987b861adca1de5add6087ee3d2d12e11a` | fix(usage): harden Claude broker polling and monitor state |
| `4f809e6cebe5626c7fb1dff8924471a09d03f206` | docs: record v9 offline installation verification |
| `d3ce0bca131b110fe5abc9af418e9c2bd81319cf` | docs: add attended v9 upgrade runbook |
| `01ab76d50234a2a66eb036ab55416933ff709076` | docs: refine attended v9 upgrade sequence |
| `a238c9eb18b107e85595c73f1ca5e13c7aa52494` | test(usage): exercise foreground collector through CLI |
| `019de448ef8a6590b0a847046cf6a71c03d2079b` | test(usage): cover broker provider flows end to end |
| `9809c24400e9f24ad86e83e45dca7bf5d9bab278` | fix(broker): recover expired protocol leases safely |
| `fba1786ef3e0f0e4882c7491de660d1a528cf758` | docs(usage): record fixed v10 installation |
| `f04bf23b0da3af4eded8c4155154148fffe9ab4e` | docs(usage): record partial v10 live evidence |
| `f292bc1ba655cff4d7de1582ee25b9d57ce6f831` | docs(usage): record schema-6 gate status |
| `62cefbbbb11ce8b6dc0634f490095d9838ebef9e` | fix: preserve monitor reset provenance |
| `f90e63ea261e9cd2a438e56a53e25f796d923985` | docs: record v11 installation provenance |
| `cea91b3bfa0f6f723ee390f2c1e27860da4002bc` | docs: update v11 usage monitor handoff |
| `cf2d7059f806337408e0e3aa8e2f4fc5055579ea` | docs: update Claude usage guidance |
| `638ea21c22207f2f28fa31dfebc514d81d989431` | docs(usage): record v11 live verification |
| `db59c4015fb4715e1d39157aaa31f4a1497c823a` | fix(usage): retain fresh used-only quota evidence |
| `d1e79ea06aba6d1d6b91e414b6fc3d3fb4123c3e` | docs(usage): record v12 isolated installation |
| `2e4a62da4bc4406764162a267163389a5fb513a7` | docs(usage): update v11 live handoff gates |

`git diff --stat M...U` is 65 files, 19963 insertions, 1066 deletions.

## Schema and wire constants actually in the trees

These are the `pub const` / `pub(crate) const` definitions, not the numbers assumed by the goal notes. U's own docs repeat the U code values. M and P do not.

| Constant | Definition site on U | M | U | P |
| --- | --- | --- | --- | --- |
| `USAGE_MONITOR_SCHEMA_VERSION` | U `crates/jackin-protocol/src/usage_monitor.rs:11`; P `crates/core/jackin-protocol/src/usage_monitor.rs:11` | `4` | `6` | `4` |
| `USAGE_BROKER_PROTOCOL_VERSION` | U `crates/jackin-protocol/src/usage_broker.rs:14`; P `crates/core/jackin-protocol/src/usage_broker.rs:14` | `"v8"` | `"v9"` | `"v8"` |
| `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION` | U `crates/jackin-usage/src/host/broker.rs:48` only | absent | `"v8"` | absent |
| `USAGE_STATUSLINE_INPUT_SCHEMA_VERSION` | same `usage_monitor.rs:14` on each tree | `2` | `2` | `2` |
| `ACCOUNT_STATE_SCHEMA_VERSION` | U/M `crates/jackin-usage/src/coordinator/state/envelope.rs`; P `crates/services/jackin-usage-coordinator/src/state/envelope.rs` | `2` | `2` (U does not change this file) | `3` |
| `PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION` | same envelope file | `1` | `1` | `2` |
| `LEGACY_ACCOUNT_STATE_SCHEMA_VERSION` | P envelope only | absent | absent | `1` |
| `PROJECTION_STATE_SCHEMA_VERSION` | same envelope file | `3` | `3` | `3` |
| `PREVIOUS_PROJECTION_STATE_SCHEMA_VERSION` | same envelope file | `2` | `2` | `2` |

U tests assert the pair directly: `usage_monitor/tests.rs` expects `USAGE_MONITOR_SCHEMA_VERSION == 6` and `USAGE_BROKER_PROTOCOL_VERSION == "v9"`. M's same test expects `4` and `"v8"`.

U `docs/content/(public)/commands/usage.mdx` states the U source versions as broker wire v9, monitor-store schema 6, statusline input schema 2, and projection-envelope schema 3. It says monitor-store schemas V1 through V5 migrate to V6, and that invalid and unsupported future schemas fail closed.

P's same document states broker wire v8, monitor-store schema 4, statusline input schema 2, account-state envelope schema 3, and projection-envelope schema 3. P's account-state 3 is a real later constant. U does not have it.

Other schema constants on U (`AUTH_SCHEMA_VERSION = 7`, account-credentials schema 2, host daemon protocol 2, reactive daemon protocol 1, rule schema 1) are outside the U-versus-M usage diff and are not the monitor/wire pair.

## Migration, refusal, corruption, and quota provenance on U

U `crates/jackin-usage/src/host/broker/monitor/storage.rs` loads versions `2..USAGE_MONITOR_SCHEMA_VERSION` (2 through 5) through `migrate_v2_or_v3`, `migrate_v4`, and `migrate_v5`, then restamps to schema 6. `reject_v6_reset_metadata_in_legacy_snapshot` refuses schema-6 reset metadata inside a legacy snapshot. `migrate_quota_fingerprint` rewrites fingerprint keys during migration.

P `crates/services/jackin-usage-broker/src/monitor/storage.rs` matches only `1` (`migrate_v1`), `2 | 3` (`migrate_v2_or_v3`), and current schema 4. Every other version, including 5 and 6, returns unavailable. A P test writes a schema-5 fixture and asserts `load` fails without rewriting the source bytes. That test treats 5 as future because P's current schema is 4. U treats 5 as a migratable predecessor of 6.

U tests that cover migration, refusal, or corrupt/malformed input (added or extended on this line):

- `v5_migration_records_unknown_or_known_none_and_restamps_event_schema`
- `v5_migration_rejects_wrong_nested_schema_and_downgraded_watermark_metadata`
- `v5_migration_unknown_reset_watermark_uses_liveness_cutoff`
- `v4_migration_preserves_strict_history_and_drops_unscoped_provider_quota`
- `v4_migration_rejects_malformed_event_schema_without_rewriting_source`
- `v4_fingerprint_migration_preserves_account_named_statusline_session_and_drops_provider_data`
- `v3_migration_rejects_wrong_or_future_nested_event_schema_without_rewriting_source`
- `future_store_schema_is_rejected` (store schema set to `USAGE_MONITOR_SCHEMA_VERSION + 1`)
- `account_reset_watermark_validation_rejects_future_receipt_and_inconsistent_max`
- `foreign_lease_recovery_fails_closed_for_live_unknown_future_and_malformed_owners`
- `foreign_lease_recovery_rejects_unknown_fields_and_oversized_payloads`
- `broker_refuses_legacy_pid_lease_without_mutating_it`
- `expired_v8_lease_is_reclaimed_only_after_dead_owner_and_unowned_socket_checks`
- `expired_v9_lease_from_another_build_can_be_reclaimed_after_owner_exit`
- CLI `cli_v5_migration_admits_new_provider_reset_without_statusline_adapter`

Coordinator quarantine tests such as `atomic_state_corrupt_schema_and_future_epoch_fail_closed` and `projection_state_is_one_atomic_envelope_and_quarantines_corruption` exist on the U tree. They are the M-era account/projection store tests. U does not change `envelope.rs`.

Quota provenance U adds on top of the shared per-field used/reset model:

- `62cefbbbb11ce8b6dc0634f490095d9838ebef9e` preserves monitor reset provenance: reset watermarks, known-none versus unknown, and lower-reset admission fences (`statusline_reset_received_at`, `validate_account_reset_metadata`, `lower_reset_*`).
- `db59c4015fb4715e1d39157aaa31f4a1497c823a` retains fresh used-only quota. A used percentage without `resets_at` updates used evidence and leaves reset unverified (`is_used_only_window`, `chosen_broker_used_only_orphan`, `cli_session_used_only_without_reset_preserves_reset_barrier_and_weekly`). A low later sample cannot clear a latched reset barrier.
- M and P already document `MonitorQuotaWindowStatus` as independent used-percent and reset provenance. They do not contain `migrate_v4`, `migrate_v5`, `used_only`, or the schema-6 reset-watermark identifiers.

## Typed provider status versus text matching of 401/403/429

U adds monitor issue codes that M and P do not have, in `MonitorIssueCode`: `ProviderRateLimited`, `ProviderUnauthorized`, `ProviderTimeout`, `ProviderUnavailable`, `ProviderNeedsSecret`. It also adds `MonitorProviderReadiness` (`Unknown`, current, stale, in progress, `RateLimited`). Projection maps the labels `"rate_limited"` and `"unauthorized"` onto those codes. It does not search response text for the digit strings `401`, `403`, or `429`.

HTTP classification on U uses the typed status accessor and a numeric field: `error.status() == Some(403)` for consent revocation, `error.status() == Some(401)` for the single credential reread, and `provider_http_status: Some(401)`. Fixture tests pass numeric `status: 401`, `403`, and `429`. `cli_http_429_retry_after_delta_seconds_survives_restart` and `cli_http_429_retry_after_date_survives_restart` keep a typed 429 retry deadline across restart. `unchanged_401_does_not_retry` and `run_non_401_failure` (403, transport, timeout) keep those outcomes distinct.

P does not contain those new monitor issue codes or `MonitorProviderReadiness`. P's newer owner of typed HTTP failure is `crates/services/jackin-usage-provider-core/src/refresh.rs` (`ProviderErrorKind::{Other, Timeout, Transport, HttpStatus, Decode}`, `ProviderFailureMetadata.http_status`) and `crates/services/jackin-usage-provider-claude` (`collector.rs` records `provider_http_status`, including 401 and 429; `wave.rs` uses `error.status() == Some(403)` and `Some(401)`). That is typed status, not substring matching, and it is the relocated provider path rather than U's schema-6 monitor codes.

`watch_deadline_reached` / `watch_batch_timeout_does_not_end_the_absolute_cli_deadline` are on U and absent from P.

## Live acceptance

Live acceptance is not evidenced as complete. U says it is incomplete.

`plans/claude-unattended-monitor/claude-code-handoff.md` at U is titled as a draft that is not ready for live use. It says two natural v11 collection generations were recorded and that live acceptance remains false: Weekly evidence was current while Session projection lacked a direct used percentage and reset, and the monitor kept a stale used percentage and reset. It says the v12 source can derive a zero-used observation from a remaining value, that path has not been verified live, and the v12 pair passed only an isolated smoke. The v11-to-v12 cutover approval is pending. The handoff also names local install paths and process ids; those operational details are not copied here.

`docs/content/(public)/commands/usage.mdx` at U says the v11 CLI/broker pair passed an isolated synthetic fixture smoke for monitor schema 6, broker wire v9, sibling-broker selection, and a bounded watch. That fixture did not read existing monitor state, use a real Keychain credential, authenticate, or make a provider request, and it does not establish live readiness. It says the prior v10 run had two successful provider collections but weekly monitor publication failed, so two-window acceptance remained incomplete. It says the v11 pair has not had live verification and that existing-state migration and live collection verification remain pending.

P's usage doc has a different, older claim: experimental collection "has not been verified against a live account or provider" and "has not been validated against a live endpoint or account." P does not contain U's v8–v12 installation or live-verification files, and P's `claude-code-handoff.md` is still the v2 installed-fixture note.

## Files U changes that P does not already contain

No blob at U for these 65 paths equals any blob in P. The additions with no P counterpart at all:

- `crates/jackin-usage/src/host/broker/monitor/tests/fingerprint_migration.rs`
- `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs`
- `crates/jackin/src/bin/usage-broker-fixture.rs`
- `crates/jackin/tests/usage_provider_flow.rs`
- `crates/jackin/tests/usage_provider_flow/auth_outcomes.rs`
- `plans/claude-unattended-monitor/v8-installation.json`
- `plans/claude-unattended-monitor/v8-live-verification.json`
- `plans/claude-unattended-monitor/v9-installation.json`
- `plans/claude-unattended-monitor/v9-operator-upgrade.md`
- `plans/claude-unattended-monitor/v10-installation.json`
- `plans/claude-unattended-monitor/v10-live-verification.json`
- `plans/claude-unattended-monitor/v10-live-verification.md`
- `plans/claude-unattended-monitor/v11-installation.json`
- `plans/claude-unattended-monitor/v11-live-verification.json`
- `plans/claude-unattended-monitor/v11-live-verification.md`
- `plans/claude-unattended-monitor/v12-installation.json`

U-modified paths whose distinguishing behavior (schema 6, wire v9, v4/v5 migration, used-only quota, reset watermarks, watch deadline, new provider issue codes, v8–v12 handoff) is absent from the P tree:

- `crates/jackin-protocol/src/usage_broker.rs`
- `crates/jackin-protocol/src/usage_broker/tests.rs`
- `crates/jackin-protocol/src/usage_monitor.rs`
- `crates/jackin-protocol/src/usage_monitor/tests.rs`
- `crates/jackin-usage/src/host/broker.rs`
- `crates/jackin-usage/src/host/broker/dispatch_ops.rs`
- `crates/jackin-usage/src/host/broker/monitor.rs`
- `crates/jackin-usage/src/host/broker/monitor/evaluation.rs`
- `crates/jackin-usage/src/host/broker/monitor/evidence.rs`
- `crates/jackin-usage/src/host/broker/monitor/legacy/conversion.rs`
- `crates/jackin-usage/src/host/broker/monitor/projection.rs`
- `crates/jackin-usage/src/host/broker/monitor/quota.rs`
- `crates/jackin-usage/src/host/broker/monitor/status.rs`
- `crates/jackin-usage/src/host/broker/monitor/storage.rs`
- `crates/jackin-usage/src/host/broker/monitor/tests.rs`
- `crates/jackin-usage/src/host/broker/monitor/validation.rs`
- `crates/jackin-usage/src/host/broker/probe.rs`
- `crates/jackin-usage/src/host/broker/serve_loop.rs`
- `crates/jackin-usage/src/host/broker/serve_loop/tests.rs`
- `crates/jackin-usage/src/host/broker/service.rs`
- `crates/jackin-usage/src/host/broker/test_support.rs` (added)
- `crates/jackin-usage/src/host/broker/tests.rs`
- `crates/jackin-usage/src/host/broker/waits.rs`
- `crates/jackin-usage/src/usage.rs`
- `crates/jackin-usage/src/usage/claude.rs`
- `crates/jackin-usage/src/usage/claude/lease.rs`
- `crates/jackin-usage/src/usage/claude/lease/tests.rs`
- `crates/jackin-usage/src/coordinator/construct.rs`
- `crates/jackin-usage/src/coordinator/finish.rs`
- `crates/jackin-usage/src/coordinator/policy.rs`
- `crates/jackin-usage/src/coordinator/policy/tests.rs`
- `crates/jackin-usage/src/coordinator/tests.rs`
- `crates/jackin-usage/src/host.rs`
- `crates/jackin-usage/src/lib.rs`
- `crates/jackin-usage/Cargo.toml`
- `crates/jackin-usage/tests/fixtures/contracts/provider-call-allowlist.json`
- `crates/jackin/Cargo.toml`
- `crates/jackin/src/cli/usage.rs`
- `crates/jackin/src/cli/usage/tests.rs`
- `crates/jackin/src/lib.rs`
- `crates/jackin/tests/broker_service_lifecycle.rs`
- `docs/content/(public)/commands/usage.mdx`
- `docs/content/research/product/desktop/usage-provider-apis/03-claude-api.mdx`
- `plans/claude-unattended-monitor/bootstrap-contract.md`
- `plans/claude-unattended-monitor/claude-code-handoff.md`
- `plans/claude-unattended-monitor/installed-smoke.py`
- `plans/claude-unattended-monitor/statusline-contract.md`
- `plans/claude-unattended-monitor/task-queue.md`
- `plans/claude-unattended-monitor/verification.md`

## Where P already has a newer owner for the same behavior

P is a descendant of M and has split the flat crates. For these behaviors the P path is the later home. The U delta above is not in that home.

| U / M path | P owner | What P already holds |
| --- | --- | --- |
| `crates/jackin-protocol/src/usage_broker.rs` | `crates/core/jackin-protocol/src/usage_broker.rs` | wire `"v8"`, not U's `"v9"` |
| `crates/jackin-protocol/src/usage_monitor.rs` | `crates/core/jackin-protocol/src/usage_monitor.rs` | monitor schema `4`; issue enum has `MissingReset` and `CollectorAuthRequired` and does not have U's `ProviderRateLimited` / `ProviderUnauthorized` family |
| `crates/jackin-usage/src/host/broker/monitor/storage.rs` | `crates/services/jackin-usage-broker/src/monitor/storage.rs` | migrates v1–v3 to schema 4 and refuses later schemas, including the schema-5 state U migrates |
| `crates/jackin-usage/src/usage/claude.rs` and `lease.rs` | `crates/services/jackin-usage-provider-claude/` (`collector.rs`, `wave.rs`, `lease.rs`) plus `crates/services/jackin-usage-provider-core/src/refresh.rs` | typed `ProviderErrorKind` and `error.status()` / `provider_http_status` for 401, 403, and 429 |
| `crates/jackin-usage/src/coordinator/state/envelope.rs` (unchanged by U) | `crates/services/jackin-usage-coordinator/src/state/envelope.rs` | account-state schema 3, previous 2, legacy 1; projection schema still 3. This is newer than U's account schema 2 |
| `crates/jackin-usage/src/coordinator/policy.rs` and `policy/tests.rs` | `crates/services/jackin-usage-coordinator/src/policy.rs` and `policy/tests.rs` | blobs equal M, so P owns the pre-U policy text at the new path |
| `crates/jackin/src/cli/usage.rs` | `crates/apps/jackin/src/cli/usage.rs` | relocated CLI; blob is neither M nor U |
| `plans/claude-unattended-monitor/bootstrap-contract.md` | same path on P | blob equals M; U's edit is absent |
| `docs/content/(public)/commands/usage.mdx` | same path on P | documents schema 4 / wire v8 / account schema 3, not U's schema 6 / wire v9 |

## T — 2 commits reachable from T and not from M

### `f8fd6f0f1803470b2d5efeac93482f11a2debee8` — refactor: simplify mise task surface

Parent is the T/M merge base `9b6e1d92`. Files:

`.github/workflows/ci.yml`, `COMMITS.md`, `CONTRIBUTING.md`, `Cargo.toml`, `PROJECT_STRUCTURE.md`, `TESTING.md`, `crates/jackin-dev/src/main.rs`, `crates/jackin-usage-ffi/README.md`, `crates/jackin-usage-ffi/boltffi.toml`, `crates/jackin-usage/src/host/broker.rs`, `crates/jackin-xtask/Cargo.toml`, `crates/jackin-xtask/src/ci_evidence.rs`, `crates/jackin-xtask/src/ci_evidence/fixtures/ci-evidence.yml`, `crates/jackin-xtask/src/ci_evidence/fixtures/ci-push-head-ledger.yml`, `crates/jackin-xtask/src/ci_evidence/tests.rs`, `crates/jackin-xtask/src/ci_evidence/validation.rs`, `crates/jackin-xtask/src/ci_stage.rs`, `crates/jackin-xtask/src/clippy_affected.rs`, `crates/jackin-xtask/src/desktop.rs`, `crates/jackin-xtask/src/desktop/tests.rs`, `crates/jackin-xtask/src/main.rs`, `crates/jackin-xtask/src/preview.rs`, `crates/jackin-xtask/src/preview/tests.rs`, `crates/jackin-xtask/src/release_archive.rs`, `crates/jackin-xtask/tests/homebrew_preview_008.rs`, `crates/jackin/Cargo.toml`, `docker/construct/README.md`, `docs/content/(public)/(role-authoring)/developing/construct-image.mdx`, `docs/content/reference/getting-oriented/xtasks.mdx`, `docs/content/reference/runtime/configuration.mdx`, `docs/content/research/engineering/build-performance/rust-build-cache-hygiene.mdx`, `docs/content/research/engineering/build-performance/shared-ci-compiler-cache.mdx`, `docs/content/research/engineering/desktop-verification/commands.mdx`, `docs/content/roadmap/rust-build-cache-hygiene.mdx`, `hk.pkl`, `mise.lock`, `mise.toml`, `native/Design/Prototypes/UnifiedAgentUsage/DESIGN_AUDIT.md`, `native/Design/Prototypes/UnifiedAgentUsage/SIGNOFF.md`, `native/Design/UnifiedAgentUsage/SwiftProjectReadiness.md`, `native/Design/UnifiedAgentUsage/SwiftProjectSetupAudit.md`, `native/README.md`, `native/Scripts/VisualQA/capture-final-matrix.sh`, `native/Tests/JackinUsageBridgeTests/ArchitectureTests.swift`, `native/mise.toml`, `research/agent-usage-platform/08-surfaces-release-tooling.md`, `research/agent-usage-platform/09-shipped-surface-review.md`, `rust-toolchain.toml`, `scripts/release-preview-package`, `scripts/test-mbx-cargo-wrapper.py` (deleted), `scripts/test-pre-commit-snapshot`.

### `9ef617d014df15c6d967501bcbbdcb3689ed453c` — chore: modernize Rust and mise tooling

Files:

`CONTRIBUTING.md`, `Cargo.lock`, `Cargo.toml`, `crates/jackin-config/fuzz/Cargo.lock`, `crates/jackin-diagnostics/src/observability/otlp/retry.rs`, `crates/jackin-docker/src/error.rs`, `crates/jackin-docker/src/net.rs`, `crates/jackin-env/fuzz/Cargo.lock`, `crates/jackin-lints/Cargo.lock`, `crates/jackin-lints/Cargo.toml`, `crates/jackin-manifest/fuzz/Cargo.lock`, `crates/jackin-manifest/fuzz/Cargo.toml`, `crates/jackin-protocol/fuzz/Cargo.lock`, `crates/jackin-telemetry/registry/rust.toml`, `crates/jackin-telemetry/src/schema/attrs.rs`, `crates/jackin-usage-ffi/Cargo.toml`, `crates/jackin-xtask/src/ci_evidence/tests.rs`, `crates/jackin-xtask/src/ci_junit.rs`, `crates/jackin-xtask/src/desktop.rs`, `crates/jackin-xtask/src/desktop/tests.rs`, `crates/jackin-xtask/src/telemetry_registry.rs`, `docs/bun.lock`, `docs/mise.lock`, `docs/mise.toml`, `docs/package.json`, `hk.pkl`, `mise.lock`, `mise.toml`, `native/Design/Prototypes/UnifiedAgentUsage/Package.swift`, `native/Package.swift`, `rust-toolchain.toml`, `scripts/phase0-apple-container.sh`.

## Whether P already contains each T behavior

History: neither SHA is in P. Content:

`f8fd6f0`'s mise split is already in P, then edited further. Root `mise.toml` is 836 lines on the merge base and on M, and 57 lines on both T and P. Both T and P add `native/mise.toml`, delete `scripts/test-mbx-cargo-wrapper.py` (still present on M), and share an identical `scripts/release-preview-package` blob. P's `mise.toml` and `native/mise.toml` are not the T blobs. P raises `min_version` from T's `2026.10.3` to `2026.10.7`, pins `mr-boxington` `1.23.0` instead of T's `1.22.0`, pins `rust = { version = "1.99.0", mr_boxington = true }` instead of T's `version = "latest"`, and calls `mbx +1.99.0` from native tasks.

`9ef617d0`'s Rust floor is already in P. Both set `Cargo.toml` `rust-version = "1.99"` and `rust-toolchain.toml` `channel = "1.99.0"`. The toolchain file text differs: T rewrites the comment; P keeps M's comment and only changes the channel. `docs/package.json` and `docs/mise.toml` blobs are identical on T and P. Root `Cargo.toml` is not identical, because P also restructures the workspace.

## Rust floor

| Tree | `Cargo.toml` `rust-version` | `rust-toolchain.toml` `channel` |
| --- | --- | --- |
| M | `1.97` (`Cargo.toml:45`) | `1.97.1` |
| U | `1.97` (unchanged from M) | `1.97.1` |
| T | `1.99` (`Cargo.toml:45`) | `1.99.0` |
| P | `1.99` (`Cargo.toml:163`, workspace) | `1.99.0` |
| T/M merge base `9b6e1d92` | `1.97` | `1.97.1` |

The M floor is Cargo `1.97` with channel `1.97.1`, not a bare channel string `1.97`. T and P both sit on Cargo `1.99` and channel `1.99.0`.

## M — 4 commits reachable from M and not from T

These are the commits after `9b6e1d92`. P contains them because P contains M. T does not.

### `868ce53519234f879d26a8bd2dffaa30fae2d729` — Salvage supplemental snapshot: money test, scroll clamp, exec cancel (#1118)

`crates/jackin-capsule/src/daemon/control_reply.rs`, `crates/jackin-capsule/src/daemon/control_reply/tests.rs`, `crates/jackin-console/src/tui/screens/editor/view.rs`, `crates/jackin-console/src/tui/screens/editor/view/frame.rs`, `crates/jackin-console/src/tui/screens/editor/view/frame_regression_tests.rs`, `crates/jackin-usage-ffi/src/dto.rs`.

### `3f3ddd7c48284b648b083305f969ee3e709b74b7` — feat(usage): add explicit Claude usage monitoring (#1122)

`Cargo.lock`, `crates/jackin-capsule/src/daemon/tests.rs`, `crates/jackin-capsule/src/usage_relay_proxy.rs`, `crates/jackin-capsule/src/usage_relay_proxy/tests.rs`, `crates/jackin-console/src/tui/screens/usage.rs`, `crates/jackin-console/src/tui/screens/usage/tests.rs`, `crates/jackin-protocol/src/lib.rs`, `crates/jackin-protocol/src/usage_broker.rs`, `crates/jackin-protocol/src/usage_broker/tests.rs`, `crates/jackin-protocol/src/usage_monitor.rs`, `crates/jackin-protocol/src/usage_monitor/tests.rs`, `crates/jackin-runtime/src/usage_relay.rs`, `crates/jackin-runtime/src/usage_relay/tests.rs`, `crates/jackin-runtime/tests/scope_verification.rs`, `crates/jackin-telemetry/registry/attributes.yaml`, `crates/jackin-telemetry/src/schema/attrs.rs`, `crates/jackin-telemetry/src/schema/enums.rs`, `crates/jackin-telemetry/src/schema/metrics.rs`, `crates/jackin-telemetry/src/schema/spans.rs`, `crates/jackin-usage-ffi/README.md`, `crates/jackin-usage-ffi/src/bridge.rs`, `crates/jackin-usage-ffi/src/bridge/tests.rs`, `crates/jackin-usage-ffi/src/bridge/tests/selected_account_route.rs`, `crates/jackin-usage-ffi/src/discovery.rs`, `crates/jackin-usage-ffi/src/discovery/tests.rs`, `crates/jackin-usage-ffi/src/dto.rs`, `crates/jackin-usage-ffi/src/lib.rs`, `crates/jackin-usage-ffi/src/presentation.rs`, `crates/jackin-usage/Cargo.toml`, `crates/jackin-usage/README.md`, `crates/jackin-usage/src/coordinator.rs`, `crates/jackin-usage/src/coordinator/cadence.rs`, `crates/jackin-usage/src/coordinator/catalog.rs`, `crates/jackin-usage/src/coordinator/config.rs`, `crates/jackin-usage/src/coordinator/construct.rs`, `crates/jackin-usage/src/coordinator/entries.rs`, `crates/jackin-usage/src/coordinator/errors.rs`, `crates/jackin-usage/src/coordinator/finish.rs`, `crates/jackin-usage/src/coordinator/jobs.rs`, `crates/jackin-usage/src/coordinator/join.rs`, `crates/jackin-usage/src/coordinator/outcome.rs`, `crates/jackin-usage/src/coordinator/policy.rs`, `crates/jackin-usage/src/coordinator/policy/tests.rs`, `crates/jackin-usage/src/coordinator/reconcile.rs`, `crates/jackin-usage/src/coordinator/refresh.rs`, `crates/jackin-usage/src/coordinator/state.rs`, `crates/jackin-usage/src/coordinator/state/envelope.rs`, `crates/jackin-usage/src/coordinator/state/sanitize.rs`, `crates/jackin-usage/src/coordinator/state/stores.rs`, `crates/jackin-usage/src/coordinator/state/tests.rs`, `crates/jackin-usage/src/coordinator/state/validate.rs`, `crates/jackin-usage/src/coordinator/tests.rs`, `crates/jackin-usage/src/coordinator/upkeep.rs`, `crates/jackin-usage/src/coordinator/worker.rs`, `crates/jackin-usage/src/host.rs`, `crates/jackin-usage/src/host/accounts.rs`, `crates/jackin-usage/src/host/broker.rs`, `crates/jackin-usage/src/host/broker/catalog.rs`, `crates/jackin-usage/src/host/broker/catalog/tests.rs`, `crates/jackin-usage/src/host/broker/catalog_diagnostics.rs`, `crates/jackin-usage/src/host/broker/catalog_diagnostics/tests.rs`, `crates/jackin-usage/src/host/broker/client.rs`, `crates/jackin-usage/src/host/broker/client_security.rs`, `crates/jackin-usage/src/host/broker/credential_scope.rs`, `crates/jackin-usage/src/host/broker/dispatch_ops.rs`, `crates/jackin-usage/src/host/broker/monitor.rs`, `crates/jackin-usage/src/host/broker/monitor/evaluation.rs`, `crates/jackin-usage/src/host/broker/monitor/evidence.rs`, `crates/jackin-usage/src/host/broker/monitor/legacy.rs`, `crates/jackin-usage/src/host/broker/monitor/legacy/conversion.rs`, `crates/jackin-usage/src/host/broker/monitor/legacy/tests.rs`, `crates/jackin-usage/src/host/broker/monitor/legacy/validation.rs`, `crates/jackin-usage/src/host/broker/monitor/projection.rs`, `crates/jackin-usage/src/host/broker/monitor/quota.rs`, `crates/jackin-usage/src/host/broker/monitor/reconcile.rs`, `crates/jackin-usage/src/host/broker/monitor/spend.rs`, `crates/jackin-usage/src/host/broker/monitor/spend/tests.rs`, `crates/jackin-usage/src/host/broker/monitor/status.rs`, `crates/jackin-usage/src/host/broker/monitor/statusline.rs`, `crates/jackin-usage/src/host/broker/monitor/statusline/tests.rs`, `crates/jackin-usage/src/host/broker/monitor/storage.rs`, `crates/jackin-usage/src/host/broker/monitor/tests.rs`, `crates/jackin-usage/src/host/broker/monitor/validation.rs`, `crates/jackin-usage/src/host/broker/monitor/validation/policy_history_tests.rs`, `crates/jackin-usage/src/host/broker/monitor_api.rs`, `crates/jackin-usage/src/host/broker/publish.rs`, `crates/jackin-usage/src/host/broker/publish/catalog_diagnostics.rs`, `crates/jackin-usage/src/host/broker/publish/catalog_diagnostics/tests.rs`, `crates/jackin-usage/src/host/broker/publish/tests.rs`, `crates/jackin-usage/src/host/broker/serve_loop.rs`, `crates/jackin-usage/src/host/broker/serve_loop/tests.rs`, `crates/jackin-usage/src/host/broker/service.rs`, `crates/jackin-usage/src/host/broker/tests.rs`, `crates/jackin-usage/src/host/broker/waits.rs`, `crates/jackin-usage/src/host/credential_resolver.rs`, `crates/jackin-usage/src/host/discovery.rs`, `crates/jackin-usage/src/host/discovery/profile_validation.rs`, `crates/jackin-usage/src/host/discovery/tests.rs`, `crates/jackin-usage/src/host/model.rs`, `crates/jackin-usage/src/host/projection.rs`, `crates/jackin-usage/src/host/projection/tests.rs`, `crates/jackin-usage/src/host/projection_runtime.rs`, `crates/jackin-usage/src/host/tests.rs`, `crates/jackin-usage/src/lib.rs`, `crates/jackin-usage/src/tests.rs`, `crates/jackin-usage/src/usage.rs`, `crates/jackin-usage/src/usage/claude.rs`, `crates/jackin-usage/src/usage/claude/keychain.rs`, `crates/jackin-usage/src/usage/claude/keychain/tests.rs`, `crates/jackin-usage/src/usage/claude/lease.rs`, `crates/jackin-usage/src/usage/claude/lease/tests.rs`, `crates/jackin-usage/src/usage/claude/tests.rs`, `crates/jackin-usage/src/usage/format.rs`, `crates/jackin-usage/src/usage/refresh.rs`, `crates/jackin-usage/src/usage/tests.rs`, `crates/jackin-usage/tests/fixtures/contracts/provider-call-allowlist.json`, `crates/jackin/Cargo.toml`, `crates/jackin/src/app.rs`, `crates/jackin/src/app/tests.rs`, `crates/jackin/src/bin/usage-broker/main.rs`, `crates/jackin/src/bin/usage-broker/tests.rs`, `crates/jackin/src/cli/telemetry.rs`, `crates/jackin/src/cli/tests.rs`, `crates/jackin/src/cli/usage.rs`, `crates/jackin/src/cli/usage/auth.rs`, `crates/jackin/src/cli/usage/errors.rs`, `crates/jackin/src/cli/usage/statusline.rs`, `crates/jackin/src/cli/usage/store.rs`, `crates/jackin/src/cli/usage/store/tests.rs`, `crates/jackin/src/cli/usage/tests.rs`, `crates/jackin/src/console/adapter/run.rs`, `crates/jackin/src/main.rs`, `crates/jackin/src/tests.rs`, `crates/jackin/tests/broker_installation.rs`, `crates/jackin/tests/broker_service_lifecycle.rs`, `crates/jackin/tests/docs_commands.rs`, `crates/jackin/tests/usage_bootstrap_offline.rs`, `crates/jackin/tests/usage_broker_e2e.rs`, `crates/jackin/tests/usage_monitor_offline.rs`, `docs/content/(public)/commands/usage.mdx`, `docs/content/(public)/guides/macos-usage-menu-bar.mdx`, `docs/content/research/product/desktop/usage-provider-apis/01-current-coverage.mdx`, `docs/content/research/product/desktop/usage-provider-apis/03-claude-api.mdx`, `native/README.md`, `plans/claude-unattended-monitor/` (bootstrap, handoff, protocol-v7, statusline, v2 and v3 installation/smoke artifacts, verification), `plans/unified-agent-usage/spec/canonical-projection.md`, `ratchet.toml`.

### `835df8071da4b2802eef8619250e3c6b5bbfb4a6` — fix(usage): add bounded auth malformed diagnostics (#1123)

`crates/jackin-usage/Cargo.toml`, `crates/jackin-usage/src/host/broker/service.rs`, `crates/jackin-usage/src/host/broker/tests.rs`, `crates/jackin-usage/src/usage.rs`, `crates/jackin-usage/src/usage/claude.rs`, `crates/jackin-usage/src/usage/claude/auth_diagnostic_tests.rs`, `crates/jackin-usage/src/usage/claude/lease.rs`, `crates/jackin-usage/src/usage/claude/lease/tests.rs`, `crates/jackin/src/bin/usage-broker/main.rs`, `crates/jackin/src/bin/usage-broker/tests.rs`, `plans/claude-unattended-monitor/auth-malformed-diagnostic.md`, `plans/claude-unattended-monitor/task-queue.md`, `plans/claude-unattended-monitor/v4-installation.json`, `plans/claude-unattended-monitor/v5-installation.json`, `plans/claude-unattended-monitor/v6-installation.json`, `plans/claude-unattended-monitor/verification.md`.

### `1c3dd1be4a3b1ed518133ec3899c084c792370ff` — fix(usage): retain foreground broker for auth lease (#1124)

`crates/jackin-usage/src/host/broker/serve_loop.rs`, `crates/jackin-usage/src/host/broker/serve_loop/tests.rs`, `plans/claude-unattended-monitor/task-queue.md`, `plans/claude-unattended-monitor/v7-installation.json`.

## Return

- U schema/wire actually found: monitor schema **6**, broker wire **v9** (previous lease wire **v8**), statusline input schema **2**, projection envelope **3**, account-state envelope **2**. M is monitor **4** / wire **v8** / statusline **2** / projection **3** / account **2**. P is monitor **4** / wire **v8** / statusline **2** / projection **3** / account **3**.
- Live acceptance is **not** evidenced. U's handoff says live acceptance remains false, and the usage doc says the schema-6 / wire-v9 result is an isolated fixture smoke with live verification still pending.
- T commits not in P: `f8fd6f0f1803470b2d5efeac93482f11a2debee8` and `9ef617d014df15c6d967501bcbbdcb3689ed453c`. P already has the simplified mise surface and the Rust 1.99 floor, with later mise/mbx pins than T.
- M commits missing from T: `868ce53519234f879d26a8bd2dffaa30fae2d729`, `3f3ddd7c48284b648b083305f969ee3e709b74b7`, `835df8071da4b2802eef8619250e3c6b5bbfb4a6`, `1c3dd1be4a3b1ed518133ec3899c084c792370ff`.
