# TASK-008 gap check

Not an approval. No implementation, no cargo, no live provider calls, no git edits.

Integration `1d76b4d11e3248d261594f45900f2f807b8c0807`. U `2e4a62da4bc4406764162a267163389a5fb513a7`. P `fba69d60c2373156dc0dcecee8e085c83776966f`.

Scope: `jackin-usage-provider-core`, `jackin-usage-provider-claude`, `jackin-usage-provider-codex`, and `jackin-usage-output` on the integration tree. U provider parsing was opened only where that tree lacks the rule. `jackin-usage-output` is stdout/stderr only (`src/lib.rs:9`) and holds none of these decisions.

## Typed status, not message text

ALREADY_PRESENT.

Credential and retry authority is `ProviderError::status`, `ProviderErrorKind`, and `UsageSnapshotStatus`. `provider_outcome` maps presence only (`jackin-usage-provider-core/src/outcome.rs:94`).

- `ProviderErrorKind` and `ProviderError::status` keep HTTP status off display text (`jackin-usage-provider-core/src/refresh.rs:13`, `refresh.rs:115`). `From<String>` stores no status (`refresh.rs:180`). Timeout, transport, and decode store no status (`refresh.rs:141`).
- Unauthorized and rate-limit checks read `status()` (`refresh.rs:199`, `refresh.rs:203`). `rate_limit()` requires typed HTTP 429 plus a parsed delay and receipt time (`refresh.rs:123`).
- Text that mentions 401, 403, 429, or `Retry-After` does not authorize or set a deadline (`jackin-usage-provider-core/src/tests/case_04.rs:208`, `case_04.rs:226`, `case_04.rs:355`). Codex repeats that split (`jackin-usage-provider-codex/src/tests/case_01.rs:7`).

U `usage/refresh.rs:106` and `usage/refresh.rs:190` match this. Nothing to port.

## Claude inference-only scope

ALREADY_PRESENT.

Inference-only classification is typed HTTP 403 (`jackin-usage-provider-claude/src/wave.rs:118`). Typed 401 becomes `NeedsLogin`; typed 403 and 429 stay `Stale` (`wave.rs:188`). Transport, decode, and plain strings that mention 403 do not qualify (`jackin-usage-provider-claude/src/tests/case_01.rs:7`, `case_01.rs:21`).

U `usage/claude.rs:466` is the same predicate. Nothing to port.

## 100% remaining means zero used only from a source observation

PORT.

The shared fraction helper already refuses this fabrication. A negative, non-finite, or absent utilization is not 100% remaining (`jackin-usage-provider-core/src/format/numbers.rs:16`, `numbers.rs:29`; locked by `src/tests/case_04.rs:396`). Account snapshots turn remaining into used only when `remaining_percent` is `Some` (`src/view/snapshots.rs:74`). Claude omits a limit with no `percent` (`jackin-usage-provider-claude/src/windows.rs:96`). Observed utilization `0.0` is the case that may become remaining 100 (`src/tests/case_02.rs:65`, `case_02.rs:115`).

Codex does not use that guard. A negative raw percent stays `Some` (`jackin-usage-provider-codex/src/types.rs:89`) and `used_percent_clamped` clamps it to 0 (`types.rs:96`). `push_codex_window` then stores remaining 100 and a `0% used` label (`src/windows.rs:229`). The test locks that result for `used_percent: -3` (`src/tests/case_01.rs:68`).

U `usage/codex.rs:527` and `usage/codex.rs:963` do the same clamp. Do not copy that. The edit is the integration fraction rule, not a U cherry-pick.

U monitor `projection.rs:270` can derive used from an observed `remaining_raw_percent`, including 100 to 0. That code is not provider parsing. It arrived in checkpoint `db59c401` with monitor projection, quota, and diagnostics. U's monitor schema is 6 (`crates/jackin-protocol/src/usage_monitor.rs:11`). REJECT porting that path. It would require schema 6. Do not recommend schema 6 or v9.

## Do not invent reset timestamps

ALREADY_PRESENT.

Claude reset is parsed ISO or absent (`jackin-usage-provider-claude/src/windows.rs:79`, `windows.rs:119`). A routines window with utilization and no `resets_at` keeps remaining and has no reset (`src/tests/case_02.rs:115`). Codex uses `reset_at` when present, otherwise a non-negative `reset_after_seconds` added to fetch time (`jackin-usage-provider-codex/src/types.rs:102`). Both fields absent stays `None`. U `usage/codex.rs:535` matches. Nothing to port.

## Session and weekly freshness stay independent

ALREADY_PRESENT.

Session and weekly are separate buckets and separate status-bar slots (`jackin-usage-provider-claude/src/windows.rs:98`, `jackin-usage-provider-core/src/view/status.rs:79`). A missing legacy window is not created (`windows.rs:215`). An absent Opus row is omitted rather than filled from another window (`src/tests/case_02.rs:119`). A failed view copies one status onto its buckets only when the view itself is not fresh (`jackin-usage-provider-core/src/view/refresh.rs:13`); that is one failed fetch, not a fresh weekly observation refreshing a session.

U provider parsing stamps each built window `Fresh` the same way (`usage/claude.rs:1890`). U's stronger per-window evidence is `projection_window_used_only_is_current` (`host/broker/monitor/projection.rs:685`) under monitor schema 6. REJECT that port. It is not provider parsing and would require schema 6.

## Schema 4, wire v8

ALREADY_PRESENT.

Integration monitor schema is 4 (`crates/core/jackin-protocol/src/usage_monitor.rs:11`). Broker wire is v8 (`crates/core/jackin-protocol/src/usage_broker.rs:14`). The four crates do not define or bump either version. U monitor schema is 6 (`crates/jackin-protocol/src/usage_monitor.rs:11`). REJECT any schema 6 or wire v9 recommendation.

## Edit list

1. In `jackin-usage-provider-codex/src/types.rs`, `used_percent_clamped` (`types.rs:96`): return `None` when the raw used percent is negative. Keep `0` as observed zero used. Keep over-cap values clamped only for the bar, with the raw label unchanged.
2. In `jackin-usage-provider-codex/src/windows.rs`, `push_codex_window` (`windows.rs:229`): do not turn a negative raw percent into remaining 100 or a `0% used` label. Leave used and remaining unset.
3. In `jackin-usage-provider-codex/src/tests/case_01.rs` (`case_01.rs:68`): expect no remaining and no zero-used label for `used_percent: -3`.

No other edit. Do not port U monitor used-only evidence, schema 6, or wire v9.

## Landed

Commit `544ae930` on `integration/branch-consolidation-20261013` (pushed).

- `used_percent_clamped` returns `None` when the raw percent is negative. Zero stays observed. Over-cap still clamps the bar with `min(100)`.
- `push_codex_window` labels only a non-negative raw percent. Negative input leaves `used_label` and `remaining_percent` unset.
- `codex_over_cap_keeps_raw_label_with_clamped_bar` expects `142% used` / remaining 0, `64% used` for 63.7, and `None` / `None` for `-3`.

The nextest command in the TASK-007 landed note ran this test in the same invocation. Exit 0. This note does not approve TASK-008.
