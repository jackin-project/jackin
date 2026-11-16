# DEBRIS Ratification List — 2026-11-14

**Purpose:** one reviewable document for the operator's one-word approval of the DEBRIS branch deletions.
**Status:** READ-ONLY publication. **No branch in this document has been merged or deleted by it.** The deletion of every branch in the approval set happens only after the operator's explicit word (§4).
**Compiled:** 2026-11-14, from the merged inventory `rewrite/triage/branch-inventory-2026-11-12.md` (jackin `main`, PR #1280) plus the completed audits. **The audits are committed with this document** under `rewrite/triage/audits/2026-11-14/`: `debris-audit.md` (18-row audit: 15 CONFIRMED, 3 reclassified), `debris-ratification.md`, `smith-debris-verify.md`, `unproven-review.md`, `inventory-parity.md`, `architect-smith-audit.md` and `baseline-rows-audit.md`. They are in the repository, not only under `/tmp`, so the evidence that justifies every deletion verdict can be reviewed from any checkout and survives a cleared `/tmp`.
**Live check:** every branch name **and tip SHA** below was re-read from the live remotes with `gh api --hostname github.com repos/<repo>/branches --paginate --jq '.[] | "\(.name) \(.commit.sha)"'` on 2026-11-14, and re-verified at 2026-11-14 13:17 and 20:12 UTC (§7). Branches that are already gone are marked **ALREADY GONE**.
**Tip guard:** every deletion command in §1 is leased against the exact tip SHA recorded in that row, so a branch that moved since this table was compiled is **not** deleted (§1).
**Target guard:** every deletion command in §1 names its repository by its **HTTPS URL** (`https://github.com/<repo>.git`), never by the remote alias `origin`, so a clone whose `origin` points at a fork can never delete a ref in the wrong repository (§1, §1b).
**Rewrite guard:** Git rewrites even an explicit repository URL when the configuration carries a `url.<base>.insteadOf` or `url.<base>.pushInsteadOf` entry, so a push that names `https://github.com/<repo>.git` can still land in another repository - in a fork or a mirror that holds the audited SHA, where the lease of the row still succeeds and the ref is deleted there. Every push of the deletion run therefore runs in a **sanitized Git context** - a private global configuration that holds only the credential helpers whose backing store is known not to hold a plaintext credential, built from an allowlist one key at a time so that no credential-bearing value is ever written to a temporary file, and a scratch repository whose own configuration is empty - and the run refuses to start when that context still carries a rewrite or a credential helper that can write a credential to a file (§1).
**Credential guard:** the private global configuration of the sanitized Git context is built from an allowlist - only the credential helpers whose backing store is known not to hold a plaintext credential (`osxkeychain`, `manager`, `manager-core`, `wincred`, `libsecret`, `gnome-keychain`, `gpgme`, `cache`) and the empty value that resets the helper list - and every value is read one key at a time, so the configuration of the operator is never written to a file and no credential-bearing value (`http.*.extraHeader`, an inline credential helper) can reach a temporary file or the output of the run. A file-backed helper (`store` writes its input to a plaintext file), an unknown program (a plain program name is not a proof of storage behavior - it resolves to whatever program answers it on this host) and a helper that carries arguments or a shell command are **refused, not copied**, and one refused helper refuses the run before the first push; the helper list of the push context itself - the system configuration included - is checked again in the same way. Every temporary path of the run - the sanitized configuration, the scratch repository, the control repositories and the branch lists the post-check writes - lives in a private directory of the run and is removed when it ends - on the normal end, on a refusal and on a signal - and two probes, the credential probe and the cleanup probe, prove both properties before the first push (§1).
**Host guard:** every GitHub API call of the procedure names `github.com` explicitly, because `gh` honors the `GH_HOST` of the environment: a run whose environment carries a `GH_HOST` would otherwise read and write another host - a GitHub Enterprise instance - where the 15 refs of the ratification are not, and the guards of the run would pass against a repository the operator never ratified (§1).
**Path guard:** no path of the procedure is a fixed path under `/tmp`. The manifest of the pre-flight, the sanitized configuration, the scratch repository, the control repositories and the branch lists of the post-check live in private directories (`mktemp -d`) that the run removes when it ends, because a predictable path can be written, replaced with a link or read by every other process on the host. The deletion run takes its manifest from the §1 table of this publication - the same rows the pre-flight checks - and never from a file that another process can touch (§1).
**Digest guard:** the manifest digest is computed by a helper that takes the SHA-256 from `sha256sum` where the host has it and from `shasum -a 256` where it does not, so the check runs on a stock macOS as well as on a Linux host; both print the same lowercase hex digest (§1).
**Absent-path accounting:** §1a accounts for **every** path that each of the 15 deletions would remove, re-derived against the live `main` tips; §1b gives the commands that reproduce the numbers and names the `main` tips they were measured against.
**Manifest guard:** the operator's word in §4 carries the SHA-256 digest of the 15 (repository, branch, tip) triples of §1 — `72224143c3f5aeb19e7a4cd12ecb19ecf65caca66c26b9e77a448de064bccb6a`. The pre-flight recomputes that digest from the live tips and refuses to proceed when it differs, and it regenerates every §1 deletion command from its own (repository, branch, tip) triple and refuses a row whose command is not that exact push — so a word can never be applied to a different 15-ref manifest, and a command edited after the ratification cannot change what runs. The deletion run re-checks the same digest against an in-memory snapshot of the manifest - read once from the §1 table of this publication, never from a file - and builds every push from that snapshot (§1, §4).
**Revision guard:** the operator's word in §4 carries, next to the manifest digest, the SHA-256 digest of **this publication at the revision the operator reviewed**, and the deletion run executes only that revision: it recomputes the digest of the publication it reads and refuses the run when the file is not the one the word names. A publication edited after the ratification - in the table, in the commands or in the procedure itself - therefore **invalidates the word**, and every further run is a refusal; the deletion block is executed from the verified file, not copied (§4), so what runs is the revision the word names (§1, §4).
**Archive prerequisite:** no deletion may run before every branch this document deletes — and every captured retained ref — is preserved under `refs/heads/archive/` **and verified again after the namespace lock is active**, and the `refs/heads/archive/**/*` namespace stays locked by a repository ruleset for the run, with each deletion push re-reading its own archive ref immediately before it runs. Fetching a recorded SHA is not preservation: once the last ref to an object is gone, the object is gone from the remote. Nothing in the deletions touches an archive ref and the lock keeps any other writer from touching one either, so the archive closes the window that the retained-ref check alone cannot close (§1, §1a, §9).

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
| §6 | Recorded deletion — the baseline deletion, its proof and its provenance | 1 |
| §7 | Live verification against the remotes | 15 + 9 + 4 + 50 |
| §8 | Excluded rows (VALUABLE / ALREADY GONE — must not be deleted) | 6 |

---

## 1. Approval set — CONFIRMED DEBRIS (15 branches)

These 15 are the **only** branches the operator's word covers. Each row carries its own one-line evidence and the exact deletion command. The command **names the repository by its HTTPS URL**, not by a remote alias, so it deletes the ref in the repository named in the row whatever the local `origin` points at; it can be run from any clone of that repository (or any clone at all) and it deletes exactly one branch. Every path that the deletion would remove is accounted for in §1a.

**A push of these commands must run in the sanitized Git context of the deletion run below.** Git rewrites even an explicit repository URL when the configuration carries a `url.<base>.insteadOf` or `url.<base>.pushInsteadOf` entry, so a push outside that context can land in another repository - in a fork or a mirror that holds the audited SHA, where the lease of the row still succeeds and the ref is deleted there. The deletion run sets that context up, checks it and refuses to push when it still carries a rewrite; a command an operator runs by hand must be preceded by the same check (§1).

**Every deletion command is tip-guarded.** Each command carries `--force-with-lease=refs/heads/<branch>:<recorded-tip-SHA>`, so the remote ref must still point at exactly the SHA recorded in the `tip` column when the delete runs. If the branch has gained a commit since this table was compiled, the push is **rejected** (`stale info`) and nothing is deleted; if the ref is already gone, the push **errors** (`remote ref does not exist`). The recorded tip is the audited one (§7) — run the pre-flight below and stop if a live tip differs from the recorded one.

| # | repo | branch | tip | one-line evidence | exact deletion command |
|---|---|---|---|---|---|
| 1 | jackin-project/jackin | `chore/triage-inventory-20261112` | `e13360eb87518f70c45097366e384851f113ad8d` | 1 commit adds only the generated triage artifact `rewrite/triage/branch-inventory-20261112.md` (9148 B) — the **unhyphenated** file the audited commit adds; its merged replacement is the different, hyphenated `rewrite/triage/branch-inventory-2026-11-12.md` (PR #1280, `78d3b86e`), which carries every row, count and section of it (`git cherry` 1+/0-). 6 paths are absent from `main`'s tree: that artifact, whose content never occurs in `main`'s history, and 5 usage fixtures whose content **is** in `main`'s history (§1a). | `git push --force-with-lease=refs/heads/chore/triage-inventory-20261112:e13360eb87518f70c45097366e384851f113ad8d https://github.com/jackin-project/jackin.git --delete chore/triage-inventory-20261112` |
| 2 | jackin-project/jackin | `docs/pr1228-replacement-20261026` | `912fe737e08fb540be67573ce8b086b30c77ec0c` | 62 of the 63 `docs/openshell-v2/**` paths are absent from `main`'s tree; 58 of them are the branch's own pre-reflow draft (their content never occurs in `main`'s history) and 4 have their content in `main`'s history. The pack itself was merged as PR #1234 (`3ffd6171a`) — that commit is this branch's merge base, so the 58 files are the draft of the pack that landed; docs-only, no product code (§1a). | `git push --force-with-lease=refs/heads/docs/pr1228-replacement-20261026:912fe737e08fb540be67573ce8b086b30c77ec0c https://github.com/jackin-project/jackin.git --delete docs/pr1228-replacement-20261026` |
| 3 | jackin-project/jackin | `recovery/openrouter-exact-money-20261027` | `6ebbe14490b2eddac73263b2df248252e1b10546` | Product fix landed byte-identically in merged PR #1238 (`f16eee70e`, `money_numeric.rs` = `06aa1044`, blob present in `main`'s history); branch residue is comments/extra tests in trees removed by #1264. The residue is enumerated in §1a: 22 paths whose content never occurs in `main`'s history, all of them pre-rewrite files of the crates the OpenShell rewrite replaced (`crates/core/jackin-protocol`, `crates/services/jackin-usage*`, `crates/ui/jackin-console`) plus `ratchet.toml`. | `git push --force-with-lease=refs/heads/recovery/openrouter-exact-money-20261027:6ebbe14490b2eddac73263b2df248252e1b10546 https://github.com/jackin-project/jackin.git --delete recovery/openrouter-exact-money-20261027` |
| 4 | jackin-project/jackin | `recovery/stash-2f8cc42b-20261023` | `e5e6d1fd27efc3eee52b7e18574e4870c5a4a191` | Stash snapshot, 17 paths; every absent path's content is in `main`'s history, but **3** paths that `main` holds at another revision carry a branch-only revision (1,583 added lines, §1a) — the complete count of tip-tree paths whose content is in no retained ref is **3**; single parent, no side-parent risk. | `git push --force-with-lease=refs/heads/recovery/stash-2f8cc42b-20261023:e5e6d1fd27efc3eee52b7e18574e4870c5a4a191 https://github.com/jackin-project/jackin.git --delete recovery/stash-2f8cc42b-20261023` |
| 5 | jackin-project/jackin | `recovery/stash-2fb075f8-20261023` | `4d9540abf301f80b58f81f47d7691132ce48f36c` | Stash snapshot, 48 changed paths. **13** of them are absent from `main`'s tree *and* their content never occurs in `main`'s history (§1a): 8 pre-rewrite `crates/adapters/jackin-image/src/release_evidence/**` files, 2 `crates/tools/jackin-xtask/src/ci/**` files, 1 docs page, 1 integration doc and 1 shell script. The 10 `crates/**` paths are edits to modules the OpenShell rewrite (#1264) deleted, and their content is **not** in `main` — it is shared with row 8 (identical blobs), so deleting rows 5 and 8 together removes it. **1** further path that `main` holds at another revision carries a branch-only revision (195 added lines, §1a) — the complete count of tip-tree paths whose content is in no retained ref is **14**. Single parent. | `git push --force-with-lease=refs/heads/recovery/stash-2fb075f8-20261023:4d9540abf301f80b58f81f47d7691132ce48f36c https://github.com/jackin-project/jackin.git --delete recovery/stash-2fb075f8-20261023` |
| 6 | jackin-project/jackin | `recovery/stash-88f5643c-20261023` | `ddf8861f45646f34b1ca087bd8ed6b9429f270c1` | Stash snapshot, 17 paths; every absent path's content is in `main`'s history, but **1** path that `main` holds at another revision carries a branch-only revision (4,598 added lines, §1a) — the complete count of tip-tree paths whose content is in no retained ref is **1**; single parent. | `git push --force-with-lease=refs/heads/recovery/stash-88f5643c-20261023:ddf8861f45646f34b1ca087bd8ed6b9429f270c1 https://github.com/jackin-project/jackin.git --delete recovery/stash-88f5643c-20261023` |
| 7 | jackin-project/jackin | `recovery/stash-be51ff69-20261023` | `9cdf189b812caf2395048bf0d8dbccdadf1e83a0` | Stash snapshot, 60 changed paths; **4** paths are absent from `main`'s tree *and* their content never occurs in `main`'s history (§1a) = `native/README.md`, `ratchet.toml`, 2 shell scripts — no product code; **2** further paths that `main` holds at another revision carry branch-only revisions (4,730 added lines, §1a) — the complete count of tip-tree paths whose content is in no retained ref is **6**; single parent (RUSTSEC claim already dropped as unsupported). | `git push --force-with-lease=refs/heads/recovery/stash-be51ff69-20261023:9cdf189b812caf2395048bf0d8dbccdadf1e83a0 https://github.com/jackin-project/jackin.git --delete recovery/stash-be51ff69-20261023` |
| 8 | jackin-project/jackin | `recovery/stash-d808d094-20261023` | `00f98f32514aca8005c56dd2e44e3fc03c326b46` | Stash snapshot, 54 changed paths; **13** paths are absent from `main`'s tree *and* their content never occurs in `main`'s history (§1a) — the same 10 pre-rewrite `crates/**` files as row 5 (identical blobs) plus 1 docs page and 2 `scripts/*` shell wrappers. The content is not in `main`; it is shared with row 5. **6** further paths that `main` holds at another revision carry branch-only revisions (5,710 added lines, §1a) — the complete count of tip-tree paths whose content is in no retained ref is **19**. Single parent. | `git push --force-with-lease=refs/heads/recovery/stash-d808d094-20261023:00f98f32514aca8005c56dd2e44e3fc03c326b46 https://github.com/jackin-project/jackin.git --delete recovery/stash-d808d094-20261023` |
| 9 | jackin-project/jackin | `recovery/strict-merge-gate-merge-readiness-20261022` | `2f96c6dadd4e1974f89f5a5f7407c0f6ff221fae` | Stale pre-rewrite CI gate config, 25 paths; every absent path's content is in `main`'s history, but **2** paths that `main` holds at another revision carry branch-only revisions (315 added lines, §1a) — the complete count of tip-tree paths whose content is in no retained ref is **2**; `check:merge-readiness` does not exist in main; duplicate of the `fix/restore-docs-source-globs` lineage. | `git push --force-with-lease=refs/heads/recovery/strict-merge-gate-merge-readiness-20261022:2f96c6dadd4e1974f89f5a5f7407c0f6ff221fae https://github.com/jackin-project/jackin.git --delete recovery/strict-merge-gate-merge-readiness-20261022` |
| 10 | jackin-project/jackin | `rollout/velnor-3-provider` | `092fe1db796367479029d4e94379530f7f5965c0` | Orphan line (no merge base with main, root `c24fffeb0` != main root); the tip tree is **28 paths**: 21 absent from `main`'s tree, of which **20** have content that never occurs in `main`'s history; the other 7 exist in `main` at a different revision (6) or the same revision (1). Of the 20, **18** are in no retained ref — the other 2 are held by the retained ref `wt/a15-declared-auth-producer` (`dd2008cbf`) — and 5 of the 6 different-revision paths carry a branch-only revision, so the complete count of tip-tree paths whose content is in no retained ref is **23**. 9 of the 20 are **byte-identical** (same blob SHA) to files in `ChainArgos/jackin-agent-brown` (`.dockerignore`, `.hadolint.yaml`, `.pre-commit-config.yaml`, `Dockerfile`, `hooks/preflight.sh`, `hooks/source.sh`, `jackin.role.toml`, `scripts/marketplace-audit.sh`, `zshrc.d/docker-helpers.zsh`); the other 11 are velnor-generated CI files (`.github-gen/velnor-workflow.toml`, `.github/actionlint.yaml`, `.github/ci/project.toml`, `.github/ci/.github-actions-generator-state` and 7 rendered `.github/workflows/*.yml`). No product code (§1a). | `git push --force-with-lease=refs/heads/rollout/velnor-3-provider:092fe1db796367479029d4e94379530f7f5965c0 https://github.com/jackin-project/jackin.git --delete rollout/velnor-3-provider` |
| 11 | jackin-project/jackin-agent-smith | `recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a` | `b9db5b149cc46baba9c49549432307c29e3972b0` | Pure velnor generator churn. **12** paths are absent from `main`'s tree, of which **7** have content that never occurs in `main`'s history: the generator source `.github-gen/velnor-workflow.toml`, the generator state `.github/ci/.github-actions-generator-state` and **5** rendered workflows (`.github/workflows/{ci-main,ci-policy,ci-pr,ci-unit-docker,maintenance}.yml`); the other 5 (`.err`, `.github/actionlint.yaml`, `.github/ci/project.toml`, `scripts/marketplace-audit.sh` and `.github/workflows/nightly.yml`) have their content in `main`'s history — `nightly.yml` is inherited unchanged from the merge base `8c1dbe3d`, an ancestor of `main` (blob `7db67da1` is in `main`'s object history), so it is not a branch-only blob (§1a). Own PR #206 closed unmerged; superseded by merged PR #207 (`08cb1c2f`). | `git push --force-with-lease=refs/heads/recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a:b9db5b149cc46baba9c49549432307c29e3972b0 https://github.com/jackin-project/jackin-agent-smith.git --delete recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a` |
| 12 | jackin-project/jackin-agent-smith | `recovery/20261006/jackin-agent-smith/2e7119b9c668/jackin-agent-smith` | `c58cfdcaaca85f606c2abc4ddb8f5afaae47f737` | "WIP preservation snapshot" commit with an **empty patch** (tip tree == parent tree); parent `2e7119b9` is an ancestor of main; 3-dot diff vs main = 0 files. 3 paths are absent from `main`'s tree and all 3 have their content in `main`'s history (§1a). | `git push --force-with-lease=refs/heads/recovery/20261006/jackin-agent-smith/2e7119b9c668/jackin-agent-smith:c58cfdcaaca85f606c2abc4ddb8f5afaae47f737 https://github.com/jackin-project/jackin-agent-smith.git --delete recovery/20261006/jackin-agent-smith/2e7119b9c668/jackin-agent-smith` |
| 13 | tailrocks/terminal-components-baseline | `test/pre-refactor-protection` | `fc624b7456132fb74675ab452e6b64ecb3f69a70` | 666 commits ahead but 3011/3045 changed paths are blob-identical to main; exactly **1** path absent from main = `terminal-components-baseline-goal.txt` (scratch goal prompt) and its content never occurs in `main`'s history (§1a). **3** further paths that `main` holds at another revision carry a branch-only revision that no retained ref holds — `tests/functional/cases/j04_usage.rs`, `tests/functional/cases/s05_filebrowser.rs` and `tests/functional/cases/t01_test.rs`, 95 added lines of frame/readiness synchronization (§1a adjudicates each one) — so the complete count of tip-tree paths whose content is in no retained ref is **4**. The archive prerequisite of §1 preserves those three revisions before the deletion runs. | `git push --force-with-lease=refs/heads/test/pre-refactor-protection:fc624b7456132fb74675ab452e6b64ecb3f69a70 https://github.com/tailrocks/terminal-components-baseline.git --delete test/pre-refactor-protection` |
| 14 | tailrocks/terminal-components-baseline | `wip/tuiscotti-capture-only` | `0b8b8bb077061eee6ca04466148f017e316edfd3` | **9517** paths absent from `main`'s tree. **9117** of them have content that never occurs in `main`'s history: 7236 `run-evidence/**` generated capture files, 1880 legacy `tests/functional/baselines/**/*.tui.json` baselines and the machine-local `.cargo/config.toml`. The other **400** have their content in `main`'s history: 260 `run-evidence/**` files, 98 `.tui.json` baselines that PR #6 (`1d7f63db`) deleted from `main`, 34 `src/bin/jackin/**` files that `43456dee` moved to `apps/jackin/src/bin/jackin/**` and 8 `scripts/coverage-fixtures/**` files. 9117 + 400 = 9517 — every absent path is accounted for (§1a). **245** further paths that `main` holds at another revision carry branch-only revisions (3,939 added lines, §1a) — the complete count of tip-tree paths whose content is in no retained ref is **9,362**. **0** branch-only commits touch `src`/`apps`. | `git push --force-with-lease=refs/heads/wip/tuiscotti-capture-only:0b8b8bb077061eee6ca04466148f017e316edfd3 https://github.com/tailrocks/terminal-components-baseline.git --delete wip/tuiscotti-capture-only` |
| 15 | tailrocks/terminal-components-baseline | `wip/tuiscotti-fixcheck` | `8d352ebfb932b06bf18c537e924389b0aae07e42` | **2046** paths absent from `main`'s tree. **1904** of them have content that never occurs in `main`'s history: 1880 legacy `tests/functional/baselines/**/*.tui.json` baselines, 23 `run-evidence/fix-validation/**` files and the machine-local `.cargo/config.toml`. The other **142** have their content in `main`'s history: 2 `run-evidence/fix-validation/**` files, 98 `.tui.json` baselines that PR #6 (`1d7f63db`) deleted from `main`, 34 `src/bin/jackin/**` files that `43456dee` moved to `apps/jackin/src/bin/jackin/**` and 8 `scripts/coverage-fixtures/**` files. 1904 + 142 = 2046 — every absent path is accounted for (§1a). **245** further paths that `main` holds at another revision carry branch-only revisions (3,972 added lines, §1a) — the complete count of tip-tree paths whose content is in no retained ref is **2,149**. **0** branch-only commits touch `src`/`apps`. | `git push --force-with-lease=refs/heads/wip/tuiscotti-fixcheck:8d352ebfb932b06bf18c537e924389b0aae07e42 https://github.com/tailrocks/terminal-components-baseline.git --delete wip/tuiscotti-fixcheck` |

**Counts:** 15 branches = 10 `jackin-project/jackin` + 2 `jackin-project/jackin-agent-smith` + 3 `tailrocks/terminal-components-baseline`.

### Pre-flight, deletion run and post-check — steps 3, 5 and 6 write to the remotes

The pre-flight is **five steps in order**, and all five must pass before the first deletion push;
step 6, the deletion run, executes only after the operator's word (§4); the post-check then reads
the end state. **Steps 1, 2 and 4 and the post-check are read-only. Steps 3, 5 and 6 write to the
remotes:** step 3 issues PATCH and POST requests that create or update `refs/heads/archive/**/*`
branches in all three repositories — 78 archive heads, the 15 targets plus the 63 captured retained
refs, each repository's `main` among them — step 5 installs a repository ruleset that locks
`refs/heads/archive/**/*` against deletion and force-update, and step 6 pushes the 15 deletions.
Steps 3 and 5 need write authorization on all three repositories and step 5 needs admin rights over
repository rules, so an operator whose intent is inspection only must stop before step 3: from that
point on the procedure materially mutates the remotes, and step 6 deletes branches.
It is not a transaction: the archive refs that steps 3-5 create, lock and verify — and that step 6
re-reads immediately before each deletion push — are what make the deletions safe against the window
between this check and the pushes (§9).

```bash
set -euo pipefail

# 0. REVISION GATE (first: nothing below runs unverified). The pre-flight makes
#    authenticated writes (archive refs, lock rulesets), so its own bytes are
#    authenticated before any of it runs - including this helper block itself.
#    The operator's word carries the reviewed publication digest; the file this
#    block runs from must hash to it. The §4 launcher repeats the check before
#    the deletion block, so both executable halves are gated on the same digest.
#    An operator who runs this fence from a copy must verify the copy first with
#    an independent command (typed, not copied): compare the file's SHA-256 with
#    the publication digest in the sent word before executing anything from it.
# The manifest digest of the 15 reviewed triples, defined before the gate so the
# gate checks the word's manifest digest against the reviewed value.
MANIFEST=72224143c3f5aeb19e7a4cd12ecb19ecf65caca66c26b9e77a448de064bccb6a
PUB=rewrite/triage/debris-ratification-2026-11-14.md
[ -n "${RATIFY_WORD:-}" ] || { echo "refusing: set RATIFY_WORD to the operator's ratification word before the pre-flight" >&2; exit 1; }
case "$RATIFY_WORD" in
  "RATIFY DEBRIS: 15 branches manifest sha256:"????????????????????????????????????????????????????????????????" publication sha256:"????????????????????????????????????????????????????????????????)
    ;;
  *) echo "refusing: the word is not the reviewed ratification word" >&2; exit 1 ;;
esac
want=${RATIFY_WORD##*publication sha256:}
want_manifest=${RATIFY_WORD##*manifest sha256:}; want_manifest=${want_manifest%% *}
[ "$want_manifest" = "$MANIFEST" ] || { echo "refusing: the word carries a manifest digest that is not the reviewed one" >&2; exit 1; }
case "$want" in ""|*[!0-9a-f]*) echo "refusing: the word carries no publication digest" >&2; exit 1 ;; esac
if command -v sha256sum >/dev/null 2>&1; then got=$(sha256sum < "$PUB"); got=${got%% *}
elif command -v shasum >/dev/null 2>&1; then got=$(shasum -a 256 < "$PUB"); got=${got%% *}
else echo "refusing: no SHA-256 command on this host" >&2; exit 1; fi
[ "$got" = "$want" ] || { echo "refusing: the publication changed after ratification" >&2; exit 1; }

# Every GitHub API call of this procedure is pinned to github.com. An unqualified call honors the
# GH_HOST of the environment, so a run whose environment carries a GH_HOST reads and writes
# another host - a GitHub Enterprise instance - where the 15 refs of the ratification are not,
# and the guards of the run would pass against a repository the operator never ratified. The
# wrapper names the host on every call, and the exported GH_HOST pins any other gh invocation
# of the block to the same host.
gh_api() { gh api --hostname github.com "$@"; }
export GH_HOST=github.com

# The digest helper. A stock macOS has no sha256sum and a Linux host without GNU coreutils has
# no shasum, so the helper takes the SHA-256 from whichever of the two this host has. Both print
# the same lowercase hex digest; the check below proves the one this host uses.
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 | awk '{print $1}'
  else
    echo "no SHA-256 command on this host: neither sha256sum nor shasum is installed" >&2
    return 1
  fi
}
[ "$(printf '' | sha256_of)" = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" ] \
  || { echo "the SHA-256 helper does not return the digest of the empty input"; exit 1; }

# The temporary paths of the pre-flight live in a private directory of their own. A fixed path
# under /tmp is predictable: every other process on the host can write it, replace it with a
# link or read it between the write and the read, so the manifest of the run never lands in
# such a path. The directory is private (mktemp -d) and the cleanup removes it and everything in
# it when the pre-flight ends - on the normal end, on a refusal and on a signal.
PREFLIGHT_DIR=$(mktemp -d)
cleanup_preflight() {
  result=$?
  trap - 0 HUP INT TERM
  rm -rf "$PREFLIGHT_DIR"
  exit "$result"
}
trap cleanup_preflight 0
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

# 1. MANIFEST CHECK: the 15 (repository, branch, tip) triples of §1 must be the reviewed ones,
#    and each §1 deletion command must be the push that triple generates. The manifest digest is
#    the one the operator's word in §4 carries; a word without it, or with a different one, is a refusal
#    (§4). A branch that moved is refused here and again by the --force-with-lease guard of its
#    deletion command, and a command whose --delete operand is not the row's own branch is refused
#    here, so a publication edited after the ratification cannot change what runs.
#    (The manifest digest itself is defined once above the revision gate so the
#    gate checks the word's digest against the reviewed value.)
MANIFEST_ROWS="$PREFLIGHT_DIR/manifest-rows.txt"
MANIFEST_FILE="$PREFLIGHT_DIR/manifest.txt"
LIVE_MANIFEST="$PREFLIGHT_DIR/live-manifest.txt"
sed -n '/^## 1\. Approval set/,/^\*\*Counts:\*\*/p' rewrite/triage/debris-ratification-2026-11-14.md \
  | awk -F'|' '/^\| [0-9]+ \|/ && $5 ~ /`[0-9a-f]{40}`/ {gsub(/[ `]/,"",$3); gsub(/[ `]/,"",$4); gsub(/[ `]/,"",$5); gsub(/`/,"",$7); gsub(/^ +| +$/,"",$7); print $3" "$4" "$5" "$7}' \
  | sort -k2 > "$MANIFEST_ROWS"
cut -d' ' -f1-3 "$MANIFEST_ROWS" > "$MANIFEST_FILE"
while read -r repo branch tip cmd; do
  live=$(gh_api "repos/$repo/commits/$branch" --jq '.sha') || { echo "cannot read $repo $branch"; exit 1; }
  [ "$live" = "$tip" ] || { echo "live tip of $repo $branch is $live, not the reviewed $tip"; exit 1; }
  want="git push --force-with-lease=refs/heads/$branch:$tip https://github.com/$repo.git --delete $branch"
  [ "$cmd" = "$want" ] || { echo "the §1 command of $repo $branch is not the deletion of that reviewed triple: $cmd"; exit 1; }
  echo "$repo $branch $live"
done < "$MANIFEST_ROWS" > "$LIVE_MANIFEST"
digest=$(sort -k2 "$LIVE_MANIFEST" | sha256_of)
[ "$digest" = "$MANIFEST" ] || { echo "the 15-ref manifest is not the reviewed one (digest $digest)"; exit 1; }

# target check: the §1 commands name their repository by URL, so no remote alias is used. Git
# rewrites even that URL when the configuration carries a url.<base>.insteadOf or a
# url.<base>.pushInsteadOf entry, so the deletion run below pushes them in a sanitized Git context
# and refuses to start when that context still carries a rewrite; a command an operator runs by
# hand must run in that context too.

# 2. RETAINED-REF CHECK: every captured ref must still hold the commit this document measured
#    (rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt).
#    - the ref still exists: its live tip must CONTAIN the captured commit (the ref advanced).
#      compare(base=captured, head=live) is "ahead" or "identical". A ref that was rewritten
#      without carrying the captured commit is a STOP: the copy the accounting relies on is gone.
#    - the ref is gone: the captured commit must still resolve in the repository. A squash-merged
#      PR keeps its head under refs/pull/N/head, so the objects survive the branch deletion; a
#      commit that no longer resolves is gone, and that is a STOP.
#    A ref that ADVANCED is not a failure: this check survives main moving on after this
#    publication is merged.
while read -r repo ref sha; do
  case "$repo" in \#*) continue ;; esac
  live=$(gh_api "repos/$repo/commits/$ref" --jq '.sha' 2>/dev/null) || live=GONE
  if [ "$live" != GONE ]; then
    status=$(gh_api "repos/$repo/compare/$sha...$live" --jq '.status' 2>/dev/null || echo unknown)
    case "$status" in
      identical|ahead) continue ;;
      *) echo "retained ref $repo $ref no longer holds the captured $sha (live=$live)"; exit 1 ;;
    esac
  fi
  gh_api "repos/$repo/commits/$sha" --jq '.sha' >/dev/null 2>&1 \
    || { echo "retained ref $repo $ref is gone and the captured $sha no longer resolves"; exit 1; }
done < rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt

# this publication's own branch needs no exemption any more: before the merge its live tip is
# ahead of the captured SHA, and after the merge main contains it - in both cases the captured
# commit is still reachable, which is exactly what step 2 tests.

# 3. ARCHIVE PREREQUISITE (mandatory, before ANY deletion): preserve every ref this document
#    relies on. A SHA that is fetched is not preserved: once the last ref to an object is gone, the
#    object is gone from the remote. Each archive ref holds the recorded tip, so every object the
#    deletion would orphan - the tip tree and the whole history behind it - stays reachable.
archive_ref() {                       # $1 repository, $2 name under archive/, $3 sha
  if gh_api "repos/$1/git/ref/heads/archive/$2" --jq '.object.sha' 2>/dev/null | grep -qx "$3"; then
    return 0                          # already archived at the recorded tip
  fi
  gh_api -X PATCH "repos/$1/git/refs/heads/archive/$2" -f sha="$3" --silent >/dev/null 2>&1 \
    || gh_api -X POST "repos/$1/git/refs" -f ref="refs/heads/archive/$2" -f sha="$3" --silent >/dev/null \
    || { echo "archive ref for $1 $2 could not be created"; exit 1; }
  gh_api "repos/$1/git/ref/heads/archive/$2" --jq '.object.sha' | grep -qx "$3" \
    || { echo "archive ref $1 archive/$2 does not hold $3"; exit 1; }
}
while read -r repo branch tip; do archive_ref "$repo" "$branch" "$tip"; done < "$MANIFEST_FILE"
# the retained-ref companion is an executable input: its repo/ref/SHA fields drive
# authenticated archive writes below. It is bound to the reviewed revision by digest -
# a contributor edit after review is a refusal, not a silent input change:
RETAINED_SHA_EXPECTED=2754c2826527978f42cd3d67d33d8681b104f79682fb380d62cf4523a211676f
if command -v sha256sum >/dev/null 2>&1; then retained_got=$(sha256sum < rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt); retained_got=${retained_got%% *}
elif command -v shasum >/dev/null 2>&1; then retained_got=$(shasum -a 256 < rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt); retained_got=${retained_got%% *}
else echo "refusing: no SHA-256 command on this host" >&2; exit 1; fi
[ "$retained_got" = "$RETAINED_SHA_EXPECTED" ] || { echo "refusing: the retained-ref companion changed after review" >&2; exit 1; }
while read -r repo ref sha; do
  case "$repo" in \#*) continue ;; esac
  archive_ref "$repo" "retained/$ref" "$sha"
done < rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt

# 4. VERIFICATION: re-read every archive ref from the API - the 15 target archives of the manifest
#    and every retained archive of the retained list alike. Each must hold the recorded tip, and
#    each is a ref that the deletions do not touch, so every object the deletion would orphan stays
#    reachable. With the archive tips added to the retained set, the fourth bucket of §1a (§1b
#    prints it) is 0 for every row.
while read -r repo branch tip; do
  got=$(gh_api "repos/$repo/git/ref/heads/archive/$branch" --jq '.object.sha')
  [ "$got" = "$tip" ] || { echo "archive ref $repo archive/$branch holds $got, not $tip"; exit 1; }
done < "$MANIFEST_FILE"
while read -r repo ref sha; do
  case "$repo" in \#*) continue ;; esac
  got=$(gh_api "repos/$repo/git/ref/heads/archive/retained/$ref" --jq '.object.sha') \
    || { echo "retained archive ref $repo archive/retained/$ref is gone"; exit 1; }
  [ "$got" = "$sha" ] || { echo "retained archive ref $repo archive/retained/$ref holds $got, not $sha"; exit 1; }
done < rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt

# 5. ARCHIVE NAMESPACE LOCK (mandatory, before the first deletion push): make
#    refs/heads/archive/**/* undeletable and un-force-updatable in each repository, so no authorized
#    writer can remove or rewrite an archive ref between its verification and the deletion it
#    protects. The pattern is refs/heads/archive/**/* because GitHub matches a ruleset ref_name with
#    File::FNM_PATHNAME, where `*` never crosses a `/` and `qa/**/*` is the docs' own example of a
#    pattern that spans any number of path segments: `refs/heads/archive/*` and
#    `refs/heads/archive/**` match NONE of the archive refs of step 3, because every branch name of
#    §1 carries a `/` and every archive ref is nested (archive/<branch> and archive/retained/<ref>).
#    Managing repository rules needs admin rights. The list endpoint returns summaries,
#    so an existing lock is fetched in full and must be the active, branch-targeted, unbypassed,
#    two-rule lock with no exclusions that this procedure creates - anything else is a stop;
#    repairing an archive ref later means removing the lock first.
for r in jackin-project/jackin jackin-project/jackin-agent-smith tailrocks/terminal-components-baseline; do
  lock_id=$(gh_api "repos/$r/rulesets" --jq '.[] | select(.name == "debris-archive-lock") | .id')
  if [ -n "$lock_id" ]; then
    existing=$(gh_api "repos/$r/rulesets/$lock_id" --jq '[.enforcement, (.conditions.ref_name.include | sort | join(",")), (.conditions.ref_name.exclude | sort | join(",")), ([.rules[].type] | sort | join(",")), (.bypass_actors | length), .target] | @tsv')
    printf '%s\n' "$existing" | awk -F'\t' '$1 == "active" && $2 == "refs/heads/archive/**/*" && $3 == "" && $4 == "deletion,non_fast_forward" && $5 == "0" && $6 == "branch" {ok++} END{exit !(ok == NR && NR > 0)}' \
      || { echo "the archive lock of $r is not the active, branch-targeted, unbypassed two-rule lock this procedure requires"; exit 1; }
    continue
  fi
  gh_api -X POST "repos/$r/rulesets" --input - <<'JSON' || { echo "the archive lock of $r could not be created"; exit 1; }
{
  "name": "debris-archive-lock",
  "target": "branch",
  "enforcement": "active",
  "conditions": {
    "ref_name": {"include": ["refs/heads/archive/**/*"], "exclude": []}
  },
  "rules": [{"type": "deletion"}, {"type": "non_fast_forward"}]
}
JSON
done

# POST-LOCK RE-VERIFICATION (mandatory, with the lock active, before the operator's word and any
# deletion): an authorized writer can delete or rewrite an archive ref between its individual
# step-3 check and the lock above, and the lock would then seal the missing or incorrect state.
# The lock is active now and immutable, so the state this loop reads is the state the deletions
# will see: every archive ref of step 3 - the 15 target archives and the retained archives of the
# retained list alike - must hold its recorded tip, or the run stops here.
while read -r repo branch tip; do
  got=$(gh_api "repos/$repo/git/ref/heads/archive/$branch" --jq '.object.sha')
  [ "$got" = "$tip" ] || { echo "archive ref $repo archive/$branch holds $got, not $tip after the lock"; exit 1; }
done < "$MANIFEST_FILE"
while read -r repo ref sha; do
  case "$repo" in \#*) continue ;; esac
  got=$(gh_api "repos/$repo/git/ref/heads/archive/retained/$ref" --jq '.object.sha') \
    || { echo "retained archive ref $repo archive/retained/$ref is gone after the lock"; exit 1; }
  [ "$got" = "$sha" ] || { echo "retained archive ref $repo archive/retained/$ref holds $got, not $sha after the lock"; exit 1; }
done < rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt
```

**The operator's word comes first (§4).** Set `RATIFY_WORD` to the sent line before
running anything below: the pre-flight verifies the publication digest from the word
before its first authenticated call, and the deletion run below executes only after
the word exists. Steps 1-5 must all have passed before the deletion run proceeds; the
word is taken in `RATIFY_WORD` - **from the line the operator sent, and not from this
file**, because a word that lived in the publication could be edited with it - reads the manifest once into an in-memory snapshot - the §1 table of this
publication, and not a file of the pre-flight, whose private directory is gone by then - checks
that snapshot's digest and the digest of this publication against the two the word carries, and
builds every push from that snapshot — so nothing that is not the ratified revision runs, and a
manifest or a publication that changed after the pre-flight cannot change what runs. Step 5 re-reads **every** archive ref again after the lock is active - the 15 target archives and
the retained archives of the retained list alike - so the lock never seals a missing or rewritten
archive: what is locked is what was verified. Each iteration re-reads the row's own
archive ref immediately before it runs exactly that row's §1 command, so a deletion happens only
while the archive copy it orphans is present and intact. With the step-5 lock in place no writer
can change that copy between the re-read and the push.

Every push of this run goes out through a **sanitized Git context**. Git rewrites even an explicit
repository URL when the configuration carries a `url.<base>.insteadOf` or `url.<base>.pushInsteadOf`
entry, so a rewrite on the operator's machine can send a push that names
`https://github.com/<repo>.git` to another repository - to a fork or a mirror that holds the
audited SHA, where the lease of the row still succeeds and the ref is deleted there. The run
therefore replaces the global configuration with a private copy that holds the credential helper
and nothing else, and points the Git directory at a scratch repository whose own configuration is
empty. The run then refuses to start when that context still carries a rewrite, and a control that
runs the same refusal check and the same push against two local repositories proves that the
guard sees the redirect it has to defeat.

**The private copy holds no credential.** The copy is not a copy of the operator's configuration:
it is built from an allowlist, one key at a time, and a helper is copied only when its backing
store is known not to hold a plaintext credential - `osxkeychain`, `manager`, `manager-core`,
`wincred`, `libsecret`, `gnome-keychain`, `gpgme` and `cache` - or when it is the empty value that
resets the helper list. Every other helper is **refused, not copied**, and a refused helper
refuses the run: `store` writes its input to a plaintext file, an unknown program's storage
behavior is unknown and it can persist its input - a plain program name is not a proof, it
resolves to whatever program answers it on this host - and a helper that carries arguments or a
shell command can carry the credential in its own text. The name is matched without case, because
a lookup of `git-credential-Store` finds `git-credential-store` on a case-insensitive filesystem.
Every other key - the `url.*` rewrites, the `http.*` headers, the `include.*` files - is never
read. The configuration of the operator is never written to a file, so no temporary file of this
run holds a value the allowlist rejects, and every temporary path of this run - the sanitized
configuration, the scratch repository, the control repositories and the branch lists the
post-check writes - lives in a private directory of the run and is removed when the run ends - on
the normal end, on a refusal and on a signal. The helper list of the push context itself - the
system configuration included, which stays in place because it carries the credential helper on
macOS - is checked again before the first push, so no helper that can write a credential to a
file is ever invoked by the pushes. Two probes prove the properties before the first push: the
**credential probe** builds a global configuration that carries a credential in the two forms
that carry one - an extra header and an inline credential helper - together with a file-backed
helper, its case variant and an unknown program, and runs the same sanitizing against it: the run
must be refused and the credential must reach neither a file nor the output; and a second
configuration that carries only known-safe helpers must still be copied, so the pushes keep their
authentication. The **cleanup probe** runs the cleanup of the run against fixtures of its own -
never against a path of the run - in a process that ends in each of the three ways - a subshell
for the normal end and for the refusal, a child process for the signal - and checks that no
fixture is left behind.

```bash
# --- deletion run: begin ---
set -euo pipefail

# Every GitHub API call of this run is pinned to github.com. An unqualified call honors the
# GH_HOST of the environment, so a run whose environment carries a GH_HOST reads and writes
# another host - a GitHub Enterprise instance - where the 15 refs of the ratification are not,
# and the guards of the run would pass against a repository the operator never ratified. The
# wrapper names the host on every call, and the exported GH_HOST pins any other gh invocation
# of the block to the same host.
gh_api() { gh api --hostname github.com "$@"; }
export GH_HOST=github.com

# The digest helper. A stock macOS has no sha256sum and a Linux host without GNU coreutils has
# no shasum, so the helper takes the SHA-256 from whichever of the two this host has. Both print
# the same lowercase hex digest; the check below proves the one this host uses.
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 | awk '{print $1}'
  else
    echo "no SHA-256 command on this host: neither sha256sum nor shasum is installed" >&2
    return 1
  fi
}
[ "$(printf '' | sha256_of)" = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" ] \
  || { echo "the SHA-256 helper does not return the digest of the empty input"; exit 1; }

MANIFEST=72224143c3f5aeb19e7a4cd12ecb19ecf65caca66c26b9e77a448de064bccb6a

# the manifest is read ONCE into an in-memory snapshot, and the snapshot's digest is checked
# against the manifest digest the operator's word carries: the word binds the 15 (repository,
# branch, tip) triples, so a manifest that changed after the pre-flight is a refusal, not a
# deletion. The word carries a second digest - the digest of this publication - which is checked
# below, so the revision that runs is the revision that was ratified. Every loop below runs on
# that snapshot - no file is read again, so nothing on disk can change what runs.
# The triples come from the §1 table of this publication and not from a file of the pre-flight:
# the pre-flight keeps its manifest in a private directory that it removes when it ends, and a
# manifest carried from one block to the other in a fixed path under /tmp would be readable and
# writable by every other process on the host.
PUBLICATION=rewrite/triage/debris-ratification-2026-11-14.md
[ -f "$PUBLICATION" ] || { echo "the publication $PUBLICATION is missing - run the procedure from the repository root"; exit 1; }
ROWS=()
while IFS= read -r row; do ROWS+=("$row"); done < <(
  sed -n '/^## 1\. Approval set/,/^\*\*Counts:\*\*/p' "$PUBLICATION" \
    | awk -F'|' '/^\| [0-9]+ \|/ && $5 ~ /`[0-9a-f]{40}`/ {gsub(/[ `]/,"",$3); gsub(/[ `]/,"",$4); gsub(/[ `]/,"",$5); gsub(/`/,"",$7); gsub(/^ +| +$/,"",$7); print $3" "$4" "$5" "$7}' \
    | sort -k2 | cut -d' ' -f1-3
)
readonly -a ROWS
[ "${#ROWS[@]}" -eq 15 ] || { echo "the manifest of $PUBLICATION is not the 15 reviewed rows - run the pre-flight first"; exit 1; }
digest=$(printf '%s\n' "${ROWS[@]}" | sort -k2 | sha256_of)
[ "$digest" = "$MANIFEST" ] \
  || { echo "the manifest digest is $digest, not the reviewed $MANIFEST - re-run the pre-flight"; exit 1; }

# THE WORD AND THE RATIFIED REVISION. The word of section 4 carries two digests - the manifest and
# this publication - and the run takes the word in RATIFY_WORD, from the line the operator sent
# and not from this file: a word that lived in the publication could be edited with it. The run
# executes only the revision the word names. The manifest digest of the word is checked against
# the reviewed constant and the snapshot of the table, and the publication digest of the word is
# checked against the file this run reads its rows from - so a publication edited after the
# ratification is a refusal and not a deletion, wherever the edit is: the table, the deletion
# commands, this block and every guard in it are covered by that digest, and a run that would
# execute an edit cannot start.
: "${RATIFY_WORD:?the deletion run executes only the revision the word of section 4 names - set RATIFY_WORD to the line the operator sent in section 4}"
word_re='^RATIFY DEBRIS: 15 branches manifest sha256:([0-9a-f]{64}) publication sha256:([0-9a-f]{64})$'
[[ "$RATIFY_WORD" =~ $word_re ]] || { echo "the word is not the word of section 4 - the deletion run refuses to push"; exit 1; }
word_manifest=${BASH_REMATCH[1]}
word_publication=${BASH_REMATCH[2]}
[ "$word_manifest" = "$MANIFEST" ] \
  || { echo "the manifest digest of the word is not the reviewed one - the deletion run refuses to push"; exit 1; }
rated=$(sha256_of < "$PUBLICATION")
[ "$rated" = "$word_publication" ] \
  || { echo "the publication carries the digest $rated, not the revision the word names - a change after the ratification invalidates the word"; exit 1; }

# THE SANITIZED GIT CONTEXT. Git rewrites even an explicit repository URL when the configuration
# carries a url.<base>.insteadOf or url.<base>.pushInsteadOf entry, so a rewrite on this machine
# can send a push that names https://github.com/<repo>.git to another repository - to a fork or a
# mirror that holds the audited SHA, where the lease of the row still succeeds and the ref is
# deleted there. The context of every push below is:
#   GIT_CONFIG_GLOBAL - a private configuration that holds only the credential helpers whose
#                        backing store is known not to hold a plaintext credential, so that no
#                        credential and no URL rewrite can reach a push;
#   GIT_DIR           - a scratch repository whose own configuration is empty, so the local
#                        configuration of the clone this run happens in cannot rewrite a URL.
# The system configuration stays in place - it carries the credential helper on macOS - and the
# check below refuses the run when it carries a rewrite of its own.
# THE PRIVATE DIRECTORY OF THE RUN. Every temporary path of this run - the sanitized
# configuration, the scratch repository, the control repositories and the branch lists of the
# post-check - lives in a private directory of its own. A fixed path under /tmp is predictable:
# every other process on the host can write it, replace it with a link or read it between the
# write and the read, so no path of the run is a fixed path under /tmp.
RUN_DIR=$(mktemp -d)
SANITIZED_DIR="$RUN_DIR/sanitized"
POST_DIR="$RUN_DIR/post"
CONTROL_DIR="$RUN_DIR/control"
mkdir -p "$SANITIZED_DIR" "$POST_DIR"
SANITIZED_GLOBAL="$SANITIZED_DIR/gitconfig"

# The branch lists of the post-check. The post-check reads the branch list of every repository the
# run can touch into a file of the private directory of the run, and the cleanup removes those
# files with the rest of the temporary paths of the run, so no file of the post-check survives the
# run.
POST_REPOS=(jackin-project/jackin jackin-project/jackin-agent-smith jackin-project/jackin-sentinel tailrocks/terminal-components-baseline)
post_path() { printf '%s/post-%s.txt' "$POST_DIR" "$(echo "$1" | tr '/' '-')"; }

# The cleanup. Every temporary path of this run - the sanitized configuration, the scratch
# repository, the control repositories and the branch lists of the post-check - is removed when the
# run ends, whatever ended it: the normal end, a refusal and a signal. The cleanup removes every
# path it is given and keeps the status of the run, so a refusal still exits with the status it
# refused with.
cleanup() {
  result=$?
  trap - 0 HUP INT TERM
  for path in "$@"; do
    if [ -n "$path" ]; then rm -rf "$path"; fi
  done
  exit "$result"
}
trap 'cleanup "$RUN_DIR"' 0
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

# THE KNOWN-SAFE CREDENTIAL HELPERS. A credential helper is copied into the sanitized
# configuration only when its backing store is known not to hold a plaintext credential: the
# keychains and the credential managers - osxkeychain, manager, manager-core, wincred, libsecret,
# gnome-keychain, gpgme - and the memory of the cache daemon, cache. The name is matched without
# case, because a lookup of git-credential-Store finds git-credential-store on a case-insensitive
# filesystem. Every other helper is refused: store writes its input to a plaintext file, an
# unknown program's storage behavior is unknown and it can persist its input - a plain program
# name is not a proof, it resolves to whatever program answers it on this host - and a helper that
# carries arguments or a shell command can carry the credential in its own text.
known_safe_helper() {
  case "$(printf '%s' "$1" | tr 'A-Z' 'a-z')" in
    osxkeychain | manager | manager-core | wincred | libsecret | gnome-keychain | gpgme | cache) return 0 ;;
    *) return 1 ;;
  esac
}

# The sanitizing. The configuration of the operator is read one allowlisted key at a time, and a
# value is copied into the sanitized configuration only when it cannot write a credential to a
# file:
#   credential.helper - only a known-safe helper (THE KNOWN-SAFE CREDENTIAL HELPERS above) and
#                       the empty value that resets the helper list are copied; every other
#                       helper is refused and never copied, and one refused helper refuses the
#                       run: store writes its input to a plaintext file, an unknown program can
#                       persist its input, and a helper with arguments or a shell command can
#                       carry the credential in its text.
# Every other key - the url.* rewrites, the http.* headers, the include.* files - is never read
# and never copied, and the configuration of the operator is never written to a file: every value
# is read from it directly, so no temporary file of this run ever holds a value the allowlist
# rejects. The helper list keeps the order of the operator's own list, so the helper that answers
# first is the one that answers without the sanitizing. The values are read with the tracing off,
# so a run under "set -x" does not print them, and the refusal names no value: the text of a
# helper can carry the credential itself.
sanitize_global_config() {
  directory=$1
  destination="$directory/gitconfig"
  : > "$destination"
  (
    set +x
    refused=0
    while IFS= read -r -d '' helper; do
      case "$helper" in
        '') git config --file "$destination" --add credential.helper "$helper"; continue ;;
      esac
      if known_safe_helper "$helper"; then
        git config --file "$destination" --add credential.helper "$helper"
      else
        refused=1
      fi
    done < <(git config --global --includes --null --get-all credential.helper 2>/dev/null || true)
    if [ "$refused" -eq 1 ]; then
      echo "a credential helper of the global configuration is not in the known-safe list - store writes its input to a plaintext file and an unknown program can do the same, so the pushes could leave the GitHub credential on disk; the helper is refused, not copied, and the deletion run refuses to push" >&2
      exit 1
    fi
  )
}

# THE CREDENTIAL PROBE. The sanitizing must refuse every helper that can write a credential to a
# file and must copy only a known-safe helper, and it must never carry a credential into a
# temporary file or into the output of the run. The probe runs both properties against fixtures
# of its own: a global configuration that carries a credential in the two forms that carry one -
# an extra header and an inline credential helper - together with the file-backed helper store,
# its case variant Store and the unknown program persist-helper, where the sanitizing must refuse
# the run and copy none of them; and a configuration that carries only known-safe helpers and a
# reset, where the sanitizing must succeed and copy them, so the pushes keep their
# authentication. The probe is hermetic: the credential is fabricated, the configuration it runs
# against is its own - the probe reads the global configuration of a home directory of its own,
# so the configuration of the operator is never read - and the tracing is off, so that neither
# the value nor the probe itself prints it. The probe removes its own temporary directory when it
# ends - on the normal end, on a refusal and on a signal - so a signal in the middle of the probe
# leaves nothing behind.
credential_probe() {
  (
    set +x
    probe_dir=$(mktemp -d)
    trap 'rm -rf "$probe_dir"' 0
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    sentinel='PROBE-CREDENTIAL-9f3a2b7c'
    mkdir -p "$probe_dir/home/.config" "$probe_dir/out"
    cat > "$probe_dir/home/.gitconfig" <<EOF
[http]
	extraHeader = Authorization: Basic $sentinel
[credential]
	helper = "!f() { echo username=probe; echo password=$sentinel; }; f"
	helper = store
	helper = Store
	helper = persist-helper
	helper = osxkeychain
[url "https://mirror.invalid/"]
	insteadOf = https://github.com/
EOF
    cp "$probe_dir/home/.gitconfig" "$probe_dir/home/.gitconfig.copy"
    status=0
    output=$( ( unset GIT_CONFIG_GLOBAL; export HOME="$probe_dir/home" XDG_CONFIG_HOME="$probe_dir/home/.config"; sanitize_global_config "$probe_dir/out" ) 2>&1 ) || status=$?
    if [ "$status" -eq 0 ]; then
      echo "the credential probe found the sanitizing accepted a configuration that carries a file-backed and an inline helper - the guard cannot be trusted" >&2
      exit 1
    fi
    case "$output" in
      *"$sentinel"*)
        echo "the credential probe found the credential in the output of the sanitizing - the guard cannot be trusted" >&2
        exit 1
        ;;
    esac
    if grep -R -q -F "$sentinel" "$probe_dir/out"; then
      echo "the credential probe found the credential in the sanitized context - the guard cannot be trusted" >&2
      exit 1
    fi
    files=$(find "$probe_dir/out" -type f | wc -l | tr -d ' ')
    if [ "$files" -ne 1 ]; then
      echo "the credential probe found $files files in the sanitized context - the configuration of the operator was materialized" >&2
      exit 1
    fi
    if git config --file "$probe_dir/out/gitconfig" --get-all --null credential.helper 2>/dev/null | tr '\0' '\n' | grep -q -E -e '^store$' -e '^Store$' -e '^persist-helper$' -e '^!' -e 'insteadOf' -e 'pushInsteadOf'; then
      echo "the credential probe found a refused helper or a rewrite in the sanitized context - the guard cannot be trusted" >&2
      exit 1
    fi
    if ! cmp -s "$probe_dir/home/.gitconfig" "$probe_dir/home/.gitconfig.copy"; then
      echo "the credential probe found the configuration of the operator changed - the sanitizing wrote to it" >&2
      exit 1
    fi
    mkdir -p "$probe_dir/safe/home/.config" "$probe_dir/safe/out"
    cat > "$probe_dir/safe/home/.gitconfig" <<EOF
[credential]
	helper =
	helper = osxkeychain
EOF
    ( unset GIT_CONFIG_GLOBAL; export HOME="$probe_dir/safe/home" XDG_CONFIG_HOME="$probe_dir/safe/home/.config"; sanitize_global_config "$probe_dir/safe/out" ) || {
      echo "the credential probe found the sanitizing refused a configuration that carries only known-safe helpers - the pushes would lose their authentication" >&2
      exit 1
    }
    if ! git config --file "$probe_dir/safe/out/gitconfig" --get-all --null credential.helper 2>/dev/null | tr '\0' '\n' | grep -qx 'osxkeychain'; then
      echo "the credential probe found no credential helper in the sanitized context - the sanitizing dropped what the pushes need" >&2
      exit 1
    fi
    exit 0
  )
}

# THE CLEANUP PROBE. The temporary paths of the run must be gone when the run ends, whatever ended
# it: the normal end, a refusal and a signal. The probe runs the cleanup of the run in a process
# that ends in each of the three ways - a subshell for the normal end and for the refusal, a child
# process for the signal - and checks that no temporary path is left behind and that the status of
# the run is the status the run ended with. The probe works on fixtures of its own - a sanitized
# directory, a control directory and a branch list, all of them inside the directory of the probe
# - and never names a path of the run: a probe that deletes the paths of the run would delete
# them before the run wrote them, and a path of the run that another run owns would be deleted
# with them. The signal leg runs in a child process of its own: a subshell of bash keeps the PID
# of the shell that started it in "$$", and the PID of the subshell itself is only in "BASHPID",
# which a stock macOS bash 3.2 does not have, so a subshell cannot signal itself on every host. A
# child process has its own PID in "$$" on every bash, so the child signals itself and the cleanup
# of the run removes the paths behind it. The paths of the probe travel in an array and not in the
# positional parameters: a bash 3.2 that runs the block from a command line gives a trap that is
# set inside a function the positional parameters of that function, and an array is not affected by
# that.
cleanup_probe() {
  (
    set +x
    probe_dir=$(mktemp -d)
    trap 'rm -rf "$probe_dir"' 0
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    for ending in normal failure signal; do
      sanitized="$probe_dir/sanitized-$ending"
      control="$probe_dir/control-$ending"
      post="$probe_dir/post-$ending.txt"
      mkdir -p "$sanitized" "$control"
      : > "$post"
      status=0
      case "$ending" in
        normal)
          ( cleanup_paths=("$sanitized" "$control" "$post"); trap 'cleanup "${cleanup_paths[@]}"' 0; exit 0 ) || status=$?
          ;;
        failure)
          ( cleanup_paths=("$sanitized" "$control" "$post"); trap 'cleanup "${cleanup_paths[@]}"' 0; exit 1 ) || status=$?
          ;;
        signal)
          script="$probe_dir/signal-$ending.sh"
          cat > "$script" <<'SIGNAL'
set -euo pipefail
cleanup_paths=("$@")
trap 'cleanup "${cleanup_paths[@]}"' 0
trap 'exit 143' TERM
kill -TERM $$
SIGNAL
          ( export -f cleanup; "${BASH:-bash}" "$script" "$sanitized" "$control" "$post" ) || status=$?
          ;;
      esac
      if [ -d "$sanitized" ] || [ -d "$control" ] || [ -e "$post" ]; then
        echo "the cleanup probe found a temporary path after the run ended with $ending - the cleanup is not trusted" >&2
        exit 1
      fi
      case "$ending:$status" in
        normal:0 | failure:1 | signal:143) ;;
        *)
          echo "the cleanup probe saw the status $status after the run ended with $ending - the cleanup is not trusted" >&2
          exit 1
          ;;
      esac
    done
    exit 0
  )
}

unset GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT GIT_CONFIG_SYSTEM GIT_SSL_NO_VERIFY GIT_COMMON_DIR GIT_OBJECT_DIRECTORY GIT_WORK_TREE GIT_INDEX_FILE GIT_NAMESPACE
# Tracing: inherited `set -x` (via SHELLOPTS/BASH_XTRACEFD, which survive
# BASH_ENV=/dev/null) would print every credential the guards below read, so
# tracing is off and the trace file descriptors are closed before anything runs.
set +x
unset SHELLOPTS BASH_XTRACEFD
# Certificate verification must stay on for authenticated pushes.
# GIT_CONFIG_NOSYSTEM is deliberately NOT set: the operator's credential helper
# may live in the system configuration, and every helper value the run can use
# is allowlisted below instead.
export GIT_TRACE_REDACT=1
# Hooks: inherited template and hook paths are distrusted - a lesser-privileged
# local writer could plant code that runs with the operator's rights on push.
# Every repository this run creates or uses gets an empty controlled template
# and hook path instead.
mkdir -p "$SANITIZED_DIR/empty-template" "$SANITIZED_DIR/empty-hooks" 2>/dev/null || true
export GIT_TEMPLATE_DIR="$SANITIZED_DIR/empty-template"
sanitize_global_config "$SANITIZED_DIR" || { echo "the sanitizing refused a credential helper of the global configuration - the deletion run refuses to push" >&2; exit 1; }
export GIT_CONFIG_GLOBAL="$SANITIZED_GLOBAL"
credential_probe || { echo "the credential probe refused the sanitizing - the deletion run refuses to push" >&2; exit 1; }
cleanup_probe || { echo "the cleanup probe refused the cleanup - the deletion run refuses to push" >&2; exit 1; }

git init --quiet --bare "$SANITIZED_DIR/push.git"
export GIT_DIR="$SANITIZED_DIR/push.git"
# Hooks: a template or system hooks path controlled by a lesser-privileged local
# actor would run with the operator's rights on push. Point the template at an
# empty controlled directory and pin the scratch repository's hooks path at an
# empty controlled directory, so no inherited hook executes.
mkdir -p "$SANITIZED_DIR/empty-template" "$SANITIZED_DIR/empty-hooks"
export GIT_TEMPLATE_DIR="$SANITIZED_DIR/empty-template"
git --git-dir="$SANITIZED_DIR/push.git" config core.hooksPath "$SANITIZED_DIR/empty-hooks"

# The refusal check. The context of the pushes must carry no URL rewrite. The check reads the
# configuration of that context, so a rewrite that survived the sanitizing - one in the system
# configuration, or one in a file the global configuration includes - stops the run here, before
# any push, instead of redirecting one. The output of the read is discarded: a url.<base> value
# can embed credentials, so the run prints only the refusal, never the rewrite itself. The
# control below runs the same check against a hostile configuration.
refuse_url_rewrites() {
  if git config --get-regexp '^url\..*\.(insteadOf|pushInsteadOf)$' >/dev/null; then
    return 1
  fi
  return 0
}
refuse_url_rewrites || { echo "the push context still carries a URL rewrite - the deletion run refuses to push" >&2; exit 1; }

# THE HELPER GUARD. The system configuration stays in place - it carries the credential helper on
# macOS - so a helper there is invoked by the pushes as well, and a helper that writes its input to
# a file would leave the GitHub credential on disk after they authenticate. The guard reads the
# helper list of the context the pushes run in - system configuration, sanitized global
# configuration and the empty scratch repository - and refuses the run when it carries a helper
# that is not known safe. The name is matched without case, as in the sanitizing.
refuse_unsafe_helpers() {
  while IFS= read -r -d '' helper; do
    case "$helper" in
      '') continue ;;
    esac
    known_safe_helper "$helper" || return 1
  done < <(git config --get-all --null credential.helper 2>/dev/null || true)
  # URL-scoped helpers (credential.<url>.helper) apply to matching HTTPS pushes
  # too, but the literal query above does not see them. Re-read the effective
  # list and refuse any scoped helper that is not known safe. Values are never
  # printed: a helper string can carry a credential. The parse is NUL-delimited:
  # a helper value that embeds a newline must not be split into a
  # separately-validated first line.
  while IFS= read -r -d '' pair; do
    key=${pair%%$'\n'*}
    value=${pair#*$'\n'}
    case "$key" in
      credential.helper|credential.*.helper) ;;
      *) continue ;;
    esac
    case "$value" in
      '') continue ;;
    esac
    known_safe_helper "$value" || return 1
  done < <(git config --null --list 2>/dev/null || true)
  return 0
}
refuse_unsafe_helpers || { echo "the push context carries a credential helper that can write a credential to a file - the deletion run refuses to push" >&2; exit 1; }

# The control repositories. Two local bare repositories stand in for the audited repository and
# for the mirror that holds the same commit; the push of the control names the first one. They
# are built in a context that carries no rewrite at all - no system configuration, no global
# configuration and their own fresh configuration - so their setup pushes stay local.
CONTROL_DIR="$RUN_DIR/control"
mkdir -p "$CONTROL_DIR"
CONTROL_SHA=$(
  unset GIT_DIR
  export GIT_CONFIG_NOSYSTEM=1
  export GIT_CONFIG_GLOBAL=/dev/null
  git init --quiet --bare "$CONTROL_DIR/victim.git"
  git init --quiet --bare "$CONTROL_DIR/mirror.git"
  git init --quiet "$CONTROL_DIR/seed"
  git -C "$CONTROL_DIR/seed" -c user.name=control -c user.email=control@example.invalid -c commit.gpgsign=false commit --quiet --allow-empty -m control
  git -C "$CONTROL_DIR/seed" push --quiet "$CONTROL_DIR/victim.git" HEAD:refs/heads/control
  git -C "$CONTROL_DIR/seed" push --quiet "$CONTROL_DIR/mirror.git" HEAD:refs/heads/control
  git -C "$CONTROL_DIR/seed" rev-parse HEAD
)
printf '[url "%s"]\n\tinsteadOf = %s\n\tpushInsteadOf = %s\n' "$CONTROL_DIR/mirror.git" "$CONTROL_DIR/victim.git" "$CONTROL_DIR/victim.git" > "$CONTROL_DIR/hostile.gitconfig"

# THE CONTROL: the guard is checked against the failure it has to defeat. In a context that
# carries the hostile rewrite the check must refuse and a push that names the victim repository
# must be redirected to the mirror; in this context the check must accept and the same push must
# name the repository the command names. Both pushes are dry runs against local repositories.
if (GIT_CONFIG_GLOBAL="$CONTROL_DIR/hostile.gitconfig"; refuse_url_rewrites) >/dev/null; then
  echo "the URL-rewrite control did not refuse the hostile configuration - the guard cannot be trusted" >&2
  exit 1
fi
refuse_url_rewrites || { echo "the sanitized context was refused - the guard cannot be trusted" >&2; exit 1; }

# THE HELPER CONTROL: the guard is checked against the failure it has to defeat. A context whose
# system configuration carries a file-backed helper must be refused - the pushes would invoke it
# and it would write the credential to a file - and the sanitized context of the run must be
# accepted. The hostile system configuration is a fixture of the control.
printf '[credential]\n\thelper = store\n' > "$CONTROL_DIR/hostile-system.gitconfig"
if (GIT_CONFIG_SYSTEM="$CONTROL_DIR/hostile-system.gitconfig"; refuse_unsafe_helpers) >/dev/null; then
  echo "the helper control did not refuse the file-backed helper - the guard cannot be trusted" >&2
  exit 1
fi
refuse_unsafe_helpers || { echo "the sanitized context was refused by the helper guard - the guard cannot be trusted" >&2; exit 1; }
# The two dry runs are parsed in the C locale and in the porcelain form `git push -h` documents as
# machine-readable output: the "To" header is a gettext-translated message, so an exact English
# grep of the human push output aborts the control on a host whose LC_MESSAGES is not English even
# though URL sanitization works - LC_ALL=C pins the C-locale tokens of both forms, and the
# tab-separated porcelain status line carries the leased deletion in a language-independent form.
control_out=$(GIT_CONFIG_GLOBAL="$CONTROL_DIR/hostile.gitconfig" LC_ALL=C git push --dry-run --porcelain --force-with-lease="refs/heads/control:$CONTROL_SHA" "$CONTROL_DIR/victim.git" --delete control 2>&1) \
  || { echo "the URL-rewrite control dry run failed - the guard cannot be trusted"; exit 1; }
printf '%s\n' "$control_out" | grep -qxF "To $CONTROL_DIR/mirror.git" \
  || { echo "the URL-rewrite control did not reproduce the redirect - the guard cannot be trusted"; exit 1; }
printf '%s\n' "$control_out" | awk -F'\t' '$1 == "-" && $2 == ":refs/heads/control" && $3 == "[deleted]" {ok++} END{exit !(ok == 1)}' \
  || { echo "the URL-rewrite control dry run does not report the leased deletion of refs/heads/control - the guard cannot be trusted"; exit 1; }
control_out=$(LC_ALL=C git push --dry-run --porcelain --force-with-lease="refs/heads/control:$CONTROL_SHA" "$CONTROL_DIR/victim.git" --delete control 2>&1) \
  || { echo "the sanitized-context dry run failed - the guard cannot be trusted"; exit 1; }
printf '%s\n' "$control_out" | grep -qxF "To $CONTROL_DIR/victim.git" \
  || { echo "the sanitized context did not push to the repository the command names"; exit 1; }
printf '%s\n' "$control_out" | awk -F'\t' '$1 == "-" && $2 == ":refs/heads/control" && $3 == "[deleted]" {ok++} END{exit !(ok == 1)}' \
  || { echo "the sanitized-context dry run does not report the leased deletion of refs/heads/control - the guard cannot be trusted"; exit 1; }

# Every target URL must come back unchanged from the context the pushes run in: the fetch side
# (ls-remote --get-url applies url.<base>.insteadOf) and the push side (remote get-url --push
# applies url.<base>.pushInsteadOf).
git remote add target "https://github.com/jackin-project/jackin.git"
for row in "${ROWS[@]}"; do
  read -r repo branch tip <<< "$row"
  url="https://github.com/$repo.git"
  git remote set-url target "$url"
  fetch_url=$(git ls-remote --get-url "$url")
  push_url=$(git remote get-url --push target)
  [ "$fetch_url" = "$url" ] || { echo "refusing: a target URL is rewritten in this context - the deletion run refuses to push" >&2; exit 1; }
  [ "$push_url" = "$url" ] || { echo "refusing: a push URL is rewritten in this context - the deletion run refuses to push" >&2; exit 1; }
done

# The helper guard ran before the scratch repository had a remote, so a conditional
# include keyed on the remote URL (includeIf "hasconfig:remote.*.url:...") could not
# fire then and fires now that `target` exists. Re-run the guard in the exact push
# context: a helper it refused above, or one the new remote activates, refuses here.
refuse_unsafe_helpers || { echo "the push context carries a credential helper that can write a credential to a file - the deletion run refuses to push" >&2; exit 1; }

# The same conditional include can activate a url.* rewrite only now that `target`
# exists, so the rewrite refusal is re-run in the exact push context too, and every
# row's push URL is resolved again: a URL that no longer names its repository is a
# refusal before any push.
refuse_url_rewrites || { echo "the push context carries a URL rewrite - the deletion run refuses to push" >&2; exit 1; }
for row in "${ROWS[@]}"; do
  read -r repo branch tip <<< "$row"
  url="https://github.com/$repo.git"
  git remote set-url target "$url"
  [ "$(git ls-remote --get-url "$url")" = "$url" ] || { echo "refusing: a target URL is rewritten in the push context - the deletion run refuses to push" >&2; exit 1; }
  [ "$(git remote get-url --push target)" = "$url" ] || { echo "refusing: a push URL is rewritten in the push context - the deletion run refuses to push" >&2; exit 1; }
done

# 6. DELETION RUN: one row at a time, archive first, push second. The push is built from the
#    row's own (repository, branch, tip) triple of the snapshot, so it is the §1 command of that row.
#    The archive-namespace lock is revalidated inside the loop, coupled to each push: an
#    administrator change after the pre-flight (lock removed or altered) refuses the run
#    before that row's deletion instead of deleting into an unprotected namespace.
for row in "${ROWS[@]}"; do
  read -r repo branch tip <<< "$row"
  case "$repo" in
    jackin-project/jackin|jackin-project/jackin-agent-smith|tailrocks/terminal-components-baseline)
      lock_id=$(gh_api "repos/$repo/rulesets" --jq '.[] | select(.name == "debris-archive-lock") | .id') \
        || { echo "the archive lock of $repo cannot be read before deleting $branch"; exit 1; }
      [ -n "$lock_id" ] || { echo "the archive lock of $repo is gone before deleting $branch"; exit 1; }
      existing=$(gh_api "repos/$repo/rulesets/$lock_id" --jq '[.enforcement, (.conditions.ref_name.include | sort | join(",")), (.conditions.ref_name.exclude | sort | join(",")), ([.rules[].type] | sort | join(",")), (.bypass_actors | length), .target] | @tsv') \
        || { echo "the archive lock of $repo cannot be validated before deleting $branch"; exit 1; }
      printf '%s\n' "$existing" | awk -F'\t' '$1 == "active" && $2 == "refs/heads/archive/**/*" && $3 == "" && $4 == "deletion,non_fast_forward" && $5 == "0" && $6 == "branch" {ok++} END{exit !(ok == 1)}' \
        || { echo "the archive lock of $repo is not the active, branch-targeted, unbypassed two-rule lock - refusing to delete $branch"; exit 1; }
      ;;
  esac
  got=$(gh_api "repos/$repo/git/ref/heads/archive/$branch" --jq '.object.sha') \
    || { echo "archive ref $repo archive/$branch is gone before deleting $branch"; exit 1; }
  [ "$got" = "$tip" ] || { echo "archive ref $repo archive/$branch holds $got, not $tip before deleting $branch"; exit 1; }
  git push --force-with-lease="refs/heads/$branch:$tip" "https://github.com/$repo.git" --delete "$branch"
done

# post-check: the 15 targets must be gone, every archive head of the 15 must still hold its
# recorded tip, and every protected branch must still be there
for row in "${ROWS[@]}"; do
  read -r repo branch tip <<< "$row"
  got=$(gh_api "repos/$repo/git/ref/heads/archive/$branch" --jq '.object.sha') \
    || { echo "archive ref $repo archive/$branch is gone after the deletions"; exit 1; }
  [ "$got" = "$tip" ] || { echo "archive ref $repo archive/$branch holds $got, not $tip after the deletions"; exit 1; }
done

# the branch list of every repository the run can touch, read once
for r in "${POST_REPOS[@]}"; do
  gh_api "repos/$r/branches" --paginate --jq '.[].name' > "$(post_path "$r")" \
    || { echo "the branch list of $r could not be read"; exit 1; }
done

# the 15 targets must be absent from their repositories. A grep error is not an absence: only
# status 1 (no match) is the wanted state, status 0 (the branch is still there) and every error
# status are a refusal.
for row in "${ROWS[@]}"; do
  read -r repo branch tip <<< "$row"
  list="$(post_path "$repo")"
  [ -f "$list" ] || { echo "the branch list of $repo was not read"; exit 1; }
  status=0
  grep -qxF "$branch" "$list" || status=$?
  case "$status" in
    0) echo "$repo still has the deleted branch $branch"; exit 1 ;;
    1) ;;
    *) echo "the branch list of $repo could not be read (grep exit $status)"; exit 1 ;;
  esac
done

# the protected branches - the rows of §2, §3 and §8 and each repository's main - must remain
while read -r repo branch; do
  list="$(post_path "$repo")"
  [ -f "$list" ] || { echo "the branch list of $repo was not read"; exit 1; }
  status=0
  grep -qxF "$branch" "$list" || status=$?
  case "$status" in
    0) ;;
    1) echo "the protected branch $repo $branch is gone after the deletions"; exit 1 ;;
    *) echo "the branch list of $repo could not be read (grep exit $status)"; exit 1 ;;
  esac
done <<'PROTECTED'
jackin-project/jackin main
jackin-project/jackin preserve/p0.6-prototype-20261109
jackin-project/jackin recovery/pr1230-blob-auth-77b399f1-20261027
jackin-project/jackin recovery/stash-adoption-6a0d2f03-20261025
jackin-project/jackin recovery/strip-tests-restore-20261030
jackin-project/jackin codex/openshell-integration-checkpoint-20261108-122311
jackin-project/jackin codex/openshell-verified-checkpoint-20261108
jackin-project/jackin preserve/session-boundaries-20261109
jackin-project/jackin preserve/w1-fixes-20261109
jackin-project/jackin fix/typed-model-selection
jackin-project/jackin docs/triage-inventory-reclass-2026-11-13
jackin-project/jackin-agent-smith main
jackin-project/jackin-sentinel main
jackin-project/jackin-sentinel fix/s158-postmerge-4
jackin-project/jackin-sentinel preserve/probe-identity-20261109
tailrocks/terminal-components-baseline main
tailrocks/terminal-components-baseline preserve/local-unique-20261109
PROTECTED

# the end state, for the record
gh_api repos/jackin-project/jackin/branches --paginate --jq '.[] | "\(.name) \(.commit.sha)"'
gh_api repos/jackin-project/jackin-agent-smith/branches --paginate --jq '.[] | "\(.name) \(.commit.sha)"'
gh_api repos/tailrocks/terminal-components-baseline/branches --paginate --jq '.[] | "\(.name) \(.commit.sha)"'
# --- deletion run: end ---
```

Expected after the deletions (live counts re-read 2026-11-14 20:12 UTC, §7), **counting the archive heads that step 3 creates**: jackin **70 → 130** branches (69 non-main → 129 non-main: the 10 targets are removed and 70 `archive/**` heads are added), smith **3 → 4** (2 removed, 3 added), baseline **5 → 7** (3 removed, 5 added). The jackin number grew from 24 to 70 between the two re-checks because other work in the same repository pushed branches in the meantime (§7, "Branches outside every class list"); those branches are **not** in the approval set and the operator's word does not cover them. This document's own branch `docs/debris-ratification-2026-11-14` is counted in the 70 and disappears only if the PR is merged and its branch deleted.

**Those counts are a dated snapshot, not a promise.** The deletions remove exactly the 15 refs of §1 and nothing else; the population around them moves, minute to minute. Re-read it before the run — at 2026-11-15 01:37 UTC the re-read gave jackin **69** (`gh api --hostname github.com repos/jackin-project/jackin/branches --paginate --jq '.[] | .name' | wc -l`), smith **3** (`gh api --hostname github.com repos/jackin-project/jackin-agent-smith/branches --paginate --jq '.[] | .name' | wc -l`) and baseline **37** (`gh api --hostname github.com repos/tailrocks/terminal-components-baseline/branches --paginate --jq '.[] | .name' | wc -l`) — the baseline repository alone had grown from 5 branches to 37 in the hours since §7 was written, so its post-deletion count is **39**, not 2: 34 of the 37 survive the three deletions and step 3 adds the 5 baseline archive heads. On that same re-read the arithmetic gives jackin **129** branches (69 − 10 + 70) and smith **4** (3 − 2 + 3). Only the 15 named refs are removed; every other branch stays, and every archive head is an addition, not a removal.

**The archive refs of the pre-flight survive the deletions.** They are what the deletion leaves behind: the 15 tips, and the captured tip of every retained ref, under `refs/heads/archive/`. Nothing in §1 deletes them, so every object a deletion would orphan — including the 649- and 666-commit histories of rows 13-15 and the three branch-only test revisions of row 13 — stays reachable.

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
- **in no retained ref** — the **complete** count of tip-tree paths whose *content* no surviving ref
  holds: the absent paths whose blob is in no retained ref **plus** the paths `main` holds at another
  revision whose branch revision is in no retained ref (the branch-only revisions of the third table
  below). A **retained ref** is every ref that survives the operation: `main` plus every branch of the
  repository that is not one of the 15 rows of §1 (the exact set is captured in
  `rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt`). The fourth table below accounts for
  the rest of what a deletion removes: the objects that only the branch's **history** holds, including
  content that its earlier commits carried and that is no longer in the tip tree.

  **This column counts every tip-tree path, not only the absent ones.** The count a deletion removes
  from the content is the union of the two sets above; a path `main` holds at another revision loses
  the branch's revision of it just as surely as an absent path loses its file.

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

**Row 13's three branch-only revisions, adjudicated one by one.** Row 13 (`test/pre-refactor-protection`)
is approved as DEBRIS, and its tip tree carries **three** revisions that no retained ref holds: `main`
holds those three paths at another revision, and the branch's revision of them exists nowhere else in
the retained set (`git rev-list --objects 8063050c 6923fa36` holds none of the three blobs). They are
the reason the fourth table's row 13 reads **4** and not 1. Each one is adjudicated here:

| path | branch blob | `main`'s blob | lines | what the branch's revision changes | disposition |
|---|---|---|---|---|---|
| `tests/functional/cases/j04_usage.rs` | `1a87681a7990905cdcfad06a55fcf7d0e5f76762` | `c8fee8f982699c90dec8c91ecce3c635f654ae78` | +54/-29 | replaces the bounded 40-step "up" loop that waited for the Work row with a 15 s deadline loop that navigates to the Work row itself (new helpers `is_work_row` and `work_place`) | **PRESERVED** — held by the archive prerequisite of §1 (`refs/heads/archive/test/pre-refactor-protection`); port it into the baseline repository's `main` if the operator wants the fix as code |
| `tests/functional/cases/s05_filebrowser.rs` | `0e2abeaf568edc513c8f035360a96acbfb189b62` | `c6c73e0973fdba0051c148b0434cb7e0a1f245ca` | +23/-8 | adds `url_accepted`, which accepts the URL only after the previous resolve error and the dialog title have left the screen, and replaces three `wait_text` calls with `wait_screen` | **PRESERVED** — same |
| `tests/functional/cases/t01_test.rs` | `2184f776f76fd21e271b4314c35b3fba937aa98b` | `5d4bb68311d7e8da11801bd113098670d23ac780` | +18/-2 | adds `list_without_result`, which cancels only when the form title and the test result have left the row, and replaces two `wait_absent(FORM)` calls with `wait_screen` | **PRESERVED** — same |

All three are test-harness **synchronization** fixes: they wait for a frame in which the stale content of
the previous frame is gone, instead of waiting for a text that may still be painted next to it. They are
not product code, and they are not scratch either — they are the only revision of those three test cases
that exists anywhere. The adjudication is therefore conservative: **row 13 may not be deleted before those
three revisions are preserved.** The archive prerequisite of §1 preserves them under
`refs/heads/archive/test/pre-refactor-protection`, and that archive must exist and be verified (steps 3
and 4 of §1) before the deletion command of row 13 runs. The blobs and the line counts are reproduced by

```bash
git diff --numstat c8fee8f982699c90dec8c91ecce3c635f654ae78 1a87681a7990905cdcfad06a55fcf7d0e5f76762   # +54 -29
git diff --numstat c6c73e0973fdba0051c148b0434cb7e0a1f245ca 0e2abeaf568edc513c8f035360a96acbfb189b62   # +23 -8
git diff --numstat 5d4bb68311d7e8da11801bd113098670d23ac780 2184f776f76fd21e271b4314c35b3fba937aa98b   # +18 -2
```

(54 + 23 + 18 = the 95 added lines the row carries.)

**What else the deletion removes: the objects only the branch's history holds.** A ref deletion makes
the whole history of the branch unreachable, not just its tip tree. The table below counts the objects
that no retained ref holds (`git rev-list --objects "$T" --not <every retained ref>`) — the commits that
exist only on the branch and the trees and blobs they carry, including content that earlier commits held
and the tip tree no longer has.

The first column counts **every** tip-tree path whose content no retained ref holds: the absent paths
of the first table of §1a **and** the branch-only revisions of the third table above. It is the
complete answer to "how much content does this deletion remove", and it is the number the archive
prerequisite of §1 has to cover.

| # | repo | tip-tree paths whose content is in no retained ref (complete) | objects only this branch holds (of which commits) |
|---|---|---|---|
| 1 | jackin-project/jackin | 1 | 5 (1) |
| 2 | jackin-project/jackin | 58 | 75 (3) |
| 3 | jackin-project/jackin | 22 | 335 (25) |
| 4 | jackin-project/jackin | 3 | 30 (1) |
| 5 | jackin-project/jackin | 14 | 91 (1) |
| 6 | jackin-project/jackin | 1 | 30 (1) |
| 7 | jackin-project/jackin | 6 | 163 (9) |
| 8 | jackin-project/jackin | 19 | 98 (1) |
| 9 | jackin-project/jackin | 2 | 119 (7) |
| 10 | jackin-project/jackin | 23 | 1065 (223) |
| 11 | jackin-project/jackin-agent-smith | 7 | 13 (1) |
| 12 | jackin-project/jackin-agent-smith | 0 | 1 (1) |
| 13 | tailrocks/terminal-components-baseline | 4 | 10643 (666) |
| 14 | tailrocks/terminal-components-baseline | 9362 | 14047 (649) |
| 15 | tailrocks/terminal-components-baseline | 2149 | 7408 (649) |

**Correction to the first compile of this column.** It carried the *subset* "content not in `main`'s
history" (1 / 58 / 22 / 0 / 13 / 0 / 4 / 13 / 0 / 18 of 20 / 7 / 0 / 1 / 9117 / 1904) and so omitted
the branch-only revisions of the third table — 3 / 245 / 245 for rows 13-15 and 3 / 1 / 1 / 2 / 6 / 2 /
5 for rows 4-10. The column now counts the complete set (1 / 58 / 22 / 3 / 14 / 1 / 6 / 19 / 2 / 23 /
7 / 0 / 4 / 9362 / 2149). Row 10's "18 of 20" was the absent-path subset; 2 of its 20 absent paths
are held by the retained ref `wt/a15-declared-auth-producer` (`dd2008cbf`), and 5 of its 6
different-revision paths carry a branch-only revision, so its complete count is 23.

The retained set used for the measurement (2026-11-14 22:09 UTC) was: for `jackin-project/jackin`,
`main` plus the **59** branches that are not one of the 10 jackin rows of §1 (60 captured refs in all,
`rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt`); for
`jackin-project/jackin-agent-smith`, `main` (both other branches are rows of §1); for
`tailrocks/terminal-components-baseline`, `main` and `preserve/local-unique-20261109` (the three other
non-`main` branches of that repository are rows 13, 14 and 15).

**What this means for the operator.** Rows 13-15 carry hundreds of commits whose objects only the branch
holds (666, 649 and 649 commits). Their *content* is accounted for above — generated capture output, legacy
baselines and scratch fixtures — but their **history** is not recoverable after the deletion **unless it is
preserved first**. The pre-flight of §1 does exactly that, and it is a prerequisite, not a suggestion:
before the first deletion push it creates `refs/heads/archive/<branch>` for each of the 15 rows and
`refs/heads/archive/retained/<ref>` for every captured retained ref, and step 4 verifies that each archive
ref holds the recorded tip. A ref under `refs/heads/archive/` keeps every object reachable — the tip tree
and the whole history behind it — and no deletion in §1 touches it.

```bash
# the pre-flight creates and verifies the archive refs (step 3 and step 4 of §1)
gh api --hostname github.com repos/<repo>/git/ref/heads/archive/<branch> --jq '.object.sha'   # must print the recorded tip
```

The audit's own guidance for the rows of §2 is the same. The operator's word authorizes the deletions; the
archive prerequisite is what makes them safe, and it must have run before the first push.

**What the accounting says.** For rows 1, 2, 4, 6, 7, 9, 12 and 13 the content a deletion removes is
documentation, configuration or scratch — no product code. For rows 3, 5, 8, 10 and 11 the content is
**not** in `main`: it is a pre-rewrite revision of modules the OpenShell rewrite (#1264, `f683619b5`)
deleted, the branch's own draft of a pack that landed (PR #1234), velnor generator output, or the demo
binary's old layout. The merged inventory records no port candidate for any of it. **If the operator wants
any of it, it must be extracted before the deletion** — but with the archive prerequisite of §1 in place
the deletion no longer destroys the only copy of those bytes: the archive ref holds them, and the content
stays retrievable from `refs/heads/archive/`. Row 13's three branch-only test revisions (§1a) are the one
case where the content is *not* scratch: they are preserved by the same archive ref, and an operator who
wants them as code should port them into `tailrocks/terminal-components-baseline` before the deletion.

---

## 1b. The measurement this document rests on

**Repositories and `main` tips.** Every number in §1a was measured against these tips:

| repo | `main` tip used |
|---|---|
| `jackin-project/jackin` | `926bbd49d171c2f389a766776ff3c418f8987fbd` |
| `jackin-project/jackin-agent-smith` | `a9b8c8aea74dc2c777eb8df23ea22647c8ceaf40` |
| `tailrocks/terminal-components-baseline` | `8063050cdb483e31284c107808b207c92e2d3203` |

**The commands** (parameterized by row). `M` is **pinned to the recorded SHA**, not to `origin/main`:
once `main` advances, `M` keeps the measurement reproducible, and re-measuring against a later `main` is
a deliberate act that changes the SHA in the table above.

**The repository, the retained set, the branch tip and the main tip travel together.** The clone, the
two commits and the retained-ref filter all belong to the same row, so none of them may be changed
alone — changing only the `awk` filter would fetch one repository's objects from another. Set the four
values together:

| row | `repo` | `REPO_NAME` | `T` — the row's tip | `M` — that repository's main tip |
|---|---|---|---|---|
| 1-9 | `https://github.com/jackin-project/jackin` | `jackin-project/jackin` | the `tip` column of the row in §1 | `926bbd49d171c2f389a766776ff3c418f8987fbd` |
| 10 | `https://github.com/jackin-project/jackin` | `jackin-project/jackin` | `092fe1db796367479029d4e94379530f7f5965c0` | `926bbd49d171c2f389a766776ff3c418f8987fbd` |
| 11 | `https://github.com/jackin-project/jackin-agent-smith` | `jackin-project/jackin-agent-smith` | `b9db5b149cc46baba9c49549432307c29e3972b0` | `a9b8c8aea74dc2c777eb8df23ea22647c8ceaf40` |
| 12 | as row 11 | as row 11 | `c58cfdcaaca85f606c2abc4ddb8f5afaae47f737` | as row 11 |
| 13 | `https://github.com/tailrocks/terminal-components-baseline` | `tailrocks/terminal-components-baseline` | `fc624b7456132fb74675ab452e6b64ecb3f69a70` | `8063050cdb483e31284c107808b207c92e2d3203` |
| 14 | as row 13 | as row 13 | `0b8b8bb077061eee6ca04466148f017e316edfd3` | as row 13 |
| 15 | as row 13 | as row 13 | `8d352ebfb932b06bf18c537e924389b0aae07e42` | as row 13 |

```bash
set -euo pipefail                        # fail fast: a missing object must not print a zero

# the retained-ref list lives in this repository - take its path before the clone moves us
RETAINED_FILE="$PWD/rewrite/triage/audits/2026-11-14/retained-refs-2026-11-14.txt"

# --- the row to measure: repository, retained set, tip and main tip travel together ---
repo=https://github.com/jackin-project/jackin              # the repository of the row
REPO_NAME=jackin-project/jackin                            # its name in the captured list
T=092fe1db796367479029d4e94379530f7f5965c0                 # the row's tip (row 10)
M=926bbd49d171c2f389a766776ff3c418f8987fbd                 # that repository's main tip, pinned

work=$(mktemp -d)                        # a fresh clone every time: a stale clone directory would
scratch=${scratch:-}                    # leave every later command running in the wrong checkout
cleanup_measurement() {
  result=$?
  trap - 0 HUP INT TERM
  rm -rf ${scratch:+"$scratch"} "$work"
  exit "$result"
}
trap cleanup_measurement 0
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
git clone --quiet "$repo" "$work"
cd "$work" && git fetch origin --quiet

# the scratch files of the measurement live in a private directory of their own. A fixed path
# under /tmp is predictable: every other process on the host can write it, replace it with a
# link or read it between the write and the read, and a file another process wrote under such a
# path would change the numbers this document rests on. The directory is private (mktemp -d) and
# the cleanup removes it when the measurement ends.
scratch=$(mktemp -d)

# the measurement is pinned to those commits, not to the moving branch names:
git fetch origin "$M" "$T" --quiet                         # fetch those exact commits
test "$(git rev-parse "$M^{commit}")" = "$M" || { echo "main tip $M is not in this clone"; exit 1; }

# the retained refs of THAT repository, pinned to their captured tips: fetch and validate every one
# of them, so the subtraction is over the recorded set and not over whatever the refs are today
RETAINED=$(awk -v r="$REPO_NAME" '$1==r {print $3}' "$RETAINED_FILE" | tr '\n' ' ')
for sha in $RETAINED "$T" "$M"; do
  git fetch origin "$sha" --quiet                         # GitHub serves a commit without a ref
  git cat-file -e "$sha^{commit}" || { echo "missing $sha"; exit 1; }
done

# every path of the branch tip that does not exist in main's tree
git ls-tree -r "$T" | awk '{print $4" "$3}' | sort > "$scratch/tip.txt"
git ls-tree -r "$M" | awk '{print $4" "$3}' | sort > "$scratch/main-tree.txt"
join -1 1 -2 1 -v1 "$scratch/tip.txt" "$scratch/main-tree.txt" > "$scratch/absent.txt"   # -v1: the paths of the tip that main does NOT have
wc -l < "$scratch/absent.txt"                                 # absent paths of the row

# the blob set of main's whole object history
git rev-list --objects "$M" | awk '{print $1}' | sort -u > "$scratch/main-blobs.txt"

# of the absent paths, the ones whose content never occurs in main's history
while read -r p b; do grep -qx "$b" "$scratch/main-blobs.txt" || echo "$p"; done < "$scratch/absent.txt" | wc -l

# the third bucket: paths that main still holds, with a different revision
join -1 1 -2 1 "$scratch/tip.txt" "$scratch/main-tree.txt" | awk '$2 != $3' > "$scratch/differing.txt"
wc -l < "$scratch/differing.txt"                             # blob-different paths of the row
while read -r p tb mb; do grep -qx "$tb" "$scratch/main-blobs.txt" || echo "$p"; done < "$scratch/differing.txt" | wc -l

# the COMPLETE fourth bucket: every tip-tree path whose content is in no retained ref.
# A retained ref is every ref that survives the deletion: main plus every captured retained ref.
# The count is the absent paths whose blob no retained ref holds, PLUS the paths main holds at
# another revision whose branch revision no retained ref holds.
git rev-list --objects $RETAINED | awk '{print $1}' | sort -u > "$scratch/retained-blobs.txt"
{ while read -r p b;   do grep -qx "$b"   "$scratch/retained-blobs.txt" || echo "$p"; done < "$scratch/absent.txt"
  while read -r p tb mb; do grep -qx "$tb" "$scratch/retained-blobs.txt" || echo "$p"; done < "$scratch/differing.txt"
} | sort -u | wc -l
# -> the first column of §1a's fourth table (row 10 -> 23, row 13 -> 4, row 14 -> 9362, row 15 -> 2149).
# With the archive refs of §1 in the retained set - the row's own tip is archived - it prints 0:
# nothing the deletion removes is left without a surviving holder.

# the fourth bucket: the objects only this branch's history holds
git rev-list --objects "$T" --not $RETAINED | wc -l        # objects only this branch holds
```

**The numbers this produces**, per instantiation (measured with this procedure):

| row | absent paths | of which not in `main`'s history | blob-different paths | of which the branch's revision is branch-only | tip-tree paths in no retained ref (complete) | objects only this branch holds |
|---|---|---|---|---|---|---|
| 10 | 21 | 20 | 6 | 5 | 23 | **1065** |
| 13 | 1 | 1 | 140 | 3 | 4 | **10643** |
| 14 | 9517 | 9117 | 334 | 245 | 9362 | 14047 |
| 15 | 2046 | 1904 | 334 | 245 | 2149 | 7408 |

The procedure prints exactly these numbers, in this order.

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
| 4 | jackin-project/jackin | `recovery/strip-tests-restore-20261030` | `8508f6f8` | **DEBRIS (superseded) — port verdict DISCARD as a port source; 1 PORT-LATER carve-out (P1)** | **Its own word, and only after the P1 slice is materialized.** The branch measures **3,844 path-absent / 3,868 content-unique** paths — the earlier audit's 2,464/355 was corrected on 2026-11-13 (the branch forked before the rewrite, merge-base `7c92234bd`), not 355. The **P1 carve-out** is the OMP credential-store capture hardening, and the slice is the **complete P1 file set**: `crates/adapters/jackin-omp-store/src/capture.rs` (the snapshot copy, the secret caps at `capture.rs:31-35`, the zeroize at `capture.rs:9`) **and** `crates/adapters/jackin-omp-store/src/capture/retry.rs` (the WAL binding at `capture/retry.rs:52+`), with the capture test modules that cover them (`crates/adapters/jackin-omp-store/src/capture/tests.rs` and `crates/adapters/jackin-omp-store/src/capture/tests/**`). None of it exists in `main` or on any other ref, so **the branch must stay until every file of that set is materialized** — porting `capture.rs` alone would discard the WAL binding and its tests. The P2 carve-out (the agent-status / hook-installer slice) is **DISCARD** (`rewrite/DESIGN.md:272-281`, D16 deletes agent-status inference). | **NO** |

All four are DEBRIS-class by the inventory's final classes, all four are live on the remote (§7), and **none of them is covered by the operator's word in §4**. Row 4 is not deletable until its P1 slice is ported.

---

## 3. CONTAINED candidates — containment UNPROVEN (4) — kept, NOT deletable

**These 4 are CONTAINED *candidates* whose containment is NOT proven — they are not DEBRIS, and they are not safe to delete either.**

**This publication supersedes the merged inventory's class label for these four rows.** The inventory `rewrite/triage/branch-inventory-2026-11-12.md` labels them **"CONTAINED - safe to delete"** — in its rows for `codex/openshell-integration-checkpoint-20261108-122311`, `codex/openshell-verified-checkpoint-20261108`, `preserve/session-boundaries-20261109` and `preserve/w1-fixes-20261109`, and again in its class-change table. **That label is not safe to act on**, and the inventory's own class *rule* — "**CONTAINED candidate - UNPROVEN** — the branch is *believed* to be contained, but the proof is incomplete: the paths it changes are ABSENT from `main` and carry product code, so containment is NOT proven. Such branches … must be treated as VALUABLE (NOT deletable) until every absent path is proven to be scratch/archive" — and the per-row evidence the inventory itself carries (the 2026-11-13 unproven review, quoted below) both point the other way: the five usage fixtures are absent from `main`, and the product-code paths have no inventory-grade proof.

**The disposition of these four branches is therefore UNPROVEN, KEPT — do not delete them**, whatever the class column of the inventory says. An operator who reads the inventory alone gets the opposite disposition; this section is the correction, and it is the one that governs. They are **excluded from this ratification by class, not by doubt**: the operator's word in §4 does not cover them, they are not DEBRIS, and nothing in this document proposes deleting them. They are kept here as a record, with their recorded proofs **and the proof that is still missing**.

**Missing proof — all 4 rows.** Two things are **not** proven for any of these four branches, so none of them may be called safe to delete:

1. **The 5 usage fixtures are absent from `main` and are carried only by these branches.** Verified live on 2026-11-14 against `origin/main`: the same 5 paths are present on all 4 branches and in no `main` tree — `crates/usage/jackin-usage/tests/usage/fixtures/kimi/usages_hours.json`, `crates/usage/jackin-usage/tests/usage/fixtures/kimi/usages_no_window.json`, `crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth.json`, `crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth_go_oauth.json`, `crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth_no_go.json`. The inventory counts them among the absent paths of `preserve/w1-fixes-20261109` (its 6 absent paths = 1 console test + these 5).

   **Correction: five absent paths, four history-absent contents.** The claim that these fixtures' content is not in `main` holds for **four** of the five. `opencode/auth_no_go.json` (blob `568abbfdc0199baa3a4f917a5fc41a1f46ebadb8`) **is** in `main`'s object history: `main` carried that path from the OpenShell rewrite (`f683619b5`, PR #1264) until PR #1275 removed it from the tree (`7a53f6abe`), and `git rev-list --objects 926bbd49` reproduces the blob. The other four blobs (`d3f870f3`, `8c6ac31e`, `8c429341`, `5da3b6b2`) occur nowhere in `main`'s history:

   ```bash
   scratch=$(mktemp -d)                 # the scratch file of the check lives in a private directory:
                                       # a fixed path under /tmp is predictable, and a file another
                                       # process wrote under such a path would answer the check
   trap 'rm -rf "$scratch"' 0
   M=926bbd49d171c2f389a766776ff3c418f8987fbd
   git rev-list --objects $M | awk '{print $1}' | sort -u > "$scratch/main-blobs.txt"
   for p in crates/usage/jackin-usage/tests/usage/fixtures/kimi/usages_hours.json \
            crates/usage/jackin-usage/tests/usage/fixtures/kimi/usages_no_window.json \
            crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth.json \
            crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth_go_oauth.json \
            crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth_no_go.json; do
     b=$(git rev-parse --verify --quiet "6965f5d1c2227ca6187704a2374f09bba14632d4:$p")
     grep -qx "$b" "$scratch/main-blobs.txt" && echo "$b IN-MAIN-HISTORY $p" || echo "$b history-absent $p"
   done
   ```

   ```text
   d3f870f3847fe0630dae00fc933cdeee77b3bcbc history-absent crates/usage/jackin-usage/tests/usage/fixtures/kimi/usages_hours.json
   8c6ac31ed2a722dbc238131c124e3330d171ac42 history-absent crates/usage/jackin-usage/tests/usage/fixtures/kimi/usages_no_window.json
   8c4293417afb5e97b761530c7d6575e427af4460 history-absent crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth.json
   5da3b6b2bfaeda2c100c4a5418dc96c3ecf02147 history-absent crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth_go_oauth.json
   568abbfdc0199baa3a4f917a5fc41a1f46ebadb8 IN-MAIN-HISTORY crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth_no_go.json
   ```

   The correction does not weaken the disposition: the four history-absent fixture contents are still carried
   only by these four branches, and the fifth is absent from `main`'s **tree** as well, so no `main` tree
   holds any of the five. Containment is still not proven for any of the 4, and the disposition stays
   **UNPROVEN, KEPT**.
2. **The product-code paths have no inventory-grade containment proof.** The inventory records them as UNPROVEN ("24 of the 25 absent blobs never occur in main's object history"); the (a)/(b)/(c) split below is inherited from the completed audits and is not a recorded merged-PR / patch-id / ancestry proof.

**Until both are closed — the fixture port decision is made and recorded, and the product-code paths are proven — these 4 branches are kept and are treated as VALUABLE. Do not delete them.**

| # | repo | branch | tip | absent product-code paths | disposition split | recorded proof, and what is still missing |
|---|---|---|---|---|---|---|
| 1 | jackin-project/jackin | `codex/openshell-integration-checkpoint-20261108-122311` | `6965f5d1c` | 12 | (a) 5 · (b) 7 · (c) 0 | Every absent product-code path is either already in `main` under a different path/name (a) or superseded by the later `main` rewrite (b): `agent_metadata.rs` → `launch/decide.rs` + `tests/launch/instance_identity.rs`; `secret_broker.rs` → `accounts.rs` (Muse keychain) + `op.rs` (1Password `op read`); `jackin-testkit/src/docker.rs` → `jackin-testkit/src/engine.rs` + `gateway/*`; `write-file.c` + `writer_bootstrap*` → the OpenShell `write_file` RPC; `cancellation.rs` → the `GatewayError::Cancelled` coverage in `tests/gateway/{connect,exec,download_bound,status,error_text,transfer}.rs`; `console/files.rs` → `tests/console/sources.rs:563-600`. **Missing:** the 5 usage fixtures above (absent from `main`, carried only by these branches) and the inventory-grade proof for the 12 product-code paths. |
| 2 | jackin-project/jackin | `codex/openshell-verified-checkpoint-20261108` | `ce119545c` | 12 | (a) 5 · (b) 7 · (c) 0 | Same 12 paths with **byte-identical blob SHAs** to row 1 — the two branches carry the same payload, so row 2 adds no unique residue. **Missing:** the 5 usage fixtures above (absent from `main`) and the inventory-grade proof for the 12 product-code paths. |
| 3 | jackin-project/jackin | `preserve/session-boundaries-20261109` | `16de1eedf` | 6 | (a) 3 · (b) 3 · (c) 0 | Same 6 subjects as rows 1/2, each an earlier or byte-identical revision: `write-file.c` (older blob `dafb9560`), `cancellation.rs` (identical blob), `writer_bootstrap.rs` (older, smaller blob `3b100e3c`), `writer_bootstrap_driver.c` / `_shim.c` (identical blobs), `console/files.rs` (identical blob). Strictly less content than rows 1/2, which are themselves superseded. **Missing:** the 5 usage fixtures above (absent from `main`) and the inventory-grade proof for the 6 product-code paths. |
| 4 | jackin-project/jackin | `preserve/w1-fixes-20261109` | `78d8b5f02` | 1 | (a) 1 · (b) 0 · (c) 0 | One absent product-code path: `crates/jackin-tui/tests/console/files.rs` (blob `46e156a7`, identical to rows 1-3), never in `main`; the same invariant is covered in `main` at `crates/jackin-tui/tests/console/sources.rs:563-600` (mkfifo + symlink-to-pipe) and its Claude twin. **Missing:** this branch's 6 absent paths are 1 product-code path (above) **plus the 5 usage fixtures**, which are absent from `main` — the inventory's record of them is the reason this branch is UNPROVEN and kept — and the inventory-grade proof for the product-code path. |

All 4 are still live on the remote (§7) and are **kept as UNPROVEN CONTAINED candidates** — neither deleted nor proposed for deletion by this document.

Open item flagged by the same review (not a debris question): the 5 usage fixtures above are branch-added and absent from `main`, and they encode a newer Kimi `limits[].window.timeUnit` / OpenCode `opencode-go` API shape than `main` ships. If that newer shape is wanted, it is a **port** decision for the operator — not a debris deletion. Until that decision is made and the fixtures are ported (or archived under a retention ref), these 4 branches hold the only recorded copies of the **four** history-absent fixture contents and the only recorded tree copies of the fifth, and must stay.

---

## 4. The operator's one-word approval

The word carries **two digests**: the manifest of §1, and this publication itself. The manifest
digest binds the word to the 15 (repository, branch, tip) triples; the publication digest binds it
to the exact bytes of the file the operator reviewed. A publication edited after the ratification —
in the table, in the commands, in the procedure — changes that digest and **invalidates the word**:
the deletion run recomputes the digest of the publication it reads and refuses the run unless it is
the digest the word names, so a revision that was not ratified cannot execute.

Compute the publication digest of the reviewed revision (either command prints the same lowercase
hex; the digest is the first column):

```bash
shasum -a 256 rewrite/triage/debris-ratification-2026-11-14.md   # stock macOS
sha256sum rewrite/triage/debris-ratification-2026-11-14.md       # where sha256sum exists
```

Send exactly, with `<publication digest>` replaced by that value:

```text
RATIFY DEBRIS: 15 branches manifest sha256:72224143c3f5aeb19e7a4cd12ecb19ecf65caca66c26b9e77a448de064bccb6a publication sha256:<publication digest>
```

The manifest digest is the SHA-256 of the 15 (repository, branch, tip) triples of §1, sorted by
branch. It binds the word to the manifest that was reviewed: a word without the digests, or with
different ones, is a refusal, and step 1 of the pre-flight in §1 recomputes the manifest digest from
the live tips and stops when it differs. Step 1 also regenerates each §1 deletion command from its own
(repository, branch, tip) triple and stops when the table's command is not that exact push, so the word
covers the commands that run: a `--delete` operand edited after the ratification cannot change what
executes. Step 6 takes the word in `RATIFY_WORD` — **from the line the operator sent, not from this
file** — re-checks the manifest digest against an in-memory snapshot of the manifest before the
first deletion push, re-checks the publication digest against the file it reads, and builds every
push from that snapshot, so a manifest or a publication that changed after the pre-flight is a
refusal there as well.

Run the deletion block of §1 **from this publication, not copied**, so what executes is the
revision the word names. The launcher below verifies the publication digest from the word
**before** it extracts or executes anything: a publication edited after ratification is a
refusal here, before any block reaches a shell. The block is fenced between its begin and end
markers:

```bash
PUB=rewrite/triage/debris-ratification-2026-11-14.md
RATIFY_WORD='RATIFY DEBRIS: 15 branches manifest sha256:72224143c3f5aeb19e7a4cd12ecb19ecf65caca66c26b9e77a448de064bccb6a publication sha256:<publication digest>' \
  env PUB="$PUB" BASH_ENV=/dev/null bash -c '
    set -euo pipefail
    case "$RATIFY_WORD" in
      "RATIFY DEBRIS: 15 branches manifest sha256:"????????????????????????????????????????????????????????????????" publication sha256:"????????????????????????????????????????????????????????????????)
        ;;
      *) echo "refusing: the word is not the reviewed ratification word" >&2; exit 1 ;;
    esac
    want=${RATIFY_WORD##*publication sha256:}
    case "$want" in ""|*[!0-9a-f]*) echo "refusing: the word carries no publication digest" >&2; exit 1 ;; esac
    snap=$(mktemp) || exit 1
    trap "rm -f \"$snap\"" 0
    trap "exit 129" HUP
    trap "exit 130" INT
    trap "exit 143" TERM
    cat -- "$PUB" > "$snap"
    if command -v sha256sum >/dev/null 2>&1; then got=$(sha256sum < "$snap"); got=${got%% *}
    elif command -v shasum >/dev/null 2>&1; then got=$(shasum -a 256 < "$snap"); got=${got%% *}
    else echo "refusing: no SHA-256 command on this host" >&2; exit 1; fi
    [ "$got" = "$want" ] || { echo "refusing: the publication changed after ratification" >&2; exit 1; }
    awk "/^# --- deletion run: begin ---$/,/^# --- deletion run: end ---$/" "$snap" | BASH_ENV=/dev/null bash'
```

Optionally, to also ratify retroactively the one deletion that was performed without a recorded proof (§6):

```text
RATIFY DEBRIS: 15 branches manifest sha256:72224143c3f5aeb19e7a4cd12ecb19ecf65caca66c26b9e77a448de064bccb6a publication sha256:<publication digest>
RATIFY UNRECORDED DELETION: 1 branch (tailrocks/terminal-components-baseline wip/tuiscotti-pin-fec38841)
```

The run reads only the `RATIFY DEBRIS` line; the second line is an approval of record for §6 and is
not an input to the run. A refusal, a different count, a missing or different digest, or any other
wording is a refusal:
**nothing is deleted unless the word above is sent — and nothing is deleted before the archive
prerequisite of §1 has run and been verified.**

---

## 5. Nothing merged, nothing deleted

- **No branch in the approval set has been merged.** Their tips are not ancestors of their repository's `main`, and no merge was performed for this document.
- **No branch in the approval set has been deleted.** All 15 are still on their remotes (§7); the deletion happens **only after the operator's word** (§4).
- **Nothing outside the approval set is deleted.** The 4 DEBRIS-class rows of §2 (each needs its own word; row 4 only after its P1 slice is ported) and the 4 UNPROVEN CONTAINED candidates of §3 stay as they are; the VALUABLE rows of §8 are untouched.
- This document, its commit and its PR **delete nothing**: they only publish the list. The only command shape it contains is the per-branch `git push --force-with-lease=refs/heads/<branch>:<tip-SHA> https://github.com/<repo>.git --delete <branch>` of §1, to be run by the operator after ratification, in the sanitized Git context of §1; each of those commands names its repository by URL (never by the alias `origin`) and refuses to run if the branch's tip has moved.

---

## 6. Recorded deletion — the baseline deletion, its proof and its provenance

| repo | branch | status | recorded proof |
|---|---|---|---|
| tailrocks/terminal-components-baseline | `wip/tuiscotti-pin-fec38841` | **Already deleted from the remote** — live check on 2026-11-14: the ref is absent (§7). **No deletion command is needed.** | The branch's class was CONTAINED (superseded), so no work was lost: its single commit only bumps the `tuiscotti` dev-dependency `d8d54155` → `fec38841`, and `main` already pins the same dependency at the strictly newer rev `92ef2d9e` (`main:Cargo.toml`); the commit's doc intent is already in `main:docs/functional-testing.md:96`. The tip was `1c980e88` ("chore: pin tuiscotti to fec38841 for binary baseline review"). |

**The deletion and its provenance, as the merged inventory records them.** `rewrite/triage/branch-inventory-2026-11-12.md` already records `tailrocks/terminal-components-baseline` `wip/tuiscotti-pin-fec38841` as deleted and reports **1** baseline deletion: the containment-proof table carries the row and its proof, the per-repo summary reports `tailrocks/terminal-components-baseline: 1 deleted`, and the inventory's own correction note marks its earlier revision's "kept - left on the remote because baseline had 0 deletions" sentence as **stale**. That correction - not this section - established the deletion provenance: the branch was already gone from the remote when it was inspected, so the record proves that deleting it was safe, not who deleted it, and the deletion is not attributed to the deletion worker. No count is corrected here, because the current inventory reports none wrong. The deletion happened under the superset proof above (main pins a strictly newer rev of the same dependency, two commits after `fec38841`). This document records that deletion; it does not undo it and does not re-delete it.

---

## 7. Live verification against the remotes (2026-11-14, re-verified 20:12 UTC)

Verified with `gh api --hostname github.com repos/<repo>/branches --paginate --jq '.[] | "\(.name) \(.commit.sha)"'`, first on 2026-11-14, re-verified at 13:17 UTC and re-verified again at 20:12 UTC the same day. "Live" = the branch is still on the remote with the tip recorded in the audits; **ALREADY GONE** = the ref no longer exists. All 15 rows of §1 are live with the recorded tips at 20:12 UTC; none of them is merged, none is deleted.

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
| jackin-project/jackin | `fix/typed-model-selection` | The audited branch was **ALREADY GONE** at the 13:17 UTC re-check (`gh api --hostname github.com repos/jackin-project/jackin/branches/fix/typed-model-selection` → 404): its content landed via **MERGED PR #1279** ("feat(core): add typed model selection for agent tuples", merge commit `25f8c42c660f2951d4acc8f19ff489e7e6f61c67`, merged 2026-11-14 04:12 UTC), which **is an ancestor of `main`**, and `crates/jackin-core/src/model_selection.rs` **is present in `main`** (blob `2613cbe7`). The earlier verdict carried into this document ("the typed-model-selection code is absent from `main`; open PR #1279") was pre-merge and is **stale**: there was nothing left to delete and nothing left to keep, and no row of the approval set changes. The stale *branch copy* of the inventory wrongly calls it DEBRIS — do not use that copy. **At the 20:12 UTC re-check a branch of the same name exists again** (tip `0aaf7a411ce0f35ebd9ca7e7722caed7101c65c6`, a different tip from the audited `5302848e`): it is a **new, unclassified branch**, outside the approval set, outside §2 and §3, and the operator's word does not cover it. **The inventory's contained-ref cleanup action for this ref is deferred** (`rewrite/triage/branch-inventory-2026-11-12.md`, 'Current contained-ref cleanup action'): the ref is protected here, so it stays on the remote until the operator's own word lifts this entry. |
| jackin-project/jackin | `docs/triage-inventory-reclass-2026-11-13` | Live branch (tip `cf9acb7485e1800680fd00b62767af35e9713fe6` at the 20:12 UTC re-check) created after the inventory was captured; not in any class list. **The inventory's contained-ref cleanup action for this ref is deferred** (`rewrite/triage/branch-inventory-2026-11-12.md`, 'Current contained-ref cleanup action'): the ref is protected here, so it stays on the remote until the operator's own word lifts this entry. |
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
- The paths of the procedure are private, not fixed: the pre-flight keeps its manifest in a private directory (`mktemp -d`) that it removes when it ends, the deletion run keeps the sanitized configuration, the scratch repository, the control repositories and the branch lists of the post-check in a private directory of its own, and the deletion run takes its manifest from the §1 table of this publication instead of a file. A predictable path under `/tmp` can be written, replaced with a link or read by every other process on the host, and a run that reads a manifest from such a path reads what another process put there. The digest of §4 is what binds the manifest to the operator's word, so a manifest that another process replaced is a refusal, never a deletion.
- Every GitHub API call of the procedure names `github.com`, because `gh` honors the `GH_HOST` of the environment: without the pin, a run whose environment carries a `GH_HOST` would read and write another host - a GitHub Enterprise instance - where the 15 refs of the ratification are not, and the guards of the run would pass against a repository the operator never ratified. The pin does not change what the run reads on `github.com`; it only refuses to read or write anywhere else.
- The retained-ref check in §1 is a **pre-flight, not a transaction**: a retained ref can move or be deleted between that check and the push, and a deletion lease covers only its own target. **The archive prerequisite of §1 is what closes that window**: before the first deletion push, the tip of each of the 15 rows and the captured tip of every retained ref is preserved under `refs/heads/archive/` and verified (steps 3 and 4 of the pre-flight), the `refs/heads/archive/**/*` namespace is then locked against deletion and force-update by the step-5 ruleset, each deletion push of step 6 re-reads its own archive ref immediately before it runs, and no deletion in §1 touches an archive ref. A collaborator who can move a retained branch can therefore no longer remove the last copy of content that §1a counted as preserved (row 10 relies on retained refs for 2 of its 20 paths). What the archive does not cover is the history a retained ref gained *after* the capture — the archive holds the captured tip, and the tip's own history with it. Running the deletions as one atomic server-side operation remains an option, not a requirement.
- **The decision trail carries this publication's disposition in this PR.** `rewrite/decisions.tsv:520` recorded the four branches of §3 as "CONTAINED-safe-to-delete", and `:525` named "4 CONTAINED-safe-to-delete candidates" in the 15-branch ratification row. This publication supersedes both labels (§3): the four are **UNPROVEN, KEPT** — kept, not deletable. The superseding trail rows are part of this PR: `rewrite/decisions.tsv:531` records the four §3 branches as UNPROVEN, KEPT and excluded from the 15-branch set by class, `:532` corrects the phrase of the ratification row, and `:535` records the private paths and the pinned host of the procedure and the correction of the row at `:534`, whose description of the sanitized configuration now matches the known-safe `credential.helper` allowlist the run implements, and `:536` records that allowlist - a file-backed, unknown or credential-carrying helper is refused, not copied - and the binding of the operator's word to the revision of this publication. The same commit repairs the ascending order of the trail's `ts` column, so the five inherited inversions (rows 314, 319, 383, 413 and 419) are gone and the trail and this publication agree before this PR merges. The rows the other documents cite (`:510`, `:511`, `:514`, `:520`, `:521`, `:525`) are byte-identical to the rows they cited before.
- Nothing was deleted, pushed to a branch, merged or tagged in producing this document; only this file and the seven audit files under `rewrite/triage/audits/2026-11-14/` were added.
