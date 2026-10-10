# Foreground Claude bootstrap and explicit usage collection

**Status: candidate contract; implementation and verification are pending.**
The command shapes below reflect the current CLI-port plan, not a command set
verified from a newly built main-port binary. Confirm them against the built
CLI's help before updating operator handoff instructions.

## Purpose and current gap

The requested user path is direct CLI usage without Claude settings or project
edits. The current `auth prepare` only probes a credential and discards it; the
current local split also disables the Claude collector. Neither provides a
working collection path. The older statusline observer can record callback
evidence but does not establish direct account collection, and its evidence
can still age out. No real-account callback, native Keychain, or live provider
check is available as proof for this port.

The new path must keep attended auth, provider collection, account scope, and
dispatch approval separate. `auth prepare` prepares a local foreground service;
it does not contact the provider. Collection remains off until an operator
selects and binds a scope and explicitly opts that binding into the
experimental collector.

## Planned CLI surface

These are the exact shapes reported by the CLI-port owner. Confirm flags,
position, and JSON fields from a fresh main-port build before treating them as
actual commands:

```text
jackin usage auth prepare --provider claude [--keychain-service SERVICE] [--data-dir PATH]

jackin usage binding confirm --provider claude --account LOCAL_ID --provider-account CANONICAL_ID --operator-label LABEL --confirm --approve-experimental-collector --format json --data-dir PATH

jackin usage monitor observe --provider claude --binding BINDING_ID --binding-revision REVISION --idempotency-key KEY --experimental-collector --format json --data-dir PATH
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
broker status response is secret-free and reports local service readiness only;
it does not establish endpoint availability or provider support. The planned
`service_ready` response is emitted once while the command remains attached to
the foreground service; the operator ends that process to release the lease
and cache. Do not background or detach it. The candidate versions are broker
wire v7, normalized statusline input v2, and durable monitor state v4. Keep
wire and callback input versions unchanged for the durable-state migration.

Sequence: run `auth prepare` in one attended terminal and leave it in the
foreground after `service_ready`. From a second attended terminal using the
same data directory, inspect/select the account scope, confirm the binding,
then create the explicitly opted-in observer. Do not start another bootstrap
process or stop an owner to make room. If the foreground owner exits, its
cache is gone and a new attended bootstrap is required.

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

The selected credential source is not an account identity assertion. A local
monitor account must be explicitly bound to a canonical provider account
selected from the broker's public account projection. The operator confirms
that mapping; Jackin must not infer it from a token, session ID, or display
label. A binding and a collector opt-in apply only to that mapped scope.

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

The request is limited to the operator-confirmed canonical account and an
explicit experimental opt-in. Preserve persisted attempt floors, backoff, and
429 handling for that account. On the first 401 only, reread the exact same
selected service under no-UI policy and retry once only if access material
changed, the parsed account still matches the bound scope, and the bounded
attempt permits it. Unchanged, missing, denied, or different-account
credentials get no retry. Never rotate tokens, switch sources, or persist
credentials. Do not use `claude -p /usage` as an auth or collector fallback.

An observation may collect quota evidence without an SGD receipt, goal, or
dispatch policy. It remains `observe_only`, `goal_id=null`,
dispatch-not-authorized, and `runnable=false`; enabling the collector does not
authorize agent work. Preserve strict policy and its history. Any dispatch
policy is a separate operator decision. Strict SGD retains its verified-baseline
requirements. Quota-only requires separate explicit approval and acknowledgment
that it has no SGD cap. Never claim a strict policy can be downgraded, reset
spend history, treat a collector opt-in as quota-only consent, or permit
intentional overage.

## Migration and verification gates

The candidate port carries broker wire v7, normalized statusline input v2, and
durable monitor schema v4. Migrate V1, V2, and V3 snapshots explicitly to V4.
Preserve strict policy, baselines, spend and other history, action/event
sequences, evidence ages, cooldowns, and existing unknown/latched state. The V4
durable field `historical_correction_horizon_epoch` records the latest verified
closed billing period that can no longer be corrected from retained account
receipts. It prevents later receipts from rolling an affected goal back to a
complete/known state. An older unretained correction remains audit-only and
unverified; if its horizon crosses a goal's baseline, keep that goal's known
cumulative-spend estimate while latching cumulative completeness false and
rollover unknown. Fresh newer receipts must not clear that affected-goal latch;
goals whose baseline is later than the horizon remain independently evaluated.

Initialize provider-account mappings and experimental-collector opt-ins empty.
Existing records must not gain collection permission automatically; unmapped
active guards remain blocked/unknown until explicit operator reconfirmation.
Keep strict history intact. Any migration incompatibility fails closed.

Required offline proof before updating the handoff:

- TTY rejection occurs before Keychain access; a lease conflict causes zero
  Keychain reads and never affects the existing owner.
- The foreground owner reads only the selected service, keeps bounded
  zeroizing cache material, redacts outputs, blocks UI for the service
  lifetime, and clears the cache on stop/restart.
- Auth preparation and passive status/observer paths make zero provider
  requests. Default binding approval is false; only an attended binding
  confirmation with `--approve-experimental-collector`, followed by
  `monitor observe --experimental-collector`, enables the selected scope.
  Revisions are audited; the observe flag alone never grants collection or
  dispatch authority.
- Fake-adapter tests cover no broad discovery, account isolation, cooldown,
  401 same-source/same-account retry rules, and typed 403/429 failures without
  live requests.
- V1/V2/V3-to-V4 migration fixtures preserve strict policy, baselines, spend
  history, action/event sequences, and existing unknown/latched states while
  leaving new mapping/opt-in state empty. Exercise correction horizons across
  migration, including preservation of cumulative estimates and affected-goal
  unknown latches. Strict and explicitly approved quota-only paths remain
  distinct.
- Build the exact local split/install artifact with the collector capability
  enabled, inspect its real help/output schema, then run the isolated installed
  fixture. Do not carry predecessor test counts or installed fixture results
  forward as proof for this main-port build.

No auth/settings/Claude-project writes, live Keychain or provider checks,
operator setup, or PR completion are part of this contract-writing task.
