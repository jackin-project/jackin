# Claude Code goal

**Install status: pending verification.** Prospective CLI: `/Users/donbeave/.local/share/jackin-claude-monitor-v2/bin/jackin`; expected broker is its sibling. Keep the v1 pair/state untouched; use v2 only after parent verifies both binaries. No real account callback is verified.

Use only this pair and isolated state; do not fall back to v1 or a repo build:

```bash
JACKIN=/Users/donbeave/.local/share/jackin-claude-monitor-v2/bin/jackin
DATA=/Users/donbeave/.local/share/jackin-claude-monitor-v2/state
```

Follow [`claude-code-handoff.md`](/Users/donbeave/.local/share/jackin-claude-monitor-v2/docs/claude-code-handoff.md) for one-time local service setup, attended account binding or session-only compose review (never apply the proposal), and any explicitly operator-approved policy. Unset broker override is normal; an explicitly empty or different override is a stop. Do not use Keychain/auth or force provider refresh.

Run passive doctor once; `auth_state: unknown` is informational, not readiness:

```bash
"$JACKIN" usage doctor --provider claude --unattended --format json --data-dir "$DATA"
```

Observe without a receipt or goal. Use a confirmed account binding, or—if account is not ready—an actual session ID supplied by a genuine callback/operator; never invent IDs:

```bash
"$JACKIN" usage monitor observe --provider claude --binding BINDING_ID --binding-revision BINDING_REVISION --idempotency-key OBSERVER_KEY --format json --data-dir "$DATA"
"$JACKIN" usage monitor observe --provider claude --session SESSION_ID --idempotency-key SESSION_OBSERVER_KEY --format json --data-dir "$DATA"
```

Save `binding.binding_id`/`binding.revision` when bound, `policy.revision` after approval, and returned `status.monitor_id` as `MONITOR_ID`. Observer/start and status replies use `result` plus nested `status`; watch is JSONL with nested status. Observer `purpose=observe_only`, `goal_id=null`, `runnable=false`; it cannot dispatch. Unbound evidence has no account ID, is not account-aggregated, and cannot later merge into a binding.

For status, bounded watch, and blocked dispatch-guard wait, use the exact CLI names below. Re-read status after waiting; only `status.runnable=true` permits dispatch:

```bash
"$JACKIN" usage status --monitor MONITOR_ID --format json --data-dir "$DATA"
"$JACKIN" usage watch --monitor MONITOR_ID --timeout-secs 300 --format jsonl --data-dir "$DATA"
"$JACKIN" usage wait --monitor MONITOR_ID --until runnable --timeout-secs 300 --format json --data-dir "$DATA"
"$JACKIN" usage status --monitor MONITOR_ID --format json --data-dir "$DATA"
```

There is no dispatch policy approved here. Do not start until an operator separately approves policy in a TTY; strict first activation needs a fresh verified same-account active-period SGD baseline (exponent 2, age ≤300s). No receipt is supplied. Quota-only is not approved and requires `--acknowledge-no-sgd-cap`. An approved quota-only guard reports `status.readiness.budget="disabled"`, spend unknown and SGD enforcement off; `runnable=true` still requires fresh quota and all applicable guards and provides no billing protection. Unknown model-specific and extra usage never authorizes overage. Read `status.readiness.{tracking,quota,budget,dispatch}` independently; dispatch only when authoritative `status.runnable=true`. Any prior strict policy cannot downgrade to quota-only, even before activation; activated quota-only cannot become strict. Never rename a goal or reset/replace its baseline or unknown spend. Any new attribution boundary requires explicit operator direction, never a policy/quota workaround.

```bash
"$JACKIN" usage monitor start --provider claude --binding BINDING_ID --binding-revision BINDING_REVISION --goal EXISTING_GOAL --policy-revision POLICY_REVISION --idempotency-key START_KEY --format json --data-dir "$DATA"
```

Same key/config returns the same record; check `status.lifecycle`: a stopped monitor stays stopped and its key does not reactivate it. Use a new key only for an operator-intended run, never to bypass quota or policy. Parse errors exit 2 on stderr without JSON; valid runtime commands use 0/2/3. On external blockers, checkpoint durable WIP with exact resume steps, report the stable code, exit the goal, and do not loop doctor.

Track five-hour and seven-day used/reset fields and ages independently. Process `status.latest_decision.actions` exactly once per `sequence`, in emitted array order. A `wait` action with `runnable=true` is only a reevaluation hint. At 90% checkpoint, 91% reduce to `max_parallel` (0 means no new work), 95% pause; reset time alone never releases a pause. Require reset grace plus fresh paired used/reset evidence, advanced reset, lower usage, and `runnable=true`; a five-hour reset does not clear weekly guards. For SGD50, SGD40 warns, SGD45 checkpoints with `max_parallel=0`, SGD48 pauses; unknown/stale spend pauses, and the local guard does not cap provider billing. Report-only `reset_validity` (`unknown`/`future`/`due`) with independent ages and `expected_model`/`model_guard_validity` are pending binary verification and do not change guards.
