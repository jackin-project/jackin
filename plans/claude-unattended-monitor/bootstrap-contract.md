# Foreground Claude bootstrap and explicit usage collection

**Current status (2026-10-11):** The latest pushed source checkpoint is
`62cefbbbb11ce8b6dc0634f490095d9838ebef9e`. It keeps broker wire v9 and
normalized statusline input v2 while advancing durable monitor state to schema
v6. The default-feature CLI/broker pair was built from that exact commit into
the fresh v11 prefix and passed the isolated installed smoke. Exact source and
tree IDs, toolchain and build command, binary hashes, manifest digest, and
smoke scope are recorded in [v11-installation.json](v11-installation.json).
The v11 fixture asserts schema 6 and selects the exact installed sibling
broker. The pair has not been activated against existing account state.

The v11 smoke used only private fixture state and did not access or migrate the
existing v7 account state, use the existing v10 service, authenticate, or make
a provider request. Credential shell tripwires and the local HTTP proxy
recorded zero activity; native Security Framework calls and OS-level egress
were not instrumented. Separately, the installed default-feature v10 pair was
rebuilt from commit `9809c24400e9f24ad86e83e45dca7bf5d9bab278` and passed its
private synthetic smoke; its exact evidence is in
[v10-installation.json](v10-installation.json). The subsequent v10 foreground
session and source-to-monitor evidence are described in
[v10-live-verification.md](v10-live-verification.md): the foreground command
was reported ready, the native Keychain UI event was not recorded, and two
natural provider generations succeeded. Source projections reported current
Session and All models values, but monitor publication refreshed only five-hour
usage; weekly monitor evidence remained stale. The observed `provider=current`
readiness described collector/provider state and did not establish current
monitor evidence for both quota windows. V11 has not changed that live state;
follow the separate live-verification procedure before any activation.**

The earlier `8abfa235`, `45a33093`, and 22ac install records below are
historical. The `45a33093` source-gate record is in [v3-checks.json](v3-checks.json),
and its exact installation provenance and smoke transcript are in
[v3-installed-smoke-checkpoint-45a.log](v3-installed-smoke-checkpoint-45a.log).
The earlier 22ac pair is also historical; its record is indexed in
[v3-checks.json](v3-checks.json), with the exact fixture transcript in
[v3-installed-smoke-checkpoint-22ac.log](v3-installed-smoke-checkpoint-22ac.log).

## Implemented path and limits

The direct CLI path is implemented in source and does not require Claude
settings, a statusline, or project edits. An attended foreground `auth
prepare` bootstraps the selected Claude Keychain service into a volatile,
zeroizing process cache and keeps the broker running under its no-UI guard.
After explicit local-source binding and collector approval, an observation-only
monitor can use the experimental Claude usage collector. The statusline remains
an optional, separate evidence source. The installed fixture did not exercise
successful foreground authentication or a live account/provider request, so
its results establish neither. Separately, the current v10 live checkpoint
records two successful natural provider generations and current source
projections; it does not establish continuous credential availability or
current monitor evidence for both quota windows.
The passive local service and ordinary observer do not bootstrap credentials or
make provider requests; collection requires the running foreground bootstrap,
the stored binding approval, and the explicit observer flag.

The new path must keep attended auth, provider collection, account scope, and
dispatch approval separate. `auth prepare` prepares a local foreground service;
it does not contact the provider. Collection remains off until an operator
selects and binds a scope and explicitly opts that binding into the
experimental collector.

## Implemented CLI surface

The following source CLI shapes were also present in the recorded installed
fixture's help. `--data-dir` is global; these examples place it before the
subcommand:

```text
jackin usage --data-dir PATH auth prepare --provider claude [--keychain-service SERVICE]

jackin usage --data-dir PATH binding confirm --provider claude --account LOCAL_ID --provider-account LOCAL_SOURCE_ID --operator-label LABEL --confirm --approve-experimental-collector --format json

jackin usage --data-dir PATH monitor observe --provider claude --binding BINDING_ID --binding-revision REVISION --idempotency-key KEY --experimental-collector --format json
```

