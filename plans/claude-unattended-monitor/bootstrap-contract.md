# Foreground Claude bootstrap and explicit usage collection

**Status: implemented in source; the recorded v3 installed fixture passed, and
the post-cleanup pair rebuild is pending. Real Keychain authorization,
successful foreground bootstrap, real-account binding, and live provider
collection remain unverified.** The recorded CLI and matching broker were built
from source commit `1a45196dbe24d439e596c14e22fbda59799e7b0d`; their hashes and
fixture results describe that prior pair only. Refresh provenance and rerun
the installed fixture after the dormant-helper cleanup rebuild. See the
[recorded handoff](claude-code-handoff.md) and
[fixture log](/private/tmp/jackin-v3-installed-smoke.log).

## Implemented path and limits

The direct CLI path is implemented in source and does not require Claude
settings, a statusline, or project edits. An attended foreground `auth
prepare` bootstraps the selected Claude Keychain service into a volatile,
zeroizing process cache and keeps the broker running under its no-UI guard.
After explicit local-source binding and collector approval, an observation-only
monitor can use the experimental Claude usage collector. The statusline remains
an optional, separate evidence source. The installed fixture did not exercise
successful foreground authentication or a live account/provider request, so
neither live readiness nor dialog-free credential availability is established.
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
releasing the lease and cache. Do not background or detach it. Current source
versions are broker wire v8, normalized statusline input v2, and durable
monitor state v4. Wire v8 adds the explicit `LocalSourceHandle` identity kind;
it does not change the statusline input or durable monitor schema.

Sequence: run `auth prepare` in one attended terminal and leave it in the
foreground after `service_ready`. From a second attended terminal using the
same data directory, use the reported local source ID in the confirmed
binding, then create the explicitly opted-in observer. Do not start another
bootstrap process or stop an owner to make room. If a known or unrecognized
service already owns the broker lease, bootstrap returns `broker_conflict`
before Keychain access; resolve its owner without stopping or replacing it. If
the foreground owner exits, its cache is gone and a new attended bootstrap is
required.

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
   `auth prepare` does not issue an HTTP request. Stop/restart ends the cache
   lifetime and drops the zeroizing value. There is no detached bootstrap or
   auth-bootstrap IPC operation.

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

Shared provider rate-limit hardening is in progress. The target rule anchors a
Claude attempt floor at the provider-response completion time rounded upward
to an epoch second, then adds 300 seconds. Recovery of an interrupted active
attempt starts a conservative 300-second floor from the recovery time, also
rounded upward. The latest of these floors, provider `Retry-After`, and retry
backoff wins; explicit force cannot bypass those deadlines. Focused fake-clock,
restart, and force-path verification is still pending, and no live provider
rate-limit behavior is claimed. See the [statusline contract](statusline-contract.md).

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

The current source uses broker wire v8, normalized statusline input v2, and
durable monitor schema v4. Migrate V1, V2, and V3 snapshots explicitly to V4.
Preserve strict policy, baselines, spend and other history, action/event
sequences, evidence ages, cooldowns, and existing unknown/latched state. V2
preserves its known state. V3 has no persisted correction horizon (it defaults
to `None`); for a strict goal with a baseline and a retained forward rollover
signal in the account's current-period record or the goal-period anchor,
migration preserves the known estimate while latching cumulative completeness
false and rollover unknown. It synthesizes a horizon at the latest affected
anchored period start, bounded by the store clock. Same-period goals remain
unchanged; if no affected goal is found, the account horizon remains `None`. A
closed-period anchor can indicate a rollover but
cannot prove that older corrections were retained. The V4 durable field
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

The current source implements the foreground bootstrap, selected-service
zeroizing cache, no-UI guard, explicit binding approval, and opt-in observer
path. The recorded v3 installed fixture passed help, sibling selection,
passive-service, observer, and statusline-fixture checks. It observed zero HTTP
proxy requests and no dispatch policy approvals. This fixture did not instrument
native Security Framework calls and did not exercise successful foreground
authentication. It used fixture state, not a real account or configured
evidence store. That prior binary pair used wire v7; its fixture record remains
historical evidence for that pair and says nothing about current wire v8.

The exact recorded pair will be rebuilt after dormant-helper cleanup; refresh
its source provenance and binary hashes and rerun the installed fixture before
claiming that rebuilt pair is verified. Neither the prior fixture nor current
source tests establish successful authorization on this Mac, a stable
provider-account identity, successful live collection, or an absence of
Keychain authorization dialogs. The route remains experimental, undocumented,
and unsupported; it can return 403 or change without notice. This document
does not make a provider-readiness claim or authorize live credential/provider
checks.

No auth/settings/Claude-project writes, live Keychain or provider checks,
operator setup, or PR completion are part of this contract-writing task.
