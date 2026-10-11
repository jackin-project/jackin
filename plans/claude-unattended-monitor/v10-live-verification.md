# v10 real-account verification runbook

## State at this checkpoint

**Real authentication and provider collection have not been performed.** The
installed v10 pair currently recorded in
[`v10-installation.json`](v10-installation.json) is the default-feature build
from signed source checkpoint `9809c24400e9f24ad86e83e45dca7bf5d9bab278`,
tree `2af8f8b98c61c47a92b071e0d77072446aa965f1`:

| Artifact | Path | SHA-256 |
| --- | --- | --- |
| CLI | `/Users/donbeave/.local/share/jackin-claude-monitor-v10/bin/jackin` | `9c73d480ecdaeb9280666641217a0b521a70be1414c39a86ab7c1b54b74db3b2` |
| Broker | `/Users/donbeave/.local/share/jackin-claude-monitor-v10/bin/jackin-usage-broker` | `fafeb4765667a40c8a9395e391cc07e3ca7e2d33b9d2daebe88fc0a5a6639fcf` |

Both report version `0.6.4`. The manifest records the v10 sibling-selection
and private synthetic smoke passing in 35.070 seconds. Manifest SHA-256 is
`24366ae9cf17a34d93f1c82d19bdee7f7444d3fe0a83f34410a754376cb4900f`. The prior
019 source installation remains archived in the manifest's
`installation_history[0]`. The smoke explicitly records that no real data
directory was accessed, no service was started against real data, and no
successful authentication or provider request occurred. Its schema-5 assertion
applies only to fixture state.

The existing state under review is
`/Users/donbeave/.local/share/jackin-claude-monitor-v7/state`. The recorded
observer is `binding-00000001` revision 1 and `monitor-00000001`. Its recorded
account ID, `my-claude-subscription`, is a local operator label; it does not
prove Claude account identity. Re-read current binding, source mapping, monitor
schema, status, and service state after the startup fix before relying on any
of these recorded IDs. The last read-only state report, before the fixed v10
install, identified schema 4,
sequence 1, `needs_evidence`, `tracking=waiting`, `quota=unknown`,
`budget=unknown`, `dispatch=not_authorized`, `runnable=false`, and no five-hour
or weekly usage/reset evidence. The v10 fixture's schema-5 assertion does not
mean this real state has migrated; verify the current schema after the fixed
broker opens the state.

The last read-only startup investigation found prior v8 broker PID 25738
absent and a persisted lease approximately 6,206 seconds old. Its protocol
version 8 conflicted with the current broker, so startup against the existing
state was blocked before the bounded recovery fix. The fix is signed and pushed at
`9809c24400e9f24ad86e83e45dca7bf5d9bab278`. Its focused stale-lease tests
passed 38/38; the seeded full CLI/provider flow passed 4/4; default offline CLI
regressions passed 9/9; strict feature Clippy, scoped formatting, and
diff-check passed; and independent source review is Ready. The source fix has
been installed into the v10 pair above. A read-only ownership recheck found no
PID 25738, broker process, or socket owner; the existing account service has
not been started with the new pair. The real state has not been migrated or
otherwise verified against this pair. Do not delete or edit the lease by hand,
remove the state directory, start authentication, or make a live provider
request to work around it. Recheck process ownership, lease, and state
immediately before any authentication step.

The prior [`v8-live-verification.json`](v8-live-verification.json) is archived
evidence only. It recorded a source-side successful generation but monitor
sequence 1 still had empty usage evidence and `quota_unknown`. Its percentages,
timestamps, process identifiers, and status are not current acceptance.
Likewise, the synthetic flow at checkpoint
`019de448ef8a6590b0a847046cf6a71c03d2079b` is not real-account evidence.

## Gate A: install and inspect without authentication

Do not begin attended authentication until the lease recovery change at
`9809c244` has been reviewed, its targeted and relevant integration tests have
passed through MBX, and the installed pair above is confirmed against the
manifest. Those source and isolated install gates have passed. The next step is
a fresh read-only check against the existing v7 state; the fixture smoke did
not touch it.

Using the fixed installed pair and the exact existing data directory, perform
read-only checks first:

1. Read the current installation manifest and compare the CLI and sibling
   broker paths, hashes, versions, and source/tree provenance. Use that exact
   CLI path; do not rely on `PATH` or an older prefix.
2. Read that CLI's current help for auth, service, binding, monitor, status, and
   watch syntax. Do not copy command syntax from the historical v8 artifact
   without checking the current binary.
3. Query service status and inspect the lease/process information with the
   supported read-only tools. Confirm whether any process owns the state and
   resolve its executable and arguments. Recheck that the old v8 owner is
   absent. Do not kill a process, unlink a lease, or replace a running broker.
   If progress would require disrupting a live owner, stop and obtain explicit
   approval for that specific PID, executable, and state directory.
4. Confirm the existing binding is still `binding-00000001`, revision 1, and
   maps to the expected selected Claude source; confirm the existing
   observation-only monitor is still `monitor-00000001`. Record the current
   schema and status. If these IDs or mappings differ, use the current state
   output and update this runbook rather than assuming the historical IDs.
