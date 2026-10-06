# Branch and PR Findings

- Status: IN PROGRESS
- Snapshot date: 2026-10-05

## Latest branch audit

Read-only audit time: `2026-10-05 08:46:39 UTC`. The private audit packet is `/tmp/jackin-build-split-branch-audit-20261005-084639Z.md`, SHA-256 `2fa1ed744cc0289cc000fac350ec9da800b165b1a38c1648df0155d296072e7d`.

The audited source head is `17b2b1be6a58a0e34af6d8308df915d110f4a785`. Main remains `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. The task branch has 57 commits ahead, no merge commits, and 117 changed paths (`+10,975/-1,099`).

The documentation commits through `f4902db386e029a4a481b13767f7a29b5351af49` change six plan files. They do not change source. Later docs-only commits do not require source-review rebinding.

PR [#1112](https://github.com/jackin-project/jackin/pull/1112) is open and draft at source head `17b2b1be6a58a0e34af6d8308df915d110f4a785`. Its base is main `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`.

Run `37283551725` failed Plan. Velnor `0.1.0` rejected `.velnor/config.toml` field `tasks`. Required failed, and Rust jobs were skipped. Actionlint passed. The DCO check remains required. The refresh found zero reviews, comments, or threads. No Rust test or build result is available.

The audit counted 131 branch heads, 1,104 pull heads, 1,104 all-state PR records, and 126 recovery refs. PR #1111 has dispositions for all 38 commits, including already-present no-ops.

A later read-only capture matched all 126 remote recovery refs. It found 120 distinct tip trees; two refs are disconnected from main. The ref-head inventory is `/tmp/jackin-recovery-ref-heads-20261005.csv`, SHA-256 `38ff62c5f6be689658d6e0759cbb9195714b9260fa0dcf564add27698fdf5d31`. This verifies ref coverage only; patch and behavior dispositions remain incomplete.

A Sol completeness check found 415 distinct non-merge commit OIDs across other recovery refs. They lack individual content dispositions or equivalent destination mappings. The count used this query:

```sh
git rev-list --no-merges $(git for-each-ref --format='%(refname)' refs/remotes/origin/recovery/) --not 0aa821a088e1bacf3d4d85a4c9faaa67faa85132 17b2b1be6a58a0e34af6d8308df915d110f4a785 3a28c199f17da335ecd9abd8dd67ebf1aecc0421 18bc09e9536d9b662876d2fb4205357a829caa9a 2990df17e25f30afca84804d9c402abc1ce00231 0437497d7f22fbdb7c8aad986be932704d707fe9 | sort -u | wc -l
```

This query excludes reachable commits only. It does not test patch equivalence. The 415 OIDs do not equal 415 missing changes. Examples include source3 commit `5a5d8eda8ade4caf7daa4f544fd92678e58820df` and qualification commit `14bd1471616056e07c81c59d59eafc0963b16865`.

Retry-after behavior from `9c437fd2b0b9e88a265adab9fde26b5acb523745` already exists in main at `crates/jackin-usage/src/usage.rs:1490`. Recovery content accounting remains IN PROGRESS. Migration inputs are committed at `17b2b1be`; generated outputs and tests remain open in [the migration checkpoint](reviews.md#current-migration-source-and-fixture-checkpoint).

The ten later source commits in the audited task branch are:

| Commit | Change and gate |
|---|---|
| `5b1fdde2f59d4ad7b1ca56a6258c457d85db4e61` | Route capsule cross-build through Mise and MBX. The exact-source review found workspace-target selection needed follow-up. |
| `c0892f669ad4f888246ea0a05bb8df2459973e8c` | Port operation-reference migration behavior. Exact review found malformed breadcrumb acceptance, ambiguous field selection, and legacy aliases. |
| `9ef4dc010820f706520b372a5da03cec29074c63` | Fix those source findings; review found one legacy-value validation path still missing. |
| `9f4c32ec4fcb34d228c9cb83a2f49f87db835c93` | Run rustup target selection in the workspace Mise context. Exact source review PASS; tests NOT RUN. |
| `20cb8f7054ef6322c653b5c92bec7ad8f826d810` | Route normalized legacy and versioned OpRefs through canonical validation before writes. Exact source review PASS; tests and goldens NOT RUN. |
| `17eb3b4e06a43b7eceb444eafd08c728162734f6` | Pin the supported MBX `1.22.0` release for capsule cross-builds. Exact source review PASS; builds and tests NOT RUN. |
| `3d8b979e123ca76ef13a39170c4efa09b0712925` | Add a migration-fixture output writer. Exact review found auto-discovery and destination-publication issues. |
| `8c464252b8846c08fb584b1dd3f292814addc1d4` | Isolate the writer helper and require a task-owned private output parent. Exact source review PASS; execution still waits for predecessor inputs. |
| `d08952995734baf64405197d74ec4796ba915a25` | Require successful legacy-to-versioned fixture transformations. Exact source review PASS; it also requires both predecessor directories. |
| `17b2b1be6a58a0e34af6d8308df915d110f4a785` | Add controlled OpRef predecessor inputs and fail closed if either input directory is absent. Exact source review PASS; rebake and tests NOT RUN. |

These records distinguish source review from executable acceptance. No Rust compile, Cargo test, schema check, or generated-golden review was run for these commits in the branch audit.

## Task branch

- Repository: `https://github.com/jackin-project/jackin.git`.
- Base branch and SHA: `main`, `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`.
- Task worktree: `/root/Projects/tailrocks/jackin-project/jackin-refactor-build-split`.
- Task branch: `refactor/build-split`.
- Initial task worktree state: clean at the base SHA.
- Audited source snapshot: `3b1a7789fe41e679eb9554e04862b6033cd82c94`; local and remote task refs matched.
- `git rev-list` from the base to this snapshot contains 36 commits: 23 source/test/config commits and 13 documentation commits. It contains no merge commit.
- No PR was open for this Jackin task at the snapshot.

## Historical source inventory at `3b1a7789`

