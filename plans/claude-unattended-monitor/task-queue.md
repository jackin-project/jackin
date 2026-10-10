# Claude unattended usage monitor

## Bounded auth diagnostic follow-up on main

This narrow follow-up is being developed on isolated branch
`fix/auth-malformed-diagnostic-dco`, based on remote main
`3f3ddd7c48284b648b083305f969ee3e709b74b7`. It adds a
redacted structural diagnostic to `auth_malformed` only when all operator TTY
streams are attached. The operator command is
`jackin usage --data-dir PATH auth prepare --provider claude`; the TTY-only
`error.diagnostic` contains fixed JSON kinds, recognized camel/snake alias
presence, duplicate-alias booleans, access-token string/nonempty facts, and
payload/limit byte counts. It exposes no credential values, token length,
identifiers, snippets, or unknown keys. Payloads above 65,536 bytes skip JSON
classification. Expiry and account/provider readiness are not established by
this check.

The DCO-signed source commits `eb8f6b58c5fda32f437f7e9434edabaa1d47f0f8` and
`31dc0574a8d0a052e115fa4031740eef31664af5` are on the delivery branch.
Focused MBX check, scoped format check, fixture/parser/service/CLI/lease tests, and
independent parser and installed-pair review passed. MBX installed the debug
pair from source tree `ea7246252602ed897f0027ff448050219f191533` under the new
v4 prefix; the DCO-signed branch has the identical source tree. v2 and v3
prefixes remain unchanged. Exact commands, hashes, and fixture limits are in
[v4-installation.json](v4-installation.json) and
[verification.md](verification.md). The installed non-TTY gate returns
`interaction_required` without a diagnostic and does not call the broker.
No Keychain item, live credential, account identity/status, or provider
endpoint has been inspected; the real account remains unknown. No live
bootstrap or provider request was made.

### Claude metadata alias correction and v5 install

