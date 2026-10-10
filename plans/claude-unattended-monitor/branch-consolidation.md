# Branch consolidation inventory

Audited 2026-10-10. Static inventory; no source implementation, branch, or Git
ref was changed for this record.

## Current supplemental review

The independent 28-commit source audit covered the groups below against source
tip `771bb088` and canonical target
`45a33093df524c03440ed524e71375953ee6834b`. This signed source checkpoint is
pushed and includes the Linux configuration fix. A clean MBX build completed
in 15.93 seconds. The final installed fixture passed; binary hashes and
provenance are recorded in `v3-installation.json` and `v3-installed-smoke.log`.
Static review found the lifetime lease, legacy-runtime removal, and
catalog-diagnostic publication behavior in source and fixtures. Critical
source reviews are Ready within their stated scopes, and the recorded local
source and consumer gates are green. The latest bounded source gates in
[verification.md](verification.md) include 57 coordinator tests, 11
coordinator-state tests, and 28 Claude provider/lease lifecycle tests,
alongside the selected consumer, integration, formatting, and Clippy scopes.
An intermediate Linux CI poll for source head `45a33093` had 22 checks green and 6 pending; the run continued after that snapshot. CI completion and landing
remain open; this record makes no completion, merge, or release-readiness
claim.

| Source group | Current disposition |
| --- | --- |
| `e5ee84f`, `4492d3c` | Source observation/dispatch semantics are re-expressed in current wire v8, projection v3, and store v4 contracts, with explicit migration; source wire v7 is historical, not the target version. Observation never grants dispatch. |
| `2031b55`, `427c104` | Invocation floors, catalog cooldowns, the SGD45 readiness stop, closed-period corrections, and historical uncertainty map to current coordinator/spend code and fixtures; latest bounded coordinator and spend scopes are recorded in verification.md. |
| `2e984db`, `887a1b4`, `e586381` | Exact-source attended bootstrap, guarded Keychain reads, and removal of the Claude CLI fallback are represented with forbidden-route guards. No live Keychain proof. |
| `4c6ba90`, `013e5e6`, `d06582d` | Opaque typed diagnostics, atomic catalog publication, incremental diagnostic retention, and clean-scan clearing are represented in source and fixtures; the latest recorded broker scope includes these regressions. |
| `29db185`, `f669ec7` | The legacy public `HostUsageRuntime` path is removed; the projection runtime and active broker client remain. Lifetime lease ownership, wake renewal, stale-owner fencing, and dead-owner reclamation are represented in source and fixtures; the latest broker and Claude lifecycle scopes are recorded in verification.md. |
| `6c1709e`, `7bd6871` | Direct CLI monitor/status/watch/wait contracts are retained. The signed 45a MBX build and installed fixture passed, with hashes/provenance in the v3 installation and smoke records. Predecessor install evidence and the 8288 checkpoint are not final target proof. |
| Remaining documentation/design commits | Preserve dated v2 evidence as historical. Current wire v8, projection v3, store v4, and the bootstrap contract supersede predecessor command and schema claims. Do not import predecessor success claims. |

The archive tag still peels to source tip `771bb088dd93451654f992aeef1b8976ad84ec85`. There is one delivery branch, `feat/claude-usage-monitor-main`; no raw merge of the 203 unrelated Rust-policy commits is authorized or needed. The 45a installed fixture passed; Linux CI still has six pending checks, and branch/PR review and landing remain open. This inventory does not authorize branch retirement or merge. Gate and installation provenance are tracked in [verification.md](verification.md) and the v3 installation/smoke records. All Cargo verification uses the Mise-to-MBX wrapper.

## Earlier inventory snapshot

The remaining sections preserve the earlier inventory and its then-pending gates. They do not override the supplemental review or current verification record.

## Scope and branch facts

The intended delivery branch is feat/claude-usage-monitor-main, based on
common point 868ce53519234f879d26a8bd2dffaa30fae2d729. At this audit snapshot
its HEAD is 1a45196dbe24d439e596c14e22fbda59799e7b0d. The isolated
predecessor source tip is 771bb088dd93451654f992aeef1b8976ad84ec85, preserved
by archive/claude-unattended-monitor-2026-10-10. The local branch ref
claude-unattended-monitor remains stale at
abd2260e764595cc6fed32a89725439b6f3e7df1; the archive tag, not that branch
pointer, identifies the audited source tip.

