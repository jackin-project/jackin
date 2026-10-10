# Claude Code operator handoff

**V2 install status: pending verification.** The prospective CLI is
`/Users/donbeave/.local/share/jackin-claude-monitor-v2/bin/jackin`; its expected
broker is the sibling `jackin-usage-broker`. The existing v1 pair at
`/Users/donbeave/.local/share/jackin-claude-monitor/bin/` and its state remain
untouched. Use only the v2 pair after the parent verifies its exact CLI/broker
binaries and updates this status. Source CLI/protocol names below
were checked statically; no real account statusline callback, Keychain access,
or provider request has been verified.

## Paste into Claude Code after v2 binary verification

Use one binary and one isolated data directory throughout:

```bash
JACKIN=/Users/donbeave/.local/share/jackin-claude-monitor-v2/bin/jackin
DATA=/Users/donbeave/.local/share/jackin-claude-monitor-v2/state
BROKER=/Users/donbeave/.local/share/jackin-claude-monitor-v2/bin/jackin-usage-broker
```

First confirm that `"$JACKIN" usage --help` exposes `doctor`, `service`,
`binding confirm`, `policy approve`, `monitor observe|start`, `status`,
`watch`, `wait`, and `statusline compose`. If help or binary pairing differs,
stop and report the binary paths and output. An unset `JACKIN_USAGE_BROKER_BIN`
is normal and selects the sibling broker. If explicitly set, it must be a
non-empty exact `"$BROKER"` path; if empty or different, stop and report it.
Do not change the environment or fall back to the v1 pair or a repository build.
The removed `usage host snapshot` and `usage host projection` commands are not
fallbacks.

### One-time local setup

Inspect the service in this v2 data directory:

```bash
"$JACKIN" usage service status --format json --data-dir "$DATA"
```

If the directory is confirmed new and has never had a service, start it
explicitly:

```bash
"$JACKIN" usage service start --format json --data-dir "$DATA"
```

This starts the local-only broker; it does not discover accounts, resolve
credentials, or contact Claude. If a service is already running but its exact
v2 CLI/broker identity is unknown, or status only reports `broker_unavailable`
and the directory's history is unclear, stop and report. Do not stop, kill, or
replace an unknown service.

Run passive readiness once:

```bash
"$JACKIN" usage doctor --provider claude --unattended --format json --data-dir "$DATA"
```

Doctor does not read Keychain or contact Claude. `report.auth_state: "unknown"`
and `auth_status_unknown` are informational, not evidence of authentication or
quota readiness. Do not loop doctor to wait for callbacks; use monitor status
and watch for evidence.

If the operator has supplied the exact local account ID and label, use the
account-bound path. Confirm the binding in an attended terminal:

```bash
"$JACKIN" usage binding confirm --provider claude --account ACCOUNT_ID --operator-label OPERATOR_LABEL --confirm --format json --data-dir "$DATA"
```

Binding confirmation requires stdin, stdout, and stderr to be terminals. The
binding is an operator-supplied account label, not provider authentication;
never infer the account from a session ID. Save `binding.binding_id` and
`binding.revision`. Repeating the same provider, account ID, and label returns
the same binding ID and revision. A changed label creates a new revision; do
not change it during a retry.

Print a bound statusline settings proposal for review:

```bash
"$JACKIN" usage statusline compose --binding BINDING_ID --binding-revision REVISION --settings '/absolute/path/to/existing/settings.json' --format json --data-dir "$DATA"
```

Review the JSON proposal. Composition only prints JSON and changes
`statusLine.command` in that proposed value. Do not write it to settings, apply
it, restart Claude Code, or alter project/session settings in this handoff.
No real account callback has been verified; do not claim account quota evidence
or readiness from the proposal.

If no account binding is ready, do not invent an account ID or confirm a
placeholder. Use the unbound session-only path once the real session ID is
available from a genuine statusline callback or the operator identifies the
actual Claude session:

```bash
"$JACKIN" usage statusline compose --session-only --settings '/absolute/path/to/existing/settings.json' --format json --data-dir "$DATA"
```

Review this proposal without applying it. Never generate an arbitrary session
ID. Session-only evidence remains unbound, is not merged into an account
aggregate, and cannot authorize dispatch.

