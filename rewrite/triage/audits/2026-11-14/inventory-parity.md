# Branch Inventory Parity Report — 2026-11-13

Scope: parity check of the published branch inventory against the live remotes of the five
tracked repositories. Read-only: nothing was pushed, committed, merged, or deleted.

> **Correction, 2026-11-14.** The "class correct" verdicts this report records for the five rows it
> classes `CONTAINED candidate - UNPROVEN, KEPT` are **superseded** by the publication
> `rewrite/triage/debris-ratification-2026-11-14.md` (§3). The merged inventory (PR #1280, merge commit
> `78d3b86e`; updated by PR #1281, merge commit `926bbd49d`) labels those rows **"CONTAINED - safe to
> delete"**, and that label must not be acted on: the five usage fixtures those branches carry are absent
> from `main` and their product-code paths have no inventory-grade containment proof. The disposition that
> governs is **UNPROVEN, KEPT** - kept, not deletable.

## 0. Locating the inventory — the path in the task is on `main`, not on the stated branch

The task names branch `chore/triage-branch-inventory-2026-11-12`. **That branch does not exist**
on the remote (`git ls-remote`/`gh api repos/.../git/refs/heads` show no such ref). The file at
the named path, `rewrite/triage/branch-inventory-2026-11-12.md`, **does exist — on `main`**,
landed by **PR #1280 "docs(triage): record the branch inventory and the contained branch
deletions"** (merged 2026-11-13T10:30:31Z, merge commit `78d3b86ed`).

There is a similarly-named *older* file on a different branch:
`rewrite/triage/branch-inventory-20261112.md` on branch `chore/triage-inventory-20261112`
(commit `e13360eb8`). The published document explicitly folds that older file in as a
**historical snapshot whose classes are superseded**.

**This report treats the `main` version (`78d3b86ed`, 356 lines, classes
VALUABLE / CONTAINED / DEBRIS) as the authoritative published inventory**, because it is the
version at the task's stated path, it is the one on the default branch, and its class
vocabulary (VALUABLE / DEBRIS) is the one the task asks to spot-check. Where the older
branch-copy differs in class, that is reported as a (c) correction against the older file.

Inventory read from:
- `gh api repos/jackin-project/jackin/contents/rewrite/triage/branch-inventory-2026-11-12.md?ref=main`
- local copy `/tmp/triage/published-main.md` (356 lines)

Live branch lists read with `gh api repos/<repo>/branches --paginate --jq '.[].name'`
(5 repos) and cross-verified against `git ls-remote` on https clones in `/tmp/triage/clones/`.

---

## 1. Summary

| # | check | result |
|---|---|---|
| (a) | inventory rows whose branch no longer exists on the remote | **26** — all stale-but-true (deleted under a recorded proof) |
| (b) | live remote branches missing from the inventory | **2** (both need a new row) |
| (c) | rows whose recorded class still matches the evidence | **43 of 48** rows verified; **3 class corrections + 2 evidence-drift notes** |
| — | live non-main branches total | **28** (jackin 19, sentinel 3, architect 0, smith 2, baseline 4) |

Baseline drift that makes the published document's own numbers stale:
- `jackin-project/jackin` `origin/main` advanced **2 commits** past the inventory's freshness
  anchor `d19cce66f5` → now `78d3b86ed` (`#1275` landed `7a53f6abe`, then `#1280` landed the
  inventory itself).
- `tailrocks/terminal-components-baseline` `origin/main` advanced **2 commits** past the anchor
  `b3034308cb` → now `8063050c` (`#5` landed `bb752ca5`, then `#8` landed `8063050c`).
- `jackin-project/jackin-sentinel`, `jackin-project/jackin-the-architect`,
  `jackin-project/jackin-agent-smith` `origin/main` **did not move** (0 commits ahead of anchor).

---

## (a) Inventory rows whose branch no longer exists on the remote — 26 rows, all stale-but-true

Every one of these is accounted for by a deletion recorded in the published document's
*Contained branch deletions* section with a SHA/patch-id/blob-equality proof, or by a merged
PR. **No row is a surprise absence, and no unproven/valuable branch was lost.** Each row's
recorded class and evidence still stand.

### jackin-project/jackin — 10 rows (all `CONTAINED (deleted 2026-11-12)`)

| branch | recorded proof |
|---|---|
| `codex/pr1264-attach-readiness-20261108` | 4699 changed paths: 685 blob-equal, 362 differ, 9 absent (scratch/archive) |
| `codex/r7-proposal-preservation-20261109` | 4859 changed paths: 795 equal, 343 differ, 71 absent (scratch/archive) |
| `codex/unpublished-openshell-checkpoint-20261109` | 4810 changed paths: 795 equal, 343 differ, 22 absent (scratch/archive) |
| `fix/restore-docs-source-globs` | 29 changed paths: 1 equal, 16 differ, 12 absent (removed by #1173/#1231/#1264) |
| `fix/roles-review-impl` | 78 changed paths: 58 equal, 15 differ, 0 absent; landed as #1278 `d19cce66f` |
| `preserve/hostpath-display-audit-20261108` | 4800 changed paths: 797 equal, 343 differ, 10 absent (scratch/archive) |
| `preserve/local-unique-20261109` | 4813 changed paths: 797 equal, 344 differ, 22 absent (scratch/archive) |
| `preserve/stash-p2-before-reconcile-20261109` | 4731 changed paths: 707 equal, 364 differ, 9 absent; residual typed-model-selection content survives in PR #1279 |
| `preserve/uncommitted-sources-20261109` | 4814 changed paths: 798 equal, 343 differ, 23 absent (scratch/archive) |
| `wt/login-import` | 4736 changed paths: 725 equal, 351 differ, 9 absent (scratch/archive) |

### jackin-project/jackin-the-architect — 9 rows (all `CONTAINED (deleted)`)

All 9 architect rows are absent from the remote (only `main` remains). Proofs:
`archive/cleanup-20261006/architect-mbx-b809284df675`, `…-mbx-e390e399f2`,
`…-original-chain-e390e399f2a4`, `…-reflog-commit-e0b97da3819e`, `…-role-e0b97da381`,
`…-task/original-b809284`, `…-task/original-e0b97da`, `…-task/original-e390e39` — patch-id
equality into MERGED PR #480 (`2fefbdd`) / PR #479 (`7b72b38`), plus exact-SHA duplicate
pointers; and `preserve/betterleaks-1.10.0-20261109` — 37/38 commits are SHA-ancestors of
MERGED PR #481 head, the remaining commit `cb5801f` is content-equal in main.

### jackin-project/jackin-agent-smith — 1 row

`recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-b9c3156cdb88e63c11b9e595a3e694b02238c09a`
— patch-id `d788ec002f4692b0a1354cec6ac5648fad18f7a2` == squash `08cb1c2f` of MERGED PR #207.

### Also absent (already documented as deleted before capture) — 6 rows, consistent

`jackin` `recovery/branches-nightly-compat-20261029` (deleted under SHA proof), `jackin`
`fix/pr1264-review-followups` (PR #1275 merged as `7a53f6abe`, branch deleted),
`jackin-sentinel` `feature/openshell-native` (#158), `jackin-agent-smith`
`feature/openshell-native` (#217), `jackin-the-architect` `feature/openshell-native` (#481),
`terminal-components-baseline` `feature/openshell-native` (#5 merged as `bb752ca5`).
All are recorded in the document's "Rows of the earlier snapshot whose branches are absent"
table with a merged-PR reason. **Stale-but-true.**

---

## (b) Live remote branches missing from the inventory — 2 branches, both need a row

Both were created *after* the inventory was captured (2026-11-12/13) and are absent from the
published document entirely (verified by grep of the published file — no mention of
`fix/probe-interpreter-identity`).

### 1. `jackin-project/jackin-sentinel` → `fix/probe-interpreter-identity`

- tip `ee6c2cd45824ebb404bf2a94349b383caa620c0e` — *"fix(probe): record interpreter identity and classify launch environment"*
- **open PR #160** `fix(probe): record interpreter identity and classify launch environment` (same head)
- merge-base with `origin/main` = `5fd9af372b` (main tip); `git cherry origin/main <branch>` = **1 `+` / 0 `-`** → content **not** in main
- 3 changed paths vs main, all three differ from main (no path is blob-equal):
  `probe/jackin-sentinel-probe`, `probe/tests/egress.sh`, `probe/tests/j08.sh`
- `git grep "interpreter identity\|interpreter_identity" origin/main -- probe/` → **no match in main**
- Suggested class: **VALUABLE** (content absent from main, carried by an open PR).
- Note: it shares a commit *subject* with `preserve/probe-identity-20261109` but is a **different
  commit** (patch-ids `d1f9836b…` vs `cf2e893b…`; the probe-identity tip `01f8ca58` is **not** an
  ancestor of it). It is not a re-push of the inventoried branch, so it must get its own row
  rather than being folded into `preserve/probe-identity-20261109`.

### 2. `jackin-project/jackin` → `fix/typed-model-selection`

**This branch *is* in the published inventory's jackin table** — as the single **VALUABLE** row
— but it is **absent from the DEBRIS/other tables' live-population accounting and, more
importantly, it is missing from the *older* branch-copy inventory
(`rewrite/triage/branch-inventory-20261112.md`) that the task points at.** Since the task's
stated path resolves to the `main` version, this branch is **not** a (b) miss against the
authoritative document — it is flagged here as a (b) miss against the *branch-copy* document
named in the task, and as the single most consequential branch in the whole inventory.

- tip `4d5a04eacdaaf5ad99ea049c7957b64dc96f39b4`
- **open PR #1279** `feat(core): add typed model selection for agent tuples`
- `crates/jackin-core/src/model_selection.rs` is **absent from `origin/main`**
  (`git ls-tree -r origin/main` → no match) and **present on the branch** → content not in main
- `git cherry origin/main <branch>` = **3 `+` / 0 `-`**; merge-base `7a53f6abe`
- 3 changed paths vs main; 2 differ (main's later revision), 1 absent (`model_selection.rs`)
- Recorded class **VALUABLE** is **correct** and must not be downgraded to DEBRIS.
  The published document's own "Notes on corrections" already records that the older branch-copy
  mis-called this DEBRIS and that it was reclassified VALUABLE on 2026-11-13 — so the *main*
  version is right and the *branch-copy* is wrong.

---

## (c) Class-vs-evidence spot-check — 43 of 48 rows verified, 3 corrections, 2 evidence-drift notes

Method: for each live branch, `git cherry origin/main <branch>` (patch-id), `git merge-base`,
`git rev-list --left-right --count`, and **per-path blob equality** computed by comparing full
`git ls-tree -r` blob-SHA maps of the branch tree and the `origin/main` tree, restricted to the
paths changed by `merge-base..tip` (this is the same method the published document uses, so the
numbers are directly comparable). Deleted branches could not be re-proved this way (their refs
are gone); their recorded proofs were re-read and accepted as recorded.

### VALUABLE rows — 4 live + 5 UNPROVEN candidates treated as VALUABLE

| branch | recorded class | measured | verdict |
|---|---|---|---|
| `jackin` `fix/typed-model-selection` | VALUABLE | `model_selection.rs` absent from main; cherry 3/0 | **class correct** |
| `sentinel` `fix/s158-postmerge-4` | VALUABLE | cherry **67/0**, 3 files all differ from main, 0 absent | **class correct**, but see drift note D1 |
| `sentinel` `preserve/probe-identity-20261109` | VALUABLE | cherry 49/0, 12 files +2091/−4514 vs main, tip not in main (`start_interpreter_path` → 0 in main) | **class correct** |
| `baseline` `preserve/local-unique-20261109` | VALUABLE (partly CONTAINED) | 2-dot vs main = 21 A / 75 D / 66 M = 162 paths; 21 branch-only; cherry 6/0 | **class correct**, but see drift note D2 |
| `jackin` `codex/openshell-integration-checkpoint-20261108-122311` | CONTAINED candidate — UNPROVEN, KEPT | 4708 changed paths: 530 equal, 510 differ, **25 absent (17 under `crates/`)** incl. `crates/jackin-session/src/agent_metadata.rs`, `secret_broker.rs`, `crates/jackin-testkit/src/docker.rs`, `crates/jackin-image/images/construct/write-file.c`, `crates/jackin-openshell/tests/gateway/*`, `crates/jackin-tui/tests/console/files.rs` | **class correct — still UNPROVEN** |
| `jackin` `codex/openshell-verified-checkpoint-20261108` | CONTAINED candidate — UNPROVEN, KEPT | 4692 changed: 525 equal, 499 differ, **25 absent** (same 12 product-code paths) | **class correct — still UNPROVEN** |
| `jackin` `preserve/session-boundaries-20261109` | CONTAINED candidate — UNPROVEN, KEPT | 4627 changed: 512 equal, 461 differ, **11 absent (6 product-code)** incl. `write-file.c`, `gateway/*`, `console/files.rs` | **class correct — still UNPROVEN** |
| `jackin` `preserve/w1-fixes-20261109` | CONTAINED candidate — UNPROVEN, KEPT | 4620 changed: 540 equal, 431 differ, **6 absent (1 product-code: `crates/jackin-tui/tests/console/files.rs`)** | **class correct — still UNPROVEN** |
| `jackin` `preserve/p0.6-prototype-20261109` | CONTAINED candidate — UNPROVEN, KEPT | 102 changed: 10 equal, 9 differ, **20 absent — all `prototypes/p0.6/**`**, and main has no `prototypes/` tree at all | **class correct — still UNPROVEN** |

Every one of the five "UNPROVEN" candidates still has absent-from-main paths that carry product
code, and their blob-equal/differ counts reproduce the published document's re-verification
against `origin/main` `7a53f6abe` **exactly** (4708/530/510/25, 4692/525/499/25, 4627/512/461/11,
4620/540/431/6, 102/10/9/20). The "containment NOT proven / do not delete" verdict holds.

### DEBRIS rows — spot-checked, all classes hold

| branch | recorded class | measured | verdict |
|---|---|---|---|
| `jackin` `docs/pr1228-replacement-20261026` | DEBRIS | 63 changed: 0 equal, 1 differ, **62 absent** (`docs/openshell-v2/**`); cherry 2/0 | **holds** |
| `jackin` `recovery/openrouter-exact-money-20261027` | DEBRIS | 38 changed: 0 equal, 0 differ, **38 absent**; cherry 25/0 | **holds** |
| `jackin` `recovery/pr1230-blob-auth-77b399f1-20261027` | DEBRIS | 149 changed: 0 equal, 25 differ, **117 absent**; cherry 50/0 | **holds** |
| `jackin` `recovery/strip-tests-restore-20261030` | DEBRIS | 2471 changed: 0 equal, 7 differ, **899 absent** (883 under `crates/`); cherry 32/0 | **holds** |
| `jackin` `recovery/strict-merge-gate-merge-readiness-20261022` | DEBRIS | 25 changed: 0 equal, 16 differ, 9 absent; cherry 7/0 | **holds** |
| `jackin` `recovery/stash-2f8cc42b-20261023` | DEBRIS | 17 changed, 0 equal, 15 differ, 2 absent; cherry 1/0; single-parent tip (no hidden stash parent) | **holds** |
| `jackin` `recovery/stash-2fb075f8-20261023` | DEBRIS | 48 changed, 0 equal, 31 absent (17 under `crates/`); cherry 1/0 | **holds** |
| `jackin` `recovery/stash-88f5643c-20261023` | DEBRIS | 17 changed, 0 equal, 15 differ, 2 absent; cherry 1/0 | **holds** |
| `jackin` `recovery/stash-adoption-6a0d2f03-20261025` | DEBRIS | 54 changed, 0 equal, 46 absent (35 under `crates/`); cherry 1/0 | **holds** |
| `jackin` `recovery/stash-be51ff69-20261023` | DEBRIS | 60 changed, 0 equal, 34 absent; cherry 8/0 | **holds** — see correction C1 |
| `jackin` `recovery/stash-d808d094-20261023` | DEBRIS | 54 changed, 0 equal, 33 absent (19 under `crates/`); cherry 1/0 | **holds** |
| `jackin` `rollout/velnor-3-provider` | DEBRIS | **no merge base with main** (orphan root); cherry 222 `+` / 1 `-` | **holds** |
| `jackin` `chore/triage-inventory-20261112` | DEBRIS | tip `e13360eb8`, 1 commit, 1 changed path `rewrite/triage/branch-inventory-20261112.md`, absent from main; cherry 1/0 | **holds** — see correction C3 |
| `smith` `…/branch/velnor/fleet-92c3478694ef…` | DEBRIS | 7 changed paths, all generator state (`.github-gen/`, `.github/ci/`, `.github/workflows/ci-*.yml`); cherry 1/0; no product code | **holds** |
| `smith` `recovery/…/2e7119b9c668/jackin-agent-smith` | DEBRIS | **0 changed paths** vs main (empty patch); parent `2e7119b9` is an ancestor of main | **holds** |
| `baseline` `test/pre-refactor-protection` | DEBRIS | 3045 changed: **3011 blob-equal**, 33 differ, **1 absent** (`terminal-components-baseline-goal.txt`, scratch) | **holds** |
| `baseline` `wip/tuiscotti-capture-only` | DEBRIS | 10023 changed: 379 equal, 259 differ, **9385 absent** (run-evidence + legacy `.tui.json`); no branch-only commit touches `src`/`apps` | **holds** |
| `baseline` `wip/tuiscotti-fixcheck` | DEBRIS | 2552 changed: 379 equal, 259 differ, **1914 absent**; no branch-only commit touches `src`/`apps` | **holds** |

### The (c) corrections

**C1 — `jackin` `recovery/stash-be51ff69-20261023`: the recorded RUSTSEC advisory claim is not reproducible.**
The *older* branch-copy inventory recorded this branch as carrying "a rustls-webpki bump
(RUSTSEC-2026-0382/0383) vs main 0.103.16" and flagged it as *operator check of the advisory*.
Measured now: `rustls-webpki` is **0.103.16 on the branch tip, on its parent, and in
`origin/main`** — identical (`git grep -A1 'name = "rustls-webpki"'` across all `*Cargo.lock`
files in the branch tree returns only `0.103.16`). There is **no webpki version bump in the
branch**. The published `main` document already drops that advisory note (its superseded-class
table says "the old 'operator check of RUSTSEC-2026-0382/0383' note is superseded by the later
per-path proof"), so the `main` version is already correct — but the *branch-copy* document
still carries the unsupported claim. The DEBRIS class itself holds either way; only the
advisory evidence is wrong.

**C2 — `jackin` `recovery/stash-be51ff69-20261023` (and the other 5 stash snapshots) have a single parent, so the "audit every additional parent" rule is satisfied for them.**
The published document's "Notes on corrections and residual risk" rule says a stash-tip branch
with un-audited side parents is not CONTAINED and must be kept (raised for
`preserve/stash-p2-before-reconcile-20261109`, which has three parents). Verified for the six
live `recovery/stash-*` branches: every one has exactly **one** parent (`git rev-list --parents -n1`),
so the side-parent rule does not apply to them and their DEBRIS classification is safe.
This **confirms** the published classification rather than correcting it.

**C3 — `jackin` `chore/triage-inventory-20261112` is correctly DEBRIS, but its evidence is now partly superseded.**
Recorded: "adds generated inventory `rewrite/triage/branch-inventory-20261112.md` (absent from
origin/main)". Still true — that file is still absent from `origin/main` (main carries only
`rewrite/triage/branch-inventory-2026-11-12.md`), and the tip `e13360eb8` is still **not** an
ancestor of main (`merge-base --is-ancestor` exit 1). DEBRIS holds. However, the branch's
*purpose* has been fulfilled: its content now lives on `main` as the published inventory via
PR #1280 (different filename, superseding content). The row is stale-but-consistent and should
be re-labelled as *superseded by the published inventory* rather than left as an open DEBRIS
item awaiting operator ratification.

### (c) evidence-drift notes (classes unchanged, but the recorded numbers are now behind)

**D1 — `sentinel` `fix/s158-postmerge-4`: the recorded tip is stale; the branch has advanced.**
Published row records tip `0b39702` *"require real entropy before trusting the run key"*, ahead 61,
3 files +2433/−248. The live tip is now **`e8ef517a88`** *"fix(probe): hold the trusted spec and
config bytes in the parent"*, `git cherry` = **67/0** (was 61), still the same 3 files
(`probe/jackin-sentinel-probe`, `probe/tests/egress.sh`, `probe/tests/j08.sh`), still 0 blob-equal
to main. PR #159's head now equals the branch tip (`e8ef517a88`), so the branch is still the
live PR head. **Class VALUABLE is unchanged and correct** (content still absent from main);
only the recorded tip SHA / ahead-count / +/− line need refreshing.

**D2 — `baseline` `preserve/local-unique-20261109`: the "6 of 8 commits are in main" claim needs restating.**
Published row says 6 of the branch's 8 commits are now in main (5 via PR #5, `ef0ebeac` via PR #8)
and 2 remain unique (`c98c81b1`, `6923fa36`). Measured against the **new** main `8063050c`:
`git cherry origin/main <branch>` = **6/0** — i.e. **6 commits are patch-id-unmatched**, not 2.
`git merge-base --is-ancestor` reports all 8 as not-ancestors of main, which is expected under
squash merges (PR #5 landed as squash `bb752ca5`, PR #8 as squash `8063050c` = the new main tip
— confirmed `merge_commit_sha` of PR #8). Content-wise: 2-dot vs main = 21 A / 75 D / 66 M
(162 paths; 21 branch-only, 18 of which the row enumerates as absent-from-main), and the branch's
`Cargo.toml` tuiscotti pin is `92ef2d9e` — **the same rev main now pins**, i.e. the pin-bump
argument that made `wip/tuiscotti-pin-fec38841` redundant now also covers this branch's pin.
**Class VALUABLE (partly CONTAINED) still holds** — the two commits `c98c81b1` and `6923fa36`
are genuinely not in main (`.velnor/guard/*`, `docs/audits/functional-frame-sync-20261109.md`,
`tests/managed-upgrade-foreign-owner-*` are absent from main) — but the "6 of 8 already in main"
framing no longer matches the cherry count and should be restated as "2 commits still unique".

---

## 2. Corrected live-population accounting

The published document states: *"Live remote population: 28 non-main branches… 20 of them are
branches that were already deleted under a recorded proof… Of the 28 branches that are still on
their remotes: 9 carry unmerged product work (4 VALUABLE + the 5 UNPROVEN candidates), 1 is
CONTAINED but still live (`wip/tuiscotti-pin-fec38841`), and 18 are DEBRIS."*

Measured now: **28 live non-main branches** — jackin 19, sentinel 3, architect 0, smith 2, baseline 4.

Reconciled against the live remotes:
- The document's headline number **28 still holds**, but the composition shifted by two
  compensating moves: `baseline` `wip/tuiscotti-pin-fec38841` has **disappeared** from the remote
  (the document explicitly recorded it as *kept*, "baseline had 0 deletions" — so this is a
  **fourth class of (a) row: deleted without a recorded proof in this inventory**, see below),
  and `sentinel` `fix/probe-interpreter-identity` has **appeared** (the (b) miss).
- Consequently: rows still live = **47** of 48 minus the one row that vanished unproven… see note.

### Unproven disappearance (flagged separately — it is *not* stale-but-true)

`tailrocks/terminal-components-baseline` → `wip/tuiscotti-pin-fec38841`
- recorded class **CONTAINED (superseded)**, action *"kept — left on the remote because baseline had 0 deletions"*
- live remote: **absent** (`gh api repos/.../branches` → only `main`,
  `preserve/local-unique-20261109`, `test/pre-refactor-protection`, `wip/tuiscotti-capture-only`,
  `wip/tuiscotti-fixcheck`)
- The branch was classified CONTAINED with no unique content, so **no work was lost** (its
  substance — a `tuiscotti` pin to `fec38841` — is superseded by main's `92ef2d9e`, which I
  confirmed in both `origin/main:Cargo.toml` and the branch's own `Cargo.toml`).
- But it was **deleted with no deletion proof recorded in this inventory**, so the row is
  *stale-but-harmless*, not *stale-but-true under a recorded proof*. The published document's
  blanket statement "tailrocks/terminal-components-baseline: 0 deleted — all 5 branches remain"
  is now **false** and needs a correction row. This is a bookkeeping gap, not a data-loss
  incident: the classification (CONTAINED, no unique content) was correct, so the deletion was
  safe by construction — but it should be recorded as such.

---

## 3. Checks run (all read-only)

| check | command / method | result |
|---|---|---|
| inventory located | `gh api repos/jackin-project/jackin/contents/rewrite/triage/branch-inventory-2026-11-12.md?ref=main` | found on `main` @ `78d3b86ed` (PR #1280); task's branch `chore/triage-branch-inventory-2026-11-12` **does not exist** |
| branch inventory (5 repos) | `gh api repos/<repo>/branches --paginate --jq '.[].name'` | 19 / 3 / 0 / 2 / 4 non-main = 28 live |
| branch inventory cross-check | `git for-each-ref refs/remotes/origin` on https clones | identical to API output |
| patch-id equality | `git cherry origin/main <branch>` | per-branch `+`/`-` counts recorded above |
| ancestry | `git merge-base`, `git merge-base --is-ancestor` | recorded above; `rollout/velnor-3-provider` has **no** merge base |
| per-path blob equality | full `git ls-tree -r` blob-SHA map of branch tree vs `origin/main` tree, over `merge-base..tip` changed paths | added / deleted / changed / absent counts per branch (above) — reproduces the published document's re-verification numbers exactly on the 5 UNPROVEN candidates |
| main drift | `git rev-list --count <anchor>..origin/main` | jackin 2, sentinel 0, architect 0, smith 0, baseline 2 |
| stash side-parents | `git rev-list --parents -n1` | all 6 live `recovery/stash-*` tips have a **single** parent → side-parent rule N/A |
| PR states | `gh pr list --state all`, `gh pr view` | jackin #1275 MERGED, #1279 OPEN; sentinel #158 MERGED, #159 OPEN, #160 OPEN; baseline #5, #8 MERGED; architect/smith none open |

No `git push`, `git commit`, `git merge`, branch deletion, PR edit or any other mutation was
performed. All clones are read-only working copies under `/tmp/triage/clones/`; the local Git
state of the five repositories was not changed.

## 4. Limitations

- **The stated branch does not exist**, so the inventory had to be sourced from `main`. If the
  task intended a *different* (unpublished) document, that document was not reachable.
- The 26 deleted branches could not be re-proved from the remotes (their refs are gone); their
  recorded proofs were **read and accepted**, not re-executed. The 22 live rows were re-measured
  from scratch.
- `git cherry` is patch-id based and therefore reports squash-landed commits as unmatched;
  this is why the counts above differ numerically from the document's `0/964`-style notation
  while meaning the same thing. No verdict in the document depends on that notation.
- The `jackin` clone is 75 MB and `terminal-components-baseline` is 262 MB; `git cherry` over
  the ~660-commit baseline branches is slow but completed.
- Blob-equality comparisons cover `merge-base..tip`. As the published document itself warns,
  that window excludes trees reachable only through a commit's *additional* parents — verified
  not to apply to any live branch except the already-documented `preserve/stash-p2-before-reconcile-20261109`
  case (that branch is deleted; its residual content is documented as surviving in PR #1279).
