# Retire verification: preserve/t3 refs, stash-w8, pr1264-merge

Read-only. origin/main checked at `85a921a93` (one commit past `0130af3dd`, which the earlier manifest used).

**Totals:** SAFE 1 | VALUE 0 | UNCERTAIN 23 | MISSING-VALUE confirmed 0

## Method

1. t3 refs: tree of each of the 21 refs and its parent. Distinct snapshot trees:
   - T-C `453e05085` = `5076ca2a0` (t3-0)
   - T-A `5f3b1f403` = `4691c0089` (t3-1, 2, 10-16)
   - T-B `c6278e9c9` = `573c3a09e` (t3-19-25)
   - T-D `b0043b19e` = `7bf7fe261`; `64b9ff384` (t3-26-28) adds only `rewrite/GOAL-ALL-AGENTS.md`
   - T-E `f584c11ab` = `589b3b6f4`; `976b0ebdd` (t3-29) adds only `rewrite/GOAL-ALL-AGENTS.md`
   - The 5 lines form one cumulative history from merge-base `470fc5d6f`. Consecutive deltas: T-C to T-A is 9 files (CI, Cargo, mise.toml, identity coverage test, role_workspace test). T-A to T-B is 2 files (`observe.rs`, `picker.rs`). T-B to T-D is 112 product/test files plus a 57-file `vendor/sqlx-sqlite` set. T-D to T-E is docs only.
2. Added-line novelty per snapshot against `origin/main` (per file, exact trimmed text). T-C: 6538 absent lines. Grouped by path (table below).
3. fn-name check: 103 fn names defined in snapshot test/product files are absent from main's fn-name set. Each was checked against behavior on main or against a P-task.
4. pr1264 intermediate (`bbe6b5bb7`): `9792bdd88` is an ancestor of `f0021a0`, so each intermediate-only line was checked against the 26 later PR commits (`9792bdd88..f0021a0`).

## Snapshot-level classification (T-C and cumulative)

| Group (absent lines on main, T-C) | Count | Class |
|---|---|---|
| `.github/workflows`, `.github/actions`, `.velnor`, `.mise`, lock files, `Cargo.toml`, `docs/`, `rewrite/` | ~3,000 | excluded (generated CI, docs, locks) |
| `crates/usage` Gemini (`providers/gemini.rs`, `jackin-usage-provider-gemini`) | ~890 | excluded: removed by #1296; dispatch returns unsupported |
| `crates/usage` kimi (old `limits`/`summary` schema, overflow checks, clamps) | ~100 | on main, superseded: #1296 reshaped Kimi to `usages.limit_*`/`used_ratio`; Kimi managed OAuth decision in `rewrite/decisions.tsv` (2026-11-04) |
| `crates/usage` openrouter (exact money, credits, key limit) | ~80 | covered by P02; connected path on main (`connected/openrouter.rs`) |
| `crates/usage` minimax/zai/grok windows | ~50 | superseded: #1296 rewrote minimax; main has `weekly_end_time` and interval windows; zai `window_seconds` mapping; grok deprecated-key fallback on main |
| `crates/usage` OpenCode/OMP credential bounds | ~60 | covered by P01 |
| `crates/jackin-session` accounts/credentials/launch/export | ~300 | covered by P01, P03, P04, P05, P06, P13, P16 |
| `crates/jackin-session` plan/launch multi-agent (`plan.rs`, `multi.rs`, `launch/run.rs`, `config.rs`, `agent.rs`, `codex.rs`, `claude.rs`, `instance_identity.rs`, `omp_models.rs`, `native.rs`) | ~300 | UNCERTAIN residual (see below) |
| `crates/jackin-core` catalog tests (`BlockedVendor`, `Unavailable`, typed errors) | ~120 | superseded: main's `TupleState` model (D29) |
| `crates/jackin-tui` backend refresh, observe role docs, picker | ~60 | on main (ticketed refresh; Git role loading in `observe.rs` lines 377-389) or covered by P07/P08/P18 |
| `crates/jackin-openshell`, `crates/jackin-image` | ~140 | covered by P14, P15, P17 |
| `crates/xtask` e2e steps (codex/junie), `crates/jackin-testkit` | ~600 | excluded: test harness for the old flows; Junie not in main's catalog |

