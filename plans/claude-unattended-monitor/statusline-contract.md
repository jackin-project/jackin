# Claude Code statusline contract

## Source fields and limits

Claude Code runs the configured `statusLine.command` as a local shell command,
passes its statusline JSON on stdin, and displays its stdout. It runs on session
start/resume and after assistant messages, `/compact`, permission-mode changes,
Vim-mode toggles, command changes, and `refreshInterval` timers. Reset or
prompt-cache expiration times in the last payload can also trigger a run.
Updates debounce by 300 ms; a newly triggered update cancels the in-flight
command. `refreshInterval` repeats the callback with its last payload. It does
not fetch new provider usage data.

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

Claude Code added the subscription quota fields in [v2.1.80](https://github.com/anthropics/claude-code/blob/v2.1.80/CHANGELOG.md).
Each rate-limit window is optional, and its percentage and reset are
independently optional. Windows may disappear after reset. Missing or malformed
data stays unknown; it is never converted to 0%. `used_percentage` is
utilization, not remaining quota, tokens, or spend. A reported `version` is
bounded metadata stored as `claude_code_version`, separate from Jackin's monitor
schema version. Quota fields claimed by a reported version before 2.1.80 are
rejected. A missing version is unknown, not authenticated proof of compatibility;
Jackin does not inspect the Claude Code executable to establish its version.

For quota and model inputs, the adapter consumes the five-hour and seven-day
windows and optional model label. The statusline does not provide model-specific
quota limits or permission for extra usage; the adapter ignores `extra_usage`
and cost fields.

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

## Composition behavior

`usage statusline compose` reads a settings file up to 1 MiB and prints proposed
settings JSON. A missing file is treated as an empty object for initial setup.
It does not write or create the settings file, modify account state, approve a
policy, or start Claude Code. If `statusLine` is present, it must be a command
object with a string `command`; composition changes only that command and
preserves other settings and statusline options. If absent, it proposes a
command that consumes input and renders no output before ingesting. Malformed
settings, settings over 1 MiB, and non-command status lines are rejected. An
existing command over 16 KiB of UTF-8 bytes or a composed shell command over
64 KiB is also rejected.

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

The frozen v2 monitor/store schema records optional reported client version
separately from Jackin schema version. The broker wire is v6. The explicit
one-time v1 decoder/converter preserves counters, records, evidence age,
decision sequences, barriers, and cumulative spend. Old policies retain strict
semantics and history with `migrated_v1` origin, but do not count as explicit
operator approval. Before dispatch under a migrated goal, the operator must
confirm the binding and record a new strict policy revision with `operator`
origin. Migrated bindings remain `operator_confirmed: false` with
`confirmed_at_epoch: null`; migrated policies remain `operator_confirmed: false`
with `recorded_at_epoch: null`. A callback with no recorded source time remains
unknown; migration does not invent a callback time or renew its evidence age.
An absent old baseline stays absent, and a later receipt does not retroactively
make prior history known.

The only budget-repair exception is a V1 `migrated_v1` strict policy whose
recorded budget is exactly zero SGD with exponent 2. A separate explicit
operator approval may replace it with a positive strict SGD budget, creating a
new `operator` revision while retaining the migrated policy, baseline,
cumulative spend, and historical uncertainty for audit. This exception never
permits quota-only or applies to other currencies or malformed state. Repairing
the zero budget does not establish a baseline. Approval can still record the
new operator policy revision while the existing migrated goal remains blocked
and non-runnable for dispatch until valid baseline admission. This readiness
state does not mean the policy approval was rejected. A new strict-goal
activation without valid baseline admission is rejected atomically; it reserves
no idempotency key or monitor ID. No zero spend or known history is inferred.
Unsupported or corrupt state fails closed. The installed binary/broker proof
for this v2 contract is pending; this document describes source behavior, not a
claim that a packaged install has passed proof.

## Official references

- [Customize your status line — Claude Code Docs](https://code.claude.com/docs/en/statusline)
- [Claude Code changelog, v2.1.80](https://github.com/anthropics/claude-code/blob/v2.1.80/CHANGELOG.md)