`--approve-experimental-collector` is a separate, explicit operator decision
on the binding confirmation. It records
`binding.experimental_collector_approved=true` on that audited binding revision;
the default is false. The later `monitor observe --experimental-collector`
flag can consume only an already-approved mapping. It cannot create approval,
and no unattended caller may enable provider collection by itself. Neither
flag authorizes dispatch. `auth prepare` requires all three standard streams
to be terminals. Binding confirmation is also an attended operator action. Do
not pipe or redirect either confirmation command or capture their output. A
successful foreground bootstrap emits secret-free `service_ready` metadata
once, including `source.account_id` and
`source.scope: "claude_keychain_service"`. The account ID is Jackin's canonical
local source ID, not an authenticated provider identity; readiness does not
establish endpoint availability or provider support. The public account
projection labels this locally derived hash as
`identity_kind: "local_source_handle"`; it is not a provider-issued account
identity. The command stays attached to the foreground service until it exits,
releasing the lease and cache. Do not background or detach it. The current
working source versions are broker wire v9, normalized statusline input v2,
and durable monitor state v6. The installed v10 pair and its existing state
remain on schema v5; do not use that pair against v6 state. The v8/v4 wire and
monitor versions shown in older install artifacts are historical.

For real-account verification, first complete the read-only inventory and the
explicit presence-confirmation gate in
[v10-live-verification.md](v10-live-verification.md). Only after the user
confirms can Codex run `auth prepare` in an attended terminal and leave it in
the foreground after `service_ready`; the user approves only in the native
Keychain UI and does not run shell commands. From a second attended terminal
using the same data directory, use the reported local source ID in the
confirmed binding, then create the explicitly opted-in observer. Do not start
another bootstrap process or stop an owner to make room. If a known or
unrecognized service already owns the broker lease, bootstrap returns
`broker_conflict` before Keychain access; resolve its owner without stopping or
replacing it. If the foreground owner exits, its cache is gone and a new
attended bootstrap is required.

## Bootstrap lifetime and secret boundary

1. The caller and foreground broker independently require stdin, stdout, and
   stderr to be TTYs. Fail before any Keychain access if that condition is not
   met.
2. Claim the exact broker lifetime lease before touching Keychain. If another
   or unrecognized owner holds it, return a stable `broker_conflict` without
   probing, attaching to, stopping, replacing, or unlinking that service.
3. Read only the selected Claude Keychain service. Do not fall back to files,
   environment variables, another service, another profile, or broad account
   discovery. Keep the one selected credential in a bounded process-local
   `Zeroizing<String>` cache; do not write it to state, logs, environment, or
   IPC. Expose only secret-free status/opaque capability data.
4. Establish the no-UI guard after the attended credential read, then serve the
   normal broker in the same foreground process while retaining the lease.
   `auth prepare` does not issue an HTTP request. Stop/restart closes the
   collector generation and revokes its cached credential. There is no
   detached bootstrap or auth-bootstrap IPC operation. The initial attended
   read may use normal macOS authorization. Subsequent unattended Keychain
   reads are guarded: if they would require UI, return
   `interaction_required` without presenting a dialog. This no-UI behavior
   does not ensure that credentials remain present, valid, or accepted by the
   provider, and native live authorization has not been verified here.

## Collector generation admission and shutdown

Each foreground bootstrap binds a collector lifecycle gate to one exact
credential-cache generation. Admission checks that the gate is active, the
cached generation is still current, and the selected source and explicit
collection approval still match. The admission check is synchronized with
deactivation. Stop closes the gate and revokes only that generation's cache
entry; later operations cannot obtain a permit from it.

A permit keeps the generation's no-UI guard alive but does not hold the
lifecycle lock across provider or Keychain I/O. Deactivation does not physically
cancel or join an operation that already received a permit. Such I/O may finish
after stop; the stopped generation cannot admit another operation. A new
foreground bootstrap creates a new generation.

The selected credential source is not a provider identity assertion. A local
monitor account must be explicitly bound to the canonical local source handle
shown in the broker's public account projection. That projection marks it
`LocalSourceHandle`; the handle is locally derived and is not provider-issued
or authenticated identity. The operator confirms the mapping; Jackin must not
infer it from a token, session ID, or display label. A binding and collector
opt-in apply only to that mapped source scope.

