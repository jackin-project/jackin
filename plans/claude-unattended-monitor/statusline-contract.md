# Claude Code statusline contract

## Source fields and limits

Claude Code runs the configured `statusLine.command` as a local shell command,
passes its statusline JSON on stdin, and displays its stdout. It runs on session
start/resume and after assistant messages, `/compact`, permission-mode changes,
Vim-mode toggles, command changes, and `refreshInterval` timers. Reset or
prompt-cache expiration times in the last payload can also trigger a run.
Updates debounce by 300 ms; a newly triggered update cancels the in-flight
command. The official guide says `refreshInterval` reruns the command, but does
not promise a provider-usage refresh or specify that timer callbacks receive a
fresh provider sample. Jackin treats a timer callback as an execution, not proof
of a new provider observation.

The official [statusline guide](https://code.claude.com/docs/en/statusline)
documents these fields:

| Field | Meaning |
| --- | --- |
| `session_id` | Unique Claude Code session identifier |
| `version` | Optional reported Claude Code version |
| `model.id` | Optional model label |
| `rate_limits.five_hour.used_percentage` | 5-hour window utilization, 0–100 |
| `rate_limits.five_hour.resets_at` | 5-hour reset time, Unix epoch seconds |
| `rate_limits.seven_day.used_percentage` | 7-day window utilization, 0–100 |
| `rate_limits.seven_day.resets_at` | 7-day reset time, Unix epoch seconds |

The [v2.1.80 changelog](https://github.com/anthropics/claude-code/blob/v2.1.80/CHANGELOG.md)
records the addition of the five-hour and seven-day subscription quota fields.
The current guide says `rate_limits` appears only for Claude.ai Pro/Max
subscribers or behind a Claude apps gateway that sets a spend limit, and only
after the session's first API response. Each documented window may be absent
independently and is dropped after its `resets_at` passes. Jackin also accepts a
present window with either inner field omitted or null; the official guide does
not explicitly guarantee those partial-window shapes, so this is parser
tolerance. Missing or malformed data stays unknown; it is never converted to
0%. `used_percentage` is utilization, not remaining quota, tokens, or spend.
A reported `version` is bounded metadata stored as `claude_code_version`,
separate from Jackin's monitor schema version. Rejecting quota fields when a
reported version is before 2.1.80 is Jackin's conservative policy based on the
release note, not authenticated runtime-version verification. A missing version
is unknown; Jackin does not inspect the Claude Code executable to establish its
version.

For quota and model inputs, the adapter consumes the five-hour and seven-day
windows and optional model label. The current guide also documents
`rate_limits.spend_limit`, available from Claude Code v2.1.251 behind a
qualifying gateway; Jackin ignores that object, including its reset and
percentage. Its `used_usd`, `limit_usd`, and `period` fields require v2.1.284 or
later on both Claude Code and the gateway; Jackin also ignores them. These USD
fields are not billed amounts: `used_usd` is a gateway estimate and `limit_usd`
is the configured limit. Jackin does not map them to its SGD spend evidence or
treat them as permission for extra usage. Anthropic
documents paid-plan usage credits as allowing continued pay-as-you-go use after
included limits; the statusline input does not report whether credits are
enabled or available. The statusline guide does not document an `extra_usage`
field or model-specific quota limits. Jackin ignores unrelated `extra_usage`
and session cost fields; the documented `cost.total_cost_usd` is a client-side
list-price estimate that may differ from the actual bill.

The callback has no provider account ID or authoritative observation time.
`session_id` identifies a session, not an account. The callback cannot detect
an account switch or identify which provider credentials Claude Code chose;
Jackin does not verify that choice through credentials or provider requests.
It is not an exactly-once event: Claude Code may rerun it with unchanged input
or cancel an in-flight execution after another update. Treat callbacks as
observations, not per-turn deltas. Inactivity can leave them stale. The callback
is a best-effort local observation source, not a provider refresh scheduler.

## Scope and observation authority

`usage statusline ingest` requires exactly one explicit scope:

- `--session-only` stores the callback under its payload session ID without
  asserting any account identity.
- `--binding ID --binding-revision N` assigns it to a separately confirmed
  local account mapping. This mapping is operator-selected and does not
  authenticate the provider account.

Session-only observations stay in a separate partition, remain isolated
session evidence, and never enter an account aggregate or merge retroactively.
Account identity is never inferred from session ID, model, version, or quota
values. Before changing Claude Code's selected account or provider credentials,
the operator must review the current binding. Before ingesting evidence from a
different selected account, confirm a binding or revision mapped to that
account; the callback cannot update or verify the choice. The adapter ignores
unrelated fields, including session cost and `extra_usage`; it does not read
credentials or make provider requests.
Ingestion accepts at most 16 KiB of JSON from stdin.

Ingestion records evidence; it does not create or approve a dispatch guard. A
monitor started with `monitor observe` can track without a receipt, policy, or
quota, but its dispatch readiness is always `not_authorized` and `runnable` is
always false. A dispatch guard needs a confirmed binding and a separately
approved policy revision. Once strict policy is recorded, quota-only approval is
rejected even before first activation; do not describe strict-to-quota-only as a
supported transition. Successful passive doctor establishes only broker
availability and ingress capability. Its auth state is informational; it does
not establish quota, spend, or dispatch readiness.

Statusline ingestion remains the default supported local quota-observation
path and does not require `auth prepare`; it reads no credentials and makes no
provider request. The direct experimental collector is a separate optional
path: it requires attended foreground bootstrap and explicit approval on the
selected binding, but does not require statusline configuration. Neither path
implicitly enables, refreshes, or falls back to the other.

Each quota field ages independently. The local acceptance limit remains 300
seconds per field. Repeated identical callbacks, timer reruns, sibling-field
changes, and future reset epochs do not renew provider evidence. A callback has
no trusted per-field source timestamp, so a changed sibling cannot certify an
unchanged window, reset, or model as fresh. Time can make reset reevaluation due;
it cannot confirm reset or replenish quota. Reset grace, fresh-confirmation,
weekly, model, and threshold guards continue to apply. See the usage command
guide for the runnable rules and recovery conditions.

Status classifies each window's reset as `unknown`, `future`, or `due` in
`reset_validity`, independently of reset-field freshness. `future` and `due`
only compare the recorded reset epoch with broker time; `due` does not confirm
that allowance has reset. Status also reports configured `expected_model` and
`model_guard_validity`: `not_configured`, `unknown`, `match`, or `mismatch`.
`match` requires fresh model evidence; a missing or stale model under a
configured guard is `unknown`, and a fresh different model is `mismatch`. The
expected-model guard compares the model label only. A match is not model-specific
quota evidence, and unknown extra-usage permission never authorizes overage.
These are audit fields only. They do not renew evidence, alter age limits,
clear reset barriers, or relax quota/model guards or `runnable` decisions.

Provider refresh throttling is broker-owned and separate from statusline
callbacks. The rate-limit fix in progress targets a conservative Claude
minimum-attempt deadline: round provider-response completion up to the next
epoch second, then add 300 seconds. Recovery of an interrupted active attempt
starts a fresh 300-second floor from the rounded-up recovery time. The latest
of those floors, provider `Retry-After`, and retry backoff wins; an explicit
force refresh may bypass only success cooldown, never a retry, rate-limit, or
attempt/recovery floor. Focused fake-clock, restart, and forced-refresh
verification is pending. This is not a claim of verified or live-provider
behavior.

## Composition behavior

`usage statusline compose` reads a settings file up to 1 MiB and prints a JSON
Merge Patch with exactly one top-level property: `statusLine`. A missing file
is treated as an empty object for initial setup. For an existing command
statusline, the patch preserves its `type` and all other statusline properties,
replacing only `command`; if absent, it adds a command statusline that consumes
input and renders no output before ingesting. The patch excludes unrelated
settings keys and environment values. It does not write or create the settings
file, modify account state, approve a policy, or start Claude Code.

Review the patch and, if choosing to enable ingestion, manually merge only its
`statusLine` property into the existing settings file. Preserve every other
setting; never replace the complete file with the patch. Jackin does not apply
the patch or modify Claude settings. Malformed settings, settings over 1 MiB,
and non-command status lines are rejected. An existing command over 16 KiB of
UTF-8 bytes or a composed shell command over 64 KiB is also rejected.

For a new statusline, the output shape is:

```json
{
  "statusLine": {
    "type": "command",
    "command": "<composed command>"
  }
}
```

When an existing statusline has additional properties, they remain in the
`statusLine` object; unrelated top-level settings do not appear in the patch.

The wrapper uses Python 3's standard library in isolated mode to read Claude's
stdin once, stream it to the existing command, and retain at most 16 KiB plus
one byte. It then invokes the selected Jackin binary with `statusline ingest`,
the explicit `--session-only` or `--binding`/`--binding-revision` scope, JSON
format, and selected `--data-dir`. The existing command runs once and owns the
displayed stdout and exit status. If it exits unsuccessfully, ingestion is
skipped. Payloads over 16 KiB still flow through the existing command but skip
ingestion. If Python 3 is unavailable, the existing command runs directly once
and no evidence is ingested. Ingestion stdout and stderr are suppressed and its
timeout is two seconds; a failed or timed-out ingest does not replace the
existing command's output or successful exit status. The adapter requires a
POSIX-compatible shell. This composition does not perform OAuth lookup,
Keychain access, provider HTTP requests, or operator approval.

## Protocol and migration

The current source versions are independent: broker wire protocol v8, normalized
statusline input schema v2, and durable monitor-store schema v4. Wire v8 adds
`UsageIdentityKindV1::LocalSourceHandle` for locally derived credential-source
identity metadata. It does not claim a provider-issued or authenticated
account identity, and it does not change the statusline input or durable store.
Optional `claude_code_version` remains callback metadata; it is not Jackin's
store or input schema version. Schema v4 changes the durable local store only;
it does not change broker wire v8 or statusline input v2.

The V4 `historical_correction_horizon_epoch` records uncertainty about older
closed-period receipts no longer represented in the retained account periods.
It is a monotonic protection boundary, not evidence that a correction was
received. Later receipts or clock rollback cannot clear the affected goal's
unknown/incomplete state.

The store migrates V1, V2, and V3 snapshots explicitly to V4. It preserves
policy revisions and origins, baselines and cumulative estimates, quota-pause
latches, counters, evidence ages, cooldowns, and action/event sequences. Existing
unknown or incomplete states are not cleared; migration does not invent a
baseline, zero spend, approval, provider identity, or fresh callback time.
V1 strict policies retain `migrated_v1` history but are not operator-approved;
dispatch requires a separately confirmed binding and an operator policy
revision. Migrations do not infer new source mappings or collector approvals.
Known V2 state is preserved.

V3 did not store `historical_correction_horizon_epoch`, so it deserializes as
`None`. If no affected forward rollover is present, V3 known goal state is
preserved and the horizon remains `None`. For a V3 strict goal with a baseline
and retained account or goal anchors proving a forward period transition,
migration preserves its cumulative estimate but latches `rollover_unknown` and
`cumulative_complete: false`. A closed-period anchor only contributes when a
later goal anchor proves a rollover; it cannot rule out an older correction
that was already lost. The account horizon is the newest retained affected
current-period or goal-period start, or a qualifying closed-period end, bounded
by the snapshot clock. This records uncertainty, not proof that a correction
was received. A goal whose baseline starts at the horizon remains known; only a
strictly later horizon makes its spend incomplete. Later receipts or clock
rollback cannot clear an affected-goal latch.

The only budget-repair exception is a V1 `migrated_v1` strict policy whose
recorded budget is exactly zero SGD with exponent 2. A separate explicit
operator approval may replace it with a positive strict SGD budget, creating a
new `operator` revision while retaining the migrated policy, baseline,
cumulative spend, and historical uncertainty. This does not establish a
baseline: an affected goal remains blocked until valid baseline admission. The
exception never permits quota-only or applies to other currencies or malformed
state. New strict-goal activation without valid baseline admission fails
atomically without reserving its idempotency key or assigning a monitor ID.

For V2/V3, every persisted event's embedded status schema version must match
the enclosing source-store version before migration restamps it to V4. The
normalized source and all migrated state are validated before replacement; a
version mismatch, invalid source snapshot, or unsupported future schema fails
closed without replacing the original store.

The recorded v3 installed fixture passed for source commit
`1a45196dbe24d439e596c14e22fbda59799e7b0d` using wire v7. That fixture record
remains historical evidence for that exact pair; it does not verify the current
wire-v8 source. The installed pair is pending a rebuild after dormant-helper
cleanup and its provenance must be refreshed. That fixture did not verify a
real account, successful foreground Keychain authorization, or a live provider
request. This document describes the current source contract and does not claim
live readiness.

## Official references

- [Customize your status line — Claude Code Docs](https://code.claude.com/docs/en/statusline)
- [Claude Code changelog, v2.1.80](https://github.com/anthropics/claude-code/blob/v2.1.80/CHANGELOG.md)
- [Manage usage credits for paid Claude plans — Claude Help Center](https://support.claude.com/en/articles/12429409-manage-usage-credits-for-paid-claude-plans)

References checked 2026-10-10.
