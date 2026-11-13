# Branch Inventory — 2026-11-12

**Date:** 2026-11-12  
**Purpose:** A single consolidated inventory of the non-main branches of the five repositories under triage, with each branch classified and the evidence for that classification recorded. This document is a read-only record: it merges nothing and deletes nothing.

**Repository:** `jackin-project/jackin`  
**Document path:** `rewrite/triage/branch-inventory-2026-11-12.md`

## Class definitions

- **VALUABLE** — a feature, fix, or improvement whose content is **not** in `main`. The work still carries value and must not be lost. It is not proven contained.
- **CONTAINED** — the branch's content is **fully in `main`**, and the proof is recorded (merged PR / patch-id equality / ancestry / empty diff against main). Nothing is lost by retiring the branch.
- **DEBRIS** — logs, progress output, temporary scratch work, stash-recovery snapshots, generated artifacts, or duplicates of other branches. No unique product value.

**Counts** are per repository, per class, and are derived from the tables below.

---

## jackin-project/jackin-sentinel

Source inventory: `sentinel.md` (per-repository triage table, reproduced verbatim).

| branch | last commit | ahead | files | class | evidence |
|---|---|---|---|---|---|
| origin/fix/s158-postmerge-4 | 2026-11-13T12:31:12+07:00 (0b39702 "fix(probe): require real entropy before trusting the run key") | 61 (behind 0) | 3 files, +2433/-248: probe/jackin-sentinel-probe, probe/tests/egress.sh, probe/tests/j08.sh | VALUABLE | Open PR #159 "fix(probe): 4 confirmed post-merge findings after #158" (head sha 0b39702 == origin/fix/s158-postmerge-4 == refs/pull/159/head); PR body: all 6 new tests FAIL on origin/main, PASS with branch; `git cherry origin/main <branch>` = 61 "+" / 0 "-", `git branch -r --merged origin/main` does not list it, diff main..branch = 3 files +2433/-248. Content not in main => VALUABLE; once PR #159 merges it becomes CONTAINED (would then be provable via merged PR + empty diff). |
| origin/preserve/probe-identity-20261109 | 2026-11-09T21:39:00+07:00 (01f8ca5 "fix(probe): record interpreter identity and classify launch environment") | 51 (49 non-merge + 2 merge commits; behind 1) | 12 files, +2091/-4514 vs main: AGENTS.md, README.md, hooks/preflight.sh, hooks/setup.sh, jackin.role.toml, probe/jackin-sentinel-probe, probe/tests/egress.sh, probe/tests/fixtures/T09.json, probe/tests/fixtures/T14.json, probe/tests/fixtures/T18.json, probe/tests/hooks.sh, probe/tests/j08.sh | VALUABLE | Mixed, but tip content is not in main. Not CONTAINED: no PR (`gh pr list --head preserve/probe-identity-20261109` = []), not in `git branch -r --merged origin/main`, `git cherry` = 49 "+" / 0 "-", net diff vs main 12 files +2091/-4514. Not pure snapshot/DEBRIS: 50 of its 51 commits are shared shas with merged PR #158 head lineage (feature/openshell-native; `git merge-base` with refs/pull/158/head = 1e9431e), but tip commit 01f8ca5 is NOT an ancestor of pr-158 head (0d98105) and its implementation+tests (start_interpreter_path, launch_env_classifications, proc_argv_matches, "no denied path produced a classified denial"; +1641 lines across probe/jackin-sentinel-probe, probe/tests/egress.sh, probe/tests/j08.sh; new j08_launch_environment_* tests) grep 0 in origin/main, origin/pr-158-head and origin/pr-159-head probe and j08 tests => a real unmerged fix whose content is absent from main (its 50 older commits are a superseded snapshot of the already-merged #158 lineage; only the tip needs salvaging). |

**Per-class counts — VALUABLE: 2 · CONTAINED: 0 · DEBRIS: 0** (total branches in table: 2).

## jackin-project/jackin-the-architect

Source inventory: `architect.md` (per-repository triage table, reproduced verbatim).

