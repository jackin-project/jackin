# DEBRIS Ratification List — 2026-11-14

**Purpose:** one reviewable document for the operator's one-word approval of the DEBRIS branch deletions.
**Status:** READ-ONLY publication. **No branch in this document has been merged or deleted by it.** The deletion of every branch in the approval set happens only after the operator's explicit word (§4).
**Compiled:** 2026-11-14, from the merged inventory `rewrite/triage/branch-inventory-2026-11-12.md` (jackin `main`, PR #1280) plus the completed audits. **The audits are committed with this document** under `rewrite/triage/audits/2026-11-14/`: `debris-audit.md` (18-row audit: 15 CONFIRMED, 3 reclassified), `debris-ratification.md`, `smith-debris-verify.md`, `unproven-review.md`, `inventory-parity.md`, `architect-smith-audit.md` and `baseline-rows-audit.md`. They are in the repository, not only under `/tmp`, so the evidence that justifies every deletion verdict can be reviewed from any checkout and survives a cleared `/tmp`.
**Live check:** every branch name **and tip SHA** below was re-read from the live remotes with `gh api repos/<repo>/branches --paginate --jq '.[] | "\(.name) \(.commit.sha)"'` on 2026-11-14, and re-verified at 2026-11-14 13:17 and 20:12 UTC (§7). Branches that are already gone are marked **ALREADY GONE**.
**Tip guard:** every deletion command in §1 is leased against the exact tip SHA recorded in that row, so a branch that moved since this table was compiled is **not** deleted (§1).
**Target guard:** every deletion command in §1 names its repository by its **HTTPS URL** (`https://github.com/<repo>.git`), never by the remote alias `origin`, so a clone whose `origin` points at a fork can never delete a ref in the wrong repository (§1, §1b).
**Absent-path accounting:** §1a accounts for **every** path that each of the 15 deletions would remove, re-derived against the live `main` tips; §1b gives the commands that reproduce the numbers and names the `main` tips they were measured against.

## Contents

| § | section | rows |
|---|---|---|
| §1 | Approval set — CONFIRMED DEBRIS | 15 |
| §1a | Complete absent-path accounting for all 15 rows | 15 |
| §1b | The measurement this document rests on — repos, `main` tips, commands | — |
| §2 | Rows OUTSIDE the approval set — DEBRIS-class (superseded), each needs its own word | 4 |
| §3 | CONTAINED candidates — containment UNPROVEN (kept, NOT deletable) | 4 |
| §4 | The operator's one-word approval text | — |
| §5 | Nothing merged, nothing deleted | — |
| §6 | Recorded deletion — correction of the inventory's baseline count | 1 |
| §7 | Live verification against the remotes | 15 + 9 + 4 + 50 |
| §8 | Excluded rows (VALUABLE / ALREADY GONE — must not be deleted) | 6 |

---

## 1. Approval set — CONFIRMED DEBRIS (15 branches)

These 15 are the **only** branches the operator's word covers. Each row carries its own one-line evidence and the exact deletion command. The command **names the repository by its HTTPS URL**, not by a remote alias, so it deletes the ref in the repository named in the row whatever the local `origin` points at; it can be run from any clone of that repository (or any clone at all) and it deletes exactly one branch. Every path that the deletion would remove is accounted for in §1a.

**Every deletion command is tip-guarded.** Each command carries `--force-with-lease=refs/heads/<branch>:<recorded-tip-SHA>`, so the remote ref must still point at exactly the SHA recorded in the `tip` column when the delete runs. If the branch has gained a commit since this table was compiled, the push is **rejected** (`stale info`) and nothing is deleted; if the ref is already gone, the push **errors** (`remote ref does not exist`). The recorded tip is the audited one (§7) — run the pre-flight below and stop if a live tip differs from the recorded one.

| # | repo | branch | tip | one-line evidence | exact deletion command |
|---|---|---|---|---|---|
| 1 | jackin-project/jackin | `chore/triage-inventory-20261112` | `e13360eb87518f70c45097366e384851f113ad8d` | 1 commit adds only the generated triage artifact `rewrite/triage/branch-inventory-2026-11-12.md` (9148 B), superseded by the merged inventory in main (`git cherry` 1+/0-). 6 paths are absent from `main`'s tree: that artifact, whose content never occurs in `main`'s history, and 5 usage fixtures whose content **is** in `main`'s history (§1a). | `git push --force-with-lease=refs/heads/chore/triage-inventory-20261112:e13360eb87518f70c45097366e384851f113ad8d https://github.com/jackin-project/jackin.git --delete chore/triage-inventory-20261112` |
| 2 | jackin-project/jackin | `docs/pr1228-replacement-20261026` | `912fe737e08fb540be67573ce8b086b30c77ec0c` | 62 of the 63 `docs/openshell-v2/**` paths are absent from `main`'s tree; 58 of them are the branch's own pre-reflow draft (their content never occurs in `main`'s history) and 4 have their content in `main`'s history. The pack itself was merged as PR #1234 (`3ffd6171a`) — that commit is this branch's merge base, so the 58 files are the draft of the pack that landed; docs-only, no product code (§1a). | `git push --force-with-lease=refs/heads/docs/pr1228-replacement-20261026:912fe737e08fb540be67573ce8b086b30c77ec0c https://github.com/jackin-project/jackin.git --delete docs/pr1228-replacement-20261026` |
| 3 | jackin-project/jackin | `recovery/openrouter-exact-money-20261027` | `6ebbe14490b2eddac73263b2df248252e1b10546` | Product fix landed byte-identically in merged PR #1238 (`f16eee70e`, `money_numeric.rs` = `06aa1044`, blob present in `main`'s history); branch residue is comments/extra tests in trees removed by #1264. The residue is enumerated in §1a: 22 paths whose content never occurs in `main`'s history, all of them pre-rewrite files of the crates the OpenShell rewrite replaced (`crates/core/jackin-protocol`, `crates/services/jackin-usage*`, `crates/ui/jackin-console`) plus `ratchet.toml`. | `git push --force-with-lease=refs/heads/recovery/openrouter-exact-money-20261027:6ebbe14490b2eddac73263b2df248252e1b10546 https://github.com/jackin-project/jackin.git --delete recovery/openrouter-exact-money-20261027` |
| 4 | jackin-project/jackin | `recovery/stash-2f8cc42b-20261023` | `e5e6d1fd27efc3eee52b7e18574e4870c5a4a191` | Stash snapshot, 17 paths, **0 truly unique** blobs (all present in main's history or another ref); single parent, no side-parent risk. | `git push --force-with-lease=refs/heads/recovery/stash-2f8cc42b-20261023:e5e6d1fd27efc3eee52b7e18574e4870c5a4a191 https://github.com/jackin-project/jackin.git --delete recovery/stash-2f8cc42b-20261023` |
| 5 | jackin-project/jackin | `recovery/stash-2fb075f8-20261023` | `4d9540abf301f80b58f81f47d7691132ce48f36c` | Stash snapshot, 48 changed paths. **13** of them are absent from `main`'s tree *and* their content never occurs in `main`'s history (§1a): 8 pre-rewrite `crates/adapters/jackin-image/src/release_evidence/**` files, 2 `crates/tools/jackin-xtask/src/ci/**` files, 1 docs page, 1 integration doc and 1 shell script. The 10 `crates/**` paths are edits to modules the OpenShell rewrite (#1264) deleted, and their content is **not** in `main` — it is shared with row 8 (identical blobs), so deleting rows 5 and 8 together removes it. Single parent. | `git push --force-with-lease=refs/heads/recovery/stash-2fb075f8-20261023:4d9540abf301f80b58f81f47d7691132ce48f36c https://github.com/jackin-project/jackin.git --delete recovery/stash-2fb075f8-20261023` |
| 6 | jackin-project/jackin | `recovery/stash-88f5643c-20261023` | `ddf8861f45646f34b1ca087bd8ed6b9429f270c1` | Stash snapshot, 17 paths, **0 truly unique**; single parent. | `git push --force-with-lease=refs/heads/recovery/stash-88f5643c-20261023:ddf8861f45646f34b1ca087bd8ed6b9429f270c1 https://github.com/jackin-project/jackin.git --delete recovery/stash-88f5643c-20261023` |
| 7 | jackin-project/jackin | `recovery/stash-be51ff69-20261023` | `9cdf189b812caf2395048bf0d8dbccdadf1e83a0` | Stash snapshot, 60 changed paths; **4** paths are absent from `main`'s tree *and* their content never occurs in `main`'s history (§1a) = `native/README.md`, `ratchet.toml`, 2 shell scripts — no product code; single parent (RUSTSEC claim already dropped as unsupported). | `git push --force-with-lease=refs/heads/recovery/stash-be51ff69-20261023:9cdf189b812caf2395048bf0d8dbccdadf1e83a0 https://github.com/jackin-project/jackin.git --delete recovery/stash-be51ff69-20261023` |
| 8 | jackin-project/jackin | `recovery/stash-d808d094-20261023` | `00f98f32514aca8005c56dd2e44e3fc03c326b46` | Stash snapshot, 54 changed paths; **13** paths are absent from `main`'s tree *and* their content never occurs in `main`'s history (§1a) — the same 10 pre-rewrite `crates/**` files as row 5 (identical blobs) plus 1 docs page and 2 `scripts/*` shell wrappers. The content is not in `main`; it is shared with row 5. Single parent. | `git push --force-with-lease=refs/heads/recovery/stash-d808d094-20261023:00f98f32514aca8005c56dd2e44e3fc03c326b46 https://github.com/jackin-project/jackin.git --delete recovery/stash-d808d094-20261023` |
| 9 | jackin-project/jackin | `recovery/strict-merge-gate-merge-readiness-20261022` | `2f96c6dadd4e1974f89f5a5f7407c0f6ff221fae` | Stale pre-rewrite CI gate config, 25 paths, **0 truly unique**; `check:merge-readiness` does not exist in main; duplicate of the `fix/restore-docs-source-globs` lineage. | `git push --force-with-lease=refs/heads/recovery/strict-merge-gate-merge-readiness-20261022:2f96c6dadd4e1974f89f5a5f7407c0f6ff221fae https://github.com/jackin-project/jackin.git --delete recovery/strict-merge-gate-merge-readiness-20261022` |
| 10 | jackin-project/jackin | `rollout/velnor-3-provider` | `092fe1db796367479029d4e94379530f7f5965c0` | Orphan line (no merge base with main, root `c24fffeb0` != main root); the tip tree is 21 paths, 20 of them absent from `main`'s tree with content that never occurs in `main`'s history. 9 of the 20 are **byte-identical** (same blob SHA) to files in `ChainArgos/jackin-agent-brown` (`.dockerignore`, `.hadolint.yaml`, `.pre-commit-config.yaml`, `Dockerfile`, `hooks/preflight.sh`, `hooks/source.sh`, `jackin.role.toml`, `scripts/marketplace-audit.sh`, `zshrc.d/docker-helpers.zsh`); the other 11 are velnor-generated CI files (`.github-gen/velnor-workflow.toml`, `.github/actionlint.yaml`, `.github/ci/project.toml`, `.github/ci/.github-actions-generator-state` and 7 rendered `.github/workflows/*.yml`). No product code (§1a). | `git push --force-with-lease=refs/heads/rollout/velnor-3-provider:092fe1db796367479029d4e94379530f7f5965c0 https://github.com/jackin-project/jackin.git --delete rollout/velnor-3-provider` |
| 11 | jackin-project/jackin-agent-smith | `recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a` | `b9db5b149cc46baba9c49549432307c29e3972b0` | Pure velnor generator churn. **12** paths are absent from `main`'s tree, of which **7** have content that never occurs in `main`'s history: the generator source `.github-gen/velnor-workflow.toml`, the generator state `.github/ci/.github-actions-generator-state` and **5** rendered workflows (`.github/workflows/{ci-main,ci-policy,ci-pr,ci-unit-docker,maintenance}.yml`); the other 5 (`.err`, `.github/actionlint.yaml`, `.github/ci/project.toml`, `scripts/marketplace-audit.sh` and `.github/workflows/nightly.yml`) have their content in `main`'s history — `nightly.yml` is inherited unchanged from the merge base `8c1dbe3d`, an ancestor of `main` (blob `7db67da1` is in `main`'s object history), so it is not a branch-only blob (§1a). Own PR #206 closed unmerged; superseded by merged PR #207 (`08cb1c2f`). | `git push --force-with-lease=refs/heads/recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a:b9db5b149cc46baba9c49549432307c29e3972b0 https://github.com/jackin-project/jackin-agent-smith.git --delete recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a` |
| 12 | jackin-project/jackin-agent-smith | `recovery/20261006/jackin-agent-smith/2e7119b9c668/jackin-agent-smith` | `c58cfdcaaca85f606c2abc4ddb8f5afaae47f737` | "WIP preservation snapshot" commit with an **empty patch** (tip tree == parent tree); parent `2e7119b9` is an ancestor of main; 3-dot diff vs main = 0 files. 3 paths are absent from `main`'s tree and all 3 have their content in `main`'s history (§1a). | `git push --force-with-lease=refs/heads/recovery/20261006/jackin-agent-smith/2e7119b9c668/jackin-agent-smith:c58cfdcaaca85f606c2abc4ddb8f5afaae47f737 https://github.com/jackin-project/jackin-agent-smith.git --delete recovery/20261006/jackin-agent-smith/2e7119b9c668/jackin-agent-smith` |
| 13 | tailrocks/terminal-components-baseline | `test/pre-refactor-protection` | `fc624b7456132fb74675ab452e6b64ecb3f69a70` | 666 commits ahead but 3011/3045 changed paths are blob-identical to main; exactly **1** path absent from main = `terminal-components-baseline-goal.txt` (scratch goal prompt) and its content never occurs in `main`'s history (§1a). | `git push --force-with-lease=refs/heads/test/pre-refactor-protection:fc624b7456132fb74675ab452e6b64ecb3f69a70 https://github.com/tailrocks/terminal-components-baseline.git --delete test/pre-refactor-protection` |
| 14 | tailrocks/terminal-components-baseline | `wip/tuiscotti-capture-only` | `0b8b8bb077061eee6ca04466148f017e316edfd3` | **9517** paths absent from `main`'s tree. **9117** of them have content that never occurs in `main`'s history: 7236 `run-evidence/**` generated capture files, 1880 legacy `tests/functional/baselines/**/*.tui.json` baselines and the machine-local `.cargo/config.toml`. The other **400** have their content in `main`'s history: 260 `run-evidence/**` files, 98 `.tui.json` baselines that PR #6 (`1d7f63db`) deleted from `main`, 34 `src/bin/jackin/**` files that `43456dee` moved to `apps/jackin/src/bin/jackin/**` and 8 `scripts/coverage-fixtures/**` files. 9117 + 400 = 9517 — every absent path is accounted for (§1a). **0** branch-only commits touch `src`/`apps`. | `git push --force-with-lease=refs/heads/wip/tuiscotti-capture-only:0b8b8bb077061eee6ca04466148f017e316edfd3 https://github.com/tailrocks/terminal-components-baseline.git --delete wip/tuiscotti-capture-only` |
| 15 | tailrocks/terminal-components-baseline | `wip/tuiscotti-fixcheck` | `8d352ebfb932b06bf18c537e924389b0aae07e42` | **2046** paths absent from `main`'s tree. **1904** of them have content that never occurs in `main`'s history: 1880 legacy `tests/functional/baselines/**/*.tui.json` baselines, 23 `run-evidence/fix-validation/**` files and the machine-local `.cargo/config.toml`. The other **142** have their content in `main`'s history: 2 `run-evidence/fix-validation/**` files, 98 `.tui.json` baselines that PR #6 (`1d7f63db`) deleted from `main`, 34 `src/bin/jackin/**` files that `43456dee` moved to `apps/jackin/src/bin/jackin/**` and 8 `scripts/coverage-fixtures/**` files. 1904 + 142 = 2046 — every absent path is accounted for (§1a). **0** branch-only commits touch `src`/`apps`. | `git push --force-with-lease=refs/heads/wip/tuiscotti-fixcheck:8d352ebfb932b06bf18c537e924389b0aae07e42 https://github.com/tailrocks/terminal-components-baseline.git --delete wip/tuiscotti-fixcheck` |

**Counts:** 15 branches = 10 `jackin-project/jackin` + 2 `jackin-project/jackin-agent-smith` + 3 `tailrocks/terminal-components-baseline`.

### Pre-flight and post-check (read-only)

```bash
# pre-flight: confirm the 15 tips (name AND tip SHA) before deleting
for r in jackin-project/jackin jackin-project/jackin-agent-smith tailrocks/terminal-components-baseline; do
  gh api "repos/$r/branches" --paginate --jq '.[] | "\(.name) \(.commit.sha)"'
done

# every tip above must equal the SHA in the §1 `tip` column of the row it belongs to.
# If any differs, STOP: that branch moved since the audit, its content was not audited,
# and its deletion command is refused by the --force-with-lease guard anyway.

# target check: the §1 commands name their repository by URL, so no remote alias is used.
# If you prefer the alias `origin`, it must be the row's repository - check it first:
git remote get-url --push origin        # must print the repository named in the row

# post-check: the 15 refs must be gone; every excluded / contained / VALUABLE branch must still be there
gh api repos/jackin-project/jackin/branches --paginate --jq '.[] | "\(.name) \(.commit.sha)"'
gh api repos/jackin-project/jackin-agent-smith/branches --paginate --jq '.[] | "\(.name) \(.commit.sha)"'
gh api repos/tailrocks/terminal-components-baseline/branches --paginate --jq '.[] | "\(.name) \(.commit.sha)"'
```

Expected after the deletions (live counts re-read 2026-11-14 20:12 UTC, §7): jackin **70 → 60** branches (69 non-main → 59 non-main), smith **3 → 1**, baseline **5 → 2**. The jackin number grew from 24 to 70 between the two re-checks because other work in the same repository pushed branches in the meantime (§7, "Branches outside every class list"); those branches are **not** in the approval set and the operator's word does not cover them. This document's own branch `docs/debris-ratification-2026-11-14` is counted in the 70 and disappears only if the PR is merged and its branch deleted.

---

## 1a. Complete absent-path accounting for all 15 rows

**Every path that a deletion would remove is accounted for below.** The numbers are re-derived from the
live repositories — not inherited from the audits — against the `main` tips named in §1b.

Four numbers per row:

- **absent from `main`** — the path is in the branch tip's tree and not in `main`'s tree.
- **content not in `main`'s history** — of the absent paths, the ones whose blob never occurs in
  `main`'s object history (`git rev-list --objects main`). These are the paths whose *content* a
  deletion removes; every other absent path's content is already in `main` — in a file `main` deleted,
  under another name, or in an earlier revision.
- **same path in `main`, different revision** — the paths `main` still holds in another revision. The
  branch's revision of them is either preserved in `main`'s object history or is branch-only; the third
  table below accounts for them.
- **in no retained ref** — of those branch-only paths, the ones no surviving ref holds either. A
  **retained ref** is every ref that survives the operation: `main` plus every branch of the repository
  that is not one of the 15 rows of §1. The fourth table below accounts for the rest of what a deletion
  removes: the objects that only the branch's **history** holds, including content that its earlier
  commits carried and that is no longer in the tip tree.

| # | repo | absent from `main` | content not in `main`'s history | what those paths are |
|---|---|---|---|---|
| 1 | jackin-project/jackin | 6 | 1 | `rewrite/triage/branch-inventory-20261112.md` — the unhyphenated twin of the merged inventory `rewrite/triage/branch-inventory-2026-11-12.md` (PR #1280, `78d3b86e`): same rows, counts and sections, different filename. The other 5 absent paths (usage fixtures) have their content in `main`'s history. |
| 2 | jackin-project/jackin | 5396 | 58 | all 58 under `docs/openshell-v2/**`: the 12 pack documents (`README.md`, `PLAN.md`, `TASKS.md`, `CONTRACTS.md`, `ACCEPTANCE.md`, `PACK-CHECKS.md`, `SUBAGENTS.md`, `acceptance-cases.json`, `task-graph.json`, `traceability.json`, `TRACEABILITY.md`, `validate_pack.py`) and 46 task files (`tasks/P00-P05`, `tasks/T10-T40`, `tasks/U00-U08`). The branch's own pre-reflow draft of the pack merged as PR #1234 (`3ffd6171a`) — that commit is this branch's merge base. |
| 3 | jackin-project/jackin | 5409 | 22 | pre-rewrite product files of the crates the OpenShell rewrite (#1264) replaced: `crates/core/jackin-protocol/src/{control.rs,control/numeric_tests.rs,control/tests/case_01.rs,usage_broker.rs,usage_broker/numeric_tests.rs,usage_broker/tests/case_01.rs}` (6), `crates/services/jackin-usage-broker-publish/src/quota{,.tests}.rs` (2), `crates/services/jackin-usage-provider-core/src/quota{,.tests}.rs` (2), `crates/services/jackin-usage-provider-openrouter/{Cargo.toml,README.md,src/lib.rs,src/numeric_tests.rs,src/tests/case_01.rs,src/tests/case_02.rs,src/tests/support.rs}` (7), `crates/services/jackin-usage/src/usage.rs` (1), `crates/ui/jackin-console/src/tui/screens/usage{,.numeric_tests}.rs` and `usage/tests/case_03.rs` (3), `ratchet.toml` (1). The money fix itself is **not** in this set — it landed byte-identically (PR #1238, `money_numeric.rs` = `06aa1044`). |
| 4 | jackin-project/jackin | 4818 | 0 | every absent path's content is in `main`'s history — the 17-path stash snapshot adds nothing that `main` does not already hold. |
| 5 | jackin-project/jackin | 4787 | 13 | 8 `crates/adapters/jackin-image/src/release_evidence/**` files, 2 `crates/tools/jackin-xtask/src/ci/{tests.rs,workflow_lint.rs}` files, `docs/content/(public)/getting-started/verifying-releases.mdx`, `docs/integration/immutable-preview-publication.md`, `scripts/test-pre-commit-snapshot`. The 10 `crates/**` paths are edits to modules the OpenShell rewrite (#1264) deleted; their content is **not** in `main` and is shared with row 8 (identical blobs). |
| 6 | jackin-project/jackin | 4794 | 0 | every absent path's content is in `main`'s history. |
| 7 | jackin-project/jackin | 4755 | 4 | `native/README.md`, `ratchet.toml`, `scripts/hooks-install`, `scripts/test-pre-commit-snapshot` — no product code. |
| 8 | jackin-project/jackin | 4750 | 13 | the same 10 pre-rewrite `crates/**` files as row 5 (identical blobs) plus `docs/content/(public)/getting-started/verifying-releases.mdx`, `scripts/pre-commit-snapshot`, `scripts/test-pre-commit-snapshot`. The content is not in `main`; it is shared with row 5. |
| 9 | jackin-project/jackin | 4725 | 0 | every absent path's content is in `main`'s history; `check:merge-readiness` does not exist in `main`. |
| 10 | jackin-project/jackin | 21 | 20 | 9 byte-identical to files in `ChainArgos/jackin-agent-brown` and 11 velnor-generated CI files (§1 row 10). The 21st absent path, `config/fleet/velnor-host.env`, has its content in `main`'s history. |
| 11 | jackin-project/jackin-agent-smith | 12 | 7 | the generator source `.github-gen/velnor-workflow.toml`, the generator state `.github/ci/.github-actions-generator-state` and 5 rendered workflows (§1 row 11). The other 5 absent paths — `.err`, `.github/actionlint.yaml`, `.github/ci/project.toml`, `scripts/marketplace-audit.sh` and `.github/workflows/nightly.yml` — have their content in `main`'s history; `nightly.yml` is inherited unchanged from the merge base `8c1dbe3d`, an ancestor of `main` (blob `7db67da1`). |
| 12 | jackin-project/jackin-agent-smith | 3 | 0 | `.err` (empty file), `.github/actionlint.yaml`, `scripts/marketplace-audit.sh` — all three have their content in `main`'s history; the tip commit's patch is empty. |
| 13 | tailrocks/terminal-components-baseline | 1 | 1 | `terminal-components-baseline-goal.txt` — the scratch goal prompt. |
| 14 | tailrocks/terminal-components-baseline | 9517 | 9117 | see the breakdown below. |
| 15 | tailrocks/terminal-components-baseline | 2046 | 1904 | see the breakdown below. |

**Rows 14 and 15, path by path.** Every one of the 9517 + 2046 absent paths falls in exactly one of these
buckets, and the buckets sum to the totals, so no path is left unclassified.

| bucket | row 14 | row 15 | what it is |
|---|---|---|---|
| `run-evidence/**` — content not in `main`'s history | 7236 | 23 | generated capture output (frame dumps, action logs, HTML reports, `.sha256` manifests) |
| `tests/functional/baselines/**/*.tui.json` — content not in `main`'s history | 1880 | 1880 | legacy JSON baselines the branch added; `main` never accepted them and PR #6 (`1d7f63db`) migrated `main`'s own baselines to `.tui.pb` |
| `.cargo/config.toml` — content not in `main`'s history | 1 | 1 | machine-local absolute patch path (`/private/tmp/tuiscotti-goal-current-20261106/...`) |
| `run-evidence/**` — content in `main`'s history | 260 | 2 | capture files whose bytes also occur in `main`'s history |
| `tests/functional/baselines/**/*.tui.json` — content in `main`'s history | 98 | 98 | baselines `main` held and deleted in PR #6 (`1d7f63db`) |
| `src/bin/jackin/**` — content in `main`'s history | 34 | 34 | the demo binary, moved to `apps/jackin/src/bin/jackin/**` by `43456dee` and rewritten since |
| `scripts/coverage-fixtures/**` — content in `main`'s history | 8 | 8 | scratch fixtures whose bytes occur in `main`'s history |
| **total absent from `main`'s tree** | **9517** | **2046** | |
| of which content not in `main`'s history | 9117 | 1904 | the paths whose bytes a deletion removes |
| of which content in `main`'s history | 400 | 142 | the paths whose bytes `main` already holds |

**Paths that exist in `main` under the same name, with a different revision.** A path that `main`
still holds can also carry branch content: the branch's revision of it is not in `main`'s tree. The third
bucket closes that gap — for every path that exists in `main`'s tree with a **different blob**, the
branch's revision is either preserved in `main`'s object history (an earlier revision of the same file, so
nothing is lost) or it is branch-only.

| # | repo | same path in `main`, different revision | of which the branch's revision is preserved in `main`'s history | branch-only revisions (paths · lines) |
|---|---|---|---|---|
| 1 | jackin-project/jackin | 87 | 87 | 0 · 0 |
| 2 | jackin-project/jackin | 100 | 100 | 0 · 0 |
| 3 | jackin-project/jackin | 100 | 100 | 0 · 0 |
| 4 | jackin-project/jackin | 97 | 94 | 3 · 1583 |
| 5 | jackin-project/jackin | 97 | 96 | 1 · 195 |
| 6 | jackin-project/jackin | 97 | 96 | 1 · 4598 |
| 7 | jackin-project/jackin | 97 | 95 | 2 · 4730 |
| 8 | jackin-project/jackin | 94 | 88 | 6 · 5710 |
| 9 | jackin-project/jackin | 92 | 90 | 2 · 315 |
| 10 | jackin-project/jackin | 6 | 1 | 5 · 208 |
| 11 | jackin-project/jackin-agent-smith | 10 | 10 | 0 · 0 |
| 12 | jackin-project/jackin-agent-smith | 15 | 15 | 0 · 0 |
| 13 | tailrocks/terminal-components-baseline | 140 | 137 | 3 · 95 |
| 14 | tailrocks/terminal-components-baseline | 334 | 89 | 245 · 3939 |
| 15 | tailrocks/terminal-components-baseline | 334 | 89 | 245 · 3972 |

"Lines" are the lines the branch's revision adds over `main`'s revision of the same path
(`git diff --numstat <main-blob> <branch-blob>`). The audit's own capture-time figures for the three
baseline rows are smaller because they were measured on the paths the branch **edited** relative to its
merge base, not on the whole tip tree: `rewrite/triage/audits/2026-11-14/baseline-rows-audit.md:7-9`
records 33 blob-different paths with 876 branch-only lines for row 13, and 259 blob-different paths
(57 source files, 609 branch-only lines) for rows 14-15. The rows above are the whole-tree numbers and
therefore a superset of the audit's. The largest branch-only revisions in rows 14-15 are
`.github/workflows/ci.yml` (734 lines, the pre-rewrite CI workflow), the capture-harness cases
`tests/functional/cases/x02_exit.rs` (158) and `x03_color.rs` (59), `tests/functional/support.rs` (63),
the test documentation `docs/testing/**` and `docs/ci.md` (71) — the same residue profile the audit
recorded ("capture-harness lines", "pre-refactor variants").

**What else the deletion removes: the objects only the branch's history holds.** A ref deletion makes
the whole history of the branch unreachable, not just its tip tree. The table below counts the objects
that no retained ref holds (`git rev-list --objects "$T" --not <every retained ref>`) — the commits that
exist only on the branch and the trees and blobs they carry, including content that earlier commits held
and the tip tree no longer has.

| # | repo | tip-tree paths whose content is in no retained ref | objects only this branch holds (of which commits) |
|---|---|---|---|
| 1 | jackin-project/jackin | 1 | 5 (1) |
| 2 | jackin-project/jackin | 58 | 75 (3) |
| 3 | jackin-project/jackin | 22 | 335 (25) |
| 4 | jackin-project/jackin | 0 | 30 (1) |
| 5 | jackin-project/jackin | 13 | 91 (1) |
| 6 | jackin-project/jackin | 0 | 30 (1) |
| 7 | jackin-project/jackin | 4 | 163 (9) |
| 8 | jackin-project/jackin | 13 | 98 (1) |
| 9 | jackin-project/jackin | 0 | 119 (7) |
| 10 | jackin-project/jackin | 18 of 20 | 1065 (223) |
| 11 | jackin-project/jackin-agent-smith | 7 | 13 (1) |
| 12 | jackin-project/jackin-agent-smith | 0 | 1 (1) |
| 13 | tailrocks/terminal-components-baseline | 1 | 10643 (666) |
| 14 | tailrocks/terminal-components-baseline | 9117 | 14047 (649) |
| 15 | tailrocks/terminal-components-baseline | 1904 | 7408 (649) |

The retained set used for the measurement (2026-11-14 22:09 UTC) was: for `jackin-project/jackin`,
`main` plus the 60 branches that are not one of the 10 jackin rows of §1; for
`jackin-project/jackin-agent-smith`, `main` (both other branches are rows of §1); for
`tailrocks/terminal-components-baseline`, `main` and `preserve/local-unique-20261109` (the third
non-`main` branch is row 13).

**What this means for the operator.** Rows 13-15 carry hundreds of commits whose objects only the branch
holds (666, 649 and 649 commits). Their *content* is accounted for above — generated capture output, legacy
baselines and scratch fixtures — but their **history** is not recoverable after the deletion. If the
operator wants the history as well as the content, the branch can be preserved instead of deleted:

```bash
git push https://github.com/<repo>.git <tip>:refs/heads/archive/<branch>
```

A retention ref under `refs/heads/archive/` keeps every object reachable, and the audit's own guidance
for the rows of §2 is the same. Nothing in this document creates such a ref; the operator's word decides.

**What the accounting says.** For rows 1, 2, 4, 6, 7, 9, 12 and 13 the content a deletion removes is
documentation, configuration or scratch — no product code. For rows 3, 5, 8, 10 and 11 the content is
**not** in `main`: it is a pre-rewrite revision of modules the OpenShell rewrite (#1264, `f683619b5`)
deleted, the branch's own draft of a pack that landed (PR #1234), velnor generator output, or the demo
binary's old layout. The merged inventory records no port candidate for any of it. **If the operator wants
any of it, it must be extracted before the deletion** — the commands in §1 destroy the only copy of those
bytes.

---

## 1b. The measurement this document rests on

**Repositories and `main` tips.** Every number in §1a was measured against these tips:

| repo | `main` tip used |
|---|---|
| `jackin-project/jackin` | `926bbd49d171c2f389a766776ff3c418f8987fbd` |
| `jackin-project/jackin-agent-smith` | `a9b8c8aea74dc2c777eb8df23ea22647c8ceaf40` |
| `tailrocks/terminal-components-baseline` | `8063050cdb483e31284c107808b207c92e2d3203` |

**The commands** (one row of §1; row 14 as the worked example). `M` is **pinned to the recorded SHA**,
not to `origin/main`: once `main` advances, `M` keeps the measurement reproducible, and re-measuring
against a later `main` is a deliberate act that changes the SHA in the table above.

```bash
# the retained-ref list lives in this repository - take its path before the clone moves us
RETAINED_FILE="$PWD/rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt"
repo=https://github.com/tailrocks/terminal-components-baseline
git clone --quiet "$repo" /tmp/tcb && cd /tmp/tcb && git fetch origin
T=0b8b8bb077061eee6ca04466148f017e316edfd3                 # the §1 row's tip
M=8063050cdb483e31284c107808b207c92e2d3203                 # the main tip named above, pinned

# the measurement is pinned to that commit, not to the moving branch name:
git fetch origin "$M" --quiet                              # fetch that exact commit
test "$(git rev-parse "$M^{commit}")" = "$M" || { echo "main tip $M is not in this clone"; exit 1; }

# every path of the branch tip that does not exist in main's tree
git ls-tree -r "$T" | awk '{print $4" "$3}' | sort > /tmp/tip.txt
git ls-tree -r "$M" | awk '{print $4" "$3}' | sort > /tmp/main-tree.txt
join -1 1 -2 1 -v1 /tmp/tip.txt /tmp/main-tree.txt > /tmp/absent.txt
wc -l < /tmp/absent.txt                                    # 9517

# the blob set of main's whole object history
git rev-list --objects "$M" | awk '{print $1}' | sort -u > /tmp/main-blobs.txt

# of the absent paths, the ones whose content never occurs in main's history
while read -r p b; do grep -qx "$b" /tmp/main-blobs.txt || echo "$p"; done < /tmp/absent.txt | wc -l

# the third bucket: paths that main still holds, with a different revision
join -1 1 -2 1 /tmp/tip.txt /tmp/main-tree.txt | awk '$2 != $3' > /tmp/differing.txt
wc -l < /tmp/differing.txt                        # 334
while read -r p tb mb; do grep -qx "$tb" /tmp/main-blobs.txt || echo "$p"; done < /tmp/differing.txt | wc -l

# the fourth bucket: the objects only this branch's history holds.
# RETAINED = every ref that survives the operation: main + every branch of the repository
# that is not one of the 15 rows of §1. The refs and their tips are captured in the
# repository, so the subtraction does not move when a retained branch advances:
#   rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt  (<repo> <ref> <sha>)
RETAINED=$(awk '$1=="tailrocks/terminal-components-baseline" {print $3}' "$RETAINED_FILE" | tr '\n' ' ')
git fetch origin --quiet                 # make the pinned tips available in the clone
git rev-list --objects "$T" --not $RETAINED | wc -l        # 14047 objects for row 14
```

For the other two repositories, replace the repository name in the `awk` filter with
`jackin-project/jackin` (rows 1-10, 60 retained refs) or `jackin-project/jackin-agent-smith` (rows 11-12,
1 retained ref). The snippet as written reproduces the row-14 number **14047** and, with the jackin
filter, the row-10 number **1065** — both were measured with it.

**The audits.** The evidence that produced these verdicts is committed with this document under
`rewrite/triage/audits/2026-11-14/`: `debris-audit.md` (the 18-row audit, its method and its per-row
verdicts), `debris-ratification.md` (the list before this publication), `smith-debris-verify.md` (the
jackin-agent-smith rows), `unproven-review.md` (the per-path proof for the 4 CONTAINED candidates of §3),
`inventory-parity.md`, `architect-smith-audit.md` and `baseline-rows-audit.md`, plus
`retained-refs-2026-11-14.txt` (the retained refs and their pinned tips that the fourth bucket of §1a
was measured against). They were written against
`main` `7a53f6abe` / `78d3b86e`; §1a re-derives the numbers the deletion verdicts rest on against the tips
named above, so the verdicts can be checked without the audits' working copies.

---

## 2. Rows OUTSIDE the approval set (4 rows) — DEBRIS-class (superseded), each needs its own word

**These 4 rows are NOT part of the approval set.** The operator's word in §4 does **not** cover them; each one needs a separate, explicit word of its own. They are listed here so that the operator sees the whole picture, and so that a later deletion cannot be mistaken for a covered one.

**All four are DEBRIS-class by the merged inventory's final classes** (`rewrite/triage/branch-inventory-2026-11-12.md`): `recovery/pr1230-blob-auth-77b399f1-20261027`, `recovery/stash-adoption-6a0d2f03-20261025` and `recovery/strip-tests-restore-20261030` are recorded as **"DEBRIS (superseded) - port verdict DISCARD"**, and `preserve/p0.6-prototype-20261109` as **DEBRIS**. The classes below are the **completed** port verdicts, not the interim ones from the evidence audit: rows 2-4 were reclassified DEBRIS → VALUABLE (port candidates) on 2026-11-13 and then moved **back** to DEBRIS-class (superseded) the same day, when the completed port assessment returned DISCARD. **Row 4 carries a prerequisite: its P1 slice must be materialized before it can be deleted.**

| # | repo | branch | tip | class now (merged inventory) | what it needs | in the approval set? |
|---|---|---|---|---|---|---|
| 1 | jackin-project/jackin | `preserve/p0.6-prototype-20261109` | `63b2976a5` | **DEBRIS** (unproven review, 2026-11-13) | Its own deletion word. The branch holds a by-design non-mergeable prototype: 20/20 absent paths are `prototypes/p0.6/**`, while its evidence `rewrite/evidence/P0.6-panes.md` is byte-identical in main. The inventory lists it separately as an UNPROVEN CONTAINED candidate; the completed review reclassifies it as DEBRIS. | **NO** |
| 2 | jackin-project/jackin | `recovery/pr1230-blob-auth-77b399f1-20261027` | `77b399f1` | **DEBRIS (superseded) — port verdict DISCARD** | Its own deletion word. The branch is the only carrier of the landed-nowhere `GitBlobHasher` security fix (81 truly-unique paths, 77 of them product code, incl. `crates/tools/jackin-xtask/**`, `crates/core/jackin-process/**`), but the completed port assessment returned **DISCARD**: `main` never streams staged blobs (it delegates the OID computation to Git at `crates/jackin-session/src/project/gate.rs:58`), so a semantic port is ~1200-1600 lines and a cherry-pick conflicts on 5/5 touched files. The port reference is kept, no port. | **NO** |
| 3 | jackin-project/jackin | `recovery/stash-adoption-6a0d2f03-20261025` | `ade401e7` | **DEBRIS (superseded) — port verdict DISCARD** | Its own deletion word (or to archive it under a tag/retention ref before any future delete). It holds 24 truly-unique paths — the only surviving implementation of the `precommit_provision` / `precommit_snapshot` product design — but the rewrite program explicitly deleted that design (`rewrite/research/jackin-infra.md:231,261`: the hook-install scripts are "DELETE; replace with `mise exec --locked -- hk install`"), so the port assessment returned **DISCARD**: ~5809 LOC + ~1000 LOC of shell + two recreated host crates. The port reference is kept, no port. | **NO** |
| 4 | jackin-project/jackin | `recovery/strip-tests-restore-20261030` | `8508f6f8` | **DEBRIS (superseded) — port verdict DISCARD as a port source; 1 PORT-LATER carve-out (P1)** | **Its own word, and only after the P1 slice is materialized.** The branch measures **3,844 path-absent / 3,868 content-unique** paths — the earlier audit's 2,464/355 was corrected on 2026-11-13 (the branch forked before the rewrite, merge-base `7c92234bd`), not 355. The **P1 carve-out** is the OMP credential-store capture hardening `crates/adapters/jackin-omp-store/src/capture.rs`: it exists nowhere in `main` and on no other ref, so **the branch must stay until that slice is ported**. The P2 carve-out (the agent-status / hook-installer slice) is **DISCARD** (`rewrite/DESIGN.md:272-281`, D16 deletes agent-status inference). | **NO** |

All four are DEBRIS-class by the inventory's final classes, all four are live on the remote (§7), and **none of them is covered by the operator's word in §4**. Row 4 is not deletable until its P1 slice is ported.

---

## 3. CONTAINED candidates — containment UNPROVEN (4) — kept, NOT deletable

**These 4 are CONTAINED *candidates* whose containment is NOT proven — they are not DEBRIS, and they are not safe to delete either.**

**This publication supersedes the merged inventory's class label for these four rows.** The inventory `rewrite/triage/branch-inventory-2026-11-12.md` labels them **"CONTAINED - safe to delete"** — in its rows for `codex/openshell-integration-checkpoint-20261108-122311`, `codex/openshell-verified-checkpoint-20261108`, `preserve/session-boundaries-20261109` and `preserve/w1-fixes-20261109`, and again in its class-change table. **That label is not safe to act on**, and the inventory's own class *rule* — "**CONTAINED candidate - UNPROVEN** — the branch is *believed* to be contained, but the proof is incomplete: the paths it changes are ABSENT from `main` and carry product code, so containment is NOT proven. Such branches … must be treated as VALUABLE (NOT deletable) until every absent path is proven to be scratch/archive" — and the per-row evidence the inventory itself carries (the 2026-11-13 unproven review, quoted below) both point the other way: the five usage fixtures are absent from `main`, and the product-code paths have no inventory-grade proof.

**The disposition of these four branches is therefore UNPROVEN, KEPT — do not delete them**, whatever the class column of the inventory says. An operator who reads the inventory alone gets the opposite disposition; this section is the correction, and it is the one that governs. They are **excluded from this ratification by class, not by doubt**: the operator's word in §4 does not cover them, they are not DEBRIS, and nothing in this document proposes deleting them. They are kept here as a record, with their recorded proofs **and the proof that is still missing**.

**Missing proof — all 4 rows.** Two things are **not** proven for any of these four branches, so none of them may be called safe to delete:

1. **The 5 usage fixtures are absent from `main` and are carried only by these branches.** Verified live on 2026-11-14 against `origin/main`: the same 5 paths are present on all 4 branches and in no `main` tree — `crates/usage/jackin-usage/tests/usage/fixtures/kimi/usages_hours.json`, `crates/usage/jackin-usage/tests/usage/fixtures/kimi/usages_no_window.json`, `crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth.json`, `crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth_go_oauth.json`, `crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth_no_go.json`. The inventory counts them among the absent paths of `preserve/w1-fixes-20261109` (its 6 absent paths = 1 console test + these 5). Their content is **not** present in `main`, which is exactly what the inventory's CONTAINED definition requires — so containment is not proven for any of the 4.
2. **The product-code paths have no inventory-grade containment proof.** The inventory records them as UNPROVEN ("24 of the 25 absent blobs never occur in main's object history"); the (a)/(b)/(c) split below is inherited from the completed audits and is not a recorded merged-PR / patch-id / ancestry proof.

**Until both are closed — the fixture port decision is made and recorded, and the product-code paths are proven — these 4 branches are kept and are treated as VALUABLE. Do not delete them.**

| # | repo | branch | tip | absent product-code paths | disposition split | recorded proof, and what is still missing |
|---|---|---|---|---|---|---|
| 1 | jackin-project/jackin | `codex/openshell-integration-checkpoint-20261108-122311` | `6965f5d1c` | 12 | (a) 5 · (b) 7 · (c) 0 | Every absent product-code path is either already in `main` under a different path/name (a) or superseded by the later `main` rewrite (b): `agent_metadata.rs` → `launch/decide.rs` + `tests/launch/instance_identity.rs`; `secret_broker.rs` → `accounts.rs` (Muse keychain) + `op.rs` (1Password `op read`); `jackin-testkit/src/docker.rs` → `jackin-testkit/src/engine.rs` + `gateway/*`; `write-file.c` + `writer_bootstrap*` → the OpenShell `write_file` RPC; `cancellation.rs` → the `GatewayError::Cancelled` coverage in `tests/gateway/{connect,exec,download_bound,status,error_text,transfer}.rs`; `console/files.rs` → `tests/console/sources.rs:563-600`. **Missing:** the 5 usage fixtures above (absent from `main`, carried only by these branches) and the inventory-grade proof for the 12 product-code paths. |
| 2 | jackin-project/jackin | `codex/openshell-verified-checkpoint-20261108` | `ce119545c` | 12 | (a) 5 · (b) 7 · (c) 0 | Same 12 paths with **byte-identical blob SHAs** to row 1 — the two branches carry the same payload, so row 2 adds no unique residue. **Missing:** the 5 usage fixtures above (absent from `main`) and the inventory-grade proof for the 12 product-code paths. |
| 3 | jackin-project/jackin | `preserve/session-boundaries-20261109` | `16de1eedf` | 6 | (a) 3 · (b) 3 · (c) 0 | Same 6 subjects as rows 1/2, each an earlier or byte-identical revision: `write-file.c` (older blob `dafb9560`), `cancellation.rs` (identical blob), `writer_bootstrap.rs` (older, smaller blob `3b100e3c`), `writer_bootstrap_driver.c` / `_shim.c` (identical blobs), `console/files.rs` (identical blob). Strictly less content than rows 1/2, which are themselves superseded. **Missing:** the 5 usage fixtures above (absent from `main`) and the inventory-grade proof for the 6 product-code paths. |
| 4 | jackin-project/jackin | `preserve/w1-fixes-20261109` | `78d8b5f02` | 1 | (a) 1 · (b) 0 · (c) 0 | One absent product-code path: `crates/jackin-tui/tests/console/files.rs` (blob `46e156a7`, identical to rows 1-3), never in `main`; the same invariant is covered in `main` at `crates/jackin-tui/tests/console/sources.rs:563-600` (mkfifo + symlink-to-pipe) and its Claude twin. **Missing:** this branch's 6 absent paths are 1 product-code path (above) **plus the 5 usage fixtures**, which are absent from `main` — the inventory's record of them is the reason this branch is UNPROVEN and kept — and the inventory-grade proof for the product-code path. |

All 4 are still live on the remote (§7) and are **kept as UNPROVEN CONTAINED candidates** — neither deleted nor proposed for deletion by this document.

Open item flagged by the same review (not a debris question): the 5 usage fixtures above are branch-added and absent from `main`, and they encode a newer Kimi `limits[].window.timeUnit` / OpenCode `opencode-go` API shape than `main` ships. If that newer shape is wanted, it is a **port** decision for the operator — not a debris deletion. Until that decision is made and the fixtures are ported (or archived under a retention ref), these 4 branches hold the only recorded copies of them and must stay.

---

## 4. The operator's one-word approval

Send exactly:

```text
RATIFY DEBRIS: 15 branches
```

Optionally, to also ratify retroactively the one deletion that was performed without a recorded proof (§6):

```text
RATIFY DEBRIS: 15 branches
RATIFY UNRECORDED DELETION: 1 branch (tailrocks/terminal-components-baseline wip/tuiscotti-pin-fec38841)
```

A refusal, a different count, or any other wording is a refusal: **nothing is deleted unless the word above is sent.**

---

## 5. Nothing merged, nothing deleted

- **No branch in the approval set has been merged.** Their tips are not ancestors of their repository's `main`, and no merge was performed for this document.
- **No branch in the approval set has been deleted.** All 15 are still on their remotes (§7); the deletion happens **only after the operator's word** (§4).
- **Nothing outside the approval set is deleted.** The 4 DEBRIS-class rows of §2 (each needs its own word; row 4 only after its P1 slice is ported) and the 4 UNPROVEN CONTAINED candidates of §3 stay as they are; the VALUABLE rows of §8 are untouched.
- This document, its commit and its PR **delete nothing**: they only publish the list. The only command shape it contains is the per-branch `git push --force-with-lease=refs/heads/<branch>:<tip-SHA> https://github.com/<repo>.git --delete <branch>` of §1, to be run by the operator after ratification; each of those commands names its repository by URL (never by the alias `origin`) and refuses to run if the branch's tip has moved.

---

## 6. Recorded deletion — correction of the inventory's baseline count

| repo | branch | status | recorded proof |
|---|---|---|---|
| tailrocks/terminal-components-baseline | `wip/tuiscotti-pin-fec38841` | **Already deleted from the remote** — live check on 2026-11-14: the ref is absent (§7). **No deletion command is needed.** | The branch's class was CONTAINED (superseded), so no work was lost: its single commit only bumps the `tuiscotti` dev-dependency `d8d54155` → `fec38841`, and `main` already pins the same dependency at the strictly newer rev `92ef2d9e` (`main:Cargo.toml`); the commit's doc intent is already in `main:docs/functional-testing.md:96`. The tip was `1c980e88` ("chore: pin tuiscotti to fec38841 for binary baseline review"). |

**Correction to the merged inventory.** `rewrite/triage/branch-inventory-2026-11-12.md` records `tailrocks/terminal-components-baseline` as "0 deleted" (its branches section says the baseline branch was "left on the remote because baseline had 0 deletions", and its per-repo summary says "tailrocks/terminal-components-baseline: 0 deleted"). That claim is **stale**: `wip/tuiscotti-pin-fec38841` was in fact deleted from the remote without a recorded proof in the inventory. The corrected baseline deletion count is **1**, not 0, and the deletion happened under the superset proof above (main pins a strictly newer rev of the same dependency, two commits after `fec38841`). This document records that deletion; it does not undo it and does not re-delete it.

---

## 7. Live verification against the remotes (2026-11-14, re-verified 20:12 UTC)

Verified with `gh api repos/<repo>/branches --paginate --jq '.[] | "\(.name) \(.commit.sha)"'`, first on 2026-11-14, re-verified at 13:17 UTC and re-verified again at 20:12 UTC the same day. "Live" = the branch is still on the remote with the tip recorded in the audits; **ALREADY GONE** = the ref no longer exists. All 15 rows of §1 are live with the recorded tips at 20:12 UTC; none of them is merged, none is deleted.

### Approval set (§1) — all 15 live, none merged, none deleted

| repo | branch | live tip | status |
|---|---|---|---|
| jackin-project/jackin | `chore/triage-inventory-20261112` | `e13360eb87518f70c45097366e384851f113ad8d` | live |
| jackin-project/jackin | `docs/pr1228-replacement-20261026` | `912fe737e08fb540be67573ce8b086b30c77ec0c` | live |
| jackin-project/jackin | `recovery/openrouter-exact-money-20261027` | `6ebbe14490b2eddac73263b2df248252e1b10546` | live |
| jackin-project/jackin | `recovery/stash-2f8cc42b-20261023` | `e5e6d1fd27efc3eee52b7e18574e4870c5a4a191` | live |
| jackin-project/jackin | `recovery/stash-2fb075f8-20261023` | `4d9540abf301f80b58f81f47d7691132ce48f36c` | live |
| jackin-project/jackin | `recovery/stash-88f5643c-20261023` | `ddf8861f45646f34b1ca087bd8ed6b9429f270c1` | live |
| jackin-project/jackin | `recovery/stash-be51ff69-20261023` | `9cdf189b812caf2395048bf0d8dbccdadf1e83a0` | live |
| jackin-project/jackin | `recovery/stash-d808d094-20261023` | `00f98f32514aca8005c56dd2e44e3fc03c326b46` | live |
| jackin-project/jackin | `recovery/strict-merge-gate-merge-readiness-20261022` | `2f96c6dadd4e1974f89f5a5f7407c0f6ff221fae` | live |
| jackin-project/jackin | `rollout/velnor-3-provider` | `092fe1db796367479029d4e94379530f7f5965c0` | live |
| jackin-project/jackin-agent-smith | `recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a` | `b9db5b149cc46baba9c49549432307c29e3972b0` | live |
| jackin-project/jackin-agent-smith | `recovery/20261006/jackin-agent-smith/2e7119b9c668/jackin-agent-smith` | `c58cfdcaaca85f606c2abc4ddb8f5afaae47f737` | live |
| tailrocks/terminal-components-baseline | `test/pre-refactor-protection` | `fc624b7456132fb74675ab452e6b64ecb3f69a70` | live |
| tailrocks/terminal-components-baseline | `wip/tuiscotti-capture-only` | `0b8b8bb077061eee6ca04466148f017e316edfd3` | live |
| tailrocks/terminal-components-baseline | `wip/tuiscotti-fixcheck` | `8d352ebfb932b06bf18c537e924389b0aae07e42` | live |

### Outside the approval set

| repo | branch | live tip | status |
|---|---|---|---|
| jackin-project/jackin | `preserve/p0.6-prototype-20261109` (§2.1) | `63b2976a53df905df858e4d8a566839b79245545` | live |
| jackin-project/jackin | `recovery/pr1230-blob-auth-77b399f1-20261027` (§2.2) | `77b399f1242685ea5b8916b8a4d82818eee92f25` | live |
| jackin-project/jackin | `recovery/stash-adoption-6a0d2f03-20261025` (§2.3) | `ade401e79e3ea7f6107dbae7b4831c1f879b743c` | live |
| jackin-project/jackin | `recovery/strip-tests-restore-20261030` (§2.4) | `8508f6f8388d9988a940beb8fb0b812bf6bc1215` | live |
| jackin-project/jackin | `codex/openshell-integration-checkpoint-20261108-122311` (§3.1) | `6965f5d1c2227ca6187704a2374f09bba14632d4` | live |
| jackin-project/jackin | `codex/openshell-verified-checkpoint-20261108` (§3.2) | `ce119545c6e12619c87aad429fda5a6978d2e508` | live |
| jackin-project/jackin | `preserve/session-boundaries-20261109` (§3.3) | `16de1eedff91e0711a1c58e59f69d39d81c402bd` | live |
| jackin-project/jackin | `preserve/w1-fixes-20261109` (§3.4) | `78d8b5f02f3fd2245e18883a0c3b0086f3c91780` | live |
| tailrocks/terminal-components-baseline | `wip/tuiscotti-pin-fec38841` (§6) | — | **ALREADY GONE** |

### Branches created after the inventory was captured — not in any class list, not covered here

| repo | branch | live tip | status |
|---|---|---|---|
| jackin-project/jackin | `chore/restore-1283-remainder` | `e519e06e4885b92cf630c476b1913ae98974a27e` | **ALREADY GONE** — deleted on the remote by the merge of PR #1286 (`3c3f77db5`), 2026-11-14T14:17:24Z |
| jackin-project/jackin | `docs/triage-inventory-reclass-2026-11-13` | `cf9acb7485e1800680fd00b62767af35e9713fe6` | live, unclassified here (the branch was deleted by the merge of PR #1281 (`926bbd49d`) and has since been re-created with a new tip) |
| jackin-project/jackin | `test/cli-t09-t18-collision` | `546862232b262131a8522926c01e05b9d6f64d3f` | **ALREADY GONE** — deleted on the remote by the merge of PR #1288 (`7f13576ac`) |
| jackin-project/jackin | `test/provider-eligibility` | `4f858aaaa752637ba9cf2d83a1b5b425b9e24d44` | **ALREADY GONE** — deleted on the remote by the merge of PR #1285 (`2bdef85bb`), 2026-11-14T17:34:19Z |

**Correction to the first compile of this table.** It recorded different tips for three of these four (`chore/restore-1283-remainder` `3caf18c407c181d92c3b737cc220d196ca4777d2`, `docs/triage-inventory-reclass-2026-11-13` `441b8906fa42227ea0189f31164b8c455a2069a4`, `test/provider-eligibility` `bd0a7d88f5d89b98cf86320db8eaf06363256ddb`) and did not list `test/cli-t09-t18-collision` at all; the tips above are the live ones of 2026-11-14 20:12 UTC. All four are **unclassified**: they are outside the approval set, outside §2 and §3, and the operator's word in §4 does not cover them. They are recorded so that a later deletion cannot be mistaken for a covered one.

### Branches outside every class list

The 20:12 UTC re-check counted **70** branches in `jackin-project/jackin` (69 non-main plus `main`),
against 24 at the 13:17 UTC check. The growth is other work in the same repository. **50** of the 70 are
outside every class list of this document:

| namespace | count | state |
|---|---|---|
| `wt/*` | 30 | live, unclassified, outside the operator's word |
| `fix/*` | 9 | live, unclassified, outside the operator's word (`fix/typed-model-selection` was re-created after PR #1279 deleted it — see §8) |
| `swarm/*` | 3 | live, unclassified, outside the operator's word |
| `chore/dirty-slice-1`, `chore/dirty-slice-accounting-2026-11-14` | 2 | live, unclassified, outside the operator's word |
| `docs/triage-inventory-postmerge-2026-11-15`, `docs/triage-inventory-reclass-2026-11-13` | 2 | live, unclassified, outside the operator's word |
| `backup/w3-local-usage`, `w3/pr1281-merge`, `sup019/fix-1264`, `sup020/fix-cli-1264` | 4 | live, unclassified, outside the operator's word |

The other 20 branches of the 70 are accounted for in this document: 10 in the approval set (§1), 4 in §2,
4 in §3, this document's own branch `docs/debris-ratification-2026-11-14` and `main`. None of the 50
unclassified branches is audited here; they are outside every class list and outside the operator's word,
and they are listed only so that a later deletion cannot be mistaken for a covered one.

---

## 8. Excluded rows — VALUABLE or ALREADY GONE, must not be deleted

Not part of any approval set; listed so that no one mistakes them for debris. Rows marked **ALREADY GONE** are recorded only to show that their exclusion is not a pending deletion: there is nothing left to delete.

| repo | branch | why it is kept |
|---|---|---|
| jackin-project/jackin | `fix/typed-model-selection` | The audited branch was **ALREADY GONE** at the 13:17 UTC re-check (`gh api repos/jackin-project/jackin/branches/fix/typed-model-selection` → 404): its content landed via **MERGED PR #1279** ("feat(core): add typed model selection for agent tuples", merge commit `25f8c42c660f2951d4acc8f19ff489e7e6f61c67`, merged 2026-11-14 04:12 UTC), which **is an ancestor of `main`**, and `crates/jackin-core/src/model_selection.rs` **is present in `main`** (blob `2613cbe7`). The earlier verdict carried into this document ("the typed-model-selection code is absent from `main`; open PR #1279") was pre-merge and is **stale**: there was nothing left to delete and nothing left to keep, and no row of the approval set changes. The stale *branch copy* of the inventory wrongly calls it DEBRIS — do not use that copy. **At the 20:12 UTC re-check a branch of the same name exists again** (tip `0aaf7a411ce0f35ebd9ca7e7722caed7101c65c6`, a different tip from the audited `5302848e`): it is a **new, unclassified branch**, outside the approval set, outside §2 and §3, and the operator's word does not cover it. |
| jackin-project/jackin | `docs/triage-inventory-reclass-2026-11-13` | Live branch (tip `cf9acb7485e1800680fd00b62767af35e9713fe6` at the 20:12 UTC re-check) created after the inventory was captured; not in any class list. |
| jackin-project/jackin-sentinel | `fix/s158-postmerge-4` | VALUABLE — open PR #159. |
| jackin-project/jackin-sentinel | `preserve/probe-identity-20261109` | VALUABLE — tip commit not in `main`. |
| tailrocks/terminal-components-baseline | `preserve/local-unique-20261109` | VALUABLE — 2 commits are not in `main`; keep for cherry-pick. |
| jackin-project/jackin-agent-smith | `main`, `jackin-project/jackin` `main`, `tailrocks/terminal-components-baseline` `main` | Trunk. |

---

## 9. Limitations

- The 15 rows of §1 were re-read live from the remotes on 2026-11-14 and re-verified at 13:17 and 20:12 UTC (§7); the tip SHAs match the audited ones, and each deletion command is leased against that exact SHA, so a moved branch is refused rather than deleted. The audits behind the verdicts are committed with this document (`rewrite/triage/audits/2026-11-14/`), and the absent-path accounting of §1a is re-derived here against the `main` tips of §1b — the audits themselves were written against `main` `7a53f6abe` / `78d3b86e`, so their per-row numbers are the capture-time numbers and §1a is the current one.
- The 20 recorded CONTAINED deletions from the previous round could not be re-proved from the remotes (their refs are gone); their recorded proofs were read and accepted.
- `git cherry` counts are patch-id based and report squash-landed commits as unmatched; no verdict depends on them.
- The 4 CONTAINED candidates of §3 are listed as **UNPROVEN** CONTAINED candidates, not as DEBRIS and **not** as safe-to-delete: they are excluded from this ratification by class, not by doubt. **This publication supersedes the merged inventory's "CONTAINED - safe to delete" label for exactly these four rows** (§3); the inventory's class rule ("CONTAINED candidate - UNPROVEN") and its own per-row evidence agree with §3, and the five usage fixtures the branches carry are absent from `main`. Their containment proof is still incomplete (§3), so they are treated as VALUABLE and kept.
- The branches of §7 that are outside every class list (4 recorded individually, 50 in total at the 20:12 UTC re-check) were created after the inventory was captured; their tips moved between the compiles of this document and they are not audited here at all. They are outside every class list and outside the operator's word.
- §6 records a deletion that happened before this document; it is a ratification of a fact, not a new deletion.
- Nothing was deleted, pushed to a branch, merged or tagged in producing this document; only this file and the seven audit files under `rewrite/triage/audits/2026-11-14/` were added.
