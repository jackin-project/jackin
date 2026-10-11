# v10 real-account verification runbook

## State at this checkpoint

**Two natural provider generations succeeded after the user's explicit
confirmation, but full monitor acceptance remains incomplete because weekly
monitor evidence is still stale.** The installed v10 pair recorded in
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

The current real-state checkpoint is in
[`v10-live-verification.json`](v10-live-verification.json). The foreground
service was reported ready from the attended terminal session; the installed
CLI and sibling broker process images were verified to match manifest hashes.
Transient PID values are kept only in the owner-only local evidence copy. The
native Keychain UI observation was not recorded, so
this document does not claim that the UI prompt was seen or approved. The
existing store migrated to schema 5 while retaining binding
`binding-00000001` revision 1 and observe-only monitor `monitor-00000001`.
Its first status showed stale retained usage, 47% five-hour and 72% weekly,
with `quota_stale` and `reset_due_unverified`; these values were not a new
provider observation. Source evidence time was `1791699771` and receipt time
`1791708133`; the five-hour reset epoch `1791705000` had passed and the weekly
reset epoch `1792134000` was still future. Watch events 6, 7, and 8 repeated
stale evidence; the latest decision sequence at that point was 2.

The first natural collector result completed coordinator generation 77
successfully. It started at `1791708434`, invoked/completed the provider at
`1791708435`, recorded zero retry failures, and set the next-attempt floor to
`1791708735` (300 seconds). Source projection broker generation 150 reports all
six normalized groups current at `1791708434`, including both quota windows.
Its Session field reports 81% remaining (19% used); All models reports 18%
remaining (82% used) at weekly/calendar granularity. The Fable quota bucket
reports 77% remaining (not an active model), and extra usage reports 79%
remaining; both periods are unknown. The normalized weekly reset is
`1792133999`. Provider-normalized spend fields were reported in SGD, but exact
amounts are withheld from this public record and retained only in the owner-only
local evidence copy. These fields are not evidence of per-goal billing, a
billing baseline, hard spending protection, or a verified invoice.

Monitor watch events 16 and 18, latest decision sequence 3, show only a partial
refresh. Five-hour usage is 19%, evidence epoch `1791708434`, receipt epoch
`1791708435`, reset epoch `1791722999`, and fresh age 97 seconds at event 18.
Weekly monitor usage remains 72% with original stale evidence/receipt epochs
`1791699771`/`1791708133`, reset `1792134000`, and evidence age 8,760 seconds
at event 18, although the source projection reports All models at 82% used.
The source reset `1792133999` is one second earlier than the retained monitor
reset; reset-watermark rejection is a candidate cause under review, not a
confirmed root cause. The monitor reports `provider=current`, `quota=stale`,
`tracking=waiting`, `dispatch=not_authorized`, `runnable=false`, lifecycle
`needs_evidence`, and issue `quota_stale`. The source-to-monitor mismatch is
under investigation; no root cause is confirmed and acceptance remains
incomplete.

The second natural collector result completed coordinator generation 78: it
started at `1791708735`, invoked the provider at `1791708736`, completed at
`1791708737`, succeeded without a reported rate limit/retry, and set the next
attempt floor to `1791709037`. Source projection broker generation 152 reports
Session at 21% used and All models at 83% used, both current at
`1791708736`; the weekly reset remains `1792133999`. Monitor watch event 29
and subsequent events through 32 show five-hour usage at 21% with fresh
evidence/receipt epoch `1791708736`, while weekly monitor usage remains stale
at 72% with its original evidence/receipt epochs `1791699771`/`1791708133`.
Thus two natural provider generations succeeded, but neither completed the
two-window monitor workflow.

Both 300-second watches completed with exit code 0: exec session 65330 emitted
through the first natural result, and session 78393 (started at `1791708531`)
emitted through event 32 at `1791708826`. These session handles are not OS
PIDs. The attended foreground session 84850 is still running, as reported by
root. The installed CLI validates watch timeout values from 1 through 300
seconds. A 900-second request was rejected; `--help` lists the option but does
not print its range. Provider HTTP request count remains unmeasured, and no
manual refresh or cooldown bypass was used.