The predecessor contains 203 commits from the common point through
ff9eb01f0b3e0155a483d39e6802e4ed14d078f6 for the unrelated Rust-policy
rollout associated with PR #1121. Exclude those commits and their tree changes
from the usage port. The usage-task inventory is the 28 commits in
ff9eb01f..771bb088. That range changes 232 files (+36,856/-11,218) in the
predecessor layout. Older landing notes report 25 commits / 219 files; those
figures predate commits a9001732, 427c104b, and 771bb088.

The target branch contains the v7 contract and later implementation commits,
but source/test and plan work remains uncommitted, including untracked
predecessor-workflow artifacts. A read-only inventory is not a committed
port. Do not infer that those artifacts are current instructions or
verification.

Do not use ff9eb01f as a tree-diff base against the monolithic target. That
cross-layout comparison reports 4,444 files and obscures the usage work. Use
the predecessor range above as the source inventory, then reconcile its
semantics and evidence against the target files below.

## Predecessor commits in scope

These are all commits reachable after ff9eb01f and through the audited
predecessor head, in source order.

| SHA | Subject |
|---|---|
| e5ee84f890126232b0d326e15d90ee9bfd475026 | feat(usage): define durable unattended monitor contract |
| 2031b55235d07983bd0c7f74e80fef7208f560e5 | fix(usage): persist Claude attempt floors through catalog changes |
| 2e984db97339d6c4b5abd9a789b1b0c736837dfd | fix(usage): prohibit unattended auth UI and Claude CLI fallback |
| 887a1b474e411c038c3a61de59a7855eca31abc2 | test(usage): verify guarded Keychain reads with offline adapters |
| 4c6ba9092bc52ae0592098ca4553dddc4b71c7c3 | feat(usage): publish typed discovery diagnostics atomically |
| 013e5e6e9f736e54e71ccf427d424cca5685a7c1 | test(usage): separate diagnostic lifecycle and clearing regressions |
| d06582d366ada07140e5e5db0e41b0449e57816a | feat(usage): retain opaque unresolved credential diagnostics |
| 29db1854cf694d3a3f9cdae23659ffe5ee5da3cc | feat(usage): add durable unattended Claude monitors and broker-owned consumers |
| 19ea3f4f320b75b70edae7e14718bcdae9665f50 | docs(usage): record verified monitor commands and Claude handoff |
| e58638156b7487de3d06af49b844892ec251ce80 | fix(usage): gate operator Keychain preparation at provider boundary |
| f669ec77383324650cf3075eeb6e0a9bacc526a8 | fix(usage): retain broker authority through sleep and bounded reset retries |
| 6c1709e4bea1db9ee05d56352f11440db165e4e7 | docs(usage): describe broker cache and durable monitor commands |
| 3aeb8631eac02eef3996de542102406b24aab314 | docs(usage): record renewed offline proof and installed monitor handoff |
| bfb1538b7a89e805204a0b2f616c0268d5c65126 | docs(usage): record successful implementation publication |
| 7bd6871d0b02adabf582818c1ba1bb6f7a6d7d2b | fix(usage): pin monitor handoff to verified CLI and broker pair |
| 37fd349ccc9a780d089236028fa7e87cab172177 | docs(usage): close legacy CLI incident checkpoint |
| 69cf4a7357308087369b3fb3044564c8d80380ef | fix(usage): clarify runnable waits and verify installed statusline setup |
| 2ad0ff6bd37db81984c7b3aa4ba88281e62e7e1b | docs(usage): design independent quota tracking and explicit budget policy |
| cc4a0cb03ef82d66e5b2cf7991f858bee613932b | docs(usage): checkpoint observer and policy implementation queue |
| ea4aa082e942b797331df0e323f4d2bfcb784b84 | docs(usage): freeze observation and operator policy contract |
| 590efd9f1b0c748c7e6fa1899fa408fc8eed2cb6 | docs(usage): record v2 setup inspection and intermediate gates |
| fba23394bb3d522ca0ff33af6db008fca78fbb16 | docs(usage): checkpoint v2 verification and review findings |
| 4492d3cb91d9e45fafec2aa8acb8158268618cd9 | feat(usage): separate observation from approved dispatch policies |
| 59a57d6682e65a8d46a834e2d069f5f72ad88499 | docs(usage): publish verified installed workflow and Claude handoff |
| abd2260e764595cc6fed32a89725439b6f3e7df1 | docs(usage): reverify installed handoff and fix settings examples |
| a9001732a0b46e98d880f5b47d994030a877f47b | docs(usage): checkpoint landing review and fixture limits |
| 427c104b98d0f087356e8cd6f2c643dcf5a1b873 | fix(usage): align budget guards and persist invocation floors |
| 771bb088dd93451654f992aeef1b8976ad84ec85 | docs(usage): preserve installed v2 observation workaround limits |