The operator's structural diagnostic exposed a parser bug: serde aliased
`subscriptionType`, `subscription_type`, `rateLimitTier`, and
`rate_limit_tier` onto one field, so two distinct metadata values were
misclassified as a duplicate and rejected as `auth_malformed`. Source commit
[`cc012603`](https://github.com/jackin-project/jackin/commit/cc012603951c46d3fe909c21538af4c5b0715948)
separates subscription type from rate limit tier, keeps subscription type as
the preferred label with rate limit tier as fallback, and marks diagnostics as
duplicate only for camel/snake spellings of the same field.

Focused MBX tests passed: 39 Claude tests, 3 broker foreground-bootstrap
tests, 1 fake foreground bootstrap lifecycle test, and 1 malformed-bootstrap
single-read test. The scoped MBX format check passed. The debug CLI/broker pair
was installed to the new v5 prefix from that exact signed commit. Independent
review verified the installed hashes and versions, reran the non-TTY gate with
a broker marker sentinel, confirmed the marker and data directory stayed
absent, and rechecked the unchanged v2/v3/v4 hashes. See
[v5-installation.json](v5-installation.json) for source/tree IDs, command,
binary hashes, preserved v2/v3/v4 hashes, and the non-TTY install sentinel.
No Keychain item, live credential, account identity/status, provider endpoint,
or settings was accessed or changed. The operator must retry their own
attended account bootstrap; live account readiness remains unknown.

## Current delivery checkpoint

Delivery branch: `feat/claude-usage-monitor-main`, isolated from the running Claude checkout. Signed source checkpoint `8abfa235cce150d5382d99a5679afab49e098525` is pushed. The final seven-gate MBX rerun, scoped regression matrix, 1.29-second offline pair build, and installed fixture smoke passed. Smoke exited 0, selected the sibling broker, recorded one JSONL watch event, zero proxy requests and credential-command trips, and left no fixture state or open binaries. CLI SHA-256 is `b57e60f2f13b026ae2ec47034b61ecddc69644b230157e621c5ddb5cf447007d`; broker SHA-256 is `30b6f3b1f0777dbe9181851f83fbc2efb3f0356d290e1896bf2ccaf434fbf550`; manifest SHA-256 is `64a0a101224ad5c38f4292a59c5191a44071fc54eab3cc95e8a57a13bc454601`. Source-head CI run [38044592706](https://github.com/jackin-project/jackin/actions/runs/38044592706) passed for 8abfa: Required, all Rust matrix jobs, and Actionlint succeeded; the baseline publication job was skipped by workflow rules. A later documentation/evidence commit requires its own exact-head checks. CI run `38042237548` and its three failed Rust jobs, and the 45a installed pair, are historical. P2 reply `4237302394` links the fixing commit and is resolved. P1 follow-up proof/reply `4237303703` is resolved; general comment `6096529404` has a linked disposition.

Focused checks and the final seven-gate MBX rerun passed: seven-package fmt/check/strict Clippy; S6 rotation (1) and full scope (7); FFI rank fix (11); docs Commands (5); V1 storage (1); broker (184); capsule diagnostic dedup (1) and proxy scope (11 passed, 1 filtered). Capsule used a canonical parent directory with mode 0700, a socket with mode 0600, and a bounded accept deadline. P1's private one-way-migration rejection has fresh evidence; its follow-up proof is posted and the thread resolved. P2's FFI rank fix resets raw fields before capping; it is included in 8abfa, and its commit-linked reply and thread are resolved. The V1 end-to-end regression is included. See [verification.md](verification.md) for log paths. Source-head CI for 8abfa passed; a later documentation/evidence commit requires its own exact-head checks. The current direct-CLI contract is [bootstrap-contract.md](bootstrap-contract.md); statusline remains an optional credential-free source.

Checkpoints `8288ef4a`, `22ac3581`, `45a33093`, and `14c09cd5` are historical evidence. Checkpoint `8abfa235` is the current signed/pushed source; its final seven-gate MBX verification, scoped regression matrix, build, installed fixture smoke, and source-head CI passed. The final documentation/evidence head requires its own checks before landing.

All Cargo work must run through MBX 1.22.0. Main's Mise Cargo wrapper selects MBX, but injecting the toolchain directory before `mise exec` can shadow that wrapper. Current invocations use `MISE_AUTO_INSTALL=false mise exec -- mbx <Cargo-subcommand>` with an owned isolated `CARGO_HOME`. For commands that can launch nested Cargo, inject wrapper-first PATH **after** Mise using `MISE_AUTO_INSTALL=false mise exec -- env PATH="<mise-command-wrappers>/bin:<selected-rust-toolchain>/bin:$PATH" mbx ...`; setting PATH before Mise is insufficient. Do not invoke plain Cargo. Main already supplies MBX; draft/red PR #1120 is not a prerequisite and remains untouched.

Previously recorded pre-v8 offline gates: protocol 133, broker 173, coordinator 54, CLI 43, app 29; installation 5, lifecycle 2, bootstrap 2, monitor 5; docs 18 tests and 1,293 rendered routes. The prior installed v3 pair used wire v7 and passed isolated observation, structured-output, sibling selection and composed-statusline checks. Its proxy request and credential-command trip counts were zero; native Keychain calls were not instrumented and successful foreground auth was not exercised. These results are historical and do not validate the current v8 candidate. No real-account readiness claim follows.

The pre-v8 cleanup removed dormant Claude CLI diagnostic/parser paths and strengthened provider-call inventory detection for both direct calls and callbacks. Its recorded post-cleanup provider contract 2/2 and broker 173/173 results are historical. Current-source checks and installed-smoke checkpoint evidence are recorded in [verification.md](verification.md); exact-head CI for PR documentation checkpoint 56181d4 passed, while this docs correction needs its own checks and landing gates remain open. Remaining queue:

- [x] Historical checkpoints: Claude fake-auth/provider 25 and initial contract 8 passed; forbidden-command guard was extended and independently reviewed Ready within documented syntactic limits. The later contract run had 9 passing and 1 failing test before the 33-row inventory fixture correction; the corrected current run now passes 10/10. Latest bounded results are recorded in [verification.md](verification.md).
- [x] Pre-v8 checkpoint: FFI passive/consumer regression 9 passed through MBX; docs typecheck and 18 tests passed after published research updates. These results do not establish current wire-v8 behavior.
- [x] Verify current wire-v8 publication provider order against `HostSurfaceId::ALL`, account ordering by full display label with canonical-account-ID tie-break, and diagnostic-label lookup across the canonical surface inventory. The latest host-inventory scope passed 13 tests and the broker scope passed 183. See `/private/tmp/jackin-mbx-v4-final-host-tests.log` and `/private/tmp/jackin-mbx-v4-final-host-broker.log`.
- [x] Verify the 300-second attempt floor with fractional Retry-After under forced refresh, restart recovery, projection cadence, and active cooldown tombstones in loaded, lazy-loaded, and pending-before-purge states. Independent review confirmed the corrected tombstone assertion: non-pending-reset clears `started_at_epoch` while retaining `provider_invoked=1000` and `RetryAfter=5000`; restart at 1101 with a fresh 300-second floor chooses `max(1300, 1401)`. See `/private/tmp/jackin-mbx-v4-final-coordinator-tests2.log`, `/private/tmp/jackin-mbx-v4-final-coordinator-state-tests.log`, and `/private/tmp/jackin-mbx-v4-final-host-broker.log`.
- [x] Verify explicit `LocalSourceHandle` identity stays distinct from provider ID and stable-account identity through discovery, serialization, deduplication, and publication. Discovery passed 41 tests, host inventory passed 13, and the corrected 33-row contract baseline passed 10/10; see [verification.md](verification.md).
- [x] Reconcile the lifetime leader lock from predecessor `f669ec77`: ownership stays held on the exact inode across sleep, lock descriptors use `O_CLOEXEC`, and dead-owner takeover, wake/renewal, and owned-path cleanup regressions pass in the latest 183-test broker scope. See `/private/tmp/jackin-mbx-v4-final-host-broker.log`.
- [x] Remove the unused public `HostUsageRuntime` and all references while retaining active `HostUsageProjectionRuntime`; reference review and the latest seven-package source check passed. See `/private/tmp/jackin-mbx-v4-final-source-check.log`.
- [x] Verify catalog-diagnostic incremental publication, authoritative clean-scan clearing, revocation labels, and bounded independent reviews. The latest broker scope passed 183 tests, including catalog-diagnostic publication regressions; see `/private/tmp/jackin-mbx-v4-final-host-broker.log`.
- [x] Close the scoped security lifecycle gate: each initial HTTP request, credential reread, and retry requires a generation permit; deactivation blocks new permits, already-admitted I/O may finish, and cache mutation is serialized against the exact generation. The 28-test Claude suite and independent review cover the stop/revoke races. Static review and fake fixtures do not constitute native Keychain runtime proof; see [verification.md](verification.md).
- [x] Complete the current consumer/integration matrix: runtime, CLI, app, console, FFI, capsule, bootstrap, monitor, broker lifecycle, and broker installation scopes passed; see [verification.md](verification.md).
- [x] Record final scoped formatting and strict Clippy across all targets; see [verification.md](verification.md).
- [x] Build and install the MBX pair for checkpoint `22ac3581a70310b9792f03d71168ee8bdd799591`; isolated fixture smoke passed with exact installed hashes, selected sibling broker, zero proxy requests, and zero credential-command executions. This is checkpoint evidence; see [verification.md](verification.md).
- [x] Build/install and pass the isolated fixture smoke for checkpoint `45a33093df524c03440ed524e71375953ee6834b`. It selected the installed sibling broker with `JACKIN_USAGE_BROKER_BIN` unset; proxy requests and credential-command trips were zero; fixture state was empty and no service process/open files remained after stop. See [verification.md](verification.md) for CLI, broker, and manifest hashes.
- [x] Verify the frozen cfg fix through seven-package format, source check, strict Clippy, and affected Claude (28), broker (183), and discovery (41) suites. See [verification.md](verification.md).
- [x] Record signed, pushed source checkpoint `45a33093df524c03440ed524e71375953ee6834b` for the cfg fix and build it through MBX; the build passed in 15.93 seconds. The earlier 22ac pair remains historical and does not cover this checkpoint.
- [x] Resolve P1 with an evidence-backed rejection: the migration is private and one-way. Reply posted; thread resolved.
- [x] Final seven-gate MBX rerun passed. Seven-package fmt/check/strict Clippy and the focused regressions are recorded in [verification.md](verification.md); the capsule rerun covered diagnostic dedup and proxy scopes.
- [x] Accept P2 and verify the FFI rank fix (11 passed); raw fields reset before capping. The fix is in 8abfa; commit-linked reply `4237302394` was posted and the thread resolved.
- [x] Add the V1 end-to-end migration regression; its focused storage case passed 1 test.
- [x] Finish the final seven-gate MBX rerun for frozen fixes. Historical CI run `38042237548` failed three Rust jobs (`jackin`, `jackin-runtime`, and `jackin-capsule`) on head 14c; source-head CI rerun `38044592706` passed for 8abfa. PR documentation/evidence head 56181d4 passed exact-head run `38045612792`; this correction's new head requires its own checks.
- [x] Commit and push source checkpoint `8abfa235cce150d5382d99a5679afab49e098525` with the accepted P2 fix; final seven-gate MBX checks, scoped regression matrix, offline pair build, and installed smoke passed. Exact hashes and manifest are recorded in [verification.md](verification.md).
- [x] Complete fresh installed provenance/smoke for `8abfa235`; fixture smoke passed with one JSONL watch event, zero proxy/credential trips, and cleanup. P2 reply `4237302394` links the fixing commit and is resolved; P1 follow-up proof/reply `4237303703` is resolved; general comment `6096529404` has a linked disposition.
- [ ] Pass all required checks at the final documentation/evidence PR head, complete exact-head feedback and main-up-to-date review, and land.
- [x] Refresh and freeze historical checkpoint 22ac provenance, checks, installed smoke, and handoff evidence; see [verification.md](verification.md).
- [x] Push the reviewed documentation/evidence checkpoint `56181d4d`; exact-head run `38045612792` passed. The correction in this commit requires its own checks, tracked in the landing item above.
- [ ] Operator-controlled real credential/account/provider verification remains outside offline acceptance. Do not modify running Claude settings, credentials, environment or worktrees.

Observation alone never creates a spend baseline or grants dispatch. Collection opt-in is separate from dispatch policy approval. Strict SGD remains fail-closed; quota-only requires explicit audited approval and discloses unknown spend/disabled SGD enforcement. External prerequisites mean checkpoint/report, not endless diagnostics or automatic resumption claims.

## Historical branch audit checkpoint

The following audit notes are earlier checkpoint records; consult [branch-consolidation.md](branch-consolidation.md) and [verification.md](verification.md) for refreshed evidence.

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
  source checkpoint `c18cdbf` and need refresh. The semantic inventory spans
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

The new pair-command integration session, bootstrap fixture disposition,
current CLI/auth compilation, branch-proof review, installed-fixture
verification, and remaining Required-review dispositions remain pending; the
broker and protocol runs passed. The
telemetry gate reports eight unchanged baseline literals in
`/private/tmp/jackin-main-telemetry-gate.log`; these findings have no broad
fix or waiver. Any canonical tag for the consolidated port remains pending
until that port is verified and its ownership/reconciliation review is
complete.

### Historical branch-port queue

The checkboxes below preserve statuses from that earlier branch audit. They
are not the current delivery checklist; use the queue under “Current delivery
checkpoint” above for present status.

- [done] Confirm the local Cargo wrapper path and MBX 1.22.0 route; PR #1120
  is not needed to enable MBX on main and remains open/draft with failed
  required checks. See [mbx-prerequisite.md](mbx-prerequisite.md).
- [done] Record scoped MBX tests: Claude usage 23/0, coordinator 54/0,
  consumer compilation run 5 passed, privacy CLI 43 passed, app suite 29
  passed, consent/429 tests 25 passed, protocol 133 passed, and broker final 7
  173 passed. The consent/429 run used a changed-service compile input; none
  of these scoped gates substitutes for final-pair, integration, or install
  proof.
- [done] Capture MBX consumer compilation run 5: binary and library targets in
  `jackin`, `usage-ffi`, `runtime`, and `capsule` passed; see
  `/private/tmp/jackin-mbx-consumers-check-5.log`.
- [done] Follow-up fixes `22ed2576`, `f6c3c8b6`, and `a3ecdf62` are pushed,
  including first-success consent revocation and persisted-policy changes.
  Latest source commit `c18cdbf` awaits push confirmation from session `67152`.
- [done] Broker final 7 passed 173 tests, 0 failures, with no input-modified
  warnings; socket, legacy-format, and model-evidence tests passed. See
  `/private/tmp/jackin-mbx-broker-tests-final-7.log`.
- [done] Protocol suite passed 133 tests through MBX; see
  `/private/tmp/jackin-mbx-protocol-final.log`.
- [ ] Capture integration session `41433` after the new pair commands finish.
  Earlier broker installation passed 5/5 and lifecycle passed 2/2; the
  bootstrap stale expected-unmapped failure still needs disposition.
- [ ] Complete branch-proof review; source reviews for privacy, rate, policy,
  and socket behavior are Ready only within their stated limits.
- [done] Scoped CLI privacy suite (43/0), app suite (29/0), and consent/429
  tests (25/0) passed through MBX. The consent/429 compile used a changed
  service; auth source was unchanged after that run, but this is not final-pair
  or installed-artifact proof.
- [ ] Verify the current CLI/auth test compilation against the latest source
  commit and record its exact command and result; scoped passes above do not
  establish a final paired build.
- [done] Regenerate MDX and verify docs: 18 tests, 1,293 HTML pages,
  and HTML/hydrated rendering passed; screenshot paths are recorded above.
- [done] Complete source reviews for privacy/rate/policy/socket behavior; the
  Ready verdict is bounded by the reviewers' stated limits. Branch-proof
  review and verification of final integration behavior remain pending.
- [ ] Record disposition of the bounded source-only Rust finding and re-fetch
  reviews, comments, replies, outdated/unresolved threads, and checks at PR
  #1120's current head. It remains open/draft with Plan and Required failing.
- [ ] Do not merge PR #1120 merely to enable MBX. Consider it only if a
  separate integration audit requires its changes, and then only after all
  review findings and required checks are resolved and verified.
- [ ] Integrate the usage candidate with the resulting main branch and pass
  the required main-integration gates before the usage PR can land.
- [x] Historical predecessor milestone: the v7 protocol contract was frozen
  independently of callback input v2. Checkpoint `fa7f2f0e` is pushed; 129
  offline protocol tests passed. See [protocol-v7-verification.md](protocol-v7-verification.md).
  This is v7 evidence only; it does not verify the current v8 candidate.
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
- [ ] Finish and verify the in-flight V1/V2/V3-to-V4 migration. Source review
  is ready after source validation and nested-schema malformed-input and
  unchanged-bytes negative checks; still verify runtime behavior while
  preserving existing goals, strict policy, baselines, spend history,
  action/event sequences, evidence ages, cooldowns, and unknown/latched state
  while leaving new mappings/opt-ins empty.
- [ ] Complete remaining independent security/rate/contract reviews and
  offline fake coverage; verify actual help, JSON, and exit behavior from a
  fresh build.
- [ ] Build/install the exact local artifact and run its isolated fixture.
  Update handoff evidence only from those results; do not perform live account
  or provider checks under this task.
- [ ] Reconcile docs and review the final main-port diff. Prepare/refresh the
  usage PR only after the source, review, test, install, and main-integration
  gates are recorded; re-fetch comments, threads, and required checks before
  landing. Do not mark the PR complete before that evidence exists.
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

## Historical predecessor checks

- Historical direct-Cargo check `cargo check --offline -p jackin-protocol`:
  passed.
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

## Historical predecessor final gate queue

- At that checkpoint, implementation owners were frozen after mechanical lint
  refactors. The offline verifier owned sequential Cargo gates; security, rate
  and contract reviewers independently inspected that source snapshot.
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

## Historical predecessor observation and policy implementation — 2026-10-10

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

Historical pre-MBX baseline on `cc4a0cb0`: the direct-Cargo command
`cargo test --offline --locked -p jackin-usage-broker -p jackin-protocol -p
jackin-usage-coordinator` passed 274 tests with one pre-existing ignored test.
This is baseline evidence, not v2 proof or current run guidance.

Verification tooling incident: the read-only verification agent invoked
`mise exec --deny-net -- mbx --help`, which activated a missing configured
`cargo:codebook-lsp@0.3.42` tool and ran a global Cargo install with crates.io
traffic. The confirmed installer process tree was terminated; no Claude process
or project package build was stopped. Tool cache mutation and network traffic
occurred; exact external request count is unknown. This run is not wholly
offline. No provider, auth or Keychain verification occurred. As an
incident-specific workaround, that historical verification run avoided Mise
and used direct offline Cargo. Do not reuse that workaround as a run
instruction; the current MBX contract at the top of this file supersedes it
and disables Mise automatic installation on every invocation.
Future verification reports must distinguish this tooling traffic from
fixture provider/credential counters.

Historical V2 intermediate checks: protocol passed 132 tests with one
pre-existing ignored test; Claude provider passed 37 tests, including changed/unchanged fake-source
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

Historical V2 integrated-gate record: a direct offline Cargo all-target CLI
check passed; this is not a current run instruction or main-port gate claim.
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