At source snapshot `3b1a7789fe41e679eb9554e04862b6033cd82c94`, both `git rev-list --count base..HEAD` and `git rev-list --count --no-merges base..HEAD` returned 36. The table accounts for all 23 source, test, and configuration commits. Each listed task commit is its own destination on `refactor/build-split`, except the attributed `c72e25d` source and `c8d20fb` destination. Source presence does not mean tests passed. Direct MBX tests remain pending unless stated otherwise.

| Commit | Unique change or hunk | Source and test status |
|---|---|---|
| `491999cb4e839bae7739573a235fb53aac320b36` | Adds generated-workflow and Architect fixture contracts in `.velnor/config.toml`, manifest tests, `ci_contract.rs`, and desktop tests. | Source on branch. Cargo tests NOT RUN. |
| `3f58f7c2c2504f821a37f5c989ac86d5940d56fb` | Binds native CI assertions to generated job inventory in desktop and CI-contract tests. | Source on branch. Cargo tests NOT RUN. |
| `9a147fba7786ce5bdaa8ac669061097e3e2413c4` | Makes coverage-contract tests return structured errors. | Source on branch. Cargo tests NOT RUN. |
| `69f96830549fd74110fc56caa614ca4028b9e55f` | Updates JSON contract test APIs. | Source on branch. Cargo tests NOT RUN. |
| `08135f1ae010c63cec1bd7ff3a8125036a5c1576` | Adds per-stream multiline diagnostic redaction. | REJECTED by exact Sol review. Superseded by later redaction commits. |
| `63d5ef9046d4948a3cddb239e891db49be654d34` | Adds redaction for multiline diagnostic envelopes. | REJECTED by exact Sol review for five P1 findings. Superseded. |
| `42c6f8ed91571e68c080c2ed7c6ac62a045d6553` | Adds task-scoped model and effort flags in CLI load and role commands. | Source on branch. Focused Cargo tests NOT RUN. |
| `0556ce39b1abb9cd6b387583d932e1556ca9dfd4` | Honors `CODEX_HOME` during account discovery. | Reviewed with follow-ups `688057f` and `45fbb65`; runtime and Cargo tests NOT RUN. |
| `688057f40173d32dda04a55bff1e3868c219710d` | Aligns account discovery and editor paths with Codex home resolution. | Reviewed with route fix. Runtime and Cargo tests NOT RUN. |
| `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a` | Marks the test-only discovery seam `cfg(test)`; production discovery is unchanged. | Exact source review PASS. Cargo tests NOT RUN. |
| `f0f376f87317fd7b241c359d502e4a476351054d` | Adds OMP database and WAL capture fixtures and snapshot logic. | Source on branch. Snapshot tests NOT RUN. |
| `1638522184ef45f0cd51fa5601a5e80c7fd89762` | Handles an uncommitted OMP WAL reset suffix with a focused fixture. | Exact source review PASS; stale-comment follow-up and Cargo tests remain pending. |
| `8dac3d6eb651c4515ffedeefa3f616b64b07e1bf` | Carries task model and effort through runtime launch, capsule setup, dry run, and restore paths. | Source on branch. Focused Cargo tests NOT RUN; no exact independent approval is recorded here. |
| `234fc0ea3813d8cabe579a8a8e0a0b1162eb5230` | Preserves stored agent identity during restore. | Exact source review PASS. Restore tests NOT RUN. |
| `640b33f9598307a360484526a46c9c20bd068f4e` | Removes the unsupported initial-prompt option from runtime launch. | Exact source review PASS. Cargo tests NOT RUN. |
| `69b82de1a48cc18add3933e1995028c8aa2722e8` | Retains multiline secret context in diagnostic redaction. | REJECTED: nested PEM markers leaked. Superseded. |
| `c2a80a7dc0618007840533e956b06f4af3458a70` | Adds fail-closed nested PEM marker handling. | REJECTED: Clippy blocker and missing sequential valid-marker regression. Superseded. |
| `f52557d8ce10c2646d49f12f5de6ff7cbf2a1578` | Replaces denied `map_or` calls and adds sequential marker-block tests. | Exact source review PASS. Clippy and Cargo tests NOT RUN. Marker fixtures are not cryptographic PEM tests. |
| Source `c72e25d384ce2d8a80cf584457ec4b28e619980b`; destination `c8d20fb3a9660e1ed7819d53b3fbef410be43610` | Changes agent-status rule validation, its unit tests, and signed-bundle integration test. Cherry-pick attribution is preserved. | Exact source review PASS. Focused `jackin-agent-status` MBX tests NOT RUN. |
| `07f5ce7efe38c6c608fb975013df43e770d92b2b` | Updates the immutable Architect role fixture, digest provenance, and consumer contract. | Consumer source review PASS. Tests remain NOT RUN. |
| `41265a7550dda498c2e32126a9e68d4733215da5` | Sorts configured Velnor tasks by ID and updates the CI contract. | Source on branch. Current Plan fails against pinned Velnor `0.1.0`; see [consumer CI result](ci-coverage.md#current-jackin-consumer-check). |
| `a67ef88d5d9889a94696d306fffcfc5249e74ceb` | Attempts to stop forwarding ambient GitHub tokens to BuildKit; changes image build, version detection, and tests. | Exact review FAIL. Correction pending for obsolete detector API/re-export/tests and Unix-only imports. Tests NOT RUN. |
| `3b1a7789fe41e679eb9554e04862b6033cd82c94` | Removes the obsolete GitHub secret detector API, re-export, stale test, and non-Unix import issue. | Owner reports source-only checks PASS. Exact independent review, Cargo tests, and image build remain NOT RUN. |

The 13 documentation-only commits at this snapshot change files under `plans/jackin-build-split/` (including the parent snapshot `8d311ac656c68d83999f9ac2e1cb439056745199`): `34c32ca31e58b5e3dac71e88892778568f6f70d1`, `a65c1e705a002dfdec7a5f6b44a91b27a95a4e6d`, `504b7d484c3a7f00f6395dcb1a69dbc1fe32c51a`, `d194b71af4333f396b8e5e6b280823f363f3c4a7`, `9982fac76b6a3309adce28233d70007e8c102aac`, `8f21df367299af8cc22ac09954ddbfab982b6af4`, `b536e14324dfaea33201310f2727e4d2dee2dad1`, `c47114738fc1ee79715f7a53cab68daf5ed0839d`, `21fd2d73ed31f4118a1ee70b3844796a1abcd8f4`, `6fee4e9154cdec281d381edd9cd6848f5cd25660`, `2fbae01f477c61fc8bb6cf5c8f67de75eff566da`, `0bfac67e5b977a6a218a944f62f3bc92a1016d59`, and `8d311ac656c68d83999f9ac2e1cb439056745199`.

Model and effort fanout candidate `0007a691708b4af89aeb659805e829d34b69acb1` exists as a Git object but is not reachable from the task branch snapshot. `git branch --contains` lists unrelated PR refs only. It has no task destination. Do not call this fanout integrated. The task branch separately contains `42c6f8e` CLI flags and `8dac3d6` restore propagation. Root will decide whether a replacement is needed.

## Preserved original worktree state

- The original worktree is `/root/Projects/tailrocks/jackin-project/jackin` on `main`.
- It had one unrelated, unstaged `mise.lock` modification at task start.
- Its initial `mise.lock` SHA256 was `6be630be77daa3073ec74340cd6a113b969ee9117a69b8d48ebe5d5501df764c`.
- The initial tracked blob was `c78d5f9ff2b48c70d488fa9a4cfbf5f750ef11ce`.
- The task worktree copy has SHA256 `ac3b0998f110538c5bb34f2653afaf7f2713d7f9f026dde1bb3f50845cef06ef`.
- This work changes documentation files only.

## Exact branch and PR inventory

This is the 2026-10-05 branch audit. Counts use `origin/main` at `0aa821a088e1bacf3d4d85a4c9faaa67faa85132` as the base. Selected changes remain gated. No complete branch is approved for merge.

| Ref | Exact SHA and delta from main | Inventory status |
|---|---|---|
| `origin/main` | `0aa821a088e1bacf3d4d85a4c9faaa67faa85132` | Audit base. |
| `origin/codex/credential-routing-recovery-20260930`, PR [#1111](https://github.com/jackin-project/jackin/pull/1111) | `3a28c199f17da335ecd9abd8dd67ebf1aecc0421`; 38 commits: 36 non-merge and 2 sync merges; 95 files; `+8,106/-994` | Open draft; CLEAN. DCO, Required, 28 Rust jobs, Actionlint, and Plan passed; Publish baseline skipped. No review comments or threads at feedback refresh. |
| `origin/codex/ci-evidence-ledger-20260929`, PR [#1108](https://github.com/jackin-project/jackin/pull/1108) | `2990df17e25f30afca84804d9c402abc1ce00231`; 7 commits ahead, 2 behind; 29 files; `+8,229/-57` | Open draft; DIRTY against stale base. Policy, Rust dependency, xtask, ci-required, and Control checks failed. No review comments or threads at feedback refresh. |
| `origin/codex/account-usage-capsule-consolidation-20261004` | `18bc09e9536d9b662876d2fb4205357a829caa9a`; 11 commits after #1111; 49 commits reachable from main; 124 added files in this segment, `+11,777/-1,889`; full delta: 209 files, `+19,880/-2,880` | No PR. Review only selected groups below after their gates pass. |
| `origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/staged-index-jackin-1044-current-0437497d7f22` | `0437497d7f22fbdb7c8aad986be932704d707fe9`; merge-base `c52e912bd3757c4ca736be288b0e03779144b561`; 32 behind, 1 unique commit; 7 paths, `+206/-134` | Not a merge candidate. Review only the isolated Apple deployment-target change below. |

At the 2026-10-05T02:39:20Z feedback refresh, the only open PRs were #1111 and #1108. The refresh found zero reviews, comments, and review threads for both. The all-state paginated API inventory contained 1,103 PR records. A branch-ref refresh at 2026-10-05T02:20:13Z reported 1,157 refs. See the [fetch record](#fetch-and-feedback-records).

### Current open PR state and merge gate

| PR | State at refresh | Checks and feedback |
|---|---|---|
| #1111, head `3a28c199f17da335ecd9abd8dd67ebf1aecc0421` | CLEAN, draft | DCO, Required, 28 Rust jobs, Actionlint, and Plan passed; Publish baseline was skipped. Zero reviews, comments, and threads. |
| #1108, head `2990df17e25f30afca84804d9c402abc1ce00231` | DIRTY, draft; stale base | Policy, Rust dependency, xtask, ci-required, and Control checks failed. Zero reviews, comments, and threads. |

The `protect-main` ruleset requires resolved review threads, DCO and Required checks, and squash merges. It requires zero approvals. These rules do not approve either PR or waive their failing gates. Do not merge either full branch. Recheck feedback and required checks at each final head before merging an eligible PR.

### PR #1111 usage and source matrix

The exact `origin/main..origin/codex/credential-routing-recovery-20260930` graph has 38 commits: 36 non-merge commits and two sync merges. The delta is 95 files, `+8,106/-994`. The Sol matrix audit classified the non-merge commits as follows.

| Disposition | Commit group | Evidence or gate |
|---|---|---|
| ALREADY PRESENT; no-op | Sync merges `c256792e`, `0fb305e` | Preserve history only when needed; do not merge the branch for these commits. |
| ALREADY PRESENT; no-op | Usage discovery/coordinator: `f09c716`, `793663f`, `e3ff5d4`, `3664233` | The usage auditor reports no final-head diff on these commits' touched paths. Sol independently verified the net-zero result. |
| ALREADY PRESENT; no-op | Process, broker, CI, and hooks: `6b0fb2c`, `8a9ec7e`, `5eb3bc7`, `02b0c57`, `1e822df`, `1433c6a`, `3e1bb81`, `81a26f7` | The usage auditor reports no final-head diff on these commits' touched paths. Sol independently verified. |
| ALREADY PRESENT; no-op | Broker Arc test `a6c85ba8a154ccce224dce80d1ed5ef8fa03bc6e`; TODO and usage-evidence docs `dd1aa569678134e0d81301ec5131edf6f110d7ce` | Final-head per-path checks against main returned exit 0: one touched path for `a6c85ba8a154ccce224dce80d1ed5ef8fa03bc6e`, two for `dd1aa569678134e0d81301ec5131edf6f110d7ce`. Sol independently verified. |
| SELECT; pending integration | Private config, account authority, and authentication: `37c18b0`, `b349e1c`, `7f5ed14`, `0a98671`, `4830237`, `e426186`, `3a28c19` | Select only the private-config and authority changes. Treat usage-path hunks already present on main as no-ops. Require exact security review. |
| SELECT; pending integration | Runtime identity, workspace, and typed IDs: `7db8266`, `2610c9c` | Review as a cohesive runtime change and verify consumers. |
| SELECT; pending integration | Joined Console usage refresh: `53da175`, `cd119a39`, `ed5e240` | Keep the joined refresh and its documented persistence behavior together. |
| SELECT; pending integration | Durable config persistence and editor behavior: `1ea072f`, `60f7d661` | Review as a group with its affected schema consumers. |
| SELECT; pending integration | TUI facade and documentation: `e51b2b9`, `e901fa1`, `8bf933c`, `38db68d` | Recheck links and claims against current source before integration. |
| SELECT; pending prerequisites | Mixed account, identity, workspace, Console, and CLI commit `5375756fe301ba6a33d52040435b96bae6be9171` | Keep the commit attributed and intact. Require account and authority prerequisites. Do not duplicate its shared `account_config`, orchestration, or Console paths. |
| SELECT; pending exact integration review | Codex subagent defaults: `0a252f3`, `5344167` | The final config defaults spawned agents to Luna/max. The [Codex reference](https://developers.openai.com/codex/config-reference/) says explicit spawn model and effort override the default. Reviewers must still request Sol/medium explicitly. This config does not enforce role separation. No task destination or runtime config test is recorded. |
| REPLACE | Broad formatting and type changes: `3c33f29` | Do not port wholesale. Reapply only a separately justified fix against current source. |

These rows account for 36 non-merge commits and two sync merges. The audit also found no-op changes inside otherwise selected commit paths. The source path and dependency matrix remains IN PROGRESS. Do not merge the full #1111 branch. Integrate selected groups only after their dependencies and final gates pass.

The exact comparison enumerated each commit's changed paths, then compared those paths between main and the final PR head. Both comparisons returned exit `0`.

```python
import subprocess

base = "0aa821a088e1bacf3d4d85a4c9faaa67faa85132"
head = "3a28c199f17da335ecd9abd8dd67ebf1aecc0421"
commits = (
    "a6c85ba8a154ccce224dce80d1ed5ef8fa03bc6e",
    "dd1aa569678134e0d81301ec5131edf6f110d7ce",
)
for commit in commits:
    paths = subprocess.run(
        ["git", "diff-tree", "--no-commit-id", "--name-only", "-r", commit],
        check=True, text=True, capture_output=True,
    ).stdout.splitlines()
    result = subprocess.run(
        ["git", "diff", "--quiet", f"{base}..{head}", "--", *paths]
    )
    print(f"{commit}: {len(paths)} touched paths; diff exit {result.returncode}")
```

Output: `a6c85ba8a154ccce224dce80d1ed5ef8fa03bc6e: 1 touched paths; diff exit 0`; `dd1aa569678134e0d81301ec5131edf6f110d7ce: 2 touched paths; diff exit 0`.

#### Selected independent units and focused test scope

The path groups below preserve unit boundaries. Run the listed focused checks only after reviewed MBX activation. These checks have NOT RUN in this audit.

| Unit | Source paths | Focused test scope |
|---|---|---|
| Credential capture and account authority | `crates/jackin-runtime/src/runtime/launch/account_config.rs` and its `tests.rs`, `tests/bounds.rs`, `authority_tests.rs`; `crates/jackin-instance/src/auth.rs`, `auth/tests.rs`, `selected_source_tests.rs`; `crates/jackin-core/src/launch_selection.rs`; `crates/jackin-config/src/editor/accounts.rs`, `schema.rs`; Console `tui/screens/editor/model/state_impl/workspace.rs`; runtime `coordination.rs`, `launch.rs`, `attach.rs`, `cleanup.rs`, `usage_relay.rs`; `crates/jackin-protocol/src/control.rs`. | Account-config bounds/authority; auth/selected-source; `launch_runtime/tests.rs`, `attach/tests.rs`, `cleanup/tests.rs`, `apple_container/coordination_tests.rs`, `usage_relay/tests.rs`, and cross-platform configuration. |
| Typed identity and runtime consumers | `crates/jackin-core/src/container_id.rs`, `session_id.rs`, and `launch_selection.rs`; `crates/jackin-instance/src/manifest.rs`; runtime attach, cleanup, and launch phases. | `container_id/tests.rs`, `session_id/tests.rs`, `manifest/tests.rs`, `runtime/attach/tests.rs`, `runtime/cleanup/tests.rs`, `launch_phases/tests.rs`, and `crates/jackin/tests/per_mount_isolation_e2e.rs`. Avoid duplicate auth/runtime paths. |
| Mixed identity/auth/workspace commit `5375756fe301ba6a33d52040435b96bae6be9171` | Console service launch and TUI console/input/list/message/prompts; core launch selection; instance auth; runtime identity/workspace/coordination/attach/cleanup/launch/account config/restore/mounts/programmatic; usage relay; CLI load/prune/console adapter. | Authority tests: instance auth/selected-source and runtime account-config; identity tests: core launch-selection and runtime coordination/attach/cleanup/launch; consumer tests: Console/CLI and `load_options_e2e.rs`. |
| Joined Console usage refresh | `crates/jackin-console/src/tui/screens/usage.rs`, `tui/input/list.rs`, `tui/state.rs`, `tui/state/manager.rs`; `crates/jackin/src/console/adapter/run.rs`; Console command and operator-console docs. | `tui/screens/usage/tests.rs`, `tui/input/list/tests.rs`, and `crates/jackin/src/console/adapter/run/tests.rs`. No host provider changes are selected. |
| Config journal and editor | `crates/jackin-config/src/persist.rs`, `persist/tests.rs`, `editor/accounts.rs`, `schema.rs`; Console workspace editor state and tests. | Journal publication, recovery, and workspace editor tests. Keep separate from later schema migration. |
| TUI facade, docs, and defaults | `crates/jackin-tui/src/runtime.rs` and README; `crates/jackin-capsule/src/tui/runtime.rs`; TUI architecture and code-map docs; `.codex/config.toml` for model-default commits. | Check facade consumers and source links. Include model defaults only if this task requires them. |

Security-doc paths include `docs/content/(public)/(role-authoring)/developing/construct-image.mdx`, `docs/content/(public)/(role-authoring)/guides/role-repos.mdx`, `docs/content/(public)/getting-started/concepts.mdx`, `docs/content/(public)/getting-started/why.mdx`, and `docs/content/(public)/guides/security-model.mdx`. Recheck every claim against current code before integration.

Keep `5375756fe301ba6a33d52040435b96bae6be9171` intact and attributed. If cherry-picked, preserve provenance with `git cherry-pick -x`. Do not split shared account-config, orchestration, or Console hunks.

### Account and capsule consolidation matrix

The consolidation ref adds eleven commits after #1111. Its review does not approve the full 49-commit history.

| Commit | Disposition | Gate or scope |
|---|---|---|
| `c2c7154c02de16ba6eac9f3682fc8d5275910451` | SELECT; pending visual and generated-asset check | It outlines the hero byline with canonical Inter, avoiding host-font substitution. It also corrects `BrandMark` comments. Keep it separate. Verify generated output and rendering before integration. |
| `7102f0afd4546b6b47b87814df16782a1cbf998c` | REPLACE | Recheck documentation against current source and rewrite stale claims. |
| `72aa9142b2d176db7dcf3f0a463dd95432e2c246`, `ae634df85896a15a724b5af9563a440f9f7a6055` | SELECT | Review profile and security documentation against current behavior. |
| `7cbbbdf736c70a94961d3adb1176d1994a47d43e` | REPLACE | Use reviewed architecture-level correction `f52557d8ce10c2646d49f12f5de6ff7cbf2a1578`. Exact-source review passed; Clippy and Cargo tests remain NOT RUN. Keep integration pending. |
| `78e38612824c4e6d69ffb9d01a834a75537e7c5a` | REPLACE; root fix pending | The source unlinks a candidate before parent fsync. Replace it with a shared ordered-cleanup helper. Add fault-injection and persisted-state recovery tests before integration. |
| `455526b92a2a4350476bb192455e5e3414f7ab9a` | SELECT after fixtures | Add predecessor directories and schema, metadata, and golden fixtures. Run migration tests and schema checks under reviewed MBX. |
| `cd3ced4189fb19624359da8c0eec5684ef1bacd8` | REJECT current scaffolding; replacement design pending | The commit adds public profile-material hashing/getters, but no production consumer exists in that commit or current task head. Do not install the raw-getter API. Preserve only a concrete consumer need for a separately reviewed design. |
| `c72e25d384ce2d8a80cf584457ec4b28e619980b` | SELECT; integrated | Cherry-picked with `-x` as destination commit `c8d20fb3a9660e1ed7819d53b3fbef410be43610`. The trailer preserves source attribution. Three paths are listed below. Focused tests remain NOT RUN pending controlled MBX scheduling. |
| `e4bcb842bd151c608691a72c7af8fc6e765a3105` | SELECT; dependency gate pending | Keep OSC and capsule session changes together. Run the rule-bundle test gate before integration. |
| `18bc09e9536d9b662876d2fb4205357a829caa9a` | SELECT as one linked unit | Keep protocol, core, capsule, runtime, status, and isolation changes together across 20 paths. Do not select the transport type alone. |

The other consolidation candidate dispositions remain pending integration. The `c72e25d` rule-bundle unit is integrated, but its focused tests remain pending. Migration, transaction, and redaction gates are documented in [review findings](reviews.md#account-consolidation-review).

#### Consolidation paths and focused test gates

| Commit group | Paths and focused tests | Dependency gate |
|---|---|---|
| `455526b92a2a4350476bb192455e5e3414f7ab9a` | Config: `crates/jackin-config/src/accounts.rs`, `accounts/zshrc.rs`, `editor.rs`, `editor/tests.rs`, `migrations.rs`, `versions.rs`. Console: `crates/jackin-console/src/tui/auth_config.rs`, `tui/components/op_picker/lines.rs`, `tui/components/op_picker/tests.rs`, `tui/input/global_mounts/auth/tests.rs`, `tui/op_breadcrumb.rs`, `tui/op_picker.rs`, `tui/op_picker/tests.rs`, `tui/update/tests.rs`. Core: `crates/jackin-core/src/env_value.rs`, `env_value/tests.rs`, `op_cache.rs`, `op_reference.rs`, `op_types.rs`. Environment: `crates/jackin-env/src/op_cli.rs`, `op_runner.rs`, `op_struct.rs`, `picker.rs`, `resolve.rs`, `resolve/tests.rs`. Picker: `crates/jackin-oppicker/src/input.rs`, `lib.rs`, `load.rs`, `state.rs`. Runtime: `crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core/orchestrate/helpers.rs`. CLI: `crates/jackin/src/app/config_cmd.rs`, `crates/jackin/tests/manager_flow/secrets.rs`. Docs: `docs/content/reference/developer-reference/specs/op-picker.mdx`, `docs/content/reference/runtime/configuration.mdx`, `docs/content/reference/runtime/schema-versions.mdx`, `docs/content/research/platform/security/credential-exposure/jackin-exec-design.mdx`, `docs/content/research/platform/security/isolation-architecture/agent-isolation-architecture/01-threat-and-platform-evidence.mdx`. | Focus migration compatibility and schema checks. Add config `from-v1alpha12` and workspace `from-v1alpha10` predecessor directories, metadata, schema, and golden files. Include `docs/content/reference/crates/meta.json` from separate commit `38db68d6b41fa57b090224b360a643a0b5436024` only if review proves the dependency. |
| `78e38612824c4e6d69ffb9d01a834a75537e7c5a` | `Cargo.toml`, `Cargo.lock`; `crates/jackin-core/src/isolation_record.rs`, `workspace_label.rs`, `workspace_name.rs`, `workspace_name/tests.rs`; `crates/jackin-isolation/Cargo.toml`, `README.md`, `src/cleanup.rs`, `cleanup/tests.rs`, `error.rs`, `finalize/tests.rs`, `lib.rs`, `materialize.rs`, `materialize/tests.rs`, `ref_transaction.rs`, `ref_transaction/tests.rs`, `safe_remove.rs`, `safe_remove/tests.rs`, `state.rs`, `state/tests.rs`, `state_io.rs`, `state_io/tests.rs`; `crates/jackin-runtime/src/runtime/drift/tests.rs`, `runtime/launch/launch_pipeline/launch_core/orchestrate.rs`, `runtime/launch/restore.rs`, `runtime/launch/restore/tests.rs`, `runtime/launch/tests.rs`; `crates/jackin/tests/per_mount_isolation_e2e.rs`. | Keep this full unit. First supply and independently review an ordered-fsync recovery fix. Search for `sync_all`, `sync_data`, `fsync`, and `ordered-sync` found only this commit; no later first-parent fix or linked PR was found. No fix owner is assigned in the audited refs. Then run cleanup, ref, recovery, restore, and per-mount failure tests. |
| `cd3ced4189fb19624359da8c0eec5684ef1bacd8` | `crates/jackin-config/fuzz/Cargo.lock`, `crates/jackin-core/Cargo.toml`, `crates/jackin-core/src/{lib.rs,profile_material.rs}`, `crates/jackin-env/fuzz/Cargo.lock`, `crates/jackin-manifest/fuzz/Cargo.lock`, `crates/jackin-protocol/fuzz/Cargo.lock`. | Reject the unconsumed public getters and proof container. A replacement requires an identified caller, minimum evidence fields, invalidation event, and tests at that caller; no such unit is selected yet. |
| Source `c72e25d384ce2d8a80cf584457ec4b28e619980b`; destination `c8d20fb3a9660e1ed7819d53b3fbef410be43610` | `crates/jackin-agent-status/src/rules.rs`, `crates/jackin-agent-status/src/rules/tests.rs`, and `crates/jackin-agent-status/tests/signed_bundle.rs`. Destination commit includes `(cherry picked from commit c72e25d384ce2d8a80cf584457ec4b28e619980b)`. | Run `mise exec -- mbx test --locked -p jackin-agent-status` through the approved Mise/MBX environment. Default, all-feature, and integration coverage remain NOT RUN as applicable, pending the controlled MBX schedule. |
| `e4bcb842bd151c608691a72c7af8fc6e765a3105` | `crates/jackin-agent-status/src/{lib.rs,osc.rs,tests.rs}`, `crates/jackin-capsule/src/session.rs`, and `session/tests.rs`. | Integrate after `c72e25d`. Run decoder, session framing, and ingestion tests. |
| `18bc09e9536d9b662876d2fb4205357a829caa9a` | Full 20-path unit: capsule `attach_protocol.rs`, `client.rs`, `client/tests.rs`, `daemon/tests.rs`, `exec.rs`, `main.rs`, `socket.rs`, `tui/run.rs`, `tests/persistence_and_reattach.rs`; core `status.rs`; isolation `finalize.rs`; protocol `capsule_transport.rs`, `lib.rs`; runtime `apple_container.rs`, `attach.rs`, `attach/tests.rs`, `host_attach.rs`, `session_control.rs`, `snapshot.rs`; CLI `crates/jackin/src/cli/status.rs`. | Keep every consumer linked. Integrate after OSC and preferably cleanup. Run protocol, capsule client/daemon, persistence/reattach, runtime attach, and isolation-finalization tests together. |

These test lists define review scope. They do not record test execution or acceptance.

#### Fixing and integration gates

| Order | Work owner | Required predecessor or fix | State |
|---|---|---|---|
| 1 | Unassigned | Supply and review ordered-fsync recovery behavior before importing `78e38612824c4e6d69ffb9d01a834a75537e7c5a`. | IN PROGRESS; no fix ref found in the audited refs. |
| 2 | Unassigned | Add predecessor, schema, metadata, golden, and ambiguity fixtures for `455526b92a2a4350476bb192455e5e3414f7ab9a`. | IN PROGRESS; migration tests NOT RUN. |
| 3 | Unassigned | Define a consumer-backed replacement for `cd3ced4189fb19624359da8c0eec5684ef1bacd8`; reject its unused raw-getter API. | Design pending; no code or tests selected. |
| 4 | `consolidation_review` | Verify the `c72e25d384ce2d8a80cf584457ec4b28e619980b` cherry-pick provenance and run its focused tests. Review `e4bcb842bd151c608691a72c7af8fc6e765a3105` after the test gate. | Integrated at `c8d20fb3a9660e1ed7819d53b3fbef410be43610`; tests NOT RUN pending controlled MBX scheduling. |
| 5 | `execution_crosscheck` | Review the complete 20-path transport consumer unit after its predecessors. | IN PROGRESS; integration tests NOT RUN. |

### Unfinished fixes and destinations

The following items remain source candidates or incomplete root fixes. No listed correction has a task destination or a passing test result. Future work owners remain unassigned until root assigns them.

| Source or scope | Required correction | Destination, owner, and gate |
|---|---|---|
| `455526b92a2a4350476bb192455e5e3414f7ab9a` | Add config/workspace version predecessors, schema and metadata, golden output, and ambiguity fixtures. | No task destination. Future owner unassigned. Run migration compatibility and schema tests under reviewed MBX. |
| `78e38612824c4e6d69ffb9d01a834a75537e7c5a` | `ref_transaction` unlinks a candidate before parent fsync. Replace it with a shared ordered-cleanup helper. Add fault-injection and persisted-state recovery tests. | No task destination. Future owner unassigned. REPLACE the source unit after exact-source review and tests pass. |
| Kimi full-tree snapshot | Replace overcapture with projection-only data. | Source ref and destination not supplied. Future owner unassigned. Review the exact path set before integration. |
| Non-Unix source traversal | Fail closed on TOCTOU, or use handle-bound traversal. | Source ref and destination not supplied. Future owner unassigned. Add platform-specific race coverage. |
| Permanent namespace session leases | Add liveness-aware crash recovery for pending leases. | Source ref and destination not supplied. Future owner unassigned. Add restart and stale-lease tests. |
| OMP commits `f0f376f87317fd7b241c359d502e4a476351054d` and `1638522184ef45f0cd51fa5601a5e80c7fd89762` | The optimistic database/WAL pair reread is not a full SQLite snapshot or refresh-race proof. Add WAL-only committed-token and concurrent-writer replacement fixtures, or provide a proof-backed SQLite backup. | Source commits are on the task branch. The stronger proof or fix has no destination. Future owner unassigned. Tests NOT RUN. |

The redaction source replacement is `69b82de1a48cc18add3933e1995028c8aa2722e8`, `c2a80a7dc0618007840533e956b06f4af3458a70`, and reviewed `f52557d8ce10c2646d49f12f5de6ff7cbf2a1578`. It replaces rejected source `7cbbbdf736c70a94961d3adb1176d1994a47d43e` for selection purposes. The task branch contains the replacement commits. Clippy and Cargo tests remain NOT RUN, so final acceptance is pending.

The earlier candidate sequence is provisional. The `c72e25d` rule-bundle unit was selected and imported independently; its test gate remains open. The ordered-fsync fix must precede cleanup import. Migration fixtures must precede `455526…`. Reject the unused `cd3ced…` getters; any replacement needs a demonstrated consumer and a reviewed invalidation design. Keep the OSC and capsule-session pair `e4bcb…` after the `c72e25d` test gate, then review the linked transport group `18bc09…` as a whole. The mixed PR #1111 commit `5375756fe301ba6a33d52040435b96bae6be9171` remains a separate attributed unit and depends on account and authority prerequisites. The redaction replacement remains owned by `consolidation_review`.

### Recovery ref disposition

Recovery commit `0437497d7f22fbdb7c8aad986be932704d707fe9` has parent and merge-base `c52e912bd3757c4ca736be288b0e03779144b561`. It is 32 commits behind main and adds one unique commit across seven paths (`+206/-134`). Reject it as a merge base.

| Path | Disposition | Required action |
|---|---|---|
| `.github-gen/static/renovate-validate.yml` | REPLACE | Recreate coverage through current `.velnor` ownership. |
| `.github-gen/static/renovate.yml` | REPLACE | Recreate coverage through current `.velnor` ownership. |
| `.github-gen/visibility.toml` | REPLACE | Recreate visibility through its current owner. |
| `.github-gen/velnor-workflow.toml` | REJECT | Do not restore this old generator contract. |
| `.xcode-version` | REPLACE | Derive the pin from the current canonical source. |
| `crates/jackin-usage-ffi/boltffi.toml` | SELECT | Consider only `deployment_target = "26.0"`, pending current Apple CI. |
| `mise.toml` | REPLACE | Preserve removed `swift-package-native-ci` coverage through the current task owner. |

Do not merge or cherry-pick the recovery commit as a unit.

### PR #1108 check detail

These results refer to earlier head `2990df17e25f30afca84804d9c402abc1ce00231`. Exact-head Sol review is complete; fixes remain pending.

| Check | Result |
|---|---|
| Generated-tree policy | FAIL: collapsed Swift/Apple job members disagree on the Xcode pin. Adding `.xcode-version` did not resolve it. [Failed job](https://github.com/jackin-project/jackin/actions/runs/36701333261/job/109841216417) |
| `jackin-xtask` Clippy | FAIL: `crates/jackin-xtask/src/ci_evidence/ledger.rs:339` uses `Duration::from_secs(180)`; the lint requires `Duration::from_mins`. Tests and doctests were skipped. [Failed job](https://github.com/jackin-project/jackin/actions/runs/36701335714/job/109841803013) |
| Rust dependency policy | FAIL: `zlib-rs 0.6.8` uses Zlib, which the Apache-2.0/MIT-only base policy rejects without an operator ruling. [Failed job](https://github.com/jackin-project/jackin/actions/runs/36701335714/job/109841803175) |
| `ci-required` | Collected 41 of 41 artifacts. It reports the Clippy and `cargo-deny` failures. |
| Control Required | Mirrors the failed required-check result. |
| Advisory collector | `ci_evidence/github.rs:867-921` labels a generic workflow failure Product. `ci-evidence.yml:22,38-45` lets dispatch errors continue and missing files warn, so the collector can report green without an artifact. |

The collector issue is advisory evidence. It does not clear the failing required checks. Record final disposition after exact-head Sol review.

### Independent Sol review disposition

The independent source review covered exact PR head `2990df17e25f30afca84804d9c402abc1ce00231`. It does not approve the branch for merge.

| Group | Disposition | Evidence and action |
|---|---|---|
| Obsolete generator contract | REJECT | The PR reintroduces `.github-gen/velnor-workflow.toml` and `.github/ci/project.toml`, changes `.github/ci/.github-actions-generator-state`, and deletes `.velnor/config.toml` and current `.github/workflows/ci.yml`. Replace these contracts through current generator ownership. |
| Old workflow and Xcode registration | REJECT | Reject the PR's generated collector workflow registration and `.xcode-version` restoration. Port useful checks through current workflow ownership. |
| Bounded child and owned-process behavior | ALREADY PRESENT | Current main `0aa821a088e1bacf3d4d85a4c9faaa67faa85132` has bounded capture and `OwnedChild` in `crates/jackin-process/src/lib.rs:175-181,320-450`. Do not port duplicate process code. |
| Hook and Clippy fixes | ALREADY PRESENT | Hook predicate and three async-trait lint changes match current main in `crates/jackin-capsule/src/agent_status/hook_installer.rs`, `crates/jackin-capsule/src/exit_assess.rs`, and `crates/jackin-xtask/src/ci/tests.rs`. |
| Evidence collector and ledger | REPLACE | Port useful collection, provenance, immutable-attempt, atomic-output, and fixture behavior to current workflows and generator contracts. Preserve attribution and current ownership. |
| Bounded helper | SELECT | Select only a bounded helper that remains useful. Adapt it to main's process API and current owner. |
| Collector permission and failure reporting | REPLACE | Fix the collector and its validator. Do not broaden global workflow permissions. |
| Xcode pin intent and dependencies | REPLACE | Recompute dependencies and lockfile for the replacement. Preserve license policy and use supported native workflow ownership. |
| Workflow display labels | REPLACE | Preserve useful labels in current generated output. |

GitHub reported PR #1108 as conflicting and dirty against current main. Its latest reported run used merge SHA `65ba707` from older base `310e644`. That run does not validate integration with current main.

#### Stdin regression

In the PR source, `crates/jackin-process/src/lib.rs:489-555` waits for a bounded-output child without closing unused child stdin. The unbounded path closes it at `:355-358`. A child such as `cat` can wait for input indefinitely.

Add a one-second regression test. Preserve child exit and signal results. That regression test remains NOT RUN at the reviewed PR head.

#### Artifact permission

The PR collector downloads artifact ZIPs at `crates/jackin-xtask/src/ci_evidence/ledger.rs:336-343`. The generated [.github/workflows/ci-evidence.yml](https://github.com/jackin-project/jackin/blob/2990df17e25f30afca84804d9c402abc1ce00231/.github/workflows/ci-evidence.yml) grants only `contents: read` at lines 10-11. The validator at `crates/jackin-xtask/src/ci_evidence/validation.rs:941-948` rejects additional permission keys.

GitHub's [artifact download endpoint](https://docs.github.com/en/rest/actions/artifacts#download-an-artifact) requires Actions repository permission `read` for fine-grained tokens. No 403 was observed. Add `actions: read` only to the collector workflow and update its validator. Do not change global permissions.

Both collector workflows expose `workflow_dispatch`, but `ci_evidence.rs:444-459` and `producer.rs:16-19` reject dispatch. Their jobs use `continue-on-error`, and artifact upload warns when files are missing. The workflows are absent from current main's default-branch Actions list, so no dispatch run confirmed this behavior.

Remove the unsupported dispatch trigger or emit an explicit inapplicable result. Make missing artifacts an observer failure. Do not make this advisory collector a required merge check.

The classifier in `ci_evidence/github.rs:867-921` labels generic failed conclusions as Product. Such conclusions do not prove product causation. Add an unknown or data-quality category, or provide causal evidence.

#### Task invocation gate

Jackin `mise.toml` build, test, and lint root tasks still call `cargo xtask`. The PR adds `ci-evidence` and `ci-push-head-ledger` tasks that also call `cargo xtask` at `mise.toml:91-97`. Velnor's `VerificationTask` uses `mise run` only for proven non-Rust tasks.

The Rust-MBX variant and Jackin Mise-wrapper integration remain pending design and activation review. No invocation bypass fix is claimed.

The earlier `git cherry` pass found no patch-equivalent changes. Later per-commit path comparisons identified the no-op groups above. Selected integration work and final diff review remain IN PROGRESS.

## Fetch and feedback records

At `2026-10-05 02:20:13 UTC`, the branch auditor fetched all branch and pull request heads. Git reported 1,157 new refs. A paginated all-state PR query returned 1,103 records. The auditor verified heads with paginated API filters and `git ls-remote`. No raw transcript was saved.

```sh
git fetch origin '+refs/heads/*:refs/remotes/origin/*' '+refs/pull/*/head:refs/remotes/origin/pr/*'
gh api --paginate 'repos/jackin-project/jackin/pulls?state=all&per_page=100' --jq '.[].number' | wc -l
```

At `2026-10-05 02:39:20 UTC`, the open-PR feedback refresh found exactly #1111 and #1108. It found zero reviews, comments, and review threads for both.

### Later targeted fetch

- Command time: `2026-10-05T02:29:52+02:00`.
- Command result: exit code `0`; stdout was `ok fetched`.
- Evidence pointer: the following ref snapshot records the local refs.
- A separate shell transcript was not saved.
- This later targeted fetch did not query the API.

```sh
git fetch --no-prune --no-tags --no-write-fetch-head origin '+refs/heads/*:refs/remotes/origin/*' '+refs/pull/1111/head:refs/codex-inspection/pr/1111/head' '+refs/pull/1108/head:refs/codex-inspection/pr/1108/head'
```

The ref snapshot command was `git for-each-ref --format='%(objectname) %(refname)' refs/remotes/origin refs/codex-inspection/pr`.

```text
2990df17e25f30afca84804d9c402abc1ce00231 refs/codex-inspection/pr/1108/head
3a28c199f17da335ecd9abd8dd67ebf1aecc0421 refs/codex-inspection/pr/1111/head
0aa821a088e1bacf3d4d85a4c9faaa67faa85132 refs/remotes/origin/HEAD
18bc09e9536d9b662876d2fb4205357a829caa9a refs/remotes/origin/codex/account-usage-capsule-consolidation-20261004
2990df17e25f30afca84804d9c402abc1ce00231 refs/remotes/origin/codex/ci-evidence-ledger-20260929
3a28c199f17da335ecd9abd8dd67ebf1aecc0421 refs/remotes/origin/codex/credential-routing-recovery-20260930
0aa821a088e1bacf3d4d85a4c9faaa67faa85132 refs/remotes/origin/main
0437497d7f22fbdb7c8aad986be932704d707fe9 refs/remotes/origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/staged-index-jackin-1044-current-0437497d7f22
34c32ca31e58b5e3dac71e88892778568f6f70d1 refs/remotes/origin/refactor/build-split
```

Fetch status: PASS for ref synchronization only. The earlier branch auditor also compared PR heads against paginated API filters and `git ls-remote`. Neither result approves a diff or a merge.

## Owner

`branches` recorded the inventory and source dispositions. Exhaustive path accounting and fixing dependencies remain IN PROGRESS. This record does not approve whole-branch integration or merging.

See the [checklist](checklist.md) and [reviews](reviews.md).
