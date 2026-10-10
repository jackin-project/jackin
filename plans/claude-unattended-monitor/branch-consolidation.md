# Branch consolidation inventory

Audited 2026-10-10. Read-only inventory; no source implementation, branch, or
Git ref was changed for this record.

## Scope and branch facts

The intended delivery branch is feat/claude-usage-monitor-main at
52b8bf582a757888e8817e069350eb10f0699006, based on common point
868ce53519234f879d26a8bd2dffaa30fae2d729. The isolated predecessor is
claude-unattended-monitor at 771bb088dd93451654f992aeef1b8976ad84ec85.

The predecessor contains 203 commits from the common point through
ff9eb01f0b3e0155a483d39e6802e4ed14d078f6 for the unrelated Rust-policy
rollout associated with PR #1121. Exclude those commits and their tree changes
from the usage port. The usage-task inventory is the 28 commits in
ff9eb01f..771bb088. That range changes 232 files (+36,856/-11,218) in the
predecessor layout. Older landing notes report 25 commits / 219 files; those
figures predate commits a9001732, 427c104b, and 771bb088.

The target branch has the v7 contract commit fa7f2f0e and its checkpoint
52b8bf58. The code and document port is still a dirty worktree, including
split modules and untracked files. A read-only inventory is not a committed
port. In the target clone, the tracking ref for the predecessor was stale at
abd2260e; the isolated predecessor clone showed 771bb088. Re-fetch refs before
any branch cleanup.

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
| Monitor protocol and broker wire: crates/core/jackin-protocol/src/usage_monitor.rs, usage_broker.rs; crates/services/jackin-usage-broker-wire/** | crates/jackin-protocol/src/usage_monitor.rs, usage_broker.rs | The target freezes protocol v7 and monitor schema 3 in a candidate contract. Source monitor v2 / wire v6 behavior needs deliberate migration, not a file copy. Preserve explicit observation vs dispatch authority and fail-closed migration of policy, spend, evidence-age, and cooldown state. |
| Coordinator and durable policy state: crates/services/jackin-usage-coordinator/** | crates/jackin-usage/src/coordinator/** | Reconcile cadence, persisted attempt floors, policy decisions, restart behavior, catalog changes, state validation/sanitization, and migration. The source fix uses provider invocation time for the Claude floor. |
| Broker monitor, spend, publication, and service lifecycle: crates/services/jackin-usage-broker/**, jackin-usage-broker-publish/** | crates/jackin-usage/src/host/broker/**, including monitor/** and publish/** | Reconcile durable monitor state, ordered decisions, idempotency, bounded waits, quota latches, strict-budget readiness, late closed-period corrections, broker ownership, and publication timestamps. Source tests do not prove the target port. |
| Provider, credential resolution, discovery, and host runtime: crates/services/jackin-usage-provider-claude/**, jackin-usage-provider-core/**, jackin-usage-credential-resolver/**, jackin-usage-credential-snapshots/**, jackin-usage-discovery/**, jackin-usage-host-credentials/**, jackin-usage-host-runtime/**, jackin-usage/** | crates/jackin-usage/src/usage/claude/**, usage/refresh.rs, host/credential_resolver.rs, host/discovery.rs, host/projection*, host.rs | Preserve scoped account identity, typed opaque diagnostics, no unattended auth UI or Claude CLI fallback, cooldowns, and no broad credential source switching. The new attended bootstrap contract deliberately narrows credential access to one selected service; verify the target's exact-source behavior and cache boundary. |
| CLI, broker executable, consumers, and end-to-end tests: crates/apps/jackin/src/cli/usage/**, bin/usage-broker/**, tests/**, console adapter | crates/jackin/src/cli/usage/**, crates/jackin/src/bin/usage-broker/**, crates/jackin/src/console/adapter/**, crates/jackin/tests/** | Reconcile monitor/status/watch/wait semantics, TTY confirmation gates, statusline ingress, broker selection and installation, lifecycle, offline observation, and exit/JSON schemas. The target direct-CLI command shapes remain unverified against a fresh binary. |
| Usage FFI and presentation: crates/adapters/jackin-usage-ffi/** | crates/jackin-usage-ffi/** | Reconcile DTO/version changes, typed presentation, bridge callers, and removal of predecessor-only discovery/route paths. |
| Capsule relay authorization: crates/apps/jackin-capsule/src/usage_relay_proxy/** | crates/jackin-capsule/src/usage_relay_proxy.rs and tests | Preserve relay-side rejection of unsupported or unauthorized monitor operations. |
| Runtime usage relay: crates/services/jackin-runtime-usage-relay/** | crates/jackin-runtime/src/usage_relay.rs and tests | Reconcile protocol payloads, broker ownership, and relay failure handling. |
| Telemetry schema and registry: crates/services/jackin-telemetry/** | crates/jackin-telemetry/** | Reconcile event/attribute/metric/span names and registry consistency with the v7 monitor contract. Current telemetry gate findings remain open. |
| Command, product, and project evidence: docs/content/**usage**, plans/claude-unattended-monitor/** | docs/content/(public)/commands/usage.mdx, plans/claude-unattended-monitor/** | Keep active command docs aligned to verified target help. Keep predecessor fixture records explicitly historical. Update manifests and Cargo.lock only as required by the reconciled target crate graph. |

The late predecessor fix commit 427c104b contains three especially important
semantics: one shared strict-spend predicate blocks runnable/readiness at the
SGD45 checkpoint; a persisted closed-period anchor accounts for upward spend
corrections once and preserves uncertainty through migration; and provider
invocation time, rather than queue admission, anchors the Claude attempt floor.
Those semantics are visible in the target candidate modules, but they have not
been independently checked there. The predecessor landing note reports 133
broker tests, 47 coordinator tests, and scoped Clippy passing after those fixes;
those are predecessor results only and must not be presented as target proof.

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
- The source v2 wire/schema cannot be silently relabeled v7/schema 3. The target
  migration must preserve goals, policy origin/revisions, spend provenance,
  event/decision sequences, evidence ages, cooldowns, and existing guards;
  new account mappings and collector approvals start empty.

## Branch-specific documentation disposition

| Source document or artifact | Disposition in the canonical branch |
|---|---|
| plans/claude-unattended-monitor/operator-unblock.md | Preserve its predecessor instructions only in historical-installed-v2-workaround.md, with the archive warning. Do not make it a current direct-CLI runbook. |
| plans/claude-unattended-monitor/landing-queue.md | Do not copy wholesale. Its PR #1121 authorization, old branch counts, and landing logistics are stale for this port. Extract only fix/test facts that are verified against the candidate; preserve their source attribution. |
| claude-code-goal.md, claude-code-handoff.md, installed-smoke.py, v2-checks.json, v2-installation.json, v2-installed-schema-examples.json, v2-installed-smoke.log, v2-setup-inspection.json, verification-installed-hook.json, verification.md | Keep only as clearly labeled predecessor evidence/provenance. Their fixture and installed results apply to the predecessor source and synthetic fixtures, not the target port. |
| statusline-contract.md and v2-contract.md | Retain as source-contract references for the optional passive statusline path and predecessor monitor behavior. Reconcile any active product claims with the v7 target contract. |
| task-queue.md and unblock-design.md | Use the current main-port versions for the active direct-CLI work. Preserve predecessor progress as historical context; do not import old completion marks as current verification. |
| bootstrap-contract.md and protocol-v7-verification.md | Current target planning/evidence. bootstrap-contract remains candidate-only until implementation, migration, fresh build, and installed-fixture gates pass. |

The old landing queue's positive fix/test record is useful evidence to check,
but its branch/PR state is not current. The old operator instructions are useful
for provenance and safety constraints, but not as instructions to execute the
new path. The target still lacks a verified direct-CLI operator handoff.

## Verification status and open work

There is no equivalence claim in this inventory. The target worktree has not
passed the full port/build/install gate. The v7 protocol checkpoint reports 129
offline protocol tests passing; broker, CLI, migration, and installed-workflow
proof remain pending. The current task queue also records unresolved security
review findings: ordinary token copies are not zeroized; exact-source one-shot
401 reread is incomplete; a selected-source cache miss may fall back to other
credentials; service validation is incomplete; approved collector execution
is not wired; and the request still uses a Claude Code user agent. The
telemetry gate reports eight unchanged baseline literals. Reconcile and close
these findings in target code and offline evidence before calling the port
complete.

No tests were run for this read-only inventory. Do not transfer predecessor
test counts, installed smoke results, or statusline observations into target
verification.

## Historical archive and branch retirement

Do not create the archive tag or retire the predecessor branch during this
inventory. After every source commit and required document has a reviewed
disposition, the target worktree is committed, and target verification/review
is complete, preserve the source tip with an annotated tag such as
archive/claude-unattended-monitor-771bb088 pointing exactly to
771bb088dd93451654f992aeef1b8976ad84ec85. Verify the peeled tag SHA and publish
the tag before deleting the predecessor branch. The commit object was present
in the target clone at audit time; no archive tag existed yet.

Retire only claude-unattended-monitor, and only after re-fetching the remote
source and canonical refs, confirming the source has not advanced, confirming
no open PR/review/check state depends on the branch, and confirming the archive
tag is reachable. Keep feat/claude-usage-monitor-main as the sole feature
delivery branch. Do not remove the canonical branch or alter main as part of
that cleanup.
