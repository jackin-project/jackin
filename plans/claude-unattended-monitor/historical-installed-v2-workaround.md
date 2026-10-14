# Historical installed-v2 workaround: observer-only

**Archive only.** Copied from the predecessor branch at commit `771bb088`.
This records fixture-only behavior for the installed `4492d3cb` pair; it is
not proof of, or the working operator path for, the direct CLI implementation
on `feat/claude-usage-monitor-main`. The current implementation plan is
[bootstrap-contract.md](bootstrap-contract.md). Keep this file as provenance;
do not use its observer-only workflow to claim account collection or dispatch
readiness for the main-port branch. This predecessor note also predates the
planned binding-level `--approve-experimental-collector` flag; its commands and
fixture results provide no proof for that approval flow or the planned direct
collector path.

This is an attended path to restore local usage tracking with the installed v2
CLI. It does **not** establish that the current guard will keep the goal
runnable.

## Verified facts and limits

- Installed CLI/broker fixture proof passed for `4492d3cb91d9e45fafec2aa8acb8158268618cd9`. Later source fixes `427c104b` (reported 133 broker tests, 47 coordinator tests, Clippy) are not installed. No real-account callback/provider check is verified.
- Last read-only inspection found zero v2 bindings, accounts, monitors, and goals; the real settings file had no `statusLine`.
- No policy is approved and no settings change is authorized by this runbook; only the operator may choose and perform a manual `statusLine` merge after review.
- A prior doctor returned `broker_unavailable`, exit 3; its cause is unknown. `monitor observe` starts the local broker as needed. Do not kill/stop an unknown service.
- Statusline callbacks need no `auth prepare`, Keychain access, or HTTP. Do not sign in or force provider refresh.
- Known installed-guard limitation: reset-field freshness can age past 300 seconds when a later callback changes only usage; the unchanged reset field keeps its age and status may block again. Independent verification/fix is pending. Do not fake ingress or use timer workarounds to renew evidence.

Use only this installed pair and isolated state:

```bash
JACKIN=/Users/donbeave/.local/share/jackin-claude-monitor-v2/bin/jackin
DATA=/Users/donbeave/.local/share/jackin-claude-monitor-v2/state
SETTINGS=/Users/donbeave/.claude/settings.json
ACCOUNT_LABEL=claude-personal # replace with operator-chosen stable local label
OPERATOR_LABEL=local-operator # replace with operator's chosen label
```

## Six operator steps

1. Check the installed command surface; stop if expected v2 commands are absent.

   ```bash
   "$JACKIN" usage --help
   ```

2. Confirm a local binding. `--account` is only a stable Jackin partition label,
   not an Anthropic account ID, provider ID, UUID, credential, or identity
   proof. This command requires all three standard streams to be TTYs: run it
   directly, without a pipe/redirection/capture. Copy the returned ID/revision
   manually into `BINDING_ID` and `REVISION`.

   ```bash
   "$JACKIN" usage binding confirm --provider claude --account "$ACCOUNT_LABEL" --operator-label "$OPERATOR_LABEL" --confirm --format json --data-dir "$DATA"
   ```

   ```bash
   BINDING_ID='copy binding.binding_id here'
   REVISION='copy binding.revision here'
   ```

3. Compose a private proposal. It contains the full settings value and may
   contain secrets; inspect only `statusLine`. If the operator chooses to
   install it, manually merge only that member into the real settings file,
   preserving every other key. No agent edits or applies settings.

   ```bash
   umask 077
   PROPOSAL="$(mktemp "${TMPDIR:-/tmp}/jackin-statusline.XXXXXX")"
   "$JACKIN" usage statusline compose --binding "$BINDING_ID" --binding-revision "$REVISION" --settings "$SETTINGS" --format json --data-dir "$DATA" > "$PROPOSAL"
   jq '{statusLine}' "$PROPOSAL"
   ```