## Per-ref verdicts

| Ref | Snapshot tree | Verdict | Evidence |
|---|---|---|---|
| `t3-0` `f6c2bff00` | T-C `453e05085` | UNCERTAIN | Classification above. Residual: the jackin-session plan/launch multi-agent group, ~300 lines not body-compared. No VALUE found. |
| `t3-1` `c9a4ad50c` | T-A `5f3b1f403` | UNCERTAIN | T-C plus 9 CI/test/Cargo files, all excluded or on main. Same residual. |
| `t3-2` `d9c34f15f` | T-A | UNCERTAIN | Same as t3-1. |
| `t3-10` `1e928c0af` | T-A | UNCERTAIN | Same as t3-1. |
| `t3-11` `8d1e781bf` | T-A | UNCERTAIN | Same as t3-1. |
| `t3-12` `10034b2a4` | T-A | UNCERTAIN | Same as t3-1. |
| `t3-13` `52a1aabe0` | T-A | UNCERTAIN | Same as t3-1. |
| `t3-14` `517097494` | T-A | UNCERTAIN | Same as t3-1. |
| `t3-15` `7f1f0f92c` | T-A | UNCERTAIN | Same as t3-1. |
| `t3-16` `7fc4466a9` | T-A | UNCERTAIN | Same as t3-1. |
| `t3-19` `abe950fac` | T-B `c6278e9c9` | UNCERTAIN | T-A plus `observe.rs` Git-role loading (on main, lines 377-389). Residual as above. |
| `t3-20` `146052403` | T-B | UNCERTAIN | Same as t3-19. |
| `t3-21` `e99b4a424` | T-B | UNCERTAIN | Same as t3-19. |
| `t3-22` `d7a22844c` | T-B | UNCERTAIN | Same as t3-19. |
| `t3-23` `cd4fc9ee4` | T-B | UNCERTAIN | Same as t3-19. |
| `t3-24` `fe7c87c9e` | T-B | UNCERTAIN | Same as t3-19. |
| `t3-25` `22cd55144` | T-B | UNCERTAIN | Same as t3-19. |
| `t3-26` `d7a1c0ef1` | `64b9ff384` (T-D + GOAL-ALL-AGENTS.md) | UNCERTAIN | T-B plus 112 product/test files and vendor set. Vendor tree identical to main. GOAL-ALL-AGENTS.md is obsolete prose (excluded). Residual as above. |
| `t3-27` `1ae7d7701` | same | UNCERTAIN | Same as t3-26. |
| `t3-28` `814b61d33` | same | UNCERTAIN | Same as t3-26. |
| `t3-29` `4f4ea8bf5` | `976b0ebdd` (T-E + GOAL-ALL-AGENTS.md) | UNCERTAIN | Same product tree as T-D; the extra delta is docs only. |

## Stash `preserve/stash-w8-port-feature-openshell-native` (`f2da2d215`)

**UNCERTAIN.** Worktree delta is 8 paths, all excluded (b): `agent.rs` blocker wording, `mise.lock` and `mise.toml` pins, `rewrite/DESIGN.md`, `rewrite/blockers/T06-T08`, `rewrite/decisions.tsv`. HEAD parent `7bd1b7036` is treated below.

## Commit `7bd1b7036` (stash HEAD parent, tree `28c846980`)

**UNCERTAIN.** Its product tree equals T-E's product content (`589b3b6f4` plus 3 docs). Same residual as t3-26-29. Note: the earlier manifest said 1143 commits; `git rev-list --count` gives 2564. The commit is reachable from 17 refs, including `origin/preserve/pr1264-merge` and `origin/sup019/fix-1264`, so its commits are not lost on deletion.

