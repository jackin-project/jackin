# Audit: architect + smith rows of the published branch inventory vs live remotes
Date: audit run (read-only). Tools: gh (authenticated), git over https. No pushes/commits/merges/deletions.

## Location discrepancies found first (context)
- Task-given ref `chore/triage-branch-inventory-2026-11-12` / path on that ref does NOT exist (gh api 404).
  Actual published inventory: **`jackin-project/jackin` @ `main`**, path **`rewrite/triage/branch-inventory-2026-11-12.md`**
  (merged as PR #1280, main tip `78d3b86ed`). Earlier snapshot: branch `chore/triage-inventory-20261112`,
  path `rewrite/triage/branch-inventory-20261112.md` (commit `e13360eb`), folded in as "superseded classes".
  Both read in full; architect/smith rows audited from the published (main) file.

## Row A1 — architect repo has no non-main branch: **PASS**
- `gh api repos/jackin-project/jackin-the-architect/branches --paginate --jq '.[].name'` → `main`
- `git ls-remote --heads https://github.com/jackin-project/jackin-the-architect.git` → only
  `d10672897a2398a24dd874c0fc5f61e8247e260b refs/heads/main` (matches inventory freshness `d10672897a`).

## Row A2 — 9 deleted architect branches recorded with proof: **PASS** (every proof element re-verified live)
Recorded: exactly 9 `jackin-the-architect` rows in the "Contained branch deletions" table, all `deleted YES`
(8 `archive/cleanup-20261006/*` + `preserve/betterleaks-1.10.0-20261109`); per-repo table = 9 CONTAINED;
summary bullet records patch-id vs refs/pull/{479,480}/head, merge-commit ancestry, tree equality, 5 alias SHAs.

Live re-verification (clone + fetch of refs/pull/{479,480,481}/head):
- Deleted tips still resolvable on the remote by SHA: `b809284df67586cfd78c17d4dfcd724a567d9520`,
  `e390e399f2a443a200b81481f0b5dbcc529732de`, `e0b97da3819e591d8914e2cd43819d198fb5d3d9` — all FETCHED OK.
- **patch-id equality vs refs/pull/479+480/head**: `git cherry origin/pr-480 b809284` → `-`;
  `git cherry origin/pr-480 e390e399` → `- -`; `git cherry origin/pr-479 e0b97da` → `-`.
  Direct patch-ids: b809284 `6b0de3be…` == commit `14c4890705…` in pr/480; e390e399 `11e9597d…` == `29da3338f1…` in pr/480;
  e0b97da `cba2f1b9…` == `0592d0deeaeaa5b785fa67a43d23d3b627552720` in pr/479 (inventory's `0592d0d` — exact match).
- **merge commits ancestors of main**: `7b72b38fe1d66e35c0931899c53bf3719592bbcc` IS ancestor; `2fefbdd5731fb460bc9fdd6794ab16cc02da97d3` IS ancestor.
  gh: PR #479 MERGED 2026-10-05, PR #480 MERGED 2026-11-12, PR #481 MERGED 2026-11-12 (squash `d10672897a` = main tip).
- **tree equality**: `origin/main^{tree}` = `origin/pr-481^{tree}` = `ac133c4800b47d4a19659cd0fc78555dd47a5272` (matches recorded `ac133c48…`).
- **5 exact-SHA alias branches**: recorded alias SHA prefixes all equal the verified live tips —
  `original-chain-e390e399f2a4`→`e390e399f2a443a2…`, `role-e0b97da381`→`e0b97da3819e…`,
  `task/original-b809284`→`b809284df675…`, `task/original-e0b97da`→`e0b97da3819e…`, `task/original-e390e39`→`e390e399f2a4…` ✓
- Recorded row markers spot-checked: `2fefbdd:Dockerfile` line 9 `MBX_VERSION=1.22.0`, line 46 `mise_sha256=d24fe0bf7e61…`,
  line 70 exact `MISE_CARGO_BINSTALL_ONLY=1 mise install "cargo-binstall@…"` ✓; `7b72b38:jackin.role.toml` has `version = "v1alpha7"`;
  `git diff e0b97da 7b72b38 -- jackin.role.toml` shows only main-side extra marketplace edits (rust-skills/roadmap-skills) — as claimed.

## Row A3 — deleted `preserve/betterleaks-1.10.0-20261109`, commit cb5801f contained: **PASS**
- `cb5801f77309bbad78ed926e5d59efbf3185cd6d` fetched from remote (object alive): "chore(deps): bump betterleaks to 1.10.0 in the role toolchain",
  3 toolchain files (`jackin-pins.toml`, `mise.lock`, `mise.toml`), 1.9.0→1.10.0.
- `git rev-list cb5801f --not origin/pr-481` = exactly 1 commit (cb5801f) → **37/38 commits ancestors of merged PR #481 head** ✓; 38 ahead of main ✓.
- `git diff cb5801f origin/main -- toolchain/ | grep -ci betterleaks` → **0** ✓.
- `git show cb5801f -- toolchain/` introduces checksums `3d7585cbe4f947aef6e31662f65beb5b43b104381a691b6b81b157ebfbb0c02d` (linux-arm64)
  and `9d9ad503cd5d80d18a939352266c3fcca52f10da9b83cd5cfe646989b13940fe` (linux-x64); `origin/main:toolchain/mise.lock` carries
  exactly those two checksums at `version = "1.10.0"` → **the betterleaks content of cb5801f is byte-equal in main**:
  betterleaks block diff = empty, betterleaks-line diff = 0 lines in all 3 files.
- Nuance (not a failure): the 3 whole files differ main-vs-cb5801f because main has later edits
  (hk 2.11.1→2.12.0, mr-boxington 1.29.2→1.30.0, added `x86_64-unknown-linux-musl` target) — exactly the
  "main side is the later edit" statement of the published inventory; the earlier snapshot's "(not byte-identical to main)"
  refers to whole-file, the commit's change itself is byte-identical. Containment holds.

## Row S1 — smith's 2 remaining branches are the DEBRIS ones: **PASS** (name-prefix note)
- `git ls-remote --heads` (and gh api, paginated) → exactly 3 heads: main `a9b8c8aea…` (matches inventory freshness)
  + `recovery/20261006/jackin-agent-smith/2e7119b9c668/branch/velnor/fleet-92c3478694ef423fb4160b8c934d6671ca85b51a` (tip `b9db5b149cc…`,
  patch-id `eef2496d81fbfafda0508af1715ab2bd41fda4d7` — unique, matches inventory; PR #206 CLOSED ✓)
  + `recovery/20261006/jackin-agent-smith/2e7119b9c668/jackin-agent-smith` (tip `c58cfdcaaca…`).
- Both are the 2 DEBRIS rows of the inventory. NOTE: the audit request's short names omit the middle segment
  `jackin-agent-smith/2e7119b9c668/` (inventory's abbreviated rows use `recovery/…/`); live names match the
  inventory's FULL names exactly. Substance: PASS.

## Row S2 — deleted smith branch `…fleet-b9c3156cdb88…` recorded with proof: **PASS**
- Absent from live heads ✓ (only the 2 DEBRIS branches + main).
- Tip `ece890fd58631bf6ed0abb84eb06bb78780243e9` (= PR #207 head, still fetchable via `refs/pull/207/head`):
  `git show <tip> | git patch-id --stable` = `d788ec002f4692b0a1354cec6ac5648fad18f7a2` ==
  patch-id of squash `08cb1c2f82519bab1aa0c164879955a84d35463b` — identical, exactly the recorded value ✓.
- gh: PR #207 MERGED 2026-09-16T15:41:06Z, head branch `velnor/fleet-b9c3156cdb88e63c11b9e595a3e694b02238c09a` ✓;
  `git merge-base --is-ancestor 08cb1c2f origin/main` → YES ✓.
- Row present in "Contained branch deletions" table with this proof, `deleted YES` ✓.

## Verdict table
| # | row | result |
|---|---|---|
| 1a | architect: 0 non-main branches | **PASS** |
| 1b | architect: 9 deleted branches recorded with full proof (patch-id vs pr/479+480 head, 7b72b38 & 2fefbdd ancestors, tree ac133c48 equality, 5 exact-SHA aliases) | **PASS** |
| 2a | smith: 2 remaining branches are the 2 DEBRIS ones | **PASS** (names carry extra `…/2e7119b9c668/` segment vs request wording; match inventory) |
| 2b | smith: deleted fleet-b9c3156cdb88 recorded with patch-id proof vs squash 08cb1c2f of MERGED PR #207 | **PASS** |
| 3 | betterleaks `preserve/betterleaks-1.10.0-20261109` contained — cb5801f byte-equal in main (3d7585cb/9d9ad503) | **PASS** (change content byte-equal; whole files differ only by main's later edits) |
| 0 | inventory location as given in the request | **FAIL-as-given / corrected** — ref `chore/triage-branch-inventory-2026-11-12` 404s; published file is `rewrite/triage/branch-inventory-2026-11-12.md` on `main` (PR #1280) |

## Evidence paths / commands
- inventory: `gh api repos/jackin-project/jackin/contents/rewrite/triage/branch-inventory-2026-11-12.md?ref=main`
- clones (https, read-only): `/tmp/triage/r-architect`, `/tmp/triage/r-smith`
- copies: `/tmp/triage/inventory-published.md` (published), `/tmp/triage/inventory.md` (earlier snapshot)