## Semantic mapping to the monolithic target

Every row is a source-to-target reconciliation scope, not a claim that the
target implementation is equivalent or complete. The target paths listed are
present in the dirty port or its current branch baseline; reconcile their
content, tests, and migration behavior against the source before closing the
row.

| Predecessor area and source paths | Main-port target paths | Reconciliation notes |
|---|---|---|
| Monitor protocol and broker wire: crates/core/jackin-protocol/src/usage_monitor.rs, usage_broker.rs; crates/services/jackin-usage-broker-wire/** | crates/jackin-protocol/src/usage_monitor.rs, usage_broker.rs | The target freezes protocol v7 and candidate durable monitor schema 4. Source monitor v2 / wire v6 behavior needs deliberate migration, not a file copy. Preserve explicit observation vs dispatch authority and fail-closed migration of policy, spend, evidence-age, and cooldown state. |
| Coordinator and durable policy state: crates/services/jackin-usage-coordinator/** | crates/jackin-usage/src/coordinator/** | Reconcile cadence, persisted attempt floors, policy decisions, restart behavior, catalog changes, state validation/sanitization, and migration. The source fix uses provider invocation time for the Claude floor. |
| Broker monitor, spend, publication, and service lifecycle: crates/services/jackin-usage-broker/**, jackin-usage-broker-publish/** | crates/jackin-usage/src/host/broker/**, including monitor/** and publish/** | Reconcile durable monitor state, ordered decisions, idempotency, bounded waits, quota latches, strict-budget readiness, late closed-period corrections, broker ownership, and publication timestamps. Source tests do not prove the target port. |
| Provider, credential resolution, discovery, and host runtime: crates/services/jackin-usage-provider-claude/**, jackin-usage-provider-core/**, jackin-usage-credential-resolver/**, jackin-usage-credential-snapshots/**, jackin-usage-discovery/**, jackin-usage-host-credentials/**, jackin-usage-host-runtime/**, jackin-usage/** | crates/jackin-usage/src/usage/claude/**, usage/refresh.rs, host/credential_resolver.rs, host/discovery.rs, host/projection*, host.rs | Preserve scoped account identity, typed opaque diagnostics, no unattended auth UI or active Claude CLI fallback, cooldowns, and no broad credential source switching. The new attended bootstrap contract deliberately narrows credential access to one selected service; verify the target's exact-source behavior and cache boundary. The dormant CLI helper was removed in the candidate; see the specific reconciliation record below. |
| CLI, broker executable, consumers, and end-to-end tests: crates/apps/jackin/src/cli/usage/**, bin/usage-broker/**, tests/**, console adapter | crates/jackin/src/cli/usage/**, crates/jackin/src/bin/usage-broker/**, crates/jackin/src/console/adapter/**, crates/jackin/tests/** | Reconcile monitor/status/watch/wait semantics, TTY confirmation gates, statusline ingress, broker selection and installation, lifecycle, offline observation, and exit/JSON schemas. The target direct-CLI command shapes remain unverified against a fresh binary. |
| Usage FFI and presentation: crates/adapters/jackin-usage-ffi/** | crates/jackin-usage-ffi/** | Reconcile DTO/version changes, typed presentation, bridge callers, and removal of predecessor-only discovery/route paths. |
| Capsule relay authorization: crates/apps/jackin-capsule/src/usage_relay_proxy/** | crates/jackin-capsule/src/usage_relay_proxy.rs and tests | Preserve relay-side rejection of unsupported or unauthorized monitor operations. |
| Runtime usage relay: crates/services/jackin-runtime-usage-relay/** | crates/jackin-runtime/src/usage_relay.rs and tests | Reconcile protocol payloads, broker ownership, and relay failure handling. |
| Telemetry schema and registry: crates/services/jackin-telemetry/** | crates/jackin-telemetry/** | Reconcile event/attribute/metric/span names and registry consistency with the v7 monitor contract. Current telemetry gate findings remain open. |
| Command, product, and project evidence: docs/content/**usage**, plans/claude-unattended-monitor/** | docs/content/(public)/commands/usage.mdx, plans/claude-unattended-monitor/** | Keep active command docs aligned to verified target help. Keep predecessor fixture records explicitly historical. Update manifests and Cargo.lock only as required by the reconciled target crate graph. |

The late predecessor fix commit 427c104b contains three especially important
semantics: a shared strict-spend decision blocks readiness at the SGD45
checkpoint; a persisted closed-period anchor accounts for upward spend
corrections once and preserves uncertainty through migration; and provider
invocation time, rather than queue admission, anchors the Claude attempt floor.
Static inspection finds corresponding target candidate code: the monitor spend
decision and correction horizon are in
crates/jackin-usage/src/host/broker/monitor/spend.rs, and the persisted provider
invocation floor is in crates/jackin-usage/src/coordinator/. The target has
focused test cases for these paths, but this audit did not execute them and the
current integration/migration gates remain pending. The predecessor landing
note's test and Clippy results are predecessor evidence only.

## Independent source-to-target audit

The following is a static reconciliation of the 28-commit source range against
the canonical candidate at HEAD 1a45196d, including its uncommitted worktree.
It confirms candidate destinations and deliberate contract changes; it does
not certify behavioral equivalence, buildability, or completion of every
change among the 232 changed files.

| Source behavior | Candidate coverage found | Status and remaining proof |
|---|---|---|
| Monitor protocol, observation, policy, binding and durable state | Target protocol is v7, statusline input remains independently versioned v2, and durable state is candidate schema v4. The candidate has explicit V1 migration plus V2/V3-to-V4 conversion code, separate observation/dispatch readiness, binding-level default-off collector approval, and explicit policy records. | Contract is intentionally re-expressed; source v6/schema-v2 records must not be copied or relabeled. Migration fixtures and negative cases exist in source, but latest target migration/runtime proof and final migration review are pending. |
| Claude attempt floor and catalog/restart behavior | Target persists `provider_invoked_at_epoch`; coordinator deadline code reads it, and focused cases cover delayed queue admission and removal/re-add/restart behavior. | Static candidate coverage found. The current tree's coordinator test results are recorded at an older checkpoint and were not rerun by this audit. |
| Broker monitor, quota latches, strict budget, and spend corrections | Candidate has broker-owned durable monitor operations, account spend state, `closed_period_anchor`, `historical_correction_horizon_epoch`, and spend evaluation feeding monitor readiness. Tests cover corrections and migration uncertainty. | Source semantics are represented in the new monitor modules. Current integration rerun and migration proof remain pending; do not promote test code or older isolated results to proof for HEAD or its dirty worktree. |
| Keychain boundary, provider identity, and collection | Candidate has an exact-service `ClaudeCredentialLease`, bounded `Zeroizing<String>` cache, TTY-gated attended preparation, one bounded same-service 401 reread path, a Jackin user agent, explicit account mapping, and default-off collector approval. | The candidate deliberately moves from the predecessor's v2 unattended observer workflow to foreground attended bootstrap plus separately opted-in collection. Fresh-binary help/output, the current offline security/integration rerun, and installed-fixture proof remain pending; no real Keychain/provider check was part of this audit. |
| Typed discovery diagnostics and broker publication | The target broker has dedicated catalog-diagnostic and publication modules corresponding to source typed diagnostics and atomic publication work. | Candidate code paths are present; cross-surface regression gates and current-head review remain pending. |
| CLI, service lifecycle, relays, FFI, telemetry, and command docs | Target destinations exist in the consolidated `jackin-usage`, `jackin`, protocol, runtime, capsule, FFI, telemetry, and usage-doc surfaces listed in the mapping table above. | The layouts differ materially, so path presence is not semantic proof. Fresh CLI/install verification, current integration rerun, and telemetry finding disposition remain pending. |
| Removal of the predecessor Claude CLI fallback | Source commit `2e984db9` deletes the provider `cli.rs` and diagnostic helper as part of prohibiting CLI fallback. The target's active `claude_snapshot` no longer called that fallback. | Closed in the candidate: removed the dormant `ClaudeCliUsage`, parser, fetch helper, diagnostic API/re-exports, CLI-only tests, and stale provider-call allowlist row. Post-edit workspace Rust search found no references to those symbols. The experimental HTTP collector is unchanged. Historical research Markdown still describes the old bypass; it is not an active code path. |

No reviewed source behavior in this audit justifies a wholesale tree merge or
raw cherry-pick: the candidate has corresponding implementation or a stated
contract supersession for the core monitor, spend, coordinator, and provider
work. The former Claude CLI helper gap is closed in the candidate tree. This
is a bounded finding, not a complete no-loss certificate. A final
consolidation claim still requires
row-by-row review of the source commit changes and tests, plus the pending
target gates above.

The working tree contains fourteen untracked predecessor artifacts, including
`claude-code-goal.md`, `claude-code-handoff.md`, `installed-smoke.py`,
`statusline-contract.md`, `unblock-design.md`, `v2-contract.md`, the v2
installation/schema/check files, and `verification.md` plus its JSON/log
fixtures. Their names and contents are not proof of target behavior. Keep any
retained v2 commands, fixture outputs, and handoff claims explicitly labeled
as predecessor history; the active direct-CLI contract is
bootstrap-contract.md. Final add/exclude disposition for these untracked files
is still pending.

## Deliberate supersessions

- The predecessor's v2 statusline observer and operator setup are historical
  behavior. The target direction is the v7 foreground bootstrap and explicitly
  opted-in account collector in bootstrap-contract.md. Statusline input remains
  optional and passive; it does not prove direct account collection.
- The predecessor's installed v2 commands, fixture, and handoff do not describe
  a verified main-port binary. Do not publish them as the current procedure or
  treat its fixture results as target readiness.
- Observation remains separate from dispatch in both designs. A collector
  opt-in does not create a goal, receipt, spend baseline, policy approval, or
  runnable monitor. Strict SGD history remains intact; quota-only dispatch
  requires separate attended approval and has no SGD cap.
- The foreground target path intentionally requires TTY-gated operator
  preparation, one exact credential service, a broker lifetime lease, and an
  explicit account mapping. It must not restore broad source fallback,
  unattended auth UI, Claude CLI fallback, or implicit provider collection.
- The source v2 wire/schema cannot be silently relabeled v7/schema 4. The target
  migration must preserve goals, policy origin/revisions, spend provenance,
  event/decision sequences, evidence ages, cooldowns, and existing guards;
  new account mappings and collector approvals start empty.

## Branch-specific documentation disposition

| Source document or artifact | Disposition in the canonical branch |
|---|---|
| plans/claude-unattended-monitor/operator-unblock.md | Preserve its predecessor instructions only in historical-installed-v2-workaround.md, with the archive warning. Do not make it a current direct-CLI runbook. |
| plans/claude-unattended-monitor/landing-queue.md | Do not copy wholesale. Its PR #1121 authorization, old branch counts, and landing logistics are stale for this port. Extract only fix/test facts that are verified against the candidate; preserve their source attribution. |
| claude-code-goal.md, claude-code-handoff.md, installed-smoke.py, v2-checks.json, v2-coverage.txt, v2-installation.json, v2-installed-schema-examples.json, v2-installed-smoke.log, v2-setup-inspection.json, verification-installed-hook.json, verification.md | Keep only as clearly labeled predecessor evidence/provenance. Their fixture and installed results apply to the predecessor source and synthetic fixtures, not the target port. |
| statusline-contract.md and v2-contract.md | Retain as source-contract references for the optional passive statusline path and predecessor monitor behavior. Reconcile any active product claims with the v7 target contract. |
| task-queue.md | Use the tracked main-port queue for current work and gate status. Do not import predecessor completion marks as current verification. |
| unblock-design.md | Untracked predecessor proposal; keep as historical context or exclude. The active direct-CLI contract is bootstrap-contract.md. |
| bootstrap-contract.md and protocol-v7-verification.md | Current target planning/evidence. bootstrap-contract remains candidate-only until implementation, migration, fresh build, and installed-fixture gates pass. |

The old landing queue's positive fix/test record is useful evidence to check,
but its branch/PR state is not current. The old operator instructions are useful
for provenance and safety constraints, but not as instructions to execute the
new path. The target still lacks a verified direct-CLI operator handoff.

## Verification status and open work

There is no equivalence claim in this inventory. The current task queue records
the latest scoped protocol and broker passes, plus pending integration, current
CLI/auth, migration, branch-proof, and installed-artifact work. Those results
do not establish full current-head behavior: no main-port binary or install,
native Keychain access, or real provider/account check is verified. Earlier
security findings have corrective commits and bounded source reviews, but this
audit did not rerun their gates. The telemetry gate still reports eight
unchanged baseline literals. The Claude CLI helper cleanup is complete by
static reference search; the pending gates above still block a complete
reconciliation claim.

No tests were run for this read-only inventory. Do not transfer predecessor
test counts, installed smoke results, or statusline observations into target
verification.

## Historical archive and branch retirement

The annotated archive tag archive/claude-unattended-monitor-2026-10-10 exists
and peels to 771bb088dd93451654f992aeef1b8976ad84ec85. No branch cleanup was
performed. The local claude-unattended-monitor ref remains stale at
abd2260e764595cc6fed32a89725439b6f3e7df1; do not treat it as the archived tip
or delete it as part of this source audit. Branch retirement remains outside
this reconciliation record and requires its own current review of remote refs
and dependent PR/review/check state.
