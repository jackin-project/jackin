# DEBRIS Ratification List — operator one-word approval

> **Committed as the capture-time record (2026-11-14).** The published, operator-facing document is
> `rewrite/triage/debris-ratification-2026-11-14.md`; where the two differ, **the publication wins**.
> Two parts of this file are superseded by it and are marked below: the runnable deletion block in §4
> (it was unguarded and target-ambiguous - the publication's commands are leased against the recorded
> tip and name their repository by URL) and the four `CONTAINED-safe-to-delete` verdicts in §2 (the
> publication's §3 records them as **UNPROVEN, KEPT**, because the five usage fixtures they carry are
> absent from `main` and their product-code paths have no inventory-grade containment proof).
> The audit files named below are committed next to this one, in `rewrite/triage/audits/2026-11-14/`.

Source of truth: `jackin-project/jackin` @ `main` (PR #1280, merge `78d3b86e`)
`rewrite/triage/branch-inventory-2026-11-12.md` (18-row DEBRIS table) + completed audits
`debris-audit.md` (15 CONFIRMED / 3 RECLASSIFIED), `inventory-parity.md`,
`unproven-review.md`, `architect-smith-audit.md`, `baseline-rows-audit.md`.
READ-ONLY: nothing deleted, pushed, committed or merged. Tips below re-read live from the remotes
(`gh api repos/<repo>/git/ref/heads/<branch>`) on 2026-11-13; all 15 are still on their remotes.

## 1) APPROVE FOR DELETION — 15 branches (10 jackin · 2 smith · 3 baseline)

| # | repo | branch | tip SHA | one-line evidence |
|---|---|---|---|---|
| 1 | jackin-project/jackin | `chore/triage-inventory-20261112` | `e13360eb8` | 1 commit adds only the generated triage artifact `rewrite/triage/branch-inventory-20261112.md` (9148 B), superseded by the merged inventory in main (`git cherry` 1+/0-). |
| 2 | jackin-project/jackin | `docs/pr1228-replacement-20261026` | `912fe737` | 62/63 `docs/openshell-v2/**` paths are the pre-reflow draft of the pack merged as PR #1234 (`3ffd6171a`); docs-only, no product code. |
| 3 | jackin-project/jackin | `recovery/openrouter-exact-money-20261027` | `6ebbe144` | Product fix landed byte-identically in merged PR #1238 (`f16eee70e`, `money_numeric.rs` = `06aa1044`); branch residue is comments/extra tests in trees removed by #1264. |
| 4 | jackin-project/jackin | `recovery/stash-2f8cc42b-20261023` | `e5e6d1fd` | Stash snapshot, 17 paths, **0 truly unique** blobs (all present in main's history or another ref); single parent, no side-parent risk. |
| 5 | jackin-project/jackin | `recovery/stash-2fb075f8-20261023` | `4d9540ab` | Stash snapshot, 48 paths, only 2 unique (a design doc + a shell script), no product code; single parent. |
| 6 | jackin-project/jackin | `recovery/stash-88f5643c-20261023` | `ddf8861f` | Stash snapshot, 17 paths, **0 truly unique**; single parent. |
| 7 | jackin-project/jackin | `recovery/stash-be51ff69-20261023` | `9cdf189b` | Stash snapshot, 60 paths, 4 unique = `native/README.md`, `ratchet.toml`, 2 shell scripts — no product code; single parent (RUSTSEC claim was already dropped as unsupported). |
| 8 | jackin-project/jackin | `recovery/stash-d808d094-20261023` | `00f98f32` | Stash snapshot, 54 paths, 2 unique (both `scripts/*` shell wrappers), no product code; single parent. |
| 9 | jackin-project/jackin | `recovery/strict-merge-gate-merge-readiness-20261022` | `2f96c6da` | Stale pre-rewrite CI gate config, 25 paths, **0 truly unique**; `check:merge-readiness` does not exist in main; duplicate of the `fix/restore-docs-source-globs` lineage. |
| 10 | jackin-project/jackin | `rollout/velnor-3-provider` | `092fe1db` | Orphan line (no merge base with main, root `c24fffeb0` ≠ main root); 20 unique paths are all generated artifacts of `ChainArgos/jackin-agent-brown` — no product code. |
| 11 | jackin-project/jackin-agent-smith | `recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a` | `b9db5b14` | Pure velnor generator churn (7 paths: `.github-gen/*`, generator state, 5 rendered workflows); own PR #206 closed unmerged; superseded by merged PR #207 (`08cb1c2f`). |
| 12 | jackin-project/jackin-agent-smith | `recovery/20261006/jackin-agent-smith/2e7119b9c668/jackin-agent-smith` | `c58cfdca` | "WIP preservation snapshot" commit with an **empty patch** (tip tree == parent tree); parent `2e7119b9` is an ancestor of main; 3-dot diff vs main = 0 files. |
| 13 | tailrocks/terminal-components-baseline | `test/pre-refactor-protection` | `fc624b74` | 666 commits ahead but 3011/3045 changed paths are blob-identical to main; exactly **1** path absent from main = `terminal-components-baseline-goal.txt` (scratch goal prompt). |
| 14 | tailrocks/terminal-components-baseline | `wip/tuiscotti-capture-only` | `0b8b8bb0` | 9385 absent paths = 7236 `run-evidence/**` + 1880 legacy `.tui.json` deleted by PR #6 + machine-local `.cargo/config.toml`; **0** branch-only commits touch `src`/`apps`. |
| 15 | tailrocks/terminal-components-baseline | `wip/tuiscotti-fixcheck` | `8d352ebf` | 1914 absent paths = 1880 legacy `.tui.json` + 23 `run-evidence/fix-validation/**` + machine-local `.cargo/config.toml`; **0** branch-only commits touch `src`/`apps`. |

### EXPLICITLY EXCLUDED — 3 reclassified rows (VALUABLE, MUST NOT be deleted)
These are still DEBRIS in the merged inventory table but the completed audit reclassified them; they
are **NOT** part of the 15-branch approval set and **NOT** covered by the commands below.

| repo | branch | tip SHA | why excluded |
|---|---|---|---|
| jackin-project/jackin | `recovery/pr1230-blob-auth-77b399f1-20261027` | `77b399f1` | Only carrier of the landed-nowhere `GitBlobHasher` security fix; 81 truly unique absent paths (77 product code incl. `crates/tools/jackin-xtask/**`, `crates/core/jackin-process/**`). |
| jackin-project/jackin | `recovery/stash-adoption-6a0d2f03-20261025` | `ade401e7` | 24 truly unique paths — the only surviving implementation of the `precommit_provision` / `precommit_snapshot` product design (its removal commits are not ancestors of main). |
| jackin-project/jackin | `recovery/strip-tests-restore-20261030` | `8508f6f8` | 355 truly unique product/test files (e.g. `crates/adapters/jackin-docker/src/**`, `crates/apps/jackin-capsule/src/**`, `jackin-omp-store/src/capture.rs`) with no other copy on any remote. |

Also excluded by class (never in the DEBRIS set): `fix/typed-model-selection` (VALUABLE, open PR #1279;
the stale *branch-copy* inventory wrongly calls it DEBRIS — do not use that copy), the 4 VALUABLE rows
(`fix/typed-model-selection`, sentinel `fix/s158-postmerge-4`, sentinel `preserve/probe-identity-20261109`,
baseline `preserve/local-unique-20261109`), and the 5 CONTAINED candidates in §2.

## 2) The 5 CONTAINED candidates kept as non-deletable (jackin-project/jackin)

Current verdict from the unproven review (`unproven-review.md`): 4 are CONTAINED-safe-to-delete,
1 is DEBRIS. All 5 are still live on the remote and are **outside** the 15-branch approval set.

> **The four `CONTAINED-safe-to-delete` verdicts below are SUPERSEDED - do not act on them.** The merged
> inventory (`rewrite/triage/branch-inventory-2026-11-12.md`) classes all four as **"CONTAINED (candidate)
> - UNPROVEN, KEPT"**, and its class rule says such a branch "must be treated as VALUABLE (NOT deletable)
> until every absent path is proven to be scratch/archive". The publication's §3
> (`rewrite/triage/debris-ratification-2026-11-14.md`) records the missing proof: the five usage fixtures
> these branches carry are **absent from `main`** and their product-code paths have no inventory-grade
> (merged-PR / patch-id / ancestry) containment proof. The rows are kept here only as the capture-time
> record of the unproven review.

| branch | verdict (unproven review) | basis | action now |
|---|---|---|---|
| `codex/openshell-integration-checkpoint-20261108-122311` | **SUPERSEDED - UNPROVEN, KEPT** (the publication's §3) | 12 absent product-code paths, disposition (a)5 + (b)7 + (c)0 — each superseded by the later main rewrite | kept (not in this approval set); **not** safe to delete |
| `codex/openshell-verified-checkpoint-20261108` | **SUPERSEDED - UNPROVEN, KEPT** (the publication's §3) | byte-identical blobs to the branch above, same 12 paths, same (a)5/(b)7/(c)0 proof | kept (not in this approval set); **not** safe to delete |
| `preserve/session-boundaries-20261109` | **SUPERSEDED - UNPROVEN, KEPT** (the publication's §3) | 6 absent product-code paths, (a)3 + (b)3 + (c)0 | kept (not in this approval set); **not** safe to delete |
| `preserve/w1-fixes-20261109` | **SUPERSEDED - UNPROVEN, KEPT** (the publication's §3) | 1 absent product-code path (`crates/jackin-tui/tests/console/files.rs`), (a)1 + (b)0 + (c)0 | kept (not in this approval set); **not** safe to delete |
| `preserve/p0.6-prototype-20261109` | **DEBRIS** | 20/20 absent paths are `prototypes/p0.6/**`; the prototype is by design non-mergeable and its evidence `rewrite/evidence/P0.6-panes.md` is byte-identical in main | kept as DEBRIS — a separate approval is needed to delete it (not in the 15) |

## 3) Deletions already performed under a recorded proof

| repo | recorded deletions | proof basis |
|---|---|---|
| jackin-project/jackin | **10** | `codex/pr1264-attach-readiness-20261108`, `codex/r7-proposal-preservation-20261109`, `codex/unpublished-openshell-checkpoint-20261109`, `fix/roles-review-impl`, `fix/restore-docs-source-globs`, `preserve/hostpath-display-audit-20261108`, `preserve/local-unique-20261109`, `preserve/stash-p2-before-reconcile-20261109`, `preserve/uncommitted-sources-20261109`, `wt/login-import` — each re-proved by ancestry + patch-id + per-path blob equality against `origin/main` |
| jackin-project/jackin-the-architect | **9** | 8 `archive/cleanup-20261006/*` (patch-id equality vs MERGED PR #479 `7b72b38` / #480 `2fefbdd`, both ancestors of main; 5 exact-SHA alias pointers) + `preserve/betterleaks-1.10.0-20261109` (37/38 SHA-ancestors of PR #481 head, `cb5801f` content-equal in main) |
| jackin-project/jackin-agent-smith | **1** | `recovery/…/2e7119b9c668/branch/velnor/fleet-b9c3156cdb88e63c11b9e595a3e694b02238c09a` — patch-id `d788ec00…` == squash `08cb1c2f` of MERGED PR #207, ancestor of main |
| tailrocks/terminal-components-baseline | **0 recorded** | see the unrecorded deletion below |
| **total recorded** | **20** | |

Separate, earlier recorded deletion (historical snapshot section of the inventory):
`jackin-project/jackin` `recovery/branches-nightly-compat-20261029` — deleted under a recorded SHA proof
(tree byte-identical to `97b1b58f7`), PR #1263 closed. Not part of the 20.

### The one deletion that was NOT recorded — needs ratification
| repo | branch | status | proof to record |
|---|---|---|---|
| tailrocks/terminal-components-baseline | `wip/tuiscotti-pin-fec38841` | **already deleted from the remote** (live check: `gh api repos/tailrocks/terminal-components-baseline/git/ref/heads/wip/tuiscotti-pin-fec38841` → HTTP 404 "Not Found") but **no deletion proof was recorded** in the inventory, which still says "kept — left on the remote because baseline had 0 deletions" | Class was CONTAINED (superseded), so no work was lost: the single commit only bumps the `tuiscotti` dev-dependency `d8d54155` → `fec38841`, and `main` already pins the same dependency at the strictly newer rev `92ef2d9e` (`main:Cargo.toml`); the commit's doc intent is already in `main:docs/functional-testing.md:96`. Operator ratification = accept this deletion retroactively and record the proof. **No command needed — the branch is already gone.** |

## 4) One-word approval text and the exact deletion commands

**Send exactly:** `RATIFY DEBRIS: 15 branches`

(Optionally add the retroactive line for the unrecorded deletion: `RATIFY DEBRIS: 15 branches` +
`RATIFY UNRECORDED DELETION: 1 branch (baseline wip/tuiscotti-pin-fec38841)`.)

Pre-flight (read-only, confirms the 15 tips before deleting):
```bash
for r in jackin-project/jackin jackin-project/jackin-agent-smith tailrocks/terminal-components-baseline; do
  gh api "repos/$r/branches" --paginate --jq '.[].name'
done
```

Deletion commands for the approved 15: **superseded - use the publication's table**
(`rewrite/triage/debris-ratification-2026-11-14.md`, §1, one row per branch).

The block that stood here was a runnable `gh api -X DELETE` list. It was **unguarded** - it deleted
whatever tip the ref had at that moment, not the audited tip - and it named the repository implicitly, so
in a fork-based clone it could have hit the wrong repository. The publication's §1 replaces it with, per
branch, the exact command:

```bash
git push --force-with-lease=refs/heads/<branch>:<recorded-tip-SHA> https://github.com/<repo>.git --delete <branch>
```

which refuses to run when the branch has moved since the audit and always targets the repository named in
the row. **Nothing in this file is a runnable deletion.**

Post-check (read-only): the 15 refs must be gone; the 3 excluded + 5 CONTAINED candidates +
VALUABLE branches must still be present.
```bash
gh api repos/jackin-project/jackin/branches --paginate --jq '.[].name'
gh api repos/jackin-project/jackin-agent-smith/branches --paginate --jq '.[].name'
gh api repos/tailrocks/terminal-components-baseline/branches --paginate --jq '.[].name'
```
Expected after deletion: jackin 22 → 12 non-main, smith 2 → 0, baseline 4 → 1 (capture-time counts of
2026-11-13; the publication's §1 carries the counts re-read on 2026-11-14 at 20:12 UTC).

## 5) Limitations / notes
- Nothing was deleted, pushed, committed, merged or tagged; this file is the only artifact written.
- The 20 recorded CONTAINED deletions could not be re-proved from the remotes (refs are gone); their
  recorded proofs were read and accepted. The 15 approved branches' tips were re-read live.
- The 20 CONTAINED deletions' proofs and the 5 CONTAINED candidates' proofs come from the completed audits,
  not re-executed here.
- `git cherry` counts are patch-id based and report squash-landed commits as unmatched; no verdict depends on them.
- `preserve/p0.6-prototype-20261109` (verdict DEBRIS in the unproven review) is deliberately **not** in the
  15-branch set: it was kept as a non-deletable CONTAINED candidate and needs its own operator word.
- `docs/triage-inventory-reclass-2026-11-13` and `feat/probe-j07-launcher-identity` (jackin) and
  `fix/probe-interpreter-identity` (sentinel) are live branches created after the inventory was captured;
  they are not in any class list and are not covered here.