### Observe without creating a goal

Create one observer for the selected scope, using a durable key that you save
in the checkpoint:

```bash
# Bound account, after operator-confirmed binding:
"$JACKIN" usage monitor observe --provider claude --binding BINDING_ID --binding-revision REVISION --idempotency-key claude-account-observer-v2 --format json --data-dir "$DATA"

# Unbound session, only with the actual SESSION_ID described above:
"$JACKIN" usage monitor observe --provider claude --session SESSION_ID --idempotency-key SESSION_OBSERVER_KEY --format json --data-dir "$DATA"
```

Neither observer needs a spend receipt or creates a goal or dispatch policy.
Each only records evidence; `status.purpose` is `observe_only`,
`status.goal_id` is null, `status.readiness.dispatch` is `not_authorized`, and
`status.runnable` is always false. A successful observer creation can exit 0
while quota evidence is unknown. Save `status.monitor_id` as `MONITOR_ID`.
For an unbound observer, `status.account_id` is null and its evidence cannot be
account-aggregated or later merged into a binding. Do not run
`wait --until runnable` on an observer; it is never dispatch-runnable.

Retries with the same idempotency key and identical configuration return the
same monitor, including after it is stopped. Reusing the key with changed
configuration is an `idempotency_conflict`. A new key creates an intentional
new monitor; it does not clear account quota pauses or reset goal spend. Check
`status.lifecycle`: retrying a stopped monitor with its old key returns that
stopped record and does not reactivate it. Use a new key only for an
operator-intended new run, never to bypass a pause or policy guard.

### Monitor command schema

These are the current source CLI argument names. `--format` and `--data-dir`
are global `usage` options:

```bash
"$JACKIN" usage status --monitor MONITOR_ID --format json --data-dir "$DATA"
"$JACKIN" usage watch --monitor MONITOR_ID --timeout-secs 300 --format jsonl --data-dir "$DATA"
```

Status replies have `result` and nested `status`. Watch emits JSONL events with
`sequence`, `occurred_at_epoch`, and nested `status`. Status includes
`schema_version`, `readiness.{tracking,quota,budget,dispatch}`, authoritative
`runnable`, `five_hour`, `seven_day`, `evidence`, `latest_decision`, `issues`,
and spend fields. A blocked/degraded reply can still contain useful JSON and
exit 2; exit 3 means unavailable or invalid. Preserve the JSON and stable
`issues[].code`. These exit codes describe valid commands' runtime outcomes;
Clap argument/parse errors happen before runtime, exit 2 on stderr, and do not
produce a JSON reply.

The inspected source uses broker protocol v6 and monitor/statusline schema 2.
The parent is adding report-only reset/model metadata: per-window
`reset_validity` (`unknown`, `future`, or `due`) with independent field ages,
plus `expected_model` and `model_guard_validity`. These fields explain evidence
validity; they do not change policy or `status.runnable`. Their final JSON paths
and presence must be confirmed against the verified binary before relying on
them.

The bounded wait command is for a blocked **dispatch guard** only, and accepts
`--monitor ID --until runnable --timeout-secs 1..300`:

```bash
"$JACKIN" usage wait --monitor MONITOR_ID --until runnable --timeout-secs 300 --format json --data-dir "$DATA"
"$JACKIN" usage status --monitor MONITOR_ID --format json --data-dir "$DATA"
```

After every wait, re-read status and resume only if `status.runnable` is true.
A timeout remains blocked and requires checkpoint/reporting; time passing
alone does not release a pause.

### Decisions, quota evidence, and blockers

Read five-hour and seven-day windows independently. For each, inspect
`used_percentage_basis_points`, `used_evidence.freshness` and
`reset_evidence.freshness` separately; 9000 means 90%. Missing or stale fields
are unknown. A sibling update or repeated identical callback does not renew the
other field's age. Apply each ordered `status.latest_decision.actions` list
once per `latest_decision.sequence`, saving the last handled decision sequence
with the durable goal checkpoint:

- `checkpoint`: stop starting large packs and save durable WIP with exact
  resume steps.
