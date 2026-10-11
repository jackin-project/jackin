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

### Current provider usage without a reset

A current, unambiguous provider window may contain fresh used-only evidence
while its reset is absent. The monitor accepts only a newly observed window
whose used field has current source freshness, a strictly newer source and
receipt time than retained evidence and reset-admission fences, and an
unscoped principal-window metric. It updates only the used field, records that
field's source/receipt times and input sequence, clears the unsupported used/reset
pair, and leaves the prior reset, account reset high-water mark, statusline
reset watermark, and admission cutoff unchanged. A genuinely new provider
generation with the same used value can refresh this used-only field when its
source and receipt times advance. A replayed generation cannot. A repeated or
timer-driven statusline callback does not renew unchanged statusline evidence
and cannot fill in or refresh the provider projection's missing reset; the two
sources remain separate.

The previous reset remains visible for diagnosis. It is not paired with the
new used-only provider field. Therefore `MissingReset` keeps quota readiness
blocked even if the retained reset evidence is otherwise current or its epoch
is still `future`; `provider: current` does not mean the monitor has a usable
quota pair. The stale five-hour value remains stale until the new used-only
observation is accepted, and the reset itself keeps its original evidence age.
Unknown or missing reset data is not converted into a reset time, and an old or
future reset plus fresh used data does not make the window dispatch-ready.

A fresh provider used-only value at or above 95% creates or rebases a durable
account reset barrier and the active monitor barrier. Account barriers share one
reset fence across statusline and broker observations: latching takes the
maximum of the incoming reset, the account-wide reset maximum, the retained
broker reset, and any existing barrier high-water. A later low used-only value
cannot erase that fence. When the selected fresh high used-only value has no
paired reset, the monitor-local pause barrier may use this known associated
account reset maximum internally. That is only a recovery fence; it does not
create fresh reset evidence, fill `MissingReset`, or make quota ready. If no
reset history is known, no reset is inferred and the monitor-local barrier
fails closed. Passing the old reset plus the 60-second grace, a reset-only
update, a model-only callback, or elapsed time cannot release the pause.
Recovery requires a newly accepted, fresh used/reset pair from one evidence
source (and one session when that source is session-scoped), received after the
grace deadline, with usage below the barrier's prior high-water usage and a
reset strictly beyond the barrier's recorded reset high-water mark and no
lower than the account reset maximum.
Carrying a known account maximum into the monitor barrier lets a later
qualifying advanced pair satisfy that strict release check. A statusline
callback can supply its own paired evidence, but cannot borrow the broker
projection's used-only value or reset.

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
callbacks. The current source enforces a conservative Claude minimum-attempt
deadline: round provider-response completion up to the next epoch second, then
add 300 seconds. Recovery of an interrupted active attempt starts a fresh
300-second floor from the rounded-up recovery time. The latest of those floors,
provider `Retry-After`, and retry backoff wins; an explicit force refresh may
bypass only success cooldown, never a retry, rate-limit, or attempt/recovery
floor. The verification record lists the 57-test coordinator scope for checkpoint
22ac, including the attempt-floor and restart coverage, and the 28-test Claude
provider/lease lifecycle scope rerun for checkpoint 45a33093. These results do
not constitute live-provider verification.

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

The current source versions are independent: broker wire protocol v9,
normalized statusline input schema v2, durable monitor-store schema v6, and
durable projection-envelope schema v3. `UsageIdentityKindV1::LocalSourceHandle`
identifies a stable local credential-source handle, not a provider-issued or
authenticated account identity. Optional `claude_code_version` is callback
metadata; it is not Jackin's store or input schema version.

Monitor-store schemas V1–V5 migrate explicitly to V6, after semantic
validation. Migration preserves policy revisions and origins, baselines and
cumulative estimates, quota-pause latches, counters, evidence ages, cooldowns,
action/event sequences, and existing unknown or incomplete state. V1–V4
provider snapshots without verifiable binding/source provenance are dropped;
V5 source-bound provider provenance is retained. Migrations do not invent a
baseline, zero spend, approval, provider identity, collector opt-in, or fresh
callback time. V1 strict policies retain `migrated_v1` history but are not
operator-approved; dispatch still requires a confirmed binding and a new
operator policy revision.

V6 adds a per-window statusline reset watermark: `Unknown`, `KnownNone`, or
`Known { reset_at_epoch, received_at_epoch }`. For each V1–V5 account window, a
nonempty old `latest_reset_epochs` value becomes `Unknown`; an empty value
becomes `KnownNone`. Migration does not infer statusline reset or receipt time
from the old account-wide reset, quota values, or event history. When the old
account-wide reset is nonempty, V6 stores the old `last_now_epoch` as a strict
admission cutoff. A lower-reset repair requires a complete current provider
pair whose used, reset, paired-source, and receipt timestamps are all after
that cutoff. Such a repair may update displayed values, but does not lower the
account-wide reset maximum or release a quota barrier. V1–V5 snapshots that
contain V6-only reset metadata are rejected, including V6 snapshots relabeled
as older schemas.

Migration also preserves the monotonic
`historical_correction_horizon_epoch`. When V3 rollover history could have
hidden an older correction, migration retains the known strict-goal estimate
but latches cumulative completeness false and rollover unknown. The horizon
records possible lost history, not proof that a correction was received; later
receipts or clock rollback cannot clear an affected goal's uncertainty. A goal
whose baseline starts at the horizon remains independently known.

## v11 live evidence boundary

The installed v11 CLI/broker pair passed an isolated synthetic fixture, but
that does not establish existing-state migration or live collection. The
subsequent live record documents two natural provider generations against the
existing v7 state: weekly usage was current at 92%, while the Session
projection had 100% remaining usage but no reset. The monitor retained stale
five-hour evidence and reported `quota_stale` and `reset_due_unverified`;
acceptance remained false. The `MissingReset` behavior described above is a
source contract, not an outcome recorded in this live capture. The capture did
not publish Session used-only evidence, so it does not verify that source path
or establish readiness. See the [v11 live
verification report](v11-live-verification.md) and its [sanitized JSON
record](v11-live-verification.json); the installed-fixture provenance is in
[v11-installation.json](v11-installation.json).

## Official references

- [Customize your status line — Claude Code Docs](https://code.claude.com/docs/en/statusline)
- [Claude Code changelog, v2.1.80](https://github.com/anthropics/claude-code/blob/v2.1.80/CHANGELOG.md)
- [Manage usage credits for paid Claude plans — Claude Help Center](https://support.claude.com/en/articles/12429409-manage-usage-credits-for-paid-claude-plans)

References checked 2026-10-10.
