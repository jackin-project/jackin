# Claude Code handoff

Verified against implementation commit `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc`
and installed Jackin 0.6.4. Commands, JSON/JSONL and exit behavior were exercised
with isolated local state; no live provider or Keychain verification was run.
See [verification.md](verification.md) for checks, artifact hashes and limitations.

Operator precondition: the local-only service must already be running, with
the reviewed statusline adapter and account binding installed. During setup,
start it with `/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage service start --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state`.
Doctor is passive and reports `broker_unavailable` if setup is incomplete.

Jackin's usage monitor is the supported interface. Use only these commands:

```bash
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage doctor --provider claude --unattended --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage monitor start --provider claude --account ACCOUNT --goal GOAL --budget-sgd 50 --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage status --monitor MONITOR_ID --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage watch --monitor MONITOR_ID --format jsonl --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin usage wait --monitor MONITOR_ID --until runnable --timeout-secs 300 --format json --data-dir /Users/donbeave/.local/share/jackin-claude-monitor/state
```

`ACCOUNT` must be the operator-bound account receiving statusline observations;
`GOAL` must identify this goal. Save the returned `status.monitor_id` as
`MONITOR_ID`. A blocked start still returns that ID with exit 2. Reuse the goal
identity and monitor; creating another monitor or goal cannot bypass an account
quota pause. Operator setup must install the reviewed statusline composition
and record a fresh, explicitly verified SGD receipt before starting the goal.
Do not infer account identity from a session ID.

Read `status.five_hour` and `status.seven_day` independently. Their
`used_percentage_basis_points` are integers: 9000 means 90%. Missing values,
`unavailable` or `stale` field freshness, or a non-runnable status mean unknown
or blocked. Require `used_evidence.freshness` and `reset_evidence.freshness`
to be `current` independently for each window. Status replies contain `result`
and `status`; Watch JSONL contains events with a nested `status`. A fresh Watch attachment supplies the current
reconciled event, then later events.

Follow each `status.latest_decision.sequence` once per monitor. Persist the last
handled sequence with the goal checkpoint. Process all actions in order:

- `checkpoint`: stop starting large packs and ask every active lane to save a
  commit or durable WIP checkpoint, including exact resume steps.
- `reduce_dispatch`: honor `max_parallel`; use only small, bounded,
  checkpointed slices. `max_parallel: 0` means start no new work.
- `pause` or `wait`: finish the current slice, checkpoint, record the resume
  point, and wait for new evidence before dispatching again.
- `warn`: report the issue and keep work within the remaining verified guards.

At 90% used, checkpoint and stop large packs; at 91%, reduce concurrency; at
95%, pause after the current slice. A jump can include all these actions.
Quota pauses remain latched across idle, sleep, restart and new monitors.

Never run snapshot/bootstrap, interactive auth, or a forced provider refresh
when unattended. Monitor `refresh` only reconciles local evidence; it does not
fetch a provider. On degraded readiness, missing evidence, an unavailable
broker, or an unverifiable budget, checkpoint and report the stable issue code.
Exit 0 means the command's permitted success condition; monitor status and wait
require `status.runnable: true` before work. Exit 2 means blocked/degraded;
exit 3 means unavailable/invalid. Doctor can return 0 with informational
`auth_status_unknown`; that is not proof of current quota or credential access.

Treat reset times as wake-up hints. `reset_due_unverified` is not permission to
resume. Keep waiting for fresh paired evidence confirming the relevant reset,
and retain weekly, model and spend guards after a five-hour reset. A bounded
wait can return `wait_timeout` with exit 2; checkpoint remains in effect and a
later bounded wait may be issued. Do not bypass Retry-After. Automatic
resumption and fresh data during inactivity are not guaranteed.

Use `status.cumulative_goal_spend` for attributed goal spend and
`status.spend_period_baseline` for its account/period binding. Current spend
evidence is the `status.evidence` entry with `value.kind: "spend"`; its amount,
verification and age must be usable: account IDs and billing periods must match
the baseline, `value.verification` must be `verified`, and `age_seconds` must be
at most 300. `amount_minor: 5000` with `currency: "SGD"` and `exponent: 2`
means SGD50. Do not recompute spend by subtracting session cost.

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
