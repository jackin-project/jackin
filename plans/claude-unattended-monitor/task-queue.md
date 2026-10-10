# Claude unattended usage monitor

## Safety boundary

Work starts read-only. Implementation and verification use isolated state and
fixtures. Do not access live provider endpoints or Keychain, invoke Claude,
modify Claude auth/settings, kill sessions, or mutate their worktrees.
All delegated work uses GPT-6-Luna at max reasoning.

## Queue

- [done] Research shipped provider/auth, broker/coordinator, CLI/consumers,
  and official statusline contracts independently.
- [done] Freeze command names, observation/policy/spend contracts, and file
  ownership before parallel implementation.
- [done] Implement broker-owned noninteractive auth and bounded optional HTTP.
- [done] Implement bounded statusline ingress and explicit composition setup.
- [done] Implement durable observations, monitors, decisions, spend baselines,
  reset waits, and passive status/readiness APIs.
- [done] Migrate CLI and affected consumers; remove superseded paths.
- [done] Run deterministic offline fake clock/Keychain/HTTP coverage and
  consumer regressions; independent security/rate-limit reviews.
- [done] Build an isolated binary, verify exact commands and JSON/exit schema,
  document evidence and limitations, commit locally, and record blocked pushes.
- [done] Produce ready-to-paste Claude Code handoff from verified commands.

## Acceptance evidence

Record checks, request counts, no-dialog evidence, commit SHAs, and actual binary
paths here as they become available. Historical plans are not implementation proof.

## Implementation boundary and contract

Implementation checkout: `/tmp/jackin-claude-monitor` (branch
`claude-unattended-monitor`, base `ff9eb01f`). The original checkout is clean;
a read-only process check found Claude running, so all edits/builds remain in
the isolated checkout. No live auth, Keychain or provider checks are authorized.

Frozen commands: `usage auth prepare`, `usage service start|stop|status`,
`usage doctor --provider claude --unattended`, `usage monitor start|stop`,
`usage status`, `usage refresh`, `usage watch`, `usage wait --until runnable`,
`usage statusline ingest|compose`, `usage spend record`. Monitoring commands
select a stable monitor ID; start selects account and goal IDs. Usage-level
`--data-dir` provides isolated state. JSON status/readiness and JSONL watch
are separate formats. Superseded host snapshot/projection commands are removed.

Independent Claude OAuth refresh is disabled for durable monitors and the normal
broker executor, including desktop whole-projection refresh. `refresh` is
local reconciliation, never a forced HTTP call. Existing broker provider work
is hardened with a persisted Claude attempt floor of 300 seconds and positive
backoff; this does not guarantee avoidance of provider bans.

Official Claude Code docs: <https://code.claude.com/docs/en/statusline>.
`rate_limits.five_hour` and `.seven_day` each optionally carry
`used_percentage` and `resets_at`. Introduced in official v2.1.80 changelog.
These fields do not identify the account or provide an observation timestamp.
Integer basis points in the monitor protocol have explicit names (9000=90%).
Repeated identical callbacks retain first evidence receipt age. Missing/stale
fields remain unknown. Session list-price cost is not billing evidence.

## Ownership

- broker_research: new monitor protocol DTOs/tests and module export.
- provider_research: Claude provider/auth and narrow discovery/facade callers.
- coordinator_rate_limits: coordinator admission/backoff/recovery/tests.
- monitor_engine: new broker monitor store/engine, delegates spend/policy modules.
- broker_integration: socket/lifecycle/client integration and relay rejection.
- consumer_tests_research: CLI, bootstrap bypass, local-only broker binary.
- statusline_docs: composition helper and official input contract documentation.
- security_review: independent read-only security review.

## Verified so far

- `cargo check --offline -p jackin-protocol`: passed.
- Protocol monitor contract fixtures: 4 tests passed (agent report).
- Original worktree status after moving our newly created files: clean.

## Checkpoints and review evidence

- `e5ee84f8`: monitor contract and initial durable queue; pushed successfully.
- `2031b552`: persisted Claude attempt floor, positive backoff, catalog-reset
  transaction and restart tests; 44 coordinator tests and scoped offline Clippy
  passed. Push refused by SSH agent; no interactive authentication attempted.