5. Confirm collection is the intended capability and dispatch remains
   unauthorized. No dispatch approval, spending-policy change, quota-only
   route, or extra-usage permission is part of this verification.

If the fixed pair still cannot safely start against the existing state, stop
at this gate. Preserve the state, record the exact safe error and requested
operator action, and do not manually repair persisted lease files.

## Attended confirmation and authentication

Only after Gate A passes and the exact post-fix CLI and data paths are known,
ask the user this question verbatim, substituting those verified paths:

> I am ready to run the attended authentication and real-usage verification
> using [exact binary] and [exact data directory]. Are you present and ready
> to approve any Keychain prompt?

Wait for an explicit affirmative response. Before it arrives, do not run the
authentication command or make any live provider request. After it arrives,
Codex runs the supported attended `usage auth prepare` command itself in a PTY
with all three standard streams attached. Tell the user when the native
Keychain approval UI appears; the user approves only in that UI and does not
need to run shell commands. Never display, log, commit, or transmit credential
contents. Do not change Claude Code credentials, settings, environment,
worktrees, or Keychain ACLs.

Record the command, exact CLI/data paths, start/end time, exit status, and
redacted `service_ready` result. Verify the ready source is the one selected
by the existing binding. Preserve the distinction between the binding's local
account label and authenticated provider identity; only make an identity claim
if the provider response independently establishes it.

## Real collection and CLI acceptance

Keep the foreground service terminal open for the lifetime required by the
foreground broker lease. Verify from process metadata and the lease that the
running broker is the exact sibling of the selected CLI build. Do not detach,
stop, or replace another owner's process.

Use the current CLI's verified syntax to read service status, monitor status,
and a bounded watch. Save only sanitized output. For status and each watch
event, record:

- CLI/broker paths, versions, hashes, source provenance, and process identity;
- data directory, monitor ID, binding ID/revision, provider source scope, and
  whether the account ID is a local label or authenticated identity;
- schema version, event sequence, exit status, actual elapsed time, and
  service state;
- five-hour and weekly used percentages, reset epochs/validity, evidence source,
  source/account/session scope, evidence time, receipt time, freshness, and
  provider/readiness issue codes.

An observation-only monitor can correctly report `runnable: false` and
dispatch unauthorized while collection succeeds. Before a successful real
provider observation, missing evidence and `quota_unknown` can be expected;
`runnable: false` and dispatch unauthorized are also expected for this
observer. Verify the real CLI reports its current monitor schema (the v10
synthetic smoke used schema 5), then distinguish that initial state from
post-collection success. Determine collection success from provider readiness
and evidence, not from dispatch readiness. The status/watch path must expose
both quota windows and their evidence after a successful source generation; if
it remains empty, unknown, or stale then, do not claim acceptance.

Obtain at least two successful observations through normal collector
scheduling. The second must occur after the persisted minimum 300-second
attempt floor and after every applicable provider `Retry-After`, failure
backoff, or circuit deadline. Do not issue a force refresh, reset cooldown
state, or make requests earlier than those deadlines. Record both collection
times and source evidence times. A gap where old evidence becomes stale before
the next permitted poll is a limitation to report, not a reason to fake
freshness or shorten the floor.

For passive reads, report credential/provider access counts only when directly
measured. The CLI may not expose those counters; in that case report them as
unmeasured and separately state the architectural expectation. Do not reuse
the synthetic fixture's local request counts as live counts. While the
foreground service remains active, verify later collection does not require
another authentication prompt. If credentials become unavailable, expect a
typed `interaction_required` result without prompting or spinning in a retry
loop.

Use offline fixtures for threshold, checkpoint, reset, restart, and failure
conditions whose live reproduction could spend subscription capacity or grant
authority. Do not consume usage to reach 90%, 91%, or 95%, authorize dispatch,
weaken spending guards, or enable quota-only routing. Inspect extra usage only
if the provider genuinely returns it; estimates, unknown credits, and USD
amounts are not verified SGD billing.

## Completion and stop conditions

Do not mark real-account acceptance complete or produce a ready-to-paste
Claude Code instruction until all of the following are true:

- the post-fix installed pair and its provenance are verified;
- the approved attended authentication succeeded through the native UI;
- the intended source and existing observation-only binding/monitor are
  confirmed;
- status and bounded watch show genuine five-hour and weekly evidence with
  resets, source, timestamps, freshness, and typed issues recorded;
- a second natural successful collection respects all saved deadlines;
- passive-read and unattended behavior are measured where possible and
  otherwise explicitly labeled unmeasured;
- relevant MBX integration checks and any corrective changes are committed and
  pushed; and
- limitations for freshness gaps, native Keychain, HTTPS/TLS, OS egress,
  provider request counts, extra usage, and SGD billing are stated honestly.

If an external human action blocks progress, report that exact action and
preserve the checkpoint. Do not endlessly poll, force automatic resumption, or
claim readiness. The final Claude Code instruction must use the verified
commands and monitor ID and cover independent session/weekly guards,
90/91/95% checkpoint behavior, durable lane handoffs, once-per-sequence
actions, bounded reset waits requiring fresh evidence, and minimal extra usage
without granting new spending permission.