The existing state under review is
`~/.local/share/jackin-claude-monitor-v7/state`. The recorded
observer is `binding-00000001` revision 1 and `monitor-00000001`. Its recorded
account ID, `my-claude-subscription`, is a local operator label; it does not
prove Claude account identity. Before the fixed v10 install, the last
read-only state report identified schema 4, sequence 1, `needs_evidence`,
`tracking=waiting`, `quota=unknown`, `budget=unknown`,
`dispatch=not_authorized`, `runnable=false`, and no five-hour or weekly
usage/reset evidence. After the attended v10 startup, the actual store reported
schema 5 and retained the binding and observation-only monitor. The first
post-migration status is recorded above; its stale quota values must not be
treated as current collection success.

The last read-only startup investigation found the prior v8 broker process
absent and a persisted lease approximately 6,206 seconds old. Its protocol
version 8 conflicted with the current broker, so startup against the existing
state was blocked before the bounded recovery fix. The fix is signed and pushed at
`9809c24400e9f24ad86e83e45dca7bf5d9bab278`. Its focused stale-lease tests
passed 38/38; the seeded full CLI/provider flow passed 4/4; default offline CLI
regressions passed 9/9; strict feature Clippy, scoped formatting, and
diff-check passed; and independent source review is Ready. The source fix has
been installed into the v10 pair above. A read-only ownership recheck before
authentication found no prior broker process or socket owner. The real
state was then opened by the attended v10 service and migrated to schema 5.
Do not delete or edit the lease by hand, remove the state directory, start a
second auth process, or make a live provider request to work around stale
evidence. Preserve the running owner and wait for normal collector scheduling.

The prior [`v8-live-verification.json`](v8-live-verification.json) is archived
evidence only. It recorded a source-side successful generation but monitor
sequence 1 still had empty usage evidence and `quota_unknown`. Its percentages,
timestamps, process identifiers, and status are not current acceptance.
Likewise, the synthetic flow at checkpoint
`019de448ef8a6590b0a847046cf6a71c03d2079b` is not real-account evidence.

## Gate A: install and inspect without authentication (completed)

The lease recovery source and its MBX test gates passed; the default-feature
pair was installed from the matching source/tree and its isolated smoke passed.
Before authentication, the user explicitly confirmed presence, read-only
ownership checks found no old owner, and the exact installed CLI/broker were
identified. The v10 service then opened the existing state and reported
schema 5, retaining the existing binding and observer. The first status was
stale as recorded above; it is not collection acceptance. The watch limit was
verified from the installed CLI and source parser: 1 through 300 seconds. The
CLI help lists `--timeout-secs` but does not print the range; a 900-second
request was rejected, and a 300-second watch was started in exec session 65330.

Do not start another auth process or replace the running broker. Keep the
existing foreground owner intact while awaiting normal collector scheduling.
The remaining verification is to resolve and install the weekly
source-to-monitor publication fix, then obtain fresh monitor evidence for both
quota windows through natural collector scheduling. Respect the persisted
attempt floor and any provider retry deadlines; the two source-side successes
recorded above do not by themselves satisfy this gate.

The pre-authentication read-only checks used the fixed installed pair and the
exact existing data directory:

1. Read the current installation manifest and compare the CLI and sibling
   broker paths, hashes, versions, and source/tree provenance. Use that exact
   CLI path; do not rely on `PATH` or an older prefix.
2. The installed CLI's `usage watch --help` was checked. It lists
   `--timeout-secs` but does not display the accepted range; the source parser
   constrains it to 1..=300 seconds. Check current help before using any other
   command, and do not copy syntax from the historical v8 artifact.
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

The pair started and migrated the store to schema 5. Do not manually repair
persisted lease files or alter the binding, monitor, dispatch policy, or
spending controls during this verification.

