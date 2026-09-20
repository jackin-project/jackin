# Handoff

Handoff snapshot: `2026-09-20T15:52:12Z`.

This document preserves the in-progress integration/release goal. It is a continuation point, not a completion claim.

## Original Goal

Complete the integration and preview release of `jackin-project/jackin`.

Required outcome:

- Merge every PR open at the original goal cutoff, plus every goal-created integration, CI, evidence, and release-automation PR.
- Rigorously audit and correct PR #1002 (multi-account support), including configuration, migrations, account identity, authorization, credential isolation, capsule/container launch, sessions, console behavior, provider usage, and regression coverage.
- Regenerate all Velnor-owned workflows from the latest verified immutable `velnor-workflow` source.
- Publish a genuinely new `jackin` preview through the Homebrew tap and prove the public artifacts, formula, install, version, capsule resources, and migration/upgrade behavior.
- Preserve a provenance chain from integrated source SHA through release/tag/assets/checksums, tap commit/formula, and installed executable.

Selected channel is `preview`. Do not promote stable. Stable formula/cask publication is outside this execution.

Important constraints:

- No legacy paths, compatibility shims, aliases, or deprecation periods. Complete migrations and accept breaking changes.
- Before every PR merge: read all issue comments, formal reviews, REST inline comments, GraphQL threads including resolved/outdated threads, analyze every finding against the code, fix valid issues, reread reviews, verify the exact head and final diff, then squash-merge with the expected head SHA. Never use an admin bypass or merge unread/actionable feedback.
- Shared-branch mutations belong to the coordinator. Use normal merge synchronization; never rebase or force-push shared branches.
- Commits require `git commit -s` and `Co-authored-by: Codex <codex@openai.com>`. Push small verified milestones regularly.
- Requested coordinator/subagent settings are `gpt-5.6-luna` with max reasoning. The platform exposes no effective-settings telemetry. A usage-limit error was encountered during the last delegation wave; do not claim unobserved effective settings.

Primary references:

- Goal specification: [`jackin-goal-prompt.md`](jackin-goal-prompt.md)
- Multi-account plan: [`plans/multi-account/GOAL.md`](plans/multi-account/GOAL.md), [`plans/multi-account/ledger.md`](plans/multi-account/ledger.md)
- Jackin PR inventory: <https://github.com/jackin-project/jackin/pulls>
- Mandatory feature PR: <https://github.com/jackin-project/jackin/pull/1002>
- Velnor: <https://github.com/tailrocks/velnor>
- Homebrew tap: <https://github.com/jackin-project/homebrew-tap>
- Detailed execution ledger (outside Git): `/tmp/jackin-goal-ledger.md`

## Current State

### Repository and branch

- Repository: `/Users/donbeave/Projects/jackin-project/jackin`
- Branch: `feat/multi-account-support`
- Implementation/handoff merge baseline `39b4267da4a30213f20dbcd9cb59ab1d4dc7f071`; this final handoff metadata commit changes only `HANDOFF.md`. Verify the final branch SHA with `git rev-parse HEAD` and `git ls-remote` after push.
- Handoff merge commit: `39b4267da4a30213f20dbcd9cb59ab1d4dc7f071` (`merge: preserve corrected handoff`); it keeps the first pushed handoff commit as an ancestor without force-push.
- Implementation baseline immediately before the handoff document: `d3e5f38b37c3437db26343c3e339892dea818e73` (`docs(config): repair current schema contract`)
- Tree: clean; no staged, unstaged, or untracked files at this snapshot.
- No tag points at HEAD.
- Current `origin/main`: `a5e1022725e9fc25602e3f248fa5420de8a0a7db`.
- Merge base with main: `41796158b1e45535ae4e74d5ff048cb5bb4e0488`.
- Divergence: feature is 140 commits ahead and 4 commits behind current main.
- Missing main commits are the merges for #1006, #1008, #1010, and #1011: `3d510aac`, `1622769c`, `0163d1b7`, and `a5e10227`.
- PR #1002 is therefore conflicting/dirty against its stale base and is not merge-ready.
- The implementation diff before adding this handoff was 445 files, approximately 64,435 additions and 5,785 deletions. The current PR diff is 446 files / 64,811 additions / 5,785 deletions because this handoff is now committed. It is a large integration branch; do not rebase it.

