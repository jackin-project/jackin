# Claude Code handoff

Verified against implementation commit `6c1709e4bea1db9ee05d56352f11440db165e4e7`
and installed Jackin 0.6.4. Commands, JSON/JSONL and exit behavior were exercised
with isolated local state; no live provider or Keychain verification was run.
See [verification.md](verification.md) for checks, artifact hashes and limitations.

## Binary and command preflight

Use `/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin` for every
monitor command. Pass this same data directory to each command:
`--data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state`
Do not invoke the original repository's `target/debug/jackin` or switch
binaries during a goal. Version `0.6.4` alone does not prove this CLI
contract. Run this capability check:

```bash
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage --help
```

Continue only when it lists `monitor`, `status`, `watch` and `wait`, with no
`host` or `projection` command. Otherwise stop and report the help output and
binary path to the operator.

The broker executable can be overridden by `JACKIN_USAGE_BROKER_BIN`, which
takes priority over the CLI's installed sibling. Read only that named setting:

```bash
if [ "${JACKIN_USAGE_BROKER_BIN+x}" = x ]; then
  printf 'set: %s\n' "$JACKIN_USAGE_BROKER_BIN"
else
  printf 'unset\n'
fi
```

`unset` means the installed CLI will use its sibling
`/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin-usage-broker`.
`set:` identifies a present setting; it must name that exact path. If it is empty or
names another path, stop before service or monitor commands and report the
setting and expected path to the operator. Do not unset or change Claude's
environment. The version and CLI help checks do not validate this broker
pairing.

`usage host snapshot` and `usage host projection` are removed. If either old
command is requested or rejected, stop and report the CLI error and binary path
to the operator. Never retry it with a repository build, snapshot/bootstrap,
forced provider refresh or authentication command.

## Local setup order

First inspect service status with the installed binary and isolated data
directory:

```bash
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage service status --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
```

If it reports `running: true`, reattach only if the operator can identify it as
the previously verified local-only service for this same data directory. Its
status reply does not identify its executor. If its mode is unknown, halt this
setup and report the unknown mode; do not stop the running service.
`broker_unavailable` alone does not prove no service exists;
an incompatible or unreachable service may be present. Confirm that this data
directory has no existing service before a fresh start. That absence is
confirmed for the current setup, and the user has already authorized this safe
local-service start; do not ask for authorization again. A fresh start launches
the sibling broker with `--local-only`; it does not discover accounts, resolve
credentials, or contact providers. Start the service before proposing or
installing the statusline adapter:

```bash
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage service start --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
```

Do not use `monitor start` to start the service implicitly. Do not run auth
preparation, Keychain access, a forced provider refresh, or change the
environment. If service startup fails or `doctor` reports `broker_unavailable`,
stop and report the issue; do not fall back to a projection or another refresh
or auth path.

After the fresh service is running, generate a settings proposal with the same
binary, account, and data directory. Replace the quoted `SETTINGS_FILE`
placeholder with the operator-supplied absolute path to the existing Claude
settings file:

```bash
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage statusline compose --account ACCOUNT --settings 'SETTINGS_FILE' --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
```

Review that only `statusLine.command` changes. Composition prints JSON; it does
not write settings or start Claude. Do not apply it, restart Claude, or change
project/session settings in this task. Wait for operator review, adapter
installation, and confirmation that `ACCOUNT` is the bound account before
continuing. The statusline account, spend receipt and monitor must use the same
operator-bound `ACCOUNT`; never infer it from a session ID.

Record a genuine fresh current-period SGD account receipt before the first
monitor is created:

```bash
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage spend record --account ACCOUNT --file '/absolute/path/to/operator-supplied-current-spend.json' --verified --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
```

The receipt must match the account and active billing period, use SGD with
exponent 2, and be no more than 300 seconds old. `--verified` records operator
attestation; Jackin does not independently verify provider billing. A monitor
captures its baseline only when first created. Because this goal already has
work before monitoring, that prior spend is untracked and unknown; do not claim
the SGD50 guard caps whole-goal historical spend or change the goal ID to erase
that attribution. If the receipt is missing, stale, unverified, or incompatible,
do not create the monitor or assume zero spend.

## Monitor commands

After setup and receipt recording, use only these commands with this binary and
data directory:

```bash
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage doctor --provider claude --unattended --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage monitor start --provider claude --account ACCOUNT --goal GOAL --budget-sgd 50 --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage status --monitor MONITOR_ID --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage watch --monitor MONITOR_ID --timeout-secs 300 --format jsonl --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage wait --monitor MONITOR_ID --until runnable --timeout-secs 300 --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
```

