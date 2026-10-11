# Claude Code quota observer handoff — DRAFT, NOT READY FOR LIVE USE

> **Do not run or paste this draft into a live task yet.** The installed v11
> CLI/broker pair passed its isolated fixture, but it has not been switched onto
> the existing v7 state. The specific owner-stop and attended approval are
> pending. No v11 status/watch result or live acceptance has been recorded.

## Verified artifacts and pending gate

The installed v11 pair was built from source commit
`62cefbbbb11ce8b6dc0634f490095d9838ebef9e`. Its isolated fixture verified
broker wire v9 and monitor schema 6. It did not access the existing account
state, authenticate, or make a provider request. See
[v11-installation.json](v11-installation.json) for provenance and hashes.

The intended CLI and existing data directory are:

```bash
JACKIN='/Users/donbeave/.local/share/jackin-claude-monitor-v11/bin/jackin'
DATA='/Users/donbeave/.local/share/jackin-claude-monitor-v7/state'
MONITOR_ID='monitor-00000001'
```

The last recorded v10 state was schema 5. On first load, v11 migrates a v5
monitor store to schema 6 and saves it. Therefore even the status command below
may write the versioned state migration. Run neither command until the approved
owner handoff and attended approval are complete. Do not stop or replace a
running owner without approval for that specific process and state directory.
Verify that `JACKIN_USAGE_BROKER_BIN` is absent or points to the verified v11
sibling broker at
`/Users/donbeave/.local/share/jackin-claude-monitor-v11/bin/jackin-usage-broker`.
If it conflicts, checkpoint and report the mismatch; do not modify the
environment.

After that gate, these source-verified commands read the existing monitor
without starting authentication or requesting provider data:

```bash
"$JACKIN" usage --data-dir "$DATA" status \
  --monitor "$MONITOR_ID" --format json

"$JACKIN" usage --data-dir "$DATA" watch \
  --monitor "$MONITOR_ID" --timeout-secs 300 --format jsonl
```

Watch accepts 1–300 seconds. Use bounded intervals only. Status must identify
`monitor-00000001` and schema 6; preserve its JSON, stable issue codes, and
`latest_decision`. A status with `runnable=false` can exit 2 while still
returning useful JSON.

## How to use the observation

The recorded monitor is `observe_only`. Its expected `goal_id` is null,
`readiness.dispatch` is `not_authorized`, and `runnable` is false by design.
Those fields describe this monitor's dispatch authority; they do not grant or
revoke separately established task authorization. Do not block authorized work
solely on this observer's `runnable` value or its status exit code. The observer
does not authorize dispatch, policy, a budget, or extra usage.

Check the five-hour (`five_hour`, Session) and seven-day (`seven_day`, weekly)
windows independently. For each, inspect `used_percentage_basis_points`,
`used_evidence.freshness`, `reset_at_epoch`, `reset_validity`, and
`reset_evidence.freshness`. A used percentage and reset have separate ages; one
field's update does not refresh the other. Treat missing or stale fields as
unknown. Provider readiness alone does not make quota evidence current.

Apply the same thresholds to either window, including the weekly window:

- At 90% (`9000` basis points), checkpoint early: stop starting large packs
  and prepare the whole-work handoff, especially for the weekly window. Ask
  every active lane for a commit or durable WIP and exact resume commands.
- At 91% (`9100`), reduce concurrency.
- At 95% (`9500`), finish only the bounded current slice, complete the
  whole-work handoff, and stop starting work before usage can reach 100%.

Each lane's durable handoff must list its branch/head, changed files, failing
checks and exact outputs, exact resume commands, dependencies, and recommended
next order. Include the monitor ID, last handled decision sequence, evidence
ages/freshness, issue codes, both reset epochs/validity, and the reason work
stopped. Record the handoff and checkpoint locations so another lane can resume
without reconstructing context.

Process `latest_decision.actions` in order, once for each
`latest_decision.sequence`, and persist the last handled sequence with the
work's checkpoint. An observation-only monitor may have an empty action list;
still apply the thresholds above from fresh quota fields. Do not assume a
five-hour reset clears the independent weekly guard.

After a pause, a reset time by itself is not recovery. Use one bounded timer for
the relevant reset plus 60 seconds, then make a fresh status read or bounded
watch observation. Recovery requires fresh paired used-and-reset evidence
received after that grace period, an advanced reset, lower usage than the prior
high, and no remaining guard in either window or the separately authorized
work constraints. Do not use `wait --until runnable`: this observer never
becomes dispatch-runnable. If the paired evidence or another required condition
is still missing, checkpoint and report the stable issue code; do not poll
doctor/status in a loop. If the stop condition is reached, include it in the
durable handoff and report it with the remaining external dependency.

Unknown Claude extra usage, provider-normalized amounts, or missing SGD evidence
do not authorize paid work or overage. Preserve any existing spend controls;
this handoff grants no new task budget or spending permission. For a broker,
provider, or evidence failure, checkpoint active lanes and report the exact
issue once instead of forcing refresh, retrying authentication, or replacing a
service owner.