The broker's ordinary discovery path must use the scoped cache while bootstrap
is active; it must not clone plaintext into general credential material or
resolve a different Claude source. Only the explicitly enabled collector may
make the narrow usage request. Do not let a whole-projection refresh, a bare
`usage` display, statusline ingress, or another caller implicitly enable it.

## Statusline compose output and operator merge

`usage statusline compose` reads the supplied settings file but prints a JSON
Merge Patch with exactly one top-level property: `statusLine`. Its value is the
proposed statusline object. If the input already has a command statusline, the
proposal preserves its `type` and other statusline command metadata/options,
changing only `command` to compose the existing command with Jackin ingestion.
If the input has no statusline, the proposal adds a command statusline. The
new-statusline patch shape is:

```json
{
  "statusLine": {
    "type": "command",
    "command": "<composed command>"
  }
}
```

When an existing statusline is present, its additional `statusLine` properties
are also preserved in the patch. The output does not echo unrelated top-level
settings or environment values.

Composition is read-only: Jackin never applies the patch or writes or modifies
Claude settings. The operator reviews the patch and, if choosing to enable the
integration, manually merges only its `statusLine` property into the existing
settings file. Preserve every other setting. Never replace the complete
settings file with the patch. This contract authorizes no live Claude settings
change.

## Experimental collector and evidence

Research targets the observed Claude usage route `GET /api/oauth/usage`. It is
undocumented and has no supported public contract established here. The
collector must identify itself with an honest Jackin user agent, such as
`jackin/<version>`; never impersonate Claude Code. Describe the route and
collector as experimental, disclose that fields and availability may change,
and make no provider-support or live-readiness claim. No live request is
authorized or verified by this plan.

The request is limited to the operator-confirmed local source scope and an
explicit experimental opt-in. Preserve persisted attempt floors, backoff, and
429 handling for that account. On the first 401 only, reread the exact same
selected service under no-UI policy and retry once only if access material
changed, the parsed account still matches the bound scope, and the bounded
attempt permits it. Unchanged, missing, denied, or different-account
credentials get no retry. Never rotate tokens, switch sources, or persist
credentials. Do not use `claude -p /usage` as an auth or collector fallback.