## `preserve/pr1264-merge` (`bbe6b5bb7`)

**SAFE.** `bbe6b5bb7` is the merge of `9792bdd88` into `aa218ea83`. `9792bdd88` is an ancestor of `f0021a0` (the final PR head, tree equal to squash `f683619b5`). Intermediate-only lines vs squash: 1175 added lines. Absent from main by per-file exact text: 799 (the manifest's 632 used a tree-wide count; the difference is method, not content).

Each absent line was checked against the 26 later PR commits:

- **683 lines** were removed in a hunk that also added lines (replaced by a later commit). Not value per the rule.
- **114 lines** were removed in pure-removal hunks. Groups:
  - kimi, 52 lines: old `limits`/`summary` schema, superseded (see T-C table).
  - openrouter, 18 lines: account-credits path; the connected path on main carries the credit lookup and P02 covers exact money.
  - minimax, 3 lines: window constants replaced by interval/weekly windows on main.
  - zai, 1 line: `Some(5) => 60` replaced by main's `window_seconds` mapping.
  - `multi_session.rs`, 9 lines: worktree parent handling; main has the ancestor-substitution check (commit `e7b1bbfad`).
  - `lifecycle.rs`, 5 lines: `RuleKey::provider_rule` still used on main.
  - `load.rs`, 5 lines: main parses typed `IssueView`.
  - `credential.rs`, 1 line: `OmpHost`/`HostLogin` arms are real on main (`credential.rs:154,179`).
  - `dispatch.rs`/`host.rs`, 5 lines: cancel plumbing removal; covered by P03.
  - sha1 import changes in `export/bundle.rs` and `examples/export_memory.rs`: replaced by gix-hash (`8b410d2e6`).
  - `panes.rs`, 1 line: queued-byte accounting refactor.
  - `cli/usage.rs`, 2 lines: 403 credits route; covered by P02.
- **2 lines** are kimi fixture JSON (old schema, superseded).

No dropped line was found that lacks a main equivalent or a P-task.

## Cleared candidates (examined, not MISSING-VALUE)

- **403 classification.** Snapshot maps 403 to expired login. Main maps `Forbidden` to `UnavailableReason::Provider` (`dispatch.rs:188`), changed in #1296. Deliberate, not lost.
- **Role-tree hashing.** Snapshot says ignored files keep the image key. Main's `plan/roles.rs` test asserts ignored files change the digest. Opposite decision on main.
- **Placeholder prefix refusal.** `launch/run.rs` `placeholder()` refuses output without `PLACEHOLDER_SCHEME` and reports only the key. On main.
- **Retry-After as HTTP date.** Main parses it (`http.rs:426`), more than the snapshot.
- **Multi-instance codex placeholder.** Main test is `two_instance_launch_writes_codex_env_like_single_instance`, the native-subscription replacement.
- **Kimi/Mimo native API-key placeholder configs.** Superseded by the Kimi managed-OAuth decision; main's `plan/native.rs` forbids `KIMI_API_KEY`.
- **Gateway for non-ChatGPT accounts.** Main has `Account::needs_gateway()` (`accounts.rs:408`).

## Residual that keeps the t3 family at UNCERTAIN

About 300 added product lines in `jackin-session` plan/launch/config/agent (`plan.rs` 16, `codex.rs` 60 test lines, `claude.rs` 20, `native.rs` 25, `multi.rs` 13, `run.rs` 14, `config.rs` 10, `agent.rs` 7, `instance_identity.rs` 34, `omp_models` 17, `files.rs` 32). Group-level evidence points to superseded by the modern multi-agent plan (plan section 3 says keep main's semantics), but these lines were not compared hunk by hunk. Closing this needs a body-level pass over those files.

## Main-side note

`rewrite/DESIGN.md` line 343 was stale. Fixed in PR #1311 (draft), branch `docs/design-gemini-usage`. Not part of this verdict set.