- Provider/core/discovery/credential suites: 171 tests passed. Scoped offline
  Clippy passed for Claude provider, provider-core, discovery and credential
  resolver. Typed HTTP failure metadata reaches the broker without text parsing.
- Composition helper: 11 isolated offline fixtures passed; full package fixture
  run awaits the integrated build. Official statusline source contract recorded
  in `statusline-contract.md`.
- First broker integration run: 70 passed, 6 failed. Two legacy force-refresh
  expectations are corrected to enforce the Claude floor. Policy failures and
  independent review findings are being fixed before runnable decisions are
  accepted. Do not treat this intermediate test count as final validation.
- Independent review found and implementation addressed: cooldown loss on catalog
  revision; wrapper capture race/SIGPIPE behavior; cross-user socket attachment;
  future-period spend receipt; optional-model guard; same-goal spend reset;
  reset-field pairing; deadline scheduling; clock rollback; unrelated projection
  publication timestamps. Final review and fixture validation remain required.
- 401 behavior is intentionally fail-closed with zero automatic rereads/retries.
  The resolved credential has no safe same-source reread handle; a separately
  supplied token is a distinct bounded attempt. No refresh-token ownership is
  assumed and no CLI fallback exists.
- `2e984db`: provider/auth hardening checkpoint, backed by the provider suites
  and scoped Clippy above. No live credential or provider verification performed.
- Final consumer audit found Capsule launch still resolving credentials before
  broker attachment. A broker-only relay-capability resolution operation is
  being added using the existing exact source-proof intersections; the relay
  will send only secret-free forwarding facts. Legacy client discovery APIs
  will be removed. This is required by broker ownership, not an optional cleanup.
- Final security review also found bounded session storage could permanently
  reject the seventeenth sequential session. The engine is adding safe inactive
  session eviction while preserving active monitor/reset dependencies, and
  tightening future reset horizons before updating durable watermarks.
- Monitor fake-clock fixtures now include independent seven-day recovery,
  old-session reset replay, >10-minute idle/reopen, model/spend age boundaries,
  absent baseline persistence, and unknown currency. A combined broker fixture
  covers reset ticking while Retry-After still blocks forced provider work.
- Fake Keychain reads now share production orchestration and cover missing,
  locked, consent-required, query/search/disable errors and nested guard state.
  This is offline adapter evidence, not live macOS ACL verification.
- First facade regression run compiled and passed 67/68 cases; one provider-call
  allowlist entry is being reconciled. Final consumers and typed broker discovery
  diagnostics are still migrating; intermediate results are not final gates.
- `887a1b47`: shared native/fake Keychain orchestration and nested guard tests.
  Final provider/core/discovery/credential run: 177 tests passed; scoped
  all-target Clippy passed. Manifest lock changes are minimal and offline.
- `4c6ba90`: atomic typed publisher diagnostics; 14 publisher tests passed.
- Caller-side relay discovery and unused HostUsageRuntime are removed. The
  host-only wire catalog-injection API is removed, and forwarded env routes now
  require exact staged credential-source proofs. Broker-owned conflict retry,
  empty-scan confirmation and stale-lease tests replace retired caller hooks.
- Final restart review found watch could replay old runnable authority after
  evidence expiry. Watch now reconciles before reads; a zero cursor is a fresh
  attachment to the latest event. Initial service ticking and fake-clock/CLI
  restart regressions are being completed before the integrated broker gate.
- Final publisher run: 15 tests passed and all-target Clippy passed; diagnostic
  test extraction checkpoint `013e5e6`. Opaque discovery diagnostics checkpoint
  `d06582d` is backed by the 177-test provider/discovery run.
- Full protocol/coordinator run: 173 tests passed, one pre-existing ignored test.
- Independent policy review found two further blockers: clock-advancing no-ops
  could hide an overdue freshness wake, and a new monitor could bypass a
  per-monitor quota latch. The engine is moving the latch to durable account
  observations, including observations received before monitor creation, and
  scheduling against each monitor's last reconciliation time. These fixes need
  fake-clock regressions and final review before any completion claim.