## Attended confirmation and authentication (started)

The user explicitly confirmed presence before the attended auth command was
started. The foreground command ran in exec session 84850 and the service was
reported ready. The CLI and broker were verified against the installation
manifest; transient PID values are retained only in the owner-only local
evidence copy. The exact native Keychain UI interaction was not recorded; do not claim it was observed
or approved from this record. No credential contents were recorded. Do not
start another auth command or replace the running process as part of this
checkpoint.

The service uses the existing binding. Preserve the distinction between its
local account label and authenticated provider identity; the label does not
independently establish the Claude identity. If a further authentication
attempt becomes necessary, stop and obtain a fresh attended confirmation
before running it.

## Real collection and CLI acceptance

Keep the foreground service terminal open for the lifetime required by the
foreground broker lease. The current CLI and broker images match the installed
pair's manifest hashes. Do not detach, stop, or replace this owner.

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

The first post-migration status retained stale 47% five-hour and 72% weekly
values, with evidence time `1791699771`, receipt time `1791708133`, and issues
`quota_stale` and `reset_due_unverified`. The five-hour reset epoch
`1791705000` had passed; weekly reset `1792134000` was still future. Watch
events 6, 7, and 8 repeated that stale state. A later natural collector
generation 77 succeeded at provider invocation/completion epoch `1791708435`
with source observation epoch `1791708434` and zero retry failures. Source
projection broker generation 150 reported all six normalized groups current;
the next-attempt floor is `1791708735`. Monitor watch events 16 and 18 show
five-hour usage at 19% with evidence/receipt epochs `1791708434`/`1791708435`
and reset `1791722999`, but weekly monitor usage remains stale at 72% with its
original evidence/receipt epochs `1791699771`/`1791708133` and reset
`1792134000`. At event 18 the provider readiness was current, quota stale,
tracking waiting, dispatch unauthorized, `runnable=false`, lifecycle
`needs_evidence`, and issue `quota_stale`; evidence ages were 97 seconds and
8,760 seconds for five-hour and weekly fields. The source reports Session at
19% used and All models at 82% used; it also reports the Fable quota bucket at
77% remaining (not an active model) and extra usage at 79% remaining, both with
unknown periods. Provider-normalized spend fields were reported in SGD, but
exact amounts are withheld from this public record and retained only in the
owner-only local evidence copy. They are not a verified invoice, per-goal
billing, baseline, or hard protection.
The second natural collector generation 78 also succeeded after the saved
300-second floor: provider invocation was `1791708736`, completion was
`1791708737`, and the next floor is `1791709037`. Source projection broker
generation 152 reports Session at 21% used and All models at 83% used, both
current. Monitor watch event 29 and later events through 32 show fresh
five-hour evidence at 21%, but weekly monitor evidence remains stale at 72%.
Therefore two source generations succeeded but neither produced complete
two-window monitor evidence. Source-side weekly evidence is current while
monitor-side weekly evidence remains stale. The one-second reset-watermark
difference is a candidate cause under review, not a confirmed root cause. The
two bounded watch commands, sessions 65330 and 78393, both exited 0; the second
ended at event 32 / epoch `1791708826`. Detailed sanitized data is in
[`v10-live-verification.json`](v10-live-verification.json). Provider HTTP
request count is unmeasured. No force refresh, manual cooldown bypass, or
second auth was used. Keep the foreground process running and respect the
current saved floor `1791709037` and any provider deadlines.

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

The current v10 pair has produced two natural successful source generations,
but weekly monitor evidence remained stale after both. After the source-aware
publication fix is installed in a verified pair, verify two natural collection
cycles through the corrected monitor, with both windows current and their
source/evidence times recorded. Respect saved minimum-attempt and provider
deadlines; do not issue a force refresh, reset cooldown state, or make requests
earlier than those deadlines. A gap where old evidence becomes stale before
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
- two natural successful collections through the corrected installed pair
  refresh both monitor windows and respect all saved deadlines;
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
