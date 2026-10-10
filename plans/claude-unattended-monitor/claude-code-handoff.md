# Claude Code usage observer handoff

**Recorded installed fixture: passed; pair rebuild pending. Real-account/provider
readiness: unverified.** The installed v3 CLI and matching sibling broker
recorded below were version `0.6.4`, built from source commit
`1a45196dbe24d439e596c14e22fbda59799e7b0d`. The pair will be rebuilt after
dormant-helper cleanup. These hashes and the fixture log describe the prior
pair only; refresh provenance and rerun the installed fixture after rebuild
before treating the rebuilt pair as verified or running this handoff.

- CLI: `/Users/donbeave/.local/share/jackin-claude-monitor-v3/bin/jackin`
- Broker: `/Users/donbeave/.local/share/jackin-claude-monitor-v3/bin/jackin-usage-broker`
- CLI SHA-256: `243469d19697f339e982214e2b1b84dd44fc57f601910dec2a9983b0e397cbe0`
- Broker SHA-256: `b96073209766236a7f30eee7c34725ff283dd47e6fe30c0df3cc24fcfbf2f5e4`
- Prior installed fixture log: [/private/tmp/jackin-v3-installed-smoke.log](/private/tmp/jackin-v3-installed-smoke.log)

The isolated installed smoke passed help, sibling selection, passive-service,
observer, and statusline-fixture checks. It observed zero HTTP proxy requests
and no dispatch policy approvals. It did **not** instrument native Security
Framework calls or exercise successful foreground authentication. No real
account, Keychain authorization, or live provider request was verified; the
experimental endpoint remains unsupported and may fail. This is not a
no-dialog or live-readiness guarantee.

The v3 state directory below is currently empty and isolated; it is not a
configured account or evidence store. Use it only if creating this new local
state is intended. Do not point these commands at an old v1/v2 store or any
existing state that must be preserved. Keep the same verified binary pair and
data directory for all commands; do not use a repository `target/debug`
binary.

```bash
JACKIN='/Users/donbeave/.local/share/jackin-claude-monitor-v3/bin/jackin'
DATA='/Users/donbeave/.local/share/jackin-claude-monitor-v3/state'
```

The smoke selected the sibling broker with `JACKIN_USAGE_BROKER_BIN` unset.
Leave it unset for this install. If it is set to an empty or different path,
stop and resolve the mismatch; do not change Claude's environment to work
around it.

## Optional experimental collection without a statusline

The direct collector does not require a Claude statusline or settings change.
It is experimental: it calls the observed but undocumented
`GET /api/oauth/usage` route, which has no supported public contract here and
may return 403 or change without notice. Availability, quota freshness, and
continued authorization are not guaranteed. Jackin uses its own identity and
does not impersonate Claude Code.

### 1. Attended foreground bootstrap

Run in a real terminal with stdin, stdout, and stderr attached. An exact
`--keychain-service SERVICE` may be added if the operator has already selected
the intended generic-password service; the default service is otherwise used.

```bash
"$JACKIN" usage --data-dir "$DATA" auth prepare --provider claude
```

Bootstrap claims the broker lease before reading the selected Keychain item,
then serves from a zeroizing in-memory credential cache with the no-UI guard
active. It makes no provider request itself. macOS may require authorization
during this attended read; no dialog-free guarantee is implied. Keep this
terminal and process running while collecting. The secret-free `service_ready`
record includes `source.account_id` and
`source.scope: "claude_keychain_service"`; that ID identifies a local Jackin
source, not an authenticated Anthropic account.

The cache and lease last only for this foreground process. Exiting or
restarting it clears the cache and requires another attended bootstrap;
provider authorization may expire sooner. Do not start the passive service on
this data directory first. If bootstrap reports `broker_conflict` or ownership
is unknown, do not stop, replace, or attach to that service; resolve its owner
before retrying.

### 2. Confirm the local source binding and collector opt-in

In a second attended terminal using the same binary and state directory, choose
the local Jackin account label and use the exact `source.account_id` from
`service_ready` as `LOCAL_SOURCE_ID`. This source ID is not a verified provider
identity. Do not guess either value or infer an account from a Claude session
ID. Binding confirmation requires all three standard streams to be terminals.

```bash
"$JACKIN" usage --data-dir "$DATA" binding confirm \
  --provider claude \
  --account LOCAL_ACCOUNT_ID \
  --provider-account LOCAL_SOURCE_ID \
  --operator-label OPERATOR_LABEL \
  --confirm \
  --approve-experimental-collector \
  --format json
```

This records explicit, audited collection approval on that binding revision.
Approval defaults to false. The collector flag does not authorize dispatch or
change any policy, goal, budget, or existing spend history.

### 3. Start an observation-only monitor

Use the returned binding ID and revision, and choose a durable idempotency key
that is unique to this intended observer:

```bash
"$JACKIN" usage --data-dir "$DATA" monitor observe \
  --provider claude \
  --binding BINDING_ID \
  --binding-revision REVISION \
  --idempotency-key OBSERVER_KEY \
  --experimental-collector \
  --format json
```

