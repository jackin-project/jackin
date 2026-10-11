# Claude Code quota observer handoff — DRAFT, NOT READY FOR LIVE USE

> **Do not run or paste this draft into a live task yet.** The installed v11
> CLI/broker pair is the current live owner against the existing v7-named state;
> its attended startup and v10 replacement were authorized and completed. Two
> natural v11 collection generations were recorded, but live acceptance remains
> false: in those captures Weekly was current at 92% used, while Session
> projection reported 100% remaining with no direct used-percentage or reset
> field; the monitor retained stale 54% usage with its old reset. The v12
> source fix can derive a zero-used observation from the remaining value, but
> that path has not been verified live. Its installed pair passed only an
> isolated smoke. Explicit approval for a v11-to-v12 cutover is still pending.

## Verified artifacts and pending gate

The installed v11 pair was built from source commit
`62cefbbbb11ce8b6dc0634f490095d9838ebef9e`. Its isolated fixture verified
broker wire v9 and monitor schema 6. It did not access the existing account
state, authenticate, or make a provider request. See
[v11-installation.json](v11-installation.json) for provenance and hashes.

The attended v11 service owns the existing state. Its live report records
natural coordinator generations 126 and 127 and a bounded status/watch capture;
Weekly monitor evidence was current at 92% in those captures, while Session
remained stale at 54% with an elapsed reset. The provider Session projection
reported 100% remaining but published neither used nor reset, so the capture
does not show the later v12 used-only path or a `MissingReset` result.
Acceptance is false.
See the [sanitized v11 live report](v11-live-verification.md) and
[JSON record](v11-live-verification.json).

The installed v12 pair comes from source checkpoint
`db59c4015fb4715e1d39157aaa31f4a1497c823a`. Its isolated schema-6 / wire-v9
smoke passed, but v12 has not opened the existing state or been verified against
the live provider. Its manifest is [v12-installation.json](v12-installation.json).

The current live CLI and candidate v12 binaries are:

```bash
LIVE_JACKIN='/Users/donbeave/.local/share/jackin-claude-monitor-v11/bin/jackin'
DATA='/Users/donbeave/.local/share/jackin-claude-monitor-v7/state'
MONITOR_ID='monitor-00000001'
V12_CANDIDATE_JACKIN='/Users/donbeave/.local/share/jackin-claude-monitor-v12/bin/jackin'
V12_CANDIDATE_BROKER='/Users/donbeave/.local/share/jackin-claude-monitor-v12/bin/jackin-usage-broker'
```

The attended v11 startup already migrated the v10 schema-5 store to schema 6
and preserved `binding-00000001` revision 1 and observe-only
`monitor-00000001`. The live service is reported in terminal session `35711`
with processes `3127` and `3128`. Do not treat this draft as authorization to
stop that owner or switch to v12; the specific v11-to-v12 cutover approval is
pending. Do not stop or replace a running owner without approval for that
specific process and state directory. The v12 paths are inventory for the
installed candidate only; v12 has not opened this data directory and must not
be invoked against it until the specific cutover is approved.
Verify that `JACKIN_USAGE_BROKER_BIN` is absent or points to the verified v11
sibling broker at
`/Users/donbeave/.local/share/jackin-claude-monitor-v11/bin/jackin-usage-broker`.
If it conflicts, checkpoint and report the mismatch; do not modify the
environment.

These source-verified commands make bounded passive status/watch observations
of the existing v11 monitor without starting authentication or requesting
provider data:

```bash
"$LIVE_JACKIN" usage --data-dir "$DATA" status \
  --monitor "$MONITOR_ID" --format json

"$LIVE_JACKIN" usage --data-dir "$DATA" watch \
  --monitor "$MONITOR_ID" --timeout-secs 300 --format jsonl
```

Watch accepts 1–300 seconds. Use bounded intervals only. Status must identify
`monitor-00000001` and schema 6; preserve its JSON, stable issue codes, and
`latest_decision`. A status with `runnable=false` can exit 2 while still
returning useful JSON.

## How to use the observation

The recorded monitor is `observe_only`. Its expected `goal_id` is null,
`readiness.dispatch` is `not_authorized`, and `runnable` is false by design.
This is intrinsic to its purpose, independent of quota freshness or the stale
Session evidence. Those fields describe this monitor's dispatch authority; they
do not grant or revoke separately established task authorization. Do not block
authorized work solely on this observer's `runnable` value or its status exit
code. The observer does not authorize dispatch, policy, a budget, or extra
usage.

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