4. After any operator-authorized manual install, create one observer **before**
   relying on new usage evidence, then let Claude Code emit a genuine callback.
   The command can start the local broker; it does not authenticate or contact
   Claude. Copy `status.monitor_id` manually to `MONITOR_ID`.

   ```bash
   "$JACKIN" usage monitor observe --provider claude --binding "$BINDING_ID" --binding-revision "$REVISION" --idempotency-key claude-personal-observer-v2 --format json --data-dir "$DATA"
   ```

   ```bash
   MONITOR_ID='copy status.monitor_id here'
   ```

   ```bash
   "$JACKIN" usage status --monitor "$MONITOR_ID" --format json --data-dir "$DATA"
   "$JACKIN" usage watch --monitor "$MONITOR_ID" --timeout-secs 300 --format jsonl --data-dir "$DATA"
   ```

   Observer status is `observe_only`, has no goal, and always has
   `runnable=false`; it never authorizes dispatch. Do not loop doctor/observe.
   A repeated callback or sibling-field change does not refresh an unchanged
   evidence field.

5. Optional dispatch requires a separate explicit operator choice of
   quota-only. The last inspected v2 store had no goal record: do not invent,
   rename, or recreate a goal. The operator must provide the exact durable ID
   of the existing Claude Code goal to track; if none is established, stop at
   observation. Approval requires all three streams to be TTYs and must not be
   piped, redirected, or captured. It acknowledges there is no SGD cap, permits
   no intentional overage, and does not bypass stale evidence or strict-policy
   conflicts. Copy `policy.revision` manually after confirmation.

   ```bash
   EXISTING_GOAL='operator-confirmed existing goal ID'
   "$JACKIN" usage policy approve --binding "$BINDING_ID" --binding-revision "$REVISION" --goal "$EXISTING_GOAL" --policy quota-only --operator-label "$OPERATOR_LABEL" --confirm --acknowledge-no-sgd-cap --format json --data-dir "$DATA"
   ```

6. Only after that approval succeeds and a genuine callback is fresh, start the
   monitor for the same goal using the returned policy revision. Save its
   `status.monitor_id` to `MONITOR_ID`. For a dispatch monitor, use bounded
   status/watch/wait; after each wait re-read status and resume only if
   authoritative `status.runnable=true`.

   ```bash
   POLICY_REVISION='operator-copied policy.revision'
   "$JACKIN" usage monitor start --provider claude --binding "$BINDING_ID" --binding-revision "$REVISION" --goal "$EXISTING_GOAL" --policy-revision "$POLICY_REVISION" --idempotency-key claude-existing-goal-v2 --format json --data-dir "$DATA"
   ```

   Copy the returned `status.monitor_id` to `MONITOR_ID` manually. Then run
   bounded status/watch/wait checks; re-read status after each wait:

   ```bash
   "$JACKIN" usage status --monitor "$MONITOR_ID" --format json --data-dir "$DATA"
   "$JACKIN" usage watch --monitor "$MONITOR_ID" --timeout-secs 300 --format jsonl --data-dir "$DATA"
   "$JACKIN" usage wait --monitor "$MONITOR_ID" --until runnable --timeout-secs 300 --format json --data-dir "$DATA"
   "$JACKIN" usage status --monitor "$MONITOR_ID" --format json --data-dir "$DATA"
   ```

Five-hour and seven-day quota thresholds (90% checkpoint, 91% reduce dispatch,
95% pause) apply independently. Reset time alone does not clear a pause. If
freshness blocks status, checkpoint and report the stable issue code. Do not
manufacture freshness or claim this setup durably unblocks the goal. Quota-only
has no billing protection.

## Corrected instruction for Claude

Continue the existing Claude Code goal under its current ID; preserve its
checkpoint and completed work. Do not rename/replace it, create another goal
to reset history, or treat an observer as dispatch authorization. Do not run
`auth prepare`, sign in, force provider refresh, or poll indefinitely. Use only
bounded status/watch/wait. Resume only if the explicitly approved dispatch
monitor reports fresh valid evidence and `status.runnable=true`; quota-only has
no SGD cap and never permits intentional overage. Otherwise save a durable
checkpoint with exact resume steps, report the issue code, and stop.