- Final broker run after those fixes: 86 tests passed, including no-op expiry
  notification, account barriers recorded before monitor creation, stop/reopen,
  new-goal attempts and full-pair cross-session reset recovery. The library and
  test target compile. CLI subprocess and consumer checks are running next.
- Independent final security review: Ready after fixing original Keychain
  allocation zeroization (including malformed bytes and whitespace). This is
  static and fake-adapter evidence; live ACLs/provider availability are untested.

## Final gate queue

- Implementation owners are frozen after mechanical lint refactors. Offline
  verifier owns sequential Cargo gates; security, rate and contract reviewers
  independently inspect the current code.
- Final audit found normal desktop refresh could still poll Claude accounts.
  The production executor now rejects every Claude probe before cache lookup
  or fallback rediscovery; there is no enabled OAuth opt-in command. Other
  provider refresh paths remain enabled. Fake adapter/coordinator coverage
  remains for typed failures and persisted cooldowns.
- Provider final run: 178 passed, scoped all-target Clippy clean. Broker test
  compile errors from import cleanup and a shadowed fixture helper were fixed
  before the final broker rerun.
- Broad E2E invocation accidentally attempted three Docker image pulls; exact
  registry request count is unknown. No Claude/provider/Keychain paths ran.
  The incident, failures and fake-only reruns are recorded in verification.md.
  Docker names are now unique and images cannot be implicitly pulled; Docker
  tests will not be rerun during this task.
- All local changes remain in the isolated branch. Only the first checkpoint
  was pushed; subsequent push attempts were refused by SSH-agent signing.
  No interactive authentication or unlock was attempted.
- Final binaries will be installed in the separate prefix
  `/Users/donbeave/.local/share/jackin-claude-monitor/bin`; the standard Jackin
  installation and running Claude session remain untouched.

## Implementation checkpoint

`29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` commits the coherent monitor,
CLI, broker ownership and consumer migration. Final scoped gates pass: broker
89; provider/auth/discovery 178; facade 68; publisher 15; protocol/coordinator
173 with one pre-existing ignored test; host runtime 6; FFI 6; relay 19; Capsule
relay authorization 13; offline CLI 2; lifecycle 2. Three exact fake-only E2Es
pass: twenty clients/one provider call, persisted retry deadline/one call, and
eight recovery clients/one replacement call (two total including killed owner).
Relevant all-target Clippy, workspace formatting and diff checks pass.

The cache telemetry regression recurred in parallel. Independent investigation
identified tracing-core 0.1.36's single-dispatch cache fast path using a worker's
thread-local default. The test fixture keeps a second live registry dispatcher
to remove that condition; it does not serialize tests or change production
instrumentation. Five parallel store runs and five parallel usage runs pass
with the original span-count/privacy assertions. The underlying dependency
fast-path defect is not patched by this monitoring change.

Fake owner recovery initially persisted an empty authoritative catalog after
removal of caller catalog injection. Its private fixture now seeds the intended
schema-2 catalog before startup. Production startup already reconciles its
broker-owned catalog; no production admission guard was weakened.

Final static security and rate-limit reviews are Ready. The native DTO/API audit
is coherent; generated Swift comments were synced without changing ABI. Final
build/installation and handoff verification are in progress. Push remains
blocked by SSH-agent signing; no interactive unlock is attempted.

## Final installation and handoff

Fresh build from `29db1854` is installed separately at
`/Users/donbeave/.local/share/jackin-claude-monitor/bin/{jackin,jackin-usage-broker}`
(version 0.6.4). Installed bytes match build artifacts; canonical cache paths and
SHA-256 hashes are recorded in verification.md. Installed service lifecycle,
doctor, monitor start/status/stop, JSONL watch and bounded wait were exercised
without a broker-path override, with zero credential trips and HTTP requests.
Operator setup and a reviewed statusline composition remain required; no running
Claude settings or auth state were changed.

