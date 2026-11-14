# DEBRIS Ratification List — 2026-11-14

**Purpose:** one reviewable document for the operator's one-word approval of the DEBRIS branch deletions.
**Status:** READ-ONLY publication. **No branch in this document has been merged or deleted by it.** The deletion of every branch in the approval set happens only after the operator's explicit word (§4).
**Compiled:** 2026-11-14, from the merged inventory `rewrite/triage/branch-inventory-2026-11-12.md` (jackin `main`, PR #1280) plus the completed audits `/tmp/triage/debris-audit.md` (18-row audit: 15 CONFIRMED, 3 reclassified), `/tmp/triage/debris-ratification.md`, `/tmp/triage/smith-debris-verify.md` and `/tmp/triage/unproven-review.md`.
**Live check:** every branch name below was re-read from the live remotes with `gh api repos/<repo>/branches --paginate --jq '.[].name'` on 2026-11-14 (§7). Branches that are already gone are marked **ALREADY GONE**.

## Contents

| § | section | rows |
|---|---|---|
| §1 | Approval set — CONFIRMED DEBRIS | 15 |
| §2 | DEBRIS-class rows OUTSIDE the approval set (each needs its own word) | 4 |
| §3 | CONTAINED-safe-to-delete candidates — CONTAINED, not DEBRIS | 4 |
| §4 | The operator's one-word approval text | — |
| §5 | Nothing merged, nothing deleted | — |
| §6 | Recorded deletion — correction of the inventory's baseline count | 1 |
| §7 | Live verification against the remotes | 24 |
| §8 | Excluded rows (VALUABLE — must not be deleted) | 6 |

---

## 1. Approval set — CONFIRMED DEBRIS (15 branches)

These 15 are the **only** branches the operator's word covers. Each row carries its own one-line evidence and the exact deletion command. The command is run **inside the clone of that repository** (`git remote -v` must show the repository named in the row) and deletes exactly one branch.

| # | repo | branch | tip | one-line evidence | exact deletion command |
|---|---|---|---|---|---|
| 1 | jackin-project/jackin | `chore/triage-inventory-20261112` | `e13360eb8` | 1 commit adds only the generated triage artifact `rewrite/triage/branch-inventory-2026-11-12.md` (9148 B), superseded by the merged inventory in main (`git cherry` 1+/0-). | `git push origin --delete chore/triage-inventory-20261112` |
| 2 | jackin-project/jackin | `docs/pr1228-replacement-20261026` | `912fe737` | 62/63 `docs/openshell-v2/**` paths are the pre-reflow draft of the pack merged as PR #1234 (`3ffd6171a`); docs-only, no product code. | `git push origin --delete docs/pr1228-replacement-20261026` |
| 3 | jackin-project/jackin | `recovery/openrouter-exact-money-20261027` | `6ebbe144` | Product fix landed byte-identically in merged PR #1238 (`f16eee70e`, `money_numeric.rs` = `06aa1044`); branch residue is comments/extra tests in trees removed by #1264. | `git push origin --delete recovery/openrouter-exact-money-20261027` |
| 4 | jackin-project/jackin | `recovery/stash-2f8cc42b-20261023` | `e5e6d1fd` | Stash snapshot, 17 paths, **0 truly unique** blobs (all present in main's history or another ref); single parent, no side-parent risk. | `git push origin --delete recovery/stash-2f8cc42b-20261023` |
| 5 | jackin-project/jackin | `recovery/stash-2fb075f8-20261023` | `4d9540ab` | Stash snapshot, 48 paths, only 2 unique (a design doc + a shell script), no product code; single parent. | `git push origin --delete recovery/stash-2fb075f8-20261023` |
| 6 | jackin-project/jackin | `recovery/stash-88f5643c-20261023` | `ddf8861f` | Stash snapshot, 17 paths, **0 truly unique**; single parent. | `git push origin --delete recovery/stash-88f5643c-20261023` |
| 7 | jackin-project/jackin | `recovery/stash-be51ff69-20261023` | `9cdf189b` | Stash snapshot, 60 paths, 4 unique = `native/README.md`, `ratchet.toml`, 2 shell scripts — no product code; single parent (RUSTSEC claim already dropped as unsupported). | `git push origin --delete recovery/stash-be51ff69-20261023` |
| 8 | jackin-project/jackin | `recovery/stash-d808d094-20261023` | `00f98f32` | Stash snapshot, 54 paths, 2 unique (both `scripts/*` shell wrappers), no product code; single parent. | `git push origin --delete recovery/stash-d808d094-20261023` |
| 9 | jackin-project/jackin | `recovery/strict-merge-gate-merge-readiness-20261022` | `2f96c6da` | Stale pre-rewrite CI gate config, 25 paths, **0 truly unique**; `check:merge-readiness` does not exist in main; duplicate of the `fix/restore-docs-source-globs` lineage. | `git push origin --delete recovery/strict-merge-gate-merge-readiness-20261022` |
| 10 | jackin-project/jackin | `rollout/velnor-3-provider` | `092fe1db` | Orphan line (no merge base with main, root `c24fffeb0` != main root); 20 unique paths are all generated artifacts of `ChainArgos/jackin-agent-brown` — no product code. | `git push origin --delete rollout/velnor-3-provider` |
| 11 | jackin-project/jackin-agent-smith | `recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a` | `b9db5b14` | Pure velnor generator churn (7 paths: `.github-gen/*`, generator state, 5 rendered workflows); own PR #206 closed unmerged; superseded by merged PR #207 (`08cb1c2f`). | `git push origin --delete recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a` |
| 12 | jackin-project/jackin-agent-smith | `recovery/20261006/jackin-agent-smith/2e7119b9c668/jackin-agent-smith` | `c58cfdca` | "WIP preservation snapshot" commit with an **empty patch** (tip tree == parent tree); parent `2e7119b9` is an ancestor of main; 3-dot diff vs main = 0 files. | `git push origin --delete recovery/20261006/jackin-agent-smith/2e7119b9c668/jackin-agent-smith` |
| 13 | tailrocks/terminal-components-baseline | `test/pre-refactor-protection` | `fc624b74` | 666 commits ahead but 3011/3045 changed paths are blob-identical to main; exactly **1** path absent from main = `terminal-components-baseline-goal.txt` (scratch goal prompt). | `git push origin --delete test/pre-refactor-protection` |
| 14 | tailrocks/terminal-components-baseline | `wip/tuiscotti-capture-only` | `0b8b8bb0` | 9385 absent paths = 7236 `run-evidence/**` + 1880 legacy `.tui.json` deleted by PR #6 + machine-local `.cargo/config.toml`; **0** branch-only commits touch `src`/`apps`. | `git push origin --delete wip/tuiscotti-capture-only` |
| 15 | tailrocks/terminal-components-baseline | `wip/tuiscotti-fixcheck` | `8d352ebf` | 1914 absent paths = 1880 legacy `.tui.json` + 23 `run-evidence/fix-validation/**` + machine-local `.cargo/config.toml`; **0** branch-only commits touch `src`/`apps`. | `git push origin --delete wip/tuiscotti-fixcheck` |

**Counts:** 15 branches = 10 `jackin-project/jackin` + 2 `jackin-project/jackin-agent-smith` + 3 `tailrocks/terminal-components-baseline`.

### Pre-flight and post-check (read-only)

```bash
# pre-flight: confirm the 15 tips before deleting
for r in jackin-project/jackin jackin-project/jackin-agent-smith tailrocks/terminal-components-baseline; do
  gh api "repos/$r/branches" --paginate --jq '.[].name'
done

# post-check: the 15 refs must be gone; every excluded / contained / VALUABLE branch must still be there
gh api repos/jackin-project/jackin/branches --paginate --jq '.[].name'
gh api repos/jackin-project/jackin-agent-smith/branches --paginate --jq '.[].name'
gh api repos/tailrocks/terminal-components-baseline/branches --paginate --jq '.[].name'
```

Expected after the deletions: jackin 22 → 12 non-main branches, smith 2 → 0, baseline 4 → 1.

---

## 2. DEBRIS-class rows OUTSIDE the approval set (4 rows) — each needs its own word

**These 4 rows are NOT part of the approval set.** The operator's word in §4 does **not** cover them; each one needs a separate, explicit word of its own. They are listed here so that the operator sees the whole picture, and so that a later deletion cannot be mistaken for a covered one.

| # | repo | branch | tip | class now | what it needs | in the approval set? |
|---|---|---|---|---|---|---|
| 1 | jackin-project/jackin | `preserve/p0.6-prototype-20261109` | `63b2976a5` | **DEBRIS** (unproven review, 2026-11-13) | Its own deletion word. The branch holds a by-design non-mergeable prototype: 20/20 absent paths are `prototypes/p0.6/**`, while its evidence `rewrite/evidence/P0.6-panes.md` is byte-identical in main. The inventory lists it separately as an UNPROVEN CONTAINED candidate; the completed review reclassifies it as DEBRIS. | **NO** |
| 2 | jackin-project/jackin | `recovery/pr1230-blob-auth-77b399f1-20261027` | `77b399f1` | **RECLASSIFIED → VALUABLE** (must not be deleted) | Its own word **to keep it**. It is the only carrier of the landed-nowhere `GitBlobHasher` security fix: 81 truly unique absent paths (77 product code, incl. `crates/tools/jackin-xtask/**`, `crates/core/jackin-process/**`). | **NO** |
| 3 | jackin-project/jackin | `recovery/stash-adoption-6a0d2f03-20261025` | `ade401e7` | **RECLASSIFIED → VALUABLE** (must not be deleted) | Its own word **to keep it** (or to archive it under a tag/retention ref before any future delete). It holds 24 truly unique paths — the only surviving implementation of the `precommit_provision` / `precommit_snapshot` product design. | **NO** |
| 4 | jackin-project/jackin | `recovery/strip-tests-restore-20261030` | `8508f6f8` | **RECLASSIFIED → VALUABLE** (must not be deleted) | Its own word **to keep it** (or to archive it under a tag/retention ref before any future delete). It holds 355 truly unique product/test files (e.g. `crates/adapters/jackin-docker/src/**`, `crates/apps/jackin-capsule/src/**`, `crates/adapters/jackin-omp-store/src/capture.rs`) with no other copy on any remote. | **NO** |

Only row 1 is a DEBRIS-class deletion candidate; rows 2-4 are VALUABLE and **must not** be deleted. All four are live on the remote (§7).

---

## 3. CONTAINED-safe-to-delete candidates (4) — CONTAINED, not DEBRIS

**These 4 are CONTAINED-safe-to-delete, i.e. they are NOT debris.** Their content is proven present in `main` (or superseded by a later `main` rewrite), so they are not part of the debris ratification at all: the operator's word in §4 does not cover them, and they are not DEBRIS. They are kept here as a record, with their proofs.

| # | repo | branch | tip | absent product-code paths | disposition split | proof |
|---|---|---|---|---|---|---|
| 1 | jackin-project/jackin | `codex/openshell-integration-checkpoint-20261108-122311` | `6965f5d1c` | 12 | (a) 5 · (b) 7 · (c) 0 | Every absent product-code path is either already in `main` under a different path/name (a) or superseded by the later `main` rewrite (b): `agent_metadata.rs` → `launch/decide.rs` + `tests/launch/instance_identity.rs`; `secret_broker.rs` → `accounts.rs` (Muse keychain) + `op.rs` (1Password `op read`); `jackin-testkit/src/docker.rs` → `jackin-testkit/src/engine.rs` + `gateway/*`; `write-file.c` + `writer_bootstrap*` → the OpenShell `write_file` RPC; `cancellation.rs` → the `GatewayError::Cancelled` coverage in `tests/gateway/{connect,exec,download_bound,status,error_text,transfer}.rs`; `console/files.rs` → `tests/console/sources.rs:563-600`. |
| 2 | jackin-project/jackin | `codex/openshell-verified-checkpoint-20261108` | `ce119545c` | 12 | (a) 5 · (b) 7 · (c) 0 | Same 12 paths with **byte-identical blob SHAs** to row 1 — the two branches carry the same payload, so row 2 adds no unique residue. |
| 3 | jackin-project/jackin | `preserve/session-boundaries-20261109` | `16de1eedf` | 6 | (a) 3 · (b) 3 · (c) 0 | Same 6 subjects as rows 1/2, each an earlier or byte-identical revision: `write-file.c` (older blob `dafb9560`), `cancellation.rs` (identical blob), `writer_bootstrap.rs` (older, smaller blob `3b100e3c`), `writer_bootstrap_driver.c` / `_shim.c` (identical blobs), `console/files.rs` (identical blob). Strictly less content than rows 1/2, which are themselves superseded. |
| 4 | jackin-project/jackin | `preserve/w1-fixes-20261109` | `78d8b5f02` | 1 | (a) 1 · (b) 0 · (c) 0 | One absent product-code path: `crates/jackin-tui/tests/console/files.rs` (blob `46e156a7`, identical to rows 1-3), never in `main`; the same invariant is covered in `main` at `crates/jackin-tui/tests/console/sources.rs:563-600` (mkfifo + symlink-to-pipe) and its Claude twin. |

All 4 are still live on the remote (§7) and are **kept as CONTAINED**, not deleted by this document.

Open item flagged by the same review (not a debris question): 5 usage fixtures (`crates/usage/jackin-usage/tests/usage/fixtures/{kimi,opencode}/*`) are branch-added and absent from `main`, and they encode a newer Kimi `limits[].window.timeUnit` / OpenCode `opencode-go` API shape than `main` ships. If that newer shape is wanted, it is a **port** decision for the operator — not a debris deletion.

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
- **Nothing outside the approval set is deleted.** The 4 DEBRIS-class rows of §2 and the 4 CONTAINED candidates of §3 stay as they are; the VALUABLE rows of §8 are untouched.
- This document, its commit and its PR **delete nothing**: they only publish the list. The only command shape it contains is the per-branch `git push origin --delete <branch>` of §1, to be run by the operator after ratification.

---

## 6. Recorded deletion — correction of the inventory's baseline count

| repo | branch | status | recorded proof |
|---|---|---|---|
| tailrocks/terminal-components-baseline | `wip/tuiscotti-pin-fec38841` | **Already deleted from the remote** — live check on 2026-11-14: the ref is absent (§7). **No deletion command is needed.** | The branch's class was CONTAINED (superseded), so no work was lost: its single commit only bumps the `tuiscotti` dev-dependency `d8d54155` → `fec38841`, and `main` already pins the same dependency at the strictly newer rev `92ef2d9e` (`main:Cargo.toml`); the commit's doc intent is already in `main:docs/functional-testing.md:96`. The tip was `1c980e88` ("chore: pin tuiscotti to fec38841 for binary baseline review"). |

**Correction to the merged inventory.** `rewrite/triage/branch-inventory-2026-11-12.md` records `tailrocks/terminal-components-baseline` as "0 deleted" (its branches section says the baseline branch was "left on the remote because baseline had 0 deletions", and its per-repo summary says "tailrocks/terminal-components-baseline: 0 deleted"). That claim is **stale**: `wip/tuiscotti-pin-fec38841` was in fact deleted from the remote without a recorded proof in the inventory. The corrected baseline deletion count is **1**, not 0, and the deletion happened under the superset proof above (main pins a strictly newer rev of the same dependency, two commits after `fec38841`). This document records that deletion; it does not undo it and does not re-delete it.

---

## 7. Live verification against the remotes (2026-11-14)

Verified with `gh api repos/<repo>/branches --paginate --jq '.[] | "\(.name) \(.commit.sha)"'`. "Live" = the branch is still on the remote with the tip recorded in the audits; **ALREADY GONE** = the ref no longer exists.

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
| jackin-project/jackin | `chore/restore-1283-remainder` | `3caf18c407c181d92c3b737cc220d196ca4777d2` | live, unclassified here |
| jackin-project/jackin | `docs/triage-inventory-reclass-2026-11-13` | `441b8906fa42227ea0189f31164b8c455a2069a4` | live, unclassified here |
| jackin-project/jackin | `test/provider-eligibility` | `bd0a7d88f5d89b98cf86320db8eaf06363256ddb` | live, unclassified here |

---

## 8. Excluded rows — VALUABLE, must not be deleted

Not part of any approval set; listed so that no one mistakes them for debris.

| repo | branch | why it is kept |
|---|---|---|
| jackin-project/jackin | `fix/typed-model-selection` | VALUABLE — the typed-model-selection code is absent from `main`; open PR #1279. The stale *branch copy* of the inventory wrongly calls it DEBRIS — do not use that copy. |
| jackin-project/jackin | `docs/triage-inventory-reclass-2026-11-13` | Live branch created after the inventory was captured; not in any class list. |
| jackin-project/jackin-sentinel | `fix/s158-postmerge-4` | VALUABLE — open PR #159. |
| jackin-project/jackin-sentinel | `preserve/probe-identity-20261109` | VALUABLE — tip commit not in `main`. |
| tailrocks/terminal-components-baseline | `preserve/local-unique-20261109` | VALUABLE — 2 commits are not in `main`; keep for cherry-pick. |
| jackin-project/jackin-agent-smith | `main`, `jackin-project/jackin` `main`, `tailrocks/terminal-components-baseline` `main` | Trunk. |

---

## 9. Limitations

- The 15 rows of §1 were re-read live from the remotes on 2026-11-14 (§7); the tip SHAs match the audited ones. The evidence itself is inherited from the completed audits (`debris-audit.md`, `smith-debris-verify.md`, `unproven-review.md`) and was not re-derived here.
- The 20 recorded CONTAINED deletions from the previous round could not be re-proved from the remotes (their refs are gone); their recorded proofs were read and accepted.
- `git cherry` counts are patch-id based and report squash-landed commits as unmatched; no verdict depends on them.
- The 4 CONTAINED candidates of §3 are listed as CONTAINED-safe-to-delete, not as DEBRIS: they are excluded from this ratification by class, not by doubt.
- §6 records a deletion that happened before this document; it is a ratification of a fact, not a new deletion.
- Nothing was deleted, pushed to a branch, merged or tagged in producing this document; only this file was added.