- `reduce_dispatch`: obey `max_parallel`; zero means start no new work.
- `pause`: finish only the bounded current slice, checkpoint, and stop new
  dispatch.
- `wait`: if `status.runnable` is true, this is only a reevaluation hint; obey
  any other actions and continue within their limits. If false, checkpoint,
  use bounded wait, then re-read status.
- `warn`: report it and keep work within the remaining verified guards.

Each quota window checkpoints at 90%, reduces dispatch at 91%, and pauses at
95%; a jump can emit multiple ordered actions. A future reset time is only a
hint. After a 95% pause, wait for the reset grace and a fresh paired used-plus-
reset observation with an advanced reset and lower usage than the prior high.
Only the resulting `status.runnable` can permit work. A five-hour reset does
not clear a seven-day guard; weekly usage and reset evidence must independently
be current and clear. `reset_due_unverified`, missing evidence, or a stale
window stays blocked.

On a broker, callback, quota, or spend blocker, checkpoint the active work,
report its stable issue code and exit the goal cleanly. Do not retry doctor in
a loop, invoke authentication, force provider refresh, use old host commands,
or kill/replace a broker. `usage refresh` is local reconciliation only; it is
not a way to fetch fresh provider data. Current data during inactivity and
automatic reset confirmation are not guaranteed.

### Dispatch policy and spend (not approved in this handoff)

Do not run `policy approve` or `monitor start` during this observation setup.
There is no dispatch policy approved here, and quota-only is not currently
approved. Both binding and policy approval require an attended terminal on all
three standard streams plus explicit `--confirm`; there is no headless bypass.
Only an explicit operator choice can authorize a later policy:

```bash
# Strict approval is an operator choice; first activation also needs a genuine fresh baseline:
"$JACKIN" usage policy approve --binding BINDING_ID --binding-revision REVISION --goal EXISTING_GOAL --policy strict-sgd --budget-sgd 50 --operator-label OPERATOR_LABEL --confirm --format json --data-dir "$DATA"

# Quota-only is a separate operator choice and has no SGD spend cap:
"$JACKIN" usage policy approve --binding BINDING_ID --binding-revision REVISION --goal EXISTING_GOAL --policy quota-only --operator-label OPERATOR_LABEL --confirm --acknowledge-no-sgd-cap --format json --data-dir "$DATA"
```

Strict first activation requires a fresh, verified, same-account, active-period
SGD baseline compatible with the approved budget, with exponent 2 and age at
most 300 seconds. `--verified` on
`usage spend record --account ACCOUNT_ID --file RECEIPT --verified` is operator
attestation, not independent provider verification. No receipt is supplied for
this setup; do not fabricate or record one for observation. A failed strict
activation must not create a goal or reserve its idempotency key. The first successful
activation captures the baseline; earlier work remains unknown and is not
retroactively attributed. Keep cumulative goal spend across billing periods;
unknown, stale, or unverified spend means checkpoint and pause, never assume
zero. Quota-only reports spend enforcement disabled and spend unknown.

Policy changes fail closed: any prior strict-SGD policy cannot be changed to
quota-only, even before activation. An activated quota-only goal cannot be
changed to strict-SGD. Do not suggest a new goal as a policy-switch or quota
workaround. Any new attribution boundary requires explicit operator direction
and its own operator-approved policy; it must preserve the prior goal,
baseline, spend, decisions, and unknown history. Never rename an existing goal
or replace its baseline to erase historical uncertainty.

For a later, separately authorized dispatch start, the exact arguments are:

```bash
"$JACKIN" usage monitor start --provider claude --binding BINDING_ID --binding-revision REVISION --goal EXISTING_GOAL --policy-revision POLICY_REVISION --idempotency-key UNIQUE_START_KEY --format json --data-dir "$DATA"
```

Reuse the same start key only for the identical configuration; use a new
unique key for an intentional new run without changing the existing goal.
With a strict SGD 50 budget, SGD 40 warns, SGD 45 checkpoints and sets
`max_parallel` to zero, SGD 48 pauses, and reaching the cap blocks dispatch on
verified cumulative evidence. This is a local dispatch guard, not a cap on
Anthropic's billed charges. Quota limits remain independent of spend guards.
