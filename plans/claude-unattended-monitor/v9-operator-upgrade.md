# v9 attended operator upgrade

**Status:** awaiting an attended operator. Agents have not run either command
below against the real data directory. This is an operator runbook, not a
Claude Code prompt or handoff.

The purpose is to use the installed v9 broker with the existing monitor and
state at `/Users/donbeave/.local/share/jackin-claude-monitor-v7/state`. Keep
the existing foreground Claude Code terminal open and paste-ready. Do not
create another monitor, rebind the account, approve a policy, or change a
statusline setting as part of this step.

## Attended sequence

1. In a separate interactive terminal, stop the v8 broker with its matching
   v8 CLI and the same existing data directory:

   ```bash
   /Users/donbeave/.local/share/jackin-claude-monitor-v8/bin/jackin usage --data-dir /Users/donbeave/.local/share/jackin-claude-monitor-v7/state service stop --format json
   ```

   Wait for the command to return successfully with `service_stopped`. This
   stops the broker associated with that data directory; it does not stop
   Claude Code. If the command errors or reports another result, pause and
   report the output. Do not kill a process or try another broker binary.

2. Let the original foreground authentication operation finish and return
   normally to its terminal prompt. Keep that terminal open. Do not cancel or
   restart the original operation. Do not begin v9 authentication until both
   the v8 stop command has returned and the original terminal is back at its
   prompt.

3. In an interactive terminal, run the v9 authentication preparation against
   the same data directory. The command-local override selects the v9 broker
   sibling for this invocation only:

   ```bash
   JACKIN_USAGE_BROKER_BIN=/Users/donbeave/.local/share/jackin-claude-monitor-v9/bin/jackin-usage-broker /Users/donbeave/.local/share/jackin-claude-monitor-v9/bin/jackin usage --data-dir /Users/donbeave/.local/share/jackin-claude-monitor-v7/state auth prepare --provider claude
   ```

   Keep this command in the foreground and complete its interactive prompts
   yourself. When it returns, report the result and stop. The follow-up check
   will read only passive status/watch for existing monitor
   `monitor-00000001`; no new monitor or policy action is part of this upgrade.

If either step fails or the terminal does not return, pause and report what
happened. Keep the v7 and v8 installations and the existing state directory
available. Do not switch back to v8 while a v9 broker may still be using that
state; ask for a coordinated rollback sequence first.

## Verification boundary

The installed v9 pair was built from signed source commit
[`f5163e987b861adca1de5add6087ee3d2d12e11a`](https://github.com/jackin-project/jackin/commit/f5163e987b861adca1de5add6087ee3d2d12e11a),
tree `9418c762f07d6b6a6203def838e11ddb33a29c53`. Root reports 736 source tests
passing, strict all-target Clippy for both packages, and formatting passing.
The installed smoke ran against private synthetic state: its 35-second watch
took 35.068 seconds and emitted one unchanged event at sequence 1; the fake
broker launch sentinel, credential shell tripwires, and local HTTP proxy each
recorded zero requests. The fixture service stopped orderly. It did not test
real authentication or provider collection, and its proxy is not OS-level
egress instrumentation.

Binary hashes, versions, install command, and fixture output are recorded in
[`v9-installation.json`](v9-installation.json); broader outcomes are in
[`verification.md`](verification.md) and the outstanding acceptance gate is
tracked in [`task-queue.md`](task-queue.md). Real v9 account status and
five-hour/seven-day freshness are still unverified until the attended
follow-up. The circuit and retry policy can reduce avoidable requests but
cannot guarantee zero `429` responses or continuously fresh quota data. The
[Claude Code costs guide](https://code.claude.com/docs/en/costs) describes
cached `/usage` bars after a rate-limited request; public documentation does
not specify a subscription polling quota.
