# TASK-001 independent inventory review

Reviewer: independent of the clone and of the evidence collector run. Claims in `goal-references/01-branch-inventory.md` section "Local reconciliation (executed 2026-10-13)" were not treated as proof. Jackin source and that planning file were not modified. No fetch, push, reset, or credential inspection.

Repository: `/Users/donbeave/Projects/jackin-project/jackin`
Reviewed: 2026-10-13

## Check 1 — HEAD, branch, upstream, porcelain, worktrees, shallow

**PASS**

```
git rev-parse HEAD
1c3dd1be4a3b1ed518133ec3899c084c792370ff

git rev-parse --abbrev-ref HEAD
main

git rev-parse --abbrev-ref @{upstream}
origin/main

git status -sb
## main...origin/main

git status --porcelain
(empty; wc -c of porcelain v1 = 0)

git rev-parse --is-shallow-repository
false

test -f .git/shallow
SHALLOW_FILE_ABSENT
```

`git worktree list` at review time:

```
/Users/donbeave/Projects/jackin-project/jackin          1c3dd1be [main]
/Users/donbeave/Projects/jackin-project/wt-baseline-M  1c3dd1be (detached HEAD)
/Users/donbeave/Projects/jackin-project/wt-baseline-P  fba69d60 (detached HEAD)
/Users/donbeave/Projects/jackin-project/wt-baseline-T  9ef617d0 (detached HEAD)
/Users/donbeave/Projects/jackin-project/wt-baseline-U  2e4a62da (detached HEAD)
```

The four detached worktrees are clean (`git status --porcelain=v1 -b` is only `## HEAD (no branch)`). Their `.git/worktrees/*/HEAD` mtimes are `2026-10-13T05:06:24+0700` / `05:06:25+0700`. Collector `generated_at_utc` is `2026-10-12T22:04:19.869868+00:00` (`2026-10-13T05:04:19+0700`). They post-date collection, add no `refs/heads`, and do not dirty the primary worktree. They are not a pin, object, or collector mismatch.

## Check 2 — 79 pins versus origin and audit refs

**PASS.** Mismatches: **0**.

Pins: `goal-references/evidence/remote-branches.json` has 79 unique names and 50 distinct tips.

Compared each pin name to `refs/remotes/origin/<name>` and `refs/audit/20261013T2203Z/origin/<name>`:

| Result | Count |
| --- | ---: |
| Missing on origin | 0 |
| Missing on audit | 0 |
| SHA mismatch | 0 |
| Origin SHA != audit SHA | 0 |
| Extra origin branch not in pins | 0 |
| Extra audit ref not in pins | 0 |

`git for-each-ref` counts: `refs/remotes/origin` = 80, `refs/audit/20261013T2203Z/origin` = 79. The extra origin ref is `refs/remotes/origin/HEAD`, symbolic to `refs/remotes/origin/main`, not a pin.

Advertised heads TSV (`advertised-heads.tsv`): 79 rows, 79 names, 0 name-only differences, 0 SHA differences, 50 distinct tips.

## Check 3 — Distinct tips and collector snapshot

**PASS**

`git for-each-ref --format='%(objectname)' refs/remotes/origin | sort -u` distinct tips = **50** (HEAD symbolic ref does not add a tip). Pin distinct tips = 50.

`jackin-evidence-20261013/inventory.json`:

| Field | Value |
| --- | --- |
| `branches` | 80 (`refs/heads/main` + 79 `refs/remotes/origin/*`; `origin/HEAD` excluded) |
| `tip_summaries` | 50 |
| tip files on disk | 50, names match the 50 tip SHAs |
| `pairwise` | 1225 unique unordered pairs, 0 self-pairs, members ⊆ the 50 tips (`C(50,2)=1225`) |
| `errors` | `[]` |
| `shallow` | `false` |
| `head` (collector `head_before`) | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| `head_after` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| `refs_unchanged` | `true` |
| `worktree_status_unchanged` | `true` |
| `status_porcelain_v2` | `['']` |
| patch-id groups | 1787 |
| `commit_metadata` | 1888 |

Collector origin branch tips differ from pins: **0**.

## Check 4 — P/U/T ahead-behind versus `1c3dd1be4a3b1ed518133ec3899c084c792370ff`

**PASS**

`git rev-list --left-right --count BASE...HEAD` (left = behind, right = ahead):

```
P fba69d60c2373156dc0dcecee8e085c83776966f
0	366

U 2e4a62da4bc4406764162a267163389a5fb513a7
0	25

T 9ef617d014df15c6d967501bcbbdcb3689ed453c
4	2
```

```
git merge-base M T
9b6e1d92608fed65579b0ab5bb4b04d2a8dd602b

git merge-base M P
1c3dd1be4a3b1ed518133ec3899c084c792370ff

git merge-base M U
1c3dd1be4a3b1ed518133ec3899c084c792370ff
```

That is P 366 ahead / 0 behind, U 25 / 0, T 2 ahead / 4 behind, merge base `9b6e1d92608fed65579b0ab5bb4b04d2a8dd602b`.

## Check 5 — Local heads and primary worktree

**PASS**

```
git for-each-ref refs/heads
refs/heads/main 1c3dd1be4a3b1ed518133ec3899c084c792370ff
```

No local head besides `main`. Primary worktree porcelain is empty. The four later detached worktrees in check 1 are clean and are not local branches.

## Check 6 — Missing objects among the 79 pins

**PASS.** Missing objects: **0**.

`git cat-file --batch-check` on the 50 distinct pin tips: 50 `commit`, 0 `missing`. The same check on all 79 pin tip lines (duplicates preserved): 79 `commit`, 0 `missing`.

## Check 7 — September bundle is a different repository

**PASS**

`/Users/donbeave/Projects/jackin-project/jackin-bundle-20260923`:

```
git rev-parse HEAD
092fe1db796367479029d4e94379530f7f5965c0

git remote -v
origin  /Users/donbeave/.codex/recovery/chainargos-java-monorepo-reconciliation-20260923T/jackin/jackin-all.bundle

git log -1 --format='%s'
fix(ci): route github-hosted selectors to velnor-scale-set
```

README title and `jackin.role.toml` `[identity].name` are Agent Brown (`chainargos/jackin-agent-brown`). Root trees differ: bundle `e783d0372f4e48caead9db4a53b358bb6c47f861`, Jackin `d30c132945dfc949d475051b8e6ae99453f5265b`.

`git cat-file -t 092fe1db796367479029d4e94379530f7f5965c0` in the Jackin clone exits 128 (object absent). `git rev-list HEAD` in the bundle (223 commits) piped to Jackin `git cat-file --batch-check` tallies `223 missing`. Shared commits with the Jackin clone: **0**.

## VERDICT: ACCEPT

Checks 1–7 PASS. No pin/ref SHA mismatch, no missing pin object, no collector disagreement, and the primary worktree is clean.
