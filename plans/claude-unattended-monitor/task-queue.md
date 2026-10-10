# Claude unattended usage monitor

## Current delivery status — direct CLI priority

The current user priority is an attended, direct CLI path: foreground Claude
credential bootstrap and an explicitly opted-in account collector. The revised
contract is [bootstrap-contract.md](bootstrap-contract.md). It supersedes the
older statusline-observer-only plan as the implementation target; statusline
ingress remains optional and passive.

For branch/file accounting, see the [branch consolidation audit](#branch-consolidation-audit).

- The current worktree is branch `feat/claude-usage-monitor-main`, based on
  main `868ce535`, at canonical usage candidate HEAD `253c7cbc`; it is an
  unverified port candidate, not a completed PR.
- The port is incomplete. Agents report implementation changes for the
  foreground bootstrap, collector path, and related safeguards. Focused
  Claude usage and coordinator tests have passed, but the full CLI/consumer
  compile and installed-fixture gates remain pending; the latest broker run
  still has three horizon/migration failures. The V1/V2/V3-to-V4 migration
  engine is in flight. Treat reported code changes as unverified until the
  migration and remaining gates pass.
- Version contract: broker wire v7 and normalized statusline input v2 remain
  unchanged; the candidate durable monitor schema is v4 for the historical
  billing-correction horizon. At canonical HEAD `253c7cbc`, the protocol gate
  has 133 passing tests. The latest broker run had 153 passes and 3 failures in
  horizon/migration cases; a fix agent is working on those cases, so rerun and
  migration verification remain pending. The full CLI/consumer compile and
  installed-fixture gates have not passed.
- The read-only MBX audit confirms that main already routes Mise-managed Cargo
  through MBX 1.22.0; details and source links are in
  [mbx-prerequisite.md](mbx-prerequisite.md). The isolated Claude usage filter
  completed 23 passed, 0 failed through MBX, with fixture-only coverage; this
  is not native Keychain or whole-workspace Rust proof. The coordinator filter
  completed 54 passed, 0 failed through MBX; consumer compilation is still
  running as process `80935` (log
  `/private/tmp/jackin-mbx-consumers-check.log`). These gates do not establish
  whole-workspace Rust readiness.
- The read-only PR #1120 audit found it open and draft at `9ef617d`; Actionlint
  and DCO pass, Plan and Required fail, and 28 checks are skipped. Independent
  Rust source review retained one bounded candidate finding, with no runtime
  proof; its disposition and a current-head review remain pending (see the
  [MBX and PR audit](mbx-prerequisite.md)). PR #1120 is not needed to enable
  MBX on main; no merge has happened or been verified. Do not claim root Rust
  readiness.
- The candidate source records `historical_correction_horizon_epoch` when a
  verified closed-period billing correction is older than the two retained
  account periods and cannot be applied. The correction remains
  audit-only/unverified. This horizon is rollback protection: goals whose
  baseline predates it retain their known cumulative-spend estimate but latch
  `rollover_unknown` and incomplete cumulative spend; newer receipts do not
  clear the affected goal's latch. A goal baselined after the horizon remains
  independently evaluated. Broker rerun and migration verification are still
  required for proof.
- Earlier statements that `auth prepare` only discarded its credential and
  that the local split disabled collection describe a previous snapshot, not
  the reported current implementation. Direct usage is still unverified.
- CLI-port's planned commands are `usage auth prepare --provider claude
  [--keychain-service SERVICE] [--data-dir PATH]`, binding confirmation with
  `--provider-account ... --approve-experimental-collector`, and `usage
  monitor observe ... --experimental-collector`. The binding flag records
  explicit, audited, default-false approval on that binding revision;
  `observe --experimental-collector` only consumes an already-approved
  mapping. Neither flag grants dispatch consent, and an unattended caller
  cannot approve collection. These are agent-reported planned shapes, not yet
  verified by the main-port binary; confirm actual help and output before
  updating operator instructions.
- The new path requires all-stdio TTY gates, a foreground broker lease before
  Keychain access, one exact service in a bounded zeroizing in-memory cache,
  and an explicit account mapping/collector opt-in. Auth preparation itself
  makes no provider request. The collector targets an undocumented endpoint
  with an honest Jackin user agent; make no provider-support claim.
- Observation does not create a goal, spend receipt, or dispatch policy and
  never makes a monitor runnable. Strict spend guards remain intact; any
  quota-only dispatch approval is separate and acknowledges no SGD cap.
- No real-account callback, native Keychain read, provider request, operator
  setup, or installed proof is available for this port. Baseline port review,
  independent reviews, offline gates, fresh install/fixture proof, and PR work
  remain pending; the PR is not complete.
- The predecessor `operator-unblock.md` was committed and pushed at
  `771bb088`; the earlier “untracked” description is obsolete. Its archived
  copy here is [historical-installed-v2-workaround.md](historical-installed-v2-workaround.md).
  The predecessor branch remains preserved and is tagged
  `archive/claude-unattended-monitor-2026-10-10`, peeled to
  `771bb088dd93451654f992aeef1b8976ad84ec85` (tag object
  `21a123dcca0b2ea65eb5a9ae771bfdc456e75969`). This archive tag preserves the
  predecessor; it does not mark the main-port candidate as verified. Do not
  treat the predecessor observer-only workflow or fixture evidence as
  direct-collector proof, or copy it into this port's verification record.

### Branch consolidation audit

`feat/claude-usage-monitor-main` is the single intended delivery branch. Keep
`claude-unattended-monitor` preserved and untouched until its changes and the
main-port agent work are fully inventoried, reconciled, and independently
reviewed. A direct read-only inventory found 28 usage commits on the
predecessor after `ff9eb01f`, spanning 232 files (+36,856/-11,218). The older
25-commit/219-file counts are stale checkpoint figures from before commits
`a9001732`, `427c104b`, and `771bb088`. The separate PR #1121 dependency
contains 203 unrelated commits; it is not a prerequisite for landing the
main-port usage work. Exclude that unrelated history from the usage-port
inventory. The earlier inventory snapshot found a dirty 59 tracked-file port
plus untracked split modules, tests, and docs; those worktree counts predate
canonical HEAD `253c7cbc` and need refresh. The semantic inventory spans
protocol, coordinator, monitor/broker, Claude auth/provider, CLI/broker binary
and integration tests, runtime/FFI/telemetry, and usage docs. The full
file-level reconciliation is still pending, so consolidation is not complete.
Do not cherry-pick the predecessor history wholesale. Account for each area,
its tests, and its evidence on the single candidate before closing this item.

The old branch visibly contains the closed-period spend anchor,
invocation-time floor, and strict budget guard/test changes, but their main-port
counterparts have not been independently verified. The predecessor's separate
`landing-queue.md` is absent from this port; it records accepted fixes and
branch/PR evidence but also contains outdated broad landing instructions. If
needed, extract only relevant fix evidence into the current verification notes
after checking it against the candidate. Keep the old branch intact during
that accounting.

An earlier security review of a previous snapshot found ordinary token copies
that were not zeroized, an incomplete exact-source 401 reread, possible
credential fallback after a selected-source cache miss, incomplete service
validation, an unwired collector approval gate, and a Claude Code user agent.
Port agents report corrective changes for those findings in the current
worktree. They remain historical findings pending compile, focused offline
tests, and independent re-review; do not describe them as confirmed current
defects or as verified fixes.

Full CLI/consumer compile verification remains pending. The telemetry gate
reports eight unchanged baseline literals in
`/private/tmp/jackin-main-telemetry-gate.log`; these findings have no broad
fix or waiver. Any canonical tag for the consolidated port remains pending
until that port is verified and its ownership/reconciliation review is
complete.

### Current queue

- [done] Complete read-only MBX setup and PR #1120 status audits; see
  [mbx-prerequisite.md](mbx-prerequisite.md). Existing main MBX setup works
  independently of PR #1120. Isolated Claude and coordinator tests passed
  23/0 and 54/0 respectively; consumer compilation is still in flight.
- [ ] Capture and review consumer compilation process `80935` at
  `/private/tmp/jackin-mbx-consumers-check.log` after it exits.
- [ ] Record disposition of the bounded source-only Rust finding and re-fetch
  reviews, comments, replies, outdated/unresolved threads, and checks at PR
  #1120's current head. It remains open/draft with Plan and Required failing.
- [ ] Do not merge PR #1120 merely to enable MBX. Consider it only if a
  separate integration audit requires its changes, and then only after all
  review findings and required checks are resolved and verified.
- [ ] Integrate the usage candidate with the resulting main branch and pass
  the required main-integration gates before the usage PR can land.
- [done] Freeze the v7 protocol contract independently of callback input v2.
  Checkpoint `fa7f2f0e` is pushed; 129 offline protocol tests passed. See
  [protocol-v7-verification.md](protocol-v7-verification.md). Broker, CLI, and
  installed workflow proof remain pending.
- [ ] Verify the reported baseline port and reconcile any remaining
  independent-review findings before treating the current source tree as a
  candidate.
- [ ] Verify foreground auth ownership, exact-service zeroizing cache,
  lease/TTY/no-UI ordering, conflict behavior, and cache cleanup.
- [ ] Verify the confirmed provider-account mapping and default-off
  `--experimental-collector`, including explicit binding-level approval and
  presence in the local split artifact.
- [ ] Verify that only the selected scope uses persisted rate limits, an
  honest user agent, and the reviewed same-source 401 behavior.
- [ ] Finish and verify the in-flight V1/V2/V3-to-V4 migration, preserving
  existing goals, strict policy, baselines, spend history, action/event
  sequences, evidence ages, cooldowns, and unknown/latched state while leaving
  new mappings/opt-ins empty.
- [ ] Complete independent security/rate/contract reviews and offline fake
  coverage; verify actual help, JSON, and exit behavior from a fresh build.
- [ ] Build/install the exact local artifact and run its isolated fixture.
  Update handoff evidence only from those results; do not perform live account
  or provider checks under this task.
- [ ] Reconcile docs and review the final main-port diff. Do not mark the PR
  complete until the actual review, gates, and installed proof are recorded.
- [ ] Finish the file/commit-level reconciliation of predecessor and agent
  work; review the single-branch result before considering the preserved
  predecessor branch for any cleanup.

## Historical predecessor progress — 2026-10-10

- The usage command page and this feature's planning/evidence files are being
  ported onto main at `868ce535`; unrelated rollout documentation is excluded.
- Three independent-review fixes remain pending. The current main-port source
  has not been built or fixture-smoked as an installed pair; no current-port
  verification or readiness claim is available.
- The installed pair built from predecessor source `4492d3cb` passed its
  synthetic fixture twice. Keep that result as historical predecessor evidence;
  it does not validate the main-port source.
- No real-account callback, operator setup, native Keychain, or live provider
  check is verified. Rebuild and rerun the private fixture after review fixes
  are addressed before updating this status.
- The queue and checkpoints below record predecessor-branch implementation
  history; their completed states do not mean the main-port candidate is ready.

## Safety boundary

Work starts read-only. Implementation and verification use isolated state and
fixtures. Do not access live provider endpoints or Keychain, invoke Claude,
modify Claude auth/settings, kill sessions, or mutate their worktrees.
All delegated work uses GPT-6-Luna at max reasoning.

## Predecessor implementation queue (historical)

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

Predecessor implementation checkout: `/tmp/jackin-claude-monitor` (branch
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

## Observation and policy implementation — verified 2026-10-10

Objective: implement the full attached tracker goal, keeping collection separate
from dispatch and preserving strict SGD semantics until explicit operator approval.
Source and installed fixture proofs are recorded below. Work stayed isolated in
`/private/tmp/jackin-claude-monitor`; live Claude settings, credentials and
projects are outside mutation scope.

| Work | Owner | State |
| --- | --- | --- |
| Protocol/CLI contract and migration freeze | v2_contract_research + parent | Implemented; wire v6 / store schema 2; protocol gate passed |
| Observer, goal policy, atomic baseline and idempotency | v2_engine_research | Implemented and independently reviewed; broker 128 tests passed |
| Official statusline/version/account source contract | v2_source_review | Official research and bounded version parser implemented; v2.1.80 floor |
| Operator authorization, passive paths and rate-limit audit | v2_security_review | Security/rate reviews complete; relay gate passed; no live verification |
| CLI integration | v2_cli_implementation | Implemented; 216 CLI unit tests passed |
| Existing broker regression migration | v2_verification_research | Migrated fixtures; core/consumer gate passed 439 cases |
| Offline CLI and installed fixture migration | v2_cli_fixture_migration | Complete; 12 subprocess cases and installed workflow passed |
| Deterministic tests + independent installed-pair proof | Parent + v2_installation_review | Complete; actual pair hashes, broker PID/argv and installed workflow verified |
| Documentation, handoff, completion audit and publication | Parent + independent reviewers | Complete; installed docs/proof recorded and publication checkpoint prepared |

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

Expanded core suite passed 362 tests (broker 126), CLI unit suite 216, and
all 12 installation/lifecycle/observation subprocess tests passed. Session-bound
strict activation now succeeds with valid fixture receipt and evidence. Final
review confirmed the five accepted repairs; it identified one V1 edge still
being repaired: migrated zero-budget strict records need an explicit operator
positive-budget correction without clearing history or unknown baseline.
Clippy is running before the implementation checkpoint.

Clippy found protocol documentation/assertion style issues (fixed), then 19
broker/parser structural/style issues. Engine owner is splitting validation,
start and readiness phases and replacing long argument lists with contexts;
parser owner is naming the version tuple type. Passing tests precede this
refactor and must be rerun. New migrated-zero regression is being aligned with
the existing-goal contract: blocked status preserves uncertainty; only a new
strict goal lacking a compatible baseline rejects admission atomically.

Final consumer audit found no active consumers of removed host projection or
snapshot syntax, no compatibility aliases, and no independent Swift wire/schema
pin. Current Rust consumers use shared wire v6; monitor store v1 exists only
as explicit migration input. Historical blocked roadmap notes were not treated
as shipped/current behavior. This was a static read-only audit.

Engine refactor is frozen and broker/protocol Clippy now passes; remaining
Clippy findings are CLI branch/test style and telemetry fixture size, assigned
to their owners. Core suite is rerunning after the refactor and migrated-zero
repair. Installed-smoke now verifies the exact fixture broker executable and
launch arguments from its own PID lease plus ps, not version-string matching.

All-target Clippy passed with -D warnings for jackin, broker, protocol, relay
and Claude provider. Refactored core rerun passed 126 broker tests but one
fake HTTP server panicked on a macOS WouldBlock socket read before the second
request; the fixture owner is making bounded complete-header reads robust,
without changing admission policy or provider-count assertions. No final core
pass is claimed for that run.

Final source gates: core/consumer suite passed 439 tests (+1 pre-existing
ignored); CLI unit suite passed 216; app installation/lifecycle/observation
subprocess suite passed all 12. Formatting passes on 29 changed Rust files.
Independent final integrity review has no outstanding finding after the repairs.
Final Clippy and repeated Retry-After regression run precede implementation
commit/build/install. Real-account setup and genuine callbacks are still unverified.

Final all-target Clippy passed with -D warnings after the HTTP fixture style
repairs. Broker unit rerun passed all 128 cases; process/doc completion and
five repeated Retry-After fixture runs are the final pre-commit gate.

Broker full gate completed successfully. Retry-After/reset fake HTTP fixtures
passed five consecutive runs (2 tests per run); request-count assertions held.
All 29 changed Rust files pass scoped rustfmt and git diff --check. The source
implementation is ready for its coherent checkpoint; installed proof follows.

Implementation 4492d3cb91d9e45fafec2aa8acb8158268618cd9 is committed and
pushed. A direct offline build from that clean source installed both binaries
into a new exclusive jackin-claude-monitor-v2 prefix; old pair/state unchanged.
SHA-256/source-tree provenance is recorded in v2-installation.json. Installed
fixture execution is pending; independent handoff review corrected doctor path
and requested report-only reset/model assertions before that run.

Final installed workflow passed twice using the exact new pair and independent
private state. Both runs proved preserved composed output, no receipt/baseline
for observers, synthetic attended binding/policy approval, approved quota-only
readiness, idempotent starts, status/watch/bounded wait, independent reset/model
metadata, exact sibling broker selection, and orderly owned service cleanup.
Credential PATH tripwires and HTTP proxy counts were zero; these are not
OS-enforced native Keychain/egress counters. Fake Keychain coverage and static
routing review support the no-dialog design; no native Keychain check occurred.
The final transcript is bound to a pre/post-unchanged script SHA in the manifest.
Independent installed review verified both binary hashes and observed schema/exits.

External real-account prerequisites remain intentionally unperformed: operator
reviews/applies a composed statusline proposal outside this session, establishes
a genuine session or confirmed local binding, and supplies a real callback.
Observation requires no SGD receipt. Dispatch needs a separately approved policy;
strict first activation still needs fresh compatible SGD billing evidence. No
existing Claude goal has quota-only approval, and no strict policy can downgrade.
On that external blocker Claude should checkpoint, report and exit its goal;
there is no unattended doctor loop or promised automatic reinvocation.

Final handoff review requested two documentation repairs: scope unknown-spend
pause explicitly to strict-SGD, and record both pre/post smoke script hashes.
Both are fixed and installed copies synchronized. Source401 evidence is described
as zero implicit rereads/retries with later caller-supplied changed credentials,
not an automatic provider retry. Final old pair hashes match the starting
inspection; original checkout is clean; real V2 state remains absent, so no
synthetic approval/baseline was installed for the actual Claude goal.

Handoff reverification: source remains unchanged since binary build4492d3cb;
installed binary hashes and all installed docs matched. Independent Luna max
review found no blocker but caught quoted-tilde illustrative settings paths.
Those two printed examples now use the actual absolute path; no settings were
read or changed. Corrected installed smoke passed again with zero credential
tripwire/proxy counts and owned cleanup. Manifest/log/schema examples updated
from this fresh run; real-account prerequisites and dispatch approval remain
unverified. The correction and evidence are committed/pushed as one checkpoint.