| branch | last commit | ahead | files | class | evidence |
|---|---|---|---|---|---|
| archive/cleanup-20261006/architect-mbx-b809284df675 | 2026-10-05T11:36:14+02:00 | 1 | 1 | CONTAINED | tip `b809284` "fix architect image Rust tool provisioning" (1 file: Dockerfile). `git cherry pr/480 b809284` → `- b809284` (patch-id equal to a commit inside PR #480, `fix/architect-mbx-runtime`, gh: **MERGED 2026-11-12**, merge commit `2fefbdd` which is on main). Byte-identical markers of the commit survive in `2fefbdd:Dockerfile` (mise sha256 `d24fe0bf7e61…`, cargo_watch sha512 `2b6e288…`, lychee `1f4e0ef7…`, boltffi `342af5ab…`, `MBX_VERSION=1.22.0`). Not an ancestor of main, but content fully in main via merged PR #480. |
| archive/cleanup-20261006/architect-mbx-e390e399f2 | 2026-10-05T11:40:16+02:00 | 2 | 1 | CONTAINED | tip `e390e399` "prevent Cargo fallback while bootstrapping binstall" (its 2 ahead commits = b809284 + itself; 1 file: Dockerfile). `git cherry pr/480 e390e399` → `- b809284`, `- e390e399` (both patch-id equal inside merged PR #480). Its exact line `MISE_CARGO_BINSTALL_ONLY=1 mise install "cargo-binstall@…"` exists at `2fefbdd:Dockerfile:70`. Content in main via PR #480 (merge commit `2fefbdd`). |
| archive/cleanup-20261006/architect-original-chain-e390e399f2a4 | 2026-10-05T11:40:16+02:00 | 2 | 1 | CONTAINED | Exact duplicate pointer: `git rev-parse` = `e390e399f2a443a200b81481f0b5dbcc529732de`, same SHA as `architect-mbx-e390e399f2` (zero unique content). Same proof as above: patch-id equal (`git cherry -`) to a commit in MERGED PR #480, merge commit `2fefbdd` on main. |
| archive/cleanup-20261006/architect-reflog-commit-e0b97da3819e | 2026-10-05T02:32:22+02:00 | 1 | 1 | CONTAINED | tip `e0b97da` "fix: migrate architect manifest to v1alpha7" (1 file: jackin.role.toml: version v1alpha5→v1alpha7 + drop 6 provider blocks). `git cherry pr/479 e0b97da` → `- e0b97da` (patch-id equal to `0592d0d` "fix: migrate architect manifest to v1alpha7" in PR #479, head ref `fix/architect-role-manifest-v1alpha7`, gh: **MERGED 2026-10-05**, merge commit `7b72b38` which is on main). `git diff e0b97da 7b72b38 -- jackin.role.toml` shows only the *extra* marketplace edits #479 made on top — i.e. every change of e0b97da is present in main's `7b72b38`. |
| archive/cleanup-20261006/architect-role-e0b97da381 | 2026-10-05T02:32:22+02:00 | 1 | 1 | CONTAINED | Exact duplicate pointer: SHA `e0b97da3819e591d8914e2cd43819d198fb5d3d9`, same commit as `architect-reflog-commit-e0b97da3819e`. Same proof: patch-id equal inside MERGED PR #479 (merge commit `7b72b38`, on main). |
| archive/cleanup-20261006/architect-task/original-b809284 | 2026-10-05T11:36:14+02:00 | 1 | 1 | CONTAINED | Exact duplicate pointer: SHA `b809284df67586cfd78c17d4dfcd724a567d9520`, same commit as `architect-mbx-b809284df675`. Same proof: patch-id equal inside MERGED PR #480 (merge commit `2fefbdd`, on main). |
| archive/cleanup-20261006/architect-task/original-e0b97da | 2026-10-05T02:32:22+02:00 | 1 | 1 | CONTAINED | Exact duplicate pointer: SHA `e0b97da3819e591d8914e2cd43819d198fb5d3d9`, same commit as `architect-reflog-commit-e0b97da3819e`. Same proof: patch-id equal inside MERGED PR #479 (merge commit `7b72b38`, on main). |
| archive/cleanup-20261006/architect-task/original-e390e39 | 2026-10-05T11:40:16+02:00 | 2 | 1 | CONTAINED | Exact duplicate pointer: SHA `e390e399f2a443a200b81481f0b5dbcc529732de`, same commit as `architect-mbx-e390e399f2`. Same proof: patch-id equal inside MERGED PR #480 (merge commit `2fefbdd`, on main). |
| preserve/betterleaks-1.10.0-20261109 | 2026-11-09T21:39:52+07:00 | 38 | 53 | CONTAINED | 37 of its 38 commits (e.g. `ac71eb1`, `65d8997`, `b021144`, `bcaf211`, `5a39732`) are **ancestors of the merged PR #481 head by identical SHA** (PR #481 `feature/openshell-native`, head `063aeb6`, gh: **MERGED 2026-11-12**, squash merge `d106728` = main tip; `git rev-parse main^{tree}` == `pr/481^{tree}` = `ac133c48…`). The single commit not in the PR, `cb5801f` (betterleaks 1.9.0→1.10.0 pin: 3 files, 10+/10-), has byte-identical content in main: `git diff cb5801f main -- toolchain/* \| grep -i betterleaks` → 0 lines, and main's `toolchain/mise.lock` carries cb5801f's exact checksums `3d7585cb…`/`9d9ad503…` (set by PR481 commit `063aeb6`). Full tree diff vs main = only 8 files, and in every hunk the **main side is the later edit** (AGENTS/README D29 wording, rustup dual-checksum renovate rule, lock-toolchain fail()/coverage checks, hk 2.12.0 / mbx 1.30.0 / x86_64-musl target): main strictly supersedes the branch. Nothing of the branch is missing from main. |

**Per-class counts — VALUABLE: 0 · CONTAINED: 9 · DEBRIS: 0** (total branches in table: 9).

## jackin-project/jackin-agent-smith

Source inventory: `smith.md` (per-repository triage table, reproduced verbatim).

| branch | last commit | ahead | files | class | evidence |
|---|---|---|---|---|---|
| `recovery/…/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a` | 2026-09-16 22:24:31 +0700 (b9db5b1 "chore: sync velnor-workflow to 92c3478694ef…") | 1 | 7 (tip↔main 47) | DEBRIS | Sole commit is a generated-artifact regeneration (velnor-workflow sync to pin 92c34786): all 7 changed files are generator state/outputs (`.github-gen/velnor-workflow.toml`, `.github/ci/.github-actions-generator-state`, 5 rendered `.github/workflows/*.yml`). PR #206 (same head branch name) was **closed unmerged**; patch-id eef2496d… is unique — but the content is a superseded duplicate: regenerated 11 min later at the next pin by PR #207 (branch fleet-b9c3156c…, merged as squash 08cb1c2f, patch-id d788ec00…), which is in main. Not a real feature/fix — pure generated churn. `git branch --merged main` does not list it. |
| `recovery/…/branch/velnor/fleet-b9c3156cdb88e63c11b9e595a3e694b02238c09a` | 2026-09-16 22:35:15 +0700 (ece890f "chore: sync velnor-workflow to b9c3156c") | 1 | 7 (tip↔main 47) | CONTAINED | Content is fully in main via **merged PR #207** (merged 2026-09-16T11:41Z as squash commit 08cb1c2f, an ancestor of main). **Patch-id equality proof**: `git show <branch> \| git patch-id --stable` == `git show 08cb1c2f \| git patch-id --stable` == `d788ec002f4692b0a1354cec6ac5648fad18f7a2`. (Not listed by `git branch --merged main` only because the PR was squashed, not merged with its original commits.) |
| `recovery/…/jackin-agent-smith` | 2026-10-06 05:22:13 +0700 (c58cfdc "WIP preservation snapshot /Users/donbeave/Projects/jackin-project/jackin-agent-smith 2026-10-06") | 1 | 0 (tip↔main 38, all from main's later commits) | DEBRIS | Commit is a local-scratch "preservation snapshot" with an **empty patch** (`git show` diff is empty; tip tree 9e7fdaee… == parent tree). Parent 2e7119b ("chore(ci): migrate workflows to velnor-actions 0.1.0 (#213)") is an ancestor of main, so the branch contributes zero unique content — a snapshot/duplicate by definition. 3-dot diff vs main = 0 files. |

**Per-class counts — VALUABLE: 0 · CONTAINED: 1 · DEBRIS: 2** (total branches in table: 3).

---

## DEBRIS AWAITING OPERATOR CONFIRMATION

The list below contains only branches classified **DEBRIS** across all five repositories. It is presented so the operator can approve the whole list in a single word.

| repo | branch | evidence |
|---|---|---|
| jackin-project/jackin-agent-smith | `recovery/…/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a` | Sole commit is a generated-artifact regeneration (velnor-workflow sync to pin 92c34786): all 7 changed files are generator state/outputs (`.github-gen/velnor-workflow.toml`, `.github/ci/.github-actions-generator-state`, 5 rendered `.github/workflows/*.yml`). PR #206 (same head branch name) was **closed unmerged**; patch-id eef2496d… is unique — but the content is a superseded duplicate: regenerated 11 min later at the next pin by PR #207 (branch fleet-b9c3156c…, merged as squash 08cb1c2f, patch-id d788ec00…), which is in main. Not a real feature/fix — pure generated churn. `git branch --merged main` does not list it. |
| jackin-project/jackin-agent-smith | `recovery/…/jackin-agent-smith` | Commit is a local-scratch "preservation snapshot" with an **empty patch** (`git show` diff is empty; tip tree 9e7fdaee… == parent tree). Parent 2e7119b ("chore(ci): migrate workflows to velnor-actions 0.1.0 (#213)") is an ancestor of main, so the branch contributes zero unique content — a snapshot/duplicate by definition. 3-dot diff vs main = 0 files. |

**Total DEBRIS branches awaiting operator confirmation: 2**

> **DEBRIS branches are NOT deleted.** Nothing in this document has been deleted, merged, or modified. The DEBRIS list is a proposal awaiting explicit operator confirmation; deletion will only proceed if and when the operator approves it.

## Coverage note

The following per-repository inventories were not available at publish time and are therefore **not** represented above: `jackin.md` (jackin-project/jackin), `baseline.md` (tailrocks/terminal-components-baseline).