The first installed smoke reported broker_unavailable and removed its fixture
before diagnostic logs could be retained. Its cause is unconfirmed. Subsequent
short-path proof and default macOS long-path alias proof pass with the same
binaries; the report preserves that uncertainty.

The ready-to-paste handoff is claude-code-handoff.md. The original checkout is
still clean. Latest local implementation is committed; push remains blocked by
SSH-agent signing refusal. Only the first contract checkpoint reached origin.


## Reopened acceptance audit

The renewed goal requires current proof against the complete objective. The
previous installation is historical evidence; new changes require fresh gates
and artifacts before completion.

| Work | Owner | State |
| --- | --- | --- |
| Retain lifetime lease lock, renew after sleep, close descriptor on exec, fence stale cleanup | broker_integration | Committed f669ec7; 101 broker tests and Clippy pass |
| Maintain lease before accepting requests; retry failed reset ticks with bounded deadlines | monitor_engine | Committed f669ec7; 101 broker tests and Clippy pass |
| Exercise production ticker with fake clock, idle and suspend/wake | monitor_engine | Passed in 101-test broker gate |
| Enforce terminal gate inside operator Keychain API; remove policy bypass | provider_research | Committed e5863815; 37 provider / 43 discovery / 7 helper tests pass |
| Prove action sequence restart and independent model/spend reset guards | policy_review | Passed in 101-test broker gate |
| Correct CLI help and local refresh documentation | parent / consumer_tests_research | Committed 6c1709e; help/usage gates pass |
| Independent review, offline gates, rebuild/install and new handoff evidence | reviewers / offline_verification / parent | Done; fresh install smoke passes with zero credential/HTTP trips |

No live authentication, provider checks, Claude settings or session mutations
are authorized by this audit.


Renewed audit complete: source `6c1709e4`, auth `e5863815`, lifecycle
`f669ec7`; broker 101, provider/discovery 80, helper 7, help 1, usage 30,
offline CLI 2, lifecycle 2, and three exact fake E2Es pass. Relevant Clippy,
formatting and diff checks pass; dependency future-incompatibility notice is
recorded. Fresh installed sibling binaries pass the reusable smoke with zero
credential trips/HTTP requests and orderly stop. Updated verification and
handoff record artifact hashes, conservative field freshness, the live hung
broker limitation and operator setup requirements. Remote push is still
blocked by SSH-agent signing; no interactive unlock is attempted.


## Remote publication update — 2026-10-10

The operator-requested push succeeded without interactive authentication.
All implementation and evidence commits through `3aeb8631` are published on
`origin/claude-unattended-monitor`. The earlier SSH-agent signing refusal is
historical; it no longer blocks publication. Both the isolated worktree and
original checkout were clean at verification.


## Legacy CLI incident — 2026-10-10