The latest Velnor remote main observed after fetch is `386a5b63c515b1a92e61f579189c2cbed2d6ce09`. The feature source still pins old Velnor revision `0dc79895ff1c5e88be7c3822c437e1c5b5282e12` in `.github-gen/velnor-workflow.toml`. The previously verified Velnor runtime closure `af140ad4...` belonged to older source `4fa7a3a8...`; it must be re-established for the current source before final regeneration.

### Live Jackin PR inventory at handoff

The following was refreshed from GitHub at `2026-09-20T15:49Z`; the non-#1002 rows were unchanged from the earlier `15:36Z` inventory refresh:

| PR | Head | Base | State / disposition |
|---|---|---|---|
| [#1002](https://github.com/jackin-project/jackin/pull/1002) | `39b4267d` | `main` at stale `41796158` | Starting-scope feature; open, conflicting/dirty. Policy run `35520889714` failed generated-tree; DCO passed. |
| [#1004](https://github.com/jackin-project/jackin/pull/1004) | `073ebbc5` | `main` at stale `41796158` | Goal release-verification PR; open, stale/conflicting/unknown merge state. Candidate work exists separately at `9edc8824`. |
| [#1005](https://github.com/jackin-project/jackin/pull/1005) | `f38cb9ea` | feature at stale `3218706c` | Goal evidence PR; open/unstable and stale. Must update evidence to final SHAs and rerun exact checks. |
| [#1007](https://github.com/jackin-project/jackin/pull/1007) | `f4054488` | `main` at stale `3d510aac` | Goal-created draft CI/generated-tree PR; currently zero-diff/no-op. It must become real final generated work before merge. |
| [#1009](https://github.com/jackin-project/jackin/pull/1009) | `7787f1b4` | feature at stale `83f549a6` | Goal-created draft telemetry lifecycle PR. Root shutdown fix exists, but branch/base/checks are stale; update after feature/auth/generator repair, make ready, rereview, and merge. |
| [#1012](https://github.com/jackin-project/jackin/pull/1012) | `0a053b15` | current main `a5e10227` | New post-cutoff Apple relay PR. Record as unrelated arrival; do not let it silently expand the original scope. |

Starting-scope dependency PRs already merged:

- #963 head `4c6e71a1` → merge `00cba4a3` (bollard v0.21.1).
- #975 head `36b29fb7` → merge `3b1e1fc0` (Mr. Boxington action v1.3.1; distinct from binary version).
- #1001 head `884a2a38` → merge `665f7e37` (`cargo:sccache` desktop tooling).

Goal-created or prerequisite merges already on main:

- #1003 → `41796158` (v1alpha10→v1alpha11 migration).
- #1006 original head `2498b4e1` → `3d510aac` (versioned split atomicity; merged before the required fresh gate, so corrective work was required).
- #1008 head `9033f95d` → `1622769c` (corrective migration normalization).
- #1010 head `fa024cb8` → `0163d1b7` (capsule PTY lifecycle coverage).
- #1011 head `128ff9f7` → `a5e10227` (provider-bound meter installation).

These merges do not satisfy the still-open #1002, #1004, #1005, #1007, and #1009 gates.

### Release and tap state

- Workspace version is `0.6.4` (`Cargo.toml`), but no new selected-channel Jackin release exists.
- The generic latest release is unrelated `jackin-dev-v0.1.52`.
- Current public rolling preview: [Preview 0.6.4-preview.1181+a506eee](https://github.com/jackin-project/jackin/releases/tag/preview), published from `a506eee0581ef7add4f615281dbb90d828d5a657`.
- The `preview` tag currently targets `1c623d10e9f7e9072db4bd8ef027d4c24cd95ab3`, not the release's claimed source. The rolling release has payloads and sidecars but lacks the required `release-manifest.json`, `identity.json`, and `SHA256SUMS` provenance set. Treat it as a legacy/inconsistent release requiring the typed migration path; do not delete or overwrite it through an unsafe legacy path.
- Tap `jackin-project/homebrew-tap` main is currently `d8168bc961a4e119f2fe1d152897d3f0f1f9161c` (PR #500 already merged). `Formula/jackin-preview.rb` still advertises `0.6.4-preview.1181+a506eee`; its current blob SHA is `655462afd4b7415faced6d56c8ed4bb6a2a9f17a`.
- Stable `Formula/jackin.rb` remains disabled because stable has not been published. Do not enable it for this preview goal.
- Tap preview consumer contract is `brew tap jackin-project/tap`, then fully qualified `brew install jackin-project/tap/jackin-preview` or the tap-documented `brew install jackin@preview`; verify the current tap documentation before use.

## Completed Work

### Mainline dependency and migration work

The starting dependency PRs #963, #975, and #1001 were reviewed, checked, and merged. Main also contains the required v1alpha10/v1alpha11 migration, the first #1006 atomicity fix, the #1008 corrective migration, capsule PTY coverage, and provider-bound meter installation. The exact merge SHAs are listed above.

The #1008 correction preserves typed `UnsupportedVersion` behavior and normalizes migrated split workspaces before comparison. Its final exact-head gate passed before squash merge. Do not treat the original #1006 merge alone as sufficient: the corrective path is part of the current main baseline.

### Feature branch architecture and fixes

PR #1002's implementation is present on `feat/multi-account-support` and spans:

- account schema, discovery, editor/settings, scan/apply/cancel, persistence, migration fixtures, and account tombstones;
- provider/agent catalog expansion and account-bound model/endpoint selection;
- workspace admission and launch identity/fingerprint handling;
- credential staging, protected-key admission, capsule RPC/relay boundaries, and Docker/Apple launch paths;
- per-instance HOME/XDG/config/keyring mounts, session lifecycle, terminal/capsule UI, console account labels, and restore paths;
- usage discovery, broker/coordinator/projection state, provider adapters, scope/cache identity, and FFI projections;
- documentation, support/provider ledgers, release verification helpers, and generated CI/release artifacts.

Important fixes already integrated into the feature history include:

- atomic account credential publication and revision/fingerprint fencing;
- directory auth swap recovery, root-escape rejection, and account credential isolation;
- broker catalog activation/rotation fencing and launch-generation checks;
- effective account/provider model propagation, canonical provider stems, fail-closed empty admission, and unsupported wrapper rejection (some original worker hashes were cherry-picked with equivalent commits; verify content, not hash presence);
- persisted Amp launch fixture state so generation fencing compares the same durable account/config/default state;
- current schema documentation update `d3e5f38b`, including `source_selector`, canonical provider/agent slugs, and current version inventory.

Local exact-head evidence for the current clean feature tree is recorded below. Historical worker evidence in `/tmp/jackin-goal-ledger.md` is not a substitute for final integrated proof.

### Investigation and review evidence preserved

The ledger contains detailed independent findings and candidate branches for auth, private config publication, launch leases, credential transport, broker lifecycle, usage routing, Apple relay isolation, telemetry shutdown, documentation, Velnor, release packaging, and tap consumption. Candidate branches are intentionally not treated as accepted merely because they contain tests or a worker summary.

Known relevant candidate refs include:

- `codex/fix-auth-tree-races-20260920` → `0070b572`;
- `codex/fix-private-publication-atomicity-20260920` → `07c9dd72`;
- `codex/repair-launch-lease-blockers-20260920` → `2be2c7fa`;
- `codex/repair-credential-boundary-current-20260920` → `0d181e88`;
- `codex/repair-broker-lifecycle-20260920` → `48b4d69b`;
- `codex/repair-broker-discovery-error-state-20260920` → `7e92df16`;
- `codex/openrouter-usage-20260920` → `ebc44214`;
- `codex/fix-apple-usage-relay-peer-isolation-20260920` → `b65a2962`;
- `codex/docs-schema-inventory-followup-20260920` → `f47f4e2c`;
- `codex/release-integration-20260920` → `9edc8824`;
- `codex/pr1009-p1-shutdown-20260920` → `7787f1b4`.

These are review inputs only. Several were independently rejected for residual security, lifecycle, provenance, or stale-base defects. Do not merge or cherry-pick them without a fresh diff review against the synchronized feature branch.

## Partially Completed Work

### PR #1002 acceptance

The feature is large and substantially implemented, but it has not passed the required current-base audit or merge gate. The current branch is four main commits behind and its exact GitHub Policy check fails generated-tree validation. The current PR has no current required CI proof beyond DCO.

Open review themes from independent audits:

- Auth publication still needs descriptor-relative, canonical-identity, no-follow, inode-paired validation/use across every provider path; lock aliases and missing-source races must be closed. Generic Codex/Grok/OpenCode/Amp/Gemini/Cursor/Muse paths must reject symlink/FIFO/special-file substitutions. Omp validation and read must bind the same inode.
- Private Codex/OpenCode config publication needs journal target binding, lossless first-publication recovery, quarantine/rename-before-recursive-cleanup, nested durability, lock identity, and mid-failure tests. Forged/stale journal names must never select or delete an arbitrary sibling.
- Launch leases must remain valid across attach, restore, reconnect, final detached validation, and Docker start. Credential staging must be transactionally ordered and cleanup must not remove a concurrent replacement. Generation checks must not have a final-check/start gap.
- Credential envelopes must prove provider-family keys and routed endpoints end-to-end, including Moonshot/Kimi/Z.AI/Zhipu surfaces. Protected-key registries, relay/session material, broker CAS, rotation, mixed-agent, and mixed-material cases must fail closed. No ambient or unselected secret may reach env, metadata, logs, traces, UI, snapshots, exceptions, or artifacts.
- Broker lifecycle must make discovery/catalog/coordinator/projection/runtime publication one revision/CAS-guarded transaction. Failed or stale discovery must not republish old capabilities; corrupt state must quarantine/fail closed; account-scoped relay authorization and same-path credential rotation need proof.
- Usage selection must use effective `(agent, provider, account, organization/model/scope)` identity, not merely a provider-owner key. Claude OAuth and API-key paths differ. Unknown/non-desktop providers must remain explicit `unsupported`/`not_run`, not silently disappear or trigger a manual retry loop. Percentages, credits, tokens, currency, balance, reset timestamps, stale/error/unknown status, and scope must remain distinct.
- Restore and console paths need exact account/config identity preservation across restore, labels, tabs, panes, and duplicate instances. Settings/CLI environment surfaces must not accept arbitrary account-owned credential keys. Docker `--env-file`/metadata must not leak secrets. Supervisor PID and Apple relay peer identity must be reserved against override/PID reuse.

The optional live-provider credentials, live three-account container trace, native macOS comparison, and some Docker/OrbStack evidence are unavailable. They must stay explicitly `not_run`/unverified unless real credentials and runners are available; never fabricate successful evidence. Deterministic fixtures can prove boundaries but do not prove live-provider behavior.

### Goal-created PRs

- #1004 has a promising typed preview package/release candidate on branch `codex/release-integration-20260920` (`9edc8824`) with six payloads, manifests, tap updater wiring, provenance checks, and protected-tag migration work. The original PR head is stale and must be updated normally. The old Velnor renderer still has unsafe/unsupported rolling-tag behavior and actionlint/source issues; repin only after upstream source and immutable runtime are verified.
- #1005 contains the evidence ledger but its branch is stale. It needs final source SHA, current test counts, explicit open gaps, and exact links. Prior findings included stale provenance, nonexistent/P2 test citations, OpenRouter history, event deduplication, Docker leakage, and GUI/keyring live status.
- #1007 is a draft zero-diff placeholder after a main sync. It must carry actual final generated CI/workflow changes, then become ready and pass review/checks. Do not merge a no-op.
- #1009's `7787f1b4` candidate fixed the original shutdown/timeout root path, but the draft is based on stale feature state. Fresh review must prove facade detachment before provider flush, in-flight lease lifetime through timed-out workers, actual panic-hook/installed-meter behavior, and exact integrated CI before making it ready.
- Velnor upstream has advanced to `386a5b63`; prior #970/#971 work and the older `4fa7a3a8` runtime are not the final source/runtime. The current immutable published runtime closure must be found or published after reviewing the current source.

## Remaining Work

Execute in this order. Keep each meaningful unit signed, tested, committed, and pushed.

### 1. Re-establish live inventory and review gate inputs

Refresh refs and all open PR data before mutation:

```sh
rtk git fetch origin main
rtk git fetch origin '+refs/heads/feat/multi-account-support:refs/remotes/origin/feat/multi-account-support'
rtk git fetch velnor main
rtk gh pr list --repo jackin-project/jackin --state open --limit 100 \
  --json number,title,isDraft,headRefName,headRefOid,baseRefName,baseRefOid,mergeable,mergeStateStatus,url
rtk gh api --paginate repos/jackin-project/jackin/issues/1002/comments
rtk gh api --paginate repos/jackin-project/jackin/pulls/1002/reviews
rtk gh api --paginate repos/jackin-project/jackin/pulls/1002/comments
```

Use GraphQL review threads for every PR before its merge. Record the new cutoff and post-cutoff PRs. Preserve unrelated #1012 as an arrival after the original scope cutoff.

### 2. Synchronize the feature branch with current main

On the coordinator checkout, after reviewing the current diff:

```sh
rtk git merge --no-ff --no-commit origin/main
# resolve conflicts without dropping feature behavior
rtk git diff --cached --check
rtk git commit -s -m "merge: synchronize multi-account feature with main" \
  -m "Co-authored-by: Codex <codex@openai.com>"
rtk git push origin HEAD:feat/multi-account-support
```

Do not rebase or force-push. Re-run the full PR review gate at the new exact head. Resolve generated-tree conflicts only through the Velnor source/generator contract, not hand-edited YAML patches.

### 3. Finish and independently review #1002 behavior

For each blocker in **Partially Completed Work**, identify the enabling architectural condition, implement the structural fix, add regression coverage, and have an independent reviewer inspect the final diff. Required proof includes:

- configuration fresh install, installer seed, supported schema upgrades, repeated migration, removed-account tombstones, custom paths, defaults, env/1Password references;
- scan draft/apply/cancel, concurrent scan/edit locking, atomic writes, interrupted writes, invalid data, and failure recovery using isolated `JACKIN_CONFIG_DIR`/`JACKIN_HOME_DIR`;
- account/workspace/global/sole-eligible selection precedence, rejection of invalid/ambiguous/unauthorized selections, and atomic no-launch behavior;
- two same-provider Codex instances with account-specific routed endpoints/models and matching credential envelopes; OpenCode/Amp repeated-root rejection or real isolation;
- mixed-provider container mounts, staged files, relay capabilities, Docker metadata, environment, and sentinel credential absence;
- identity continuity through launch/status/new tab/split/resize/exit/detach/reattach/restore and settings UI operations;
- complete capsule package tests without fail-fast, portable test-shell behavior without process-global env races, and a matching locally built/exported capsule before container tests;
- provider support matrix, usage units/scope/cache identity, stale/error/unknown/timeout/rate-limit/expired/malformed behavior, and FFI/UI provenance.

Do not count worker summaries as review evidence. Use current code, current tests, exact run logs, and redacted artifacts.

### 4. Bring goal PRs to current base and merge them safely

For #1009, #1005, #1004, and #1007, update branches by normal merges from the synchronized feature/main state. Before each merge:

1. Read every issue/formal/REST/GraphQL review surface.
2. Analyze all findings against the exact code and reply/resolve where appropriate.
3. Make the PR ready if it is a draft and ensure it contains real work.
4. Run focused and required checks on the exact head.
5. Re-read reviews, inspect final diff, confirm no actionable feedback, then squash-merge with `--match-head-commit`.

Do not merge a release/evidence/CI PR merely because an equivalent change exists elsewhere. The original PR must be merged or an exact concrete external blocker must be recorded.

### 5. Resolve Velnor source/runtime and regenerate

Research current Velnor at `386a5b63c515b1a92e61f579189c2cbed2d6ce09`, its CLI help, schema parser, migration guidance, generated ownership policy, and current published runtime. Reconcile any open upstream package-release/tag-safety work. Establish a single immutable source SHA plus a provisioned published runtime closure.

Update `.github-gen/velnor-workflow.toml` to the verified source/runtime. Keep schema 2 explicit: providers, automatic providers, default dispatch providers, default branch, and each provider selector's `runs_on`. Translate desktop release and preview package publication through typed supported capabilities. Remove obsolete legacy declarations and generated copies through canonical source ownership.

Then run the exact commands supported by the selected Velnor revision, expected to include:

```sh
velnor-workflow generate . --plain
velnor-workflow generate . --check --plain
actionlint
```

Run policy with required head/base/context inputs, verify `ci-required`, DCO, checkout/merge-ref semantics, permissions, secrets, cache behavior, concurrency, and legitimate provider skips. Regenerate twice from clean inputs and require no unexplained diff.

Revisit the Mr. Boxington quota failure. The old run failed at cache import with `EDQUOT` while expanding a roughly 2.06 GiB nested tar under `/tmp`, before Rust tests. The feature's `rust-jackin` `mbx = false` is a symptom-level workaround. Verify the current Velnor v1.4 directory transport/runtime; remove the workaround only when the exact final generated Rust job uses a proven safe path and reaches tests. Preserve cache correctness rather than disabling all caching.

### 6. Run integrated CI and acceptance suites

Run current repository commands from scripts/config before execution. At minimum rerun format, diff, locked dependency resolution, workspace/unit/integration tests, strict Clippy/lint policy, migration fixtures, capsule/runtime E2E, docs/links/roadmap/research checks, desktop/macOS lanes, distribution builds, and the complete generated CI required contexts.

Separate historical failures from current evidence. Known historical failures include Mr. Boxington `EDQUOT`, a missing `/bin/zsh` test shell, an Amp launch fixture race, undocumented `ProfileSelector.entry/profile`, diagnostics MeterInstallError conformance failures, and a parallel capsule lifecycle flake. Reproduce each on the synchronized exact SHA and fix root causes; do not mask with skips.

### 7. Publish and verify the preview release

Use the existing `release_archive.rs`, `release_verify.rs`, `github.rs`, `mise` release tasks, generated preview workflow, and tap `scripts/package-update.sh`. Choose a new version of the form `X.Y.Z-preview.N+<first-seven-source-SHA>`. Ensure `JACKIN_VERSION_OVERRIDE`, packaged `jackin --version`, manifest, release, and formula all match.

The final package must include exactly these six payloads plus required provenance/sidecars/manifests:

- `jackin-aarch64-apple-darwin.tar.gz`
- `jackin-x86_64-apple-darwin.tar.gz`
- `jackin-aarch64-unknown-linux-gnu.tar.gz`
- `jackin-x86_64-unknown-linux-gnu.tar.gz`
- `jackin-capsule-aarch64-unknown-linux-gnu.tar.gz`
- `jackin-capsule-x86_64-unknown-linux-gnu.tar.gz`

Verify `velnor.package-release.v1`, `identity.json` source repository/ref/digest, manifest source commit, every SHA-256, signatures/bundles, SBOMs, attestations, capsule manifests, nonempty downloadable assets, and the rolling-tag/source mapping. Migrate the current legacy preview only through a fail-closed, serialized, provenance-preserving path; never replace immutable version tags or use an unguarded delete/recreate operation.

Run the verified tap updater with `VELNOR_PACKAGE_CHANNEL=preview` and an authentically verified package directory. Confirm a tap commit/formula update, tap CI and `mise run check`, deterministic updater behavior, and rejection of incomplete/mismatched/provenance-invalid packages.

### 8. Consumer verification and final independent gate

In a clean isolated environment, use the fully qualified tap/formula. Verify formula audit/test, binary path, architecture, `jackin --version`, help, non-destructive operation, `jackin-role`, and the correct `libexec/jackin-capsule/linux-arm64/jackin-capsule` or `linux-amd64` resource. Upgrade from the actual previous supported preview if the contract allows it; otherwise record the absence and prove first install plus isolated migration fixtures.

Assign fresh independent reviewers for integrated code/#1002, generated workflows/CI, and public release/tap installation. Challenge stale SHAs, skipped checks, unsupported live claims, secret leakage, and provenance mismatches. Fix valid findings, rerun affected checks, and update `/tmp/jackin-goal-ledger.md` and PR evidence.

## Important Decisions and Reasoning

- Preview is intentional. The user explicitly selected the existing Homebrew channel; stable first publication is not authorized.
- The `jackin-dev` release stream is a different binary/version authority. It cannot prove the selected Jackin preview release.
- Current main advanced after the original cutoff. The feature branch must absorb it normally before final review; otherwise checks and generated files describe different bases.
- Generated workflows are outputs of Velnor source/config. Hand-editing generated YAML is not an acceptable final repair. If Velnor lacks a required typed capability, fix the smallest upstream source path, test it, publish/provision its runtime, then repin and regenerate.
- The current preview release is not trustworthy merely because its six payload checksums match the old formula. Tag/source drift and missing manifest/identity/SHA256SUMS require a provenance-preserving migration.
- The current `mbx = false` Rust workaround may avoid a quota symptom but is not proof of cache correctness. Prefer fixing/validating the transport at its source before deciding whether it remains.
- Historical worker branches are evidence and candidate input, not accepted code. Several independent reviews found residual security/lifecycle defects after apparently green focused tests.
- The original #1006 merge happened before the required fresh gate; no history rewrite or revert is planned. The corrective #1008 merge is the current mainline repair, and any remaining migration defect must be fixed with a new corrective PR.
- Preserve unrelated worktrees and user files. The parent checkout is the single integration checkout; do not switch it to worker branches.

## Known Issues / Risks / Blockers

- PR #1002 is open, conflicting, 140 commits ahead/4 behind current main, and currently fails Policy generated-tree validation. It has no accepted current full CI gate.
- #1004, #1005, #1007, and #1009 are open goal-created PRs with stale bases or incomplete/no-op contents. They all need current-base review and exact checks.
- #1012 arrived after the original cutoff. It is open and must be recorded, but is not automatically part of the starting scope.
- The feature's Velnor pin is old (`0dc79895`); current Velnor main is `386a5b63`. No final current-source runtime closure is recorded yet.
- The current generated preview workflow exists in the feature tree, but final typed schema-2 regeneration and actionlint/policy proof are incomplete.
- No new release, version tag, verified six-payload package, tap update, clean install, upgrade, or installed-version proof exists.
- The public rolling preview is legacy/inconsistent as described above. Protected tag/release mutation must be performed only through the verified serialized migration contract.
- Mr. Boxington's old cache import failed with `Quota exceeded (os error 122)` before tests. The exact runner quota counters were unavailable; the log proves the failure point and nested-tar mechanism, not the precise quota limit.
- Full integrated acceptance, capsule export/container smoke, live provider matrix, native macOS comparison, and several UI/PTY flows remain unproven. Optional provider credentials are missing and must remain labeled unverified.
- A historical serial workspace run at an older feature checkpoint reported 6,289 passed, 12 failed, and 3 ignored; failures included Amp launch account-state fencing, schema-reference documentation, and diagnostics MeterInstallError conformance. A parallel capsule lifecycle flake also occurred. This is historical evidence, not a current result; rerun on the synchronized head.
- Many disposable worktrees exist under `/private/tmp` and sibling project directories. Do not delete them during handoff. They contain candidate branches and evidence.
- The last delegation wave hit the platform usage limit (`You’ve hit your usage limit... try again at Sep 26th, 2026 7:22 PM`). If delegation remains unavailable, the coordinator must continue local review rather than claiming independent verification occurred.

## Validation Performed

### Exact handoff tree: verified passing

All commands below ran on implementation baseline `d3e5f38b37c3437db26343c3e339892dea818e73` after the final fast-forward and before this handoff document was added:

- `rtk git status --short --branch` — clean.
- `rtk git rev-parse HEAD origin/feat/multi-account-support` — both `d3e5f38b37c3437db26343c3e339892dea818e73`.
- `rtk cargo fmt --all -- --check` — pass.
- `rtk git diff --check` — pass.
- `rtk cargo test --locked -p jackin-config --lib -- --test-threads=1` — 443 passed.
- `rtk cargo test --locked -p jackin-runtime --lib -- --test-threads=1` — 690 passed, 1,374 filtered, 3 suites, 113.12 seconds.
- `rtk cargo test --locked -p jackin-usage --lib -- --test-threads=1` — 483 passed, 960 filtered, 3 suites, 6.95 seconds.
- Read-only `git fetch origin main feat/multi-account-support` and `git fetch velnor main` — refs refreshed; no source files changed.

### Verified but not completion evidence

- Independent state audit read AGENTS and confirmed clean branch/remote state, current main divergence, current #1002 conflict, missing current CI proof, stale ledger references, and absent release/install evidence.
- Historical worker tests and PR checks are preserved in `/tmp/jackin-goal-ledger.md`, with commit/run links. They must be rerun or explicitly tied to the exact final SHA after synchronization.

### Not run, failing, or still required

- Full workspace test/nextest matrix, complete capsule package without fail-fast, matching capsule build/export/container E2E, native macOS release path, and complete TUI/PTY acceptance.
- Current exact #1002 required CI: DCO passes; Policy/generated-tree fails because the generated tree cannot render collapsed Rust jobs with disagreeing member tool sets; current `ci-required` proof is absent.
- Final Velnor generation/policy/actionlint from current source `386a5b63` and its provisioned immutable runtime.
- Preview package build, `release-verify`, manifest/identity/checksum/attestation verification, public release publication, rolling-tag migration, tap updater, tap CI, formula audit/test, clean install, upgrade, and installed version/resource proof.
- Live optional provider/container/native evidence.

## How to Continue

1. Read this file, [`jackin-goal-prompt.md`](jackin-goal-prompt.md), and `/tmp/jackin-goal-ledger.md`.
2. Verify `rtk git status --short --branch`, `rtk git rev-parse HEAD`, current remote refs, and the exact current open PR inventory.
3. Read all review surfaces for the PR you will mutate before editing it. Keep PR merge authorization separate from branch synchronization.
4. On `feat/multi-account-support`, normally merge current `origin/main`, resolve and test, commit with DCO/Codex trailer, and push.
5. Reproduce and structurally fix the remaining #1002 security, identity, migration, broker, usage, and session blockers. Use independent review when capacity returns; otherwise document local review limits.
6. Update and merge goal PRs #1009, #1005, #1004, and #1007 in dependency order, using expected-head squash merges only after their review/CI gates pass.
7. Resolve current Velnor source/runtime, regenerate all owned outputs, run generator check/policy/actionlint, then run exact post-merge CI.
8. Build, verify, publish, and consume the preview package through the normal release/tap lifecycle. Record each SHA/link/checksum in the ledger and this file as state advances.
9. Perform fresh final independent verification. Only then report `COMPLETE`; otherwise report the exact smallest external blocker and all completed work.

Useful commands:

```sh
rtk git status --short --branch
rtk git fetch origin main
rtk git fetch origin '+refs/heads/feat/multi-account-support:refs/remotes/origin/feat/multi-account-support'
rtk git fetch velnor main
rtk gh pr checks <number>
rtk cargo fmt --all -- --check
rtk cargo test --locked --workspace --all-features -- --test-threads=1
velnor-workflow generate . --plain
velnor-workflow generate . --check --plain
actionlint
```

## Definition of Done for the Original Goal

The original goal is complete only when:

- every starting-scope PR (#963, #975, #1001, #1002) is confirmed merged with `merged_at` and `merge_commit_sha`, and every goal-created integration/release PR that remains in scope (#1004, #1005, #1007, #1009 and any required follow-on) is also confirmed merged;
- #1002 has an evidence-backed claim/review matrix, structural fixes for valid findings, regression coverage, current-base review, and passing mandatory acceptance checks;
- all Velnor source/config/generated outputs are synchronized to one verified immutable current source/runtime, with clean regeneration, policy, actionlint, and required CI contexts;
- final main validation passes on the exact candidate SHA, with every legitimate skip explained and no pending/cancelled/absent mandatory lane hidden as success;
- a genuinely new public Jackin preview release has the exact version/source suffix, `velnor.package-release.v1` manifest, matching `identity.json`, six required payloads, checksums, signatures/bundles, SBOMs, attestations, capsule manifests, and downloadable nonempty assets;
- the rolling preview tag/release and Homebrew formula point to the same verified package; tap updater tests and tap CI pass; formula audit/test passes;
- a clean consumer installation invokes the exact released `jackin` version and architecture, includes `jackin-role` and the matching capsule resource, and first-install/migration or supported-upgrade verification passes;
- the final provenance chain is recorded and independently reviewed, no material credential-isolation/migration/CI/release defect remains, and optional live-provider gaps are clearly labeled rather than fabricated.