Save the returned `status.monitor_id` as `MONITOR_ID`; a blocked start still
returns it with exit 2. Reuse the existing goal and monitor. A new monitor or
goal cannot bypass an account quota pause. A goal started without a valid
baseline remains budget-unverifiable; a later receipt does not create a
retroactive zero-spend baseline.

Read `status.five_hour` and `status.seven_day` independently. Their
`used_percentage_basis_points` are integers: 9000 means 90%. Missing values,
`unavailable` or `stale` field freshness, or a non-runnable status mean unknown
or blocked. Require `used_evidence.freshness` and `reset_evidence.freshness`
to be `current` independently for each window. Unchanged fields can become
stale even while a sibling field changes; do not reinterpret the callback as
fresh evidence for every field. Status replies contain `result`
and `status`; Watch JSONL contains events with a nested `status`. A fresh Watch attachment supplies the current
reconciled event, then later events.

Follow each `status.latest_decision.sequence` once per monitor. Persist the last
handled sequence with the goal checkpoint. Process all actions in order:

- `checkpoint`: stop starting large packs and ask every active lane to save a
  commit or durable WIP checkpoint, including exact resume steps.
- `reduce_dispatch`: honor `max_parallel`; use only small, bounded,
  checkpointed slices. `max_parallel: 0` means start no new work.
- `pause`: finish only the current slice, checkpoint, stop new dispatch, and
  wait for new evidence before resuming.
- `wait`: if `status.runnable` is true, treat this only as a reevaluation hint;
  continue work within any checkpoint or dispatch limits. Do not pause solely
  because `wait` is present. When `status.runnable` is false, checkpoint and
  use a bounded `usage wait --until runnable`; re-read status before resuming.
- `warn`: report the issue and keep work within the remaining verified guards.

At 90% used, checkpoint and stop large packs; at 91%, reduce concurrency; at
95%, pause after the current slice. A jump can include all these actions.
Quota pauses remain latched across idle, sleep, restart and new monitors.

Never run snapshot/bootstrap, interactive auth, or a forced provider refresh
when unattended. Monitor `refresh` only reconciles local evidence; it does not
fetch a provider. On degraded readiness, missing evidence, an unavailable
broker, or an unverifiable budget, checkpoint and report the stable issue code.
A live hung broker requires operator lifecycle intervention; do not attempt
unattended process killing or lease replacement.
`status.runnable` is authoritative regardless of lifecycle or a `wait` action.
At healthy usage, a future reset can produce `wait` while the status remains
runnable; this schedules reevaluation and does not block dispatch. `pause` or
`status.runnable: false` blocks new work. Use `usage wait --until runnable`
with a bounded timeout only while blocked, then read status again before work.
Exit 0 means the command's permitted success condition. Exit 2 means
blocked/degraded; exit 3 means unavailable/invalid. Doctor can return 0 with informational
`auth_status_unknown`; that is not proof of current quota or credential access.

Treat reset times as reevaluation hints only. `reset_due_unverified` is not
permission to resume. When blocked, keep using bounded waits for fresh paired
evidence confirming the relevant reset; time passing alone does not release the
pause. Retain weekly, model and spend guards after a five-hour reset. A bounded
wait can return `wait_timeout` with exit 2; checkpoint remains in effect and a
later bounded wait may be issued. Do not bypass Retry-After. Automatic
resumption and fresh data during inactivity are not guaranteed.

Use `status.cumulative_goal_spend` for spend attributed since monitor creation,
and `status.spend_period_baseline` for its account/period binding. Current spend
evidence is the `status.evidence` entry with `value.kind: "spend"`; its amount,
verification and age must be usable: account IDs and billing periods must match
the baseline, `value.verification` must be `verified`, and `age_seconds` must be
at most 300. `amount_minor: 5000` with `currency: "SGD"` and `exponent: 2`
means SGD50. Do not recompute spend by subtracting session cost. Work done
before the first monitor has no Jackin attribution and remains unknown; a fresh
receipt cannot make that historical work part of the monitor's cumulative
spend.

Treat spend as unknown unless the response has an explicit SGD amount with
exponent 2, verified fresh evidence and a valid same-account/same-period goal
baseline. `verified` means an operator attested the receipt; Jackin has not
independently verified provider billing. Session list-price cost, USD, unknown
credits and unverified receipts are not billed SGD. Keep the configured SGD50 guard: SGD40 warns, SGD45
checkpoints and stops new work, and SGD48 pauses. Billing rollover does not
erase cumulative goal spend. `budget_unverifiable` requires checkpointing and
reporting, not a zero-spend assumption. Respect the operator-configured
account-side Anthropic monthly spending limit; Jackin is not a hard per-goal
SGD billing cap.