Claude invoked the original checkout's `target/debug/jackin usage host
projection`. Read-only provenance checks confirm that binary still exposes
the old host commands; its cache and hash differ from the installed monitor
pair, despite both reporting 0.6.4. Old projection requests a refresh. The
current monitor rejects this removed syntax. Passive doctor on the documented
installed state returns `broker_unavailable` (exit 3), a separate unmet local
service prerequisite. No service or authentication was started during diagnosis.

| Work | Owner | State |
| --- | --- | --- |
| Binary/source provenance and broker lookup diagnosis | incident_binary_provenance / incident_lookup_research | Confirmed old CLI and protocol/state lookup contract |
| Prominent installed-binary/capability and service prerequisite handoff | incident_contract_review | Done; independent broker-override review passes |
| Offline removed-command rejection / zero access regression | incident_offline_regression | Passed; offline CLI 4/4 |
| Independent review, scoped verification, commit and push | parent / reviewers | Done; implementation checkpoint 7bd6871d pushed to origin |

The original checkout and Claude settings/session remain untouched. No
compatibility alias, passive auto-start or forced refresh will be introduced.

The lookup review also found `broker_installation.rs` retained the old
expectation that bare usage starts a sibling broker. That test is being migrated
to explicit local-only `usage service start`, with passive-read assertions.
Production bare usage is already passive; no automatic start will be restored.
The first removed-command regression expected the wrong Clap error wording;
the fixture run supplied the actual parse error and the assertion was corrected.

Final incident gates: offline CLI 4/4; installation 4/4; app all-target
Clippy, workspace formatting and diff checks pass. Only the recorded dependency
future-incompatibility notice remains. Installed smoke passes with zero
credential/HTTP trips and orderly stop. No production service, original
checkout, Claude environment or settings were changed. Use the installed
capability-checked pair and explicit operator service setup; absent service
continues to report a stable failure rather than trigger provider work.


## Final prompt verification — 2026-10-10

Broad current offline gates passed: broker 101, provider/discovery 80,
protocol/coordinator 173 (one pre-existing ignored), CLI/installation 8.
Three focused persisted deadline filters also passed. Formatting/diff checks
passed; current source is unchanged, so previous Clippy proof is reused.

The installed composed hook was executed over fixture callbacks, preserving
existing output and leaving settings unchanged. It established runnable
monitors after synthetic SGD baselines, kept duplicate evidence age stable,
emitted 90/91/95% actions, and retained weekly exhaustion after lower five-hour
usage/reset. Public stop and bounded wait worked; credential/HTTP counts were
zero. Proof is verification-installed-hook.json.

Independent review exposed a real handoff error: routine Wait actions occur
while runnable=true and must be reevaluation hints, not automatic pauses.
The handoff/public docs are corrected; independent final review is Ready. The prompt also
needs explicit receipt recording before creation and must report prior
untracked task spend rather than silently treating it as zero. No real
account, credential, settings or provider was exercised or modified.

Final handoff review is Ready. The corrected instructions distinguish runnable
Wait hints from blocking Pause, permit the already-authorized fresh service
setup before adapter installation, preserve unknown existing service ownership,
record real SGD evidence before monitor creation and disclose pre-monitor
spend. No product code changed. The durable proof and handoff are committed
and published with this verification checkpoint.

## Observation and policy implementation — active 2026-10-10

Objective: implement the full attached tracker goal, keeping collection separate
from dispatch and preserving strict SGD semantics until explicit operator approval.
Current product state is not yet proof of completion. Work remains isolated in
`/private/tmp/jackin-claude-monitor`; live Claude settings, credentials and
projects are outside mutation scope.

| Work | Owner | State |
| --- | --- | --- |
| Protocol/CLI contract and migration freeze | v2_contract_research + parent | Frozen in v2-contract.md; protocol implementation active |
| Observer, goal policy, atomic baseline and idempotency | v2_engine_research | Engine + explicit migration implementation active |
| Official statusline/version/account source contract | v2_source_review | Research complete; bounded version parser patch prepared |
| Operator authorization, passive paths and rate-limit audit | v2_security_review | Research complete; host-only relay regression patch active |
| CLI integration | v2_cli_implementation | Frozen command implementation active |
| Existing broker regression migration | v2_verification_research | Fixture migration active; no tool activation |
| Offline CLI and installed fixture migration | v2_cli_fixture_migration | Implementation active; isolated state only |
| Deterministic tests + independent installed-pair proof | Parent + verifier to assign | Pending coordinated compile; fixtures only |
| Documentation, handoff, completion audit and publication | Parent + independent reviewers | Pending |

Acceptance includes observation without receipt/baseline/dispatch, auditable
persisted policy, separate readiness, atomic strict activation, no silent
downgrade, explicit account/session scope, schema migration, idempotent start,
idle/restart/sleep persistence and all existing provider/freshness/reset/spend
regressions. Real-account integration will be distinguished from fixture proof;
the operator-controlled adapter installation remains outside authorized edits.

Baseline on `cc4a0cb0`: `cargo test --offline --locked -p
jackin-usage-broker -p jackin-protocol -p jackin-usage-coordinator` passed 274
tests with one pre-existing ignored test. This is baseline evidence, not v2 proof.

Verification tooling incident: the read-only verification agent invoked
`mise exec --deny-net -- mbx --help`, which activated a missing configured
`cargo:codebook-lsp@0.3.42` tool and ran a global Cargo install with crates.io
traffic. The confirmed installer process tree was terminated; no Claude process
or project package build was stopped. Tool cache mutation and network traffic
occurred; exact external request count is unknown. This run is not wholly
offline. No provider, auth or Keychain verification occurred. Do not activate
Mise again; use direct offline Cargo commands. Future verification reports must
distinguish this tooling traffic from fixture provider/credential counters.

V2 intermediate checks: protocol passed 132 tests with one pre-existing ignored
test; Claude provider passed 37 tests, including changed/unchanged fake-source
401 behavior. Direct Cargo used `--offline --locked`. The combined provider/relay
run reached the unfinished broker migration and failed compilation; it is not
a passing gate. Engine ownership is addressing the reported borrow/type errors.
CLI and fixture migrations are prepared, pending integrated compilation.
The unchanged coordinator gate separately passed 44 tests (three suites),
including the persisted admission/deadline protections. This does not replace
integrated broker, CLI, installed-pair or migration verification.

Safe setup inspection: the selected user settings file has no `statusLine`
field. Effective project/session overrides were not inspected. The previous
installed pair remains unchanged and lacks the new observer/policy commands.
No real binding, receipt, adapter installation or callback has been verified.
The intended installation uses a separate v2 prefix so an unknown running
service continues using its existing binaries and state.

Independent V2 review found and implementation is addressing: allocator
counter collisions, invented migration timestamps, incompatible synthetic
binding IDs, malformed persisted spend anchors/currency, stale policy snapshot
handling on budget tightening, and dispatch without explicit approval of a
migrated policy. No final Ready verdict yet. Report-only reset/model validity
fields now separate descriptor audit from evidence age; unchanged values still
retain their evidence age. Latest protocol gate passed 134 tests, one ignored.
The provider-core/discovery/resolver gate passed 139 tests with 83 filtered cases;
fake Keychain evidence remains distinct from live macOS verification.

V2 integrated gate update: direct offline Cargo all-target CLI check passed.
Protocol now passes 135 cases (one existing ignored), relay 20. Broker passes
119/120; the outstanding test expects PolicyConflict for an unapproved
revision but the broker returns PolicyRequired. Fixture semantics are under
independent review. No final installed proof or completion claim yet.

Final core rerun passed 356 tests: protocol 135 (+1 ignored), broker 120,
relay 20, coordinator 44, Claude provider 37. Final independent integrity
review found four accepted gaps under repair: bound-session spend Watch
publication, unbounded evidence fingerprint retention, nested event evidence
sequence validation, and zero strict budgets accepted at approval. Installation
and lifecycle subprocess gates passed 5 and 2 tests; the main observation
subprocess workflow is investigating monitor_store_unavailable.

CLI unit gate found 212 passing cases and two telemetry migration regressions:
the command vocabulary and exhaustive fixture still reference removed usage
host snapshot syntax. A bounded agent is migrating these consumers to the
actual V2 command tree; no compatibility aliases will be restored.

Direct offline telemetry generation completed and Weaver local/vendored
registry validation passed. The full xtask returned failure after generation
on pre-existing legacy-namespace literals (e.g. jackin.role.toml in unrelated
image/manifest fixtures). Generated command enums and allowed-value tables
were updated from the authoritative registry. This gate is not claimed passing;
CLI command-tree regression tests remain the relevant migration proof.

Diagnostic rerun localized the workflow failure to strict activation at
usage_monitor_offline.rs:1080. Engine review confirms an implementation bug:
session-filtered bound guards reject sessionless account spend evidence in
state validation. The accepted relevance fix addresses this root condition;
the fixture expectation remains success and was not weakened.

Additional direct offline gates passed: credential resolver/discovery/provider
core 139 tests (83 inner filtered), usage facade/host runtime/output 75. Final
CLI/protocol/statusline/telemetry security review found no issues in its scope;
this was static review, not live credential verification. Read-only original
checkout status is still clean; old installed pair hashes match recorded V1
inspection. The new V2 prefix still does not exist before installation.