The collector flag can use only a binding that already has the explicit
approval above. It cannot grant approval by itself. This command creates an
observation-only monitor: `goal_id` is null, dispatch is `not_authorized`, and
`runnable` is false. Missing bootstrap or a passive broker produces
`collector_auth_required`; the observer does not start authentication or
restart the foreground owner. If the owner exits, repeat the attended bootstrap
before retrying collection.

## Scope and limits

Bare `usage`, `service start`/`status`, `doctor`, ordinary `monitor observe`
without `--experimental-collector`, and `usage refresh` are passive. They do not
read credentials or force a provider refresh; `usage refresh` only reconciles
local evidence. Statusline callbacks remain an optional, separate observation
source and are not needed for the direct collector.

Do not run `policy approve` or `monitor start` as part of this observer setup.
Dispatch requires a separate policy decision and explicit approval. Strict SGD
requires its own verified baseline; quota-only explicitly has no SGD cap. This
handoff authorizes neither, and it does not authorize changing an existing goal
or approving unknown spend. Do not use `claude -p /usage` as a collector or
authentication fallback.

## Passive checks and bounded follow-up

These commands inspect local state; they do not bootstrap credentials or force
a provider request:

```bash
"$JACKIN" usage --data-dir "$DATA" doctor --provider claude --unattended --format json
"$JACKIN" usage --data-dir "$DATA" service status --format json
"$JACKIN" usage --data-dir "$DATA" status --monitor MONITOR_ID --format json
"$JACKIN" usage --data-dir "$DATA" watch --monitor MONITOR_ID --timeout-secs 300 --format jsonl
```

`doctor` is a one-time local readiness report, not quota evidence. `status` is
the current decision source. `watch` is bounded to 1–300 seconds and emits new
JSONL events. During healthy active work, repeat bounded watch intervals; read
status before each new pack and between bounded slices, and after a decision
event before proceeding. Inspect nested `status.readiness`, `status.runnable`,
the five-hour and seven-day evidence, `status.latest_decision.actions`, and
`status.issues[].code`. Handle each decision sequence once. A persistent
external setup/readiness blocker calls for checkpointing and reporting its
stable issue code, not repeated doctor/status/auth polling. The command
`usage refresh --monitor MONITOR_ID` only reconciles local evidence; it is not
a provider refresh. Periodic bounded watch during healthy work is expected;
there is no guarantee of zero work loss between observations.

Dispatch is outside the observation-only path. Do not approve a policy here. A
`monitor start` is appropriate only if an operator has separately confirmed
the existing binding, goal, and policy revision and explicitly authorized
dispatch under them. Never infer approval from collector opt-in, a goal's
existence, or a cached `runnable` result. The source-level command shape is:

```bash
"$JACKIN" usage --data-dir "$DATA" monitor start \
  --provider claude \
  --binding APPROVED_BINDING_ID \
  --binding-revision APPROVED_BINDING_REVISION \
  --goal EXISTING_GOAL_ID \
  --policy-revision ALREADY_APPROVED_POLICY_REVISION \
  --idempotency-key START_KEY \
  --format json
```

This template does not assert that any such approval exists. Strict SGD still
requires a valid verified baseline. Quota-only is a separate explicit decision
with no SGD cap; neither policy is approved by this handoff. If approval or
baseline state is unknown, stop before `monitor start` and preserve existing
goal and spend history.

For any separately authorized dispatch monitor, assess five-hour and seven-day
quota windows independently. Read `used_percentage_basis_points` (9000 means
90%), each field's freshness, and `status.runnable`. Handle each
`latest_decision.sequence` once and persist the last handled sequence:

- At 90%, stop large packs and checkpoint every active lane.
- At 91%, reduce concurrency and continue only with small checkpointed slices.
- At 95%, finish only the bounded current slice, checkpoint, and stop new work.

Each lane must checkpoint with a commit or durable WIP, exact resume steps, and
the checkpoint path recorded in the existing goal’s durable coordination queue. No statusline or monitor can
guarantee that uncheckpointed work will be recovered.

These actions are operator/coordinator instructions; Jackin does not stop work
for you. A future reset may produce `Wait` while `status.runnable` is still
true; that is a reevaluation hint, not a pause. If `runnable` is true, obey the
other actions and stay within their limits. If a pause is present or
`runnable` is false, checkpoint and stop new work. Only then use the bounded
wait below, followed by a fresh status read:

```bash
"$JACKIN" usage --data-dir "$DATA" wait --monitor MONITOR_ID --until runnable --timeout-secs 300 --format json
"$JACKIN" usage --data-dir "$DATA" status --monitor MONITOR_ID --format json
```

Do not use `wait` for an observation-only monitor. Time passing or a reset hint
does not replenish allowance; resume only if the fresh status says `runnable`.
A five-hour recovery needs fresh paired lower-use and advanced-reset evidence
observed after the reset plus 60 seconds; weekly, model, and spend guards remain
in force. A five-hour reset does not clear a seven-day guard. Unknown permission
for Claude extra usage never authorizes overage. Do not intentionally exceed
verified quota unless a separate operator explicitly approves a bounded extra
usage allowance.