On restart, the coordinator reconstructs a 300-second monotonic minimum-attempt
floor only when persisted Claude state records an actual provider invocation;
queued work without an invocation timestamp gets no attempt floor. Automatic
scheduling waits until the latest applicable time among that floor, persisted
provider rate-limit, retry and success-cooldown deadlines, the in-process
monotonic cooldown, and periodic cadence. Dispatch admission independently
enforces the applicable cooldown deadlines. When a purge has a pending attempt
or active cooldown, catalog removal or rotation writes a cooldown tombstone or
pending-attempt marker before purging state, retaining the invocation and
deadline fence for a later re-add. Explicit force cannot bypass the attempt
floor, provider rate-limit deadline, or retry deadline; it may bypass only
success cooldown.
Persisted deadlines use wall-clock epochs. A sufficiently large clock change
between restarts can make an epoch deadline appear expired early, so this does
not guarantee that a `Retry-After` interval survives arbitrary clock jumps.
At historical checkpoint `45a33093`, lifecycle and recovery fixes passed the
57-test coordinator scope and 28-test Claude provider and lease lifecycle
scope. Those counts are not the current checkpoint's verification. The latest
source-head CI run [38044592706](https://github.com/jackin-project/jackin/actions/runs/38044592706)
passed for `8abfa235`, including Required, the Rust matrix, and Actionlint.
This result does not cover a later documentation/evidence commit and is not a
claim of live-provider rate-limit behavior.

An observation may collect quota evidence without an SGD receipt, goal, or
dispatch policy. It remains `observe_only`, `goal_id=null`,
dispatch-not-authorized, and `runnable=false`; enabling the collector does not
authorize agent work. Preserve strict policy and its history. Any dispatch
policy is a separate operator decision. Strict SGD retains its verified-baseline
requirements. Quota-only requires separate explicit approval and acknowledgment
that it has no SGD cap. Never claim a strict policy can be downgraded, reset
spend history, treat a collector opt-in as quota-only consent, or permit
intentional overage.

## Durable-state migration

The current working source uses broker wire v9, normalized statusline input v2,
and durable monitor schema v6. Migrate V1, V2, V3, V4, and V5 snapshots
explicitly to V6, without a legacy alias or parallel old-schema write path.
Preserve strict policy, baselines, spend and other history, action/event
sequences, evidence ages, cooldowns, and existing unknown/latched state. The
V1–V4 migrations drop provider snapshots whose current binding/source
provenance cannot be established, so those values cannot be attributed to the
wrong account or treated as fresh. V5 already has source-bound provider
provenance; its migration preserves quota and goal history.

V6 adds a per-window `statusline_reset_watermarks` value with three states:
`Unknown`, `KnownNone`, or `Known { reset_at_epoch, received_at_epoch }`.
`Known` records the highest accepted statusline reset and when that evidence
was received. For every V1–V5 account window, a nonempty legacy
`latest_reset_epochs` value migrates to `Unknown`; an empty value migrates to
`KnownNone`. Migration does not infer a statusline reset or receipt time from
the old account-wide reset, retained quota values, or event evidence. `Unknown`
keeps the account-wide reset fence from moving backward when a provider
projection is cleared; `KnownNone` means no prior statusline reset is recorded.
New accounts begin at `KnownNone`.

For a legacy window whose account-wide reset was nonempty, V6 also records
`legacy_reset_admission_after_epochs` at the old store's `last_now_epoch`. This
is an ordering cutoff, not statusline or provider evidence and not a receipt or
freshness timestamp. A lower-reset repair requires a complete current provider
quota pair whose used, reset, and paired source timestamps and receipt
timestamps are all strictly after its admission fence. For `Unknown`, that
fence is the V1–V5 migration cutoff; without a cutoff, lower-reset provider
data remains fail-closed. For `Known`, the fence is the receipt time of its own
statusline reset when that reset is at or below the account-wide reset. For
`KnownNone`, an initial valid retained provider reset equal to the account-wide
reset can establish the fence even if its older used or paired fields are
missing; the fence uses the maximum receipt time available in that window.
After a lower-reset repair, the next fence requires its matching used/reset
fields and an absent paired value. The read path validates that accepted
lower-reset cohort separately, so it does not compare the cohort against its
own receipt time.

An admitted lower reset updates displayed used/reset values but does not promote
`Unknown` to `Known` or lower the account-wide reset. It removes the paired
value, so it cannot release a latched reset barrier. A later pair must advance
beyond the barrier's prior reset and meet the account-wide reset fence, along
with the existing freshness conditions. The accepted lower-reset values remain
visible after their evidence ages out, marked stale, and dispatch stays blocked.
This lets a provider-only V5 account such as the one in the v10 live record
above display a new complete/current provider pair after migration even if
reset rounding makes its reset slightly lower. No adapter or inferred
statusline history is needed, and the old global fence and barrier remain.

A V1–V5 snapshot containing either V6-only account field,
`statusline_reset_watermarks` or `legacy_reset_admission_after_epochs`, is
rejected, including a V6 snapshot relabeled with an older schema number. The
legacy V1 decoder also rejects unknown fields. This prevents newer provenance
from being trusted after a schema downgrade.

V2 preserves its known state. V3 has no persisted correction horizon (it
defaults to `None`); for a strict goal with a baseline and a retained forward
rollover signal in the account's current-period record or the goal-period
anchor, migration preserves the known estimate while latching cumulative
completeness false and rollover unknown. It synthesizes a horizon at the
latest affected anchored period start, bounded by the store clock. Same-period
goals remain unchanged; if no affected goal is found, the account horizon
remains `None`. A closed-period anchor can indicate a rollover but cannot prove
that older corrections were retained. The V4 durable field
`historical_correction_horizon_epoch` records a period boundary after which
goal-spend completeness cannot be asserted from retained receipts. In V3
migration it represents conservative uncertainty, not proof that a correction
was received. It prevents later receipts from rolling an affected goal back to
a complete/known state. An older unretained correction remains audit-only and
unverified. A strict goal whose baseline is earlier than the horizon retains
its known cumulative-spend estimate but latches cumulative completeness false
and rollover unknown; a baseline exactly at the horizon remains known. Fresh
newer receipts must not clear an affected-goal latch; goals whose baseline is
later than the horizon remain independently evaluated.

Initialize provider-account mappings and experimental-collector opt-ins empty.
Existing records must not gain collection permission automatically; unmapped
active guards remain blocked/unknown until explicit operator reconfirmation.
Keep strict history intact. Any migration incompatibility fails closed.

## Recorded implementation evidence and remaining limits

At current source checkpoint `9809c24400e9f24ad86e83e45dca7bf5d9bab278`, the
stale-lease scope passed 38/38, the seeded synthetic provider flow passed 4/4,
and default offline CLI regressions passed 9/9. Strict feature Clippy, scoped
formatting, and diff-check passed. The earlier expanded synthetic flow at
`019de448` also passed three consecutive full-target runs. Exact results and
limits are in [verification.md](verification.md). These checks do not prove
native Keychain UI behavior or successful real account collection. The v10
pair in [v10-installation.json](v10-installation.json) contains the
default-feature build from source checkpoint `9809c244` and its private smoke
result. After explicit user confirmation, the pair opened the existing state
in the foreground and migrated it to schema 5 while preserving the current
binding and observe-only monitor. The first status showed stale 47% five-hour
and 72% weekly usage. Natural provider generations 77 and 78 then succeeded;
source projections reported Session at 19% then 21% used and All models at
82% then 83% used. Monitor watch events 16/18 and 29 through 32 refreshed only
five-hour usage; weekly monitor evidence remains stale with issue
`quota_stale`. The one-second lower source reset watermark is a candidate cause
under investigation, not a confirmed root cause. The current sanitized record is
[v10-live-verification.json](v10-live-verification.json). Native UI
observation and live provider request count remain unrecorded. The live gate
is tracked in [v10-live-verification.md](v10-live-verification.md).

The following `8abfa235`, `45a33093`, and 22ac results are historical; they do
not describe the current installed pair or close the live gate. Checkpoint
`8abfa235` passed a clean, locked MBX build in 1.29 seconds and its installed
v3 fixture passed. That fixture exercised only the actual installed v3 path
with `JACKIN_USAGE_BROKER_BIN` unset; it recorded zero HTTP-proxy requests and
zero credential-command executions. Fixture artifacts and provenance are
published in [v3-installation.json](v3-installation.json),
[v3-checks.json](v3-checks.json), and [v3-installed-smoke.log](v3-installed-smoke.log).
The historical final local formatting, source-check, and strict-Clippy checks
passed for all seven affected packages. Focused checks at that checkpoint
passed: FFI (11 tests), scope verification (7 tests), docs (5 tests), the V1
migration proof, host broker (184 tests), and capsule (1 test).
Source-head CI run [38044592706](https://github.com/jackin-project/jackin/actions/runs/38044592706)
passed for `8abfa235`, including Required, the Rust matrix, and Actionlint. PR
documentation/evidence head `56181d4d` then passed exact-head run
`38045612792`, including Required, the Rust matrix, Actionlint, and DCO. The
documentation corrections in this commit create a new head requiring its own
exact-head checks.

The `45a33093` installed pair is historical. Its MBX build took 15.93 seconds;
its local provider/lease lifecycle, host broker, and discovery scopes passed
with 28, 183, and 41 tests respectively. The separate 22ac bounded source
record includes the 57-test coordinator scope. The 22ac installed pair is
historical as well.

The 8abfa235 fixture did not instrument native Security Framework calls or
exercise successful foreground authentication. It used fixture state, not a
real account or configured evidence store. The historical 45a source gates
and installed fixture are identified by [v3-checks.json](v3-checks.json) and
[its checkpoint smoke transcript](v3-installed-smoke-checkpoint-45a.log).
Neither the build, fixture, nor source tests establish successful authorization
on this Mac, a stable provider-account identity, or successful live collection.
The no-UI guarantee applies to unattended Keychain reads after the guard is
established; the attended initial read may prompt, and credential availability
and provider acceptance are not guaranteed. The route remains experimental,
undocumented, and unsupported; it can return 403 or change without notice.
This document does not make a provider-readiness claim or authorize live
credential/provider checks.

No auth/settings/Claude-project writes, live Keychain or provider checks,
operator setup, or PR completion are part of this contract-writing task.
