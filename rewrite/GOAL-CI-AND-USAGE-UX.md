Finish the jackin CI migration, land the follow-up fixes, then redesign the usage-accounts UI/UX and verify it against my real accounts. Merge every PR through the tailrocks PR skills.

## Models and subagents

- Run the main session on Sonnet 5.5 (`/model sonnet`). The main thread plans, delegates, reviews results, and runs the PR skills. It does not do bulk reading.
- Delegate heavily with the Agent tool. Always set `model` explicitly on every Agent call:
  - `haiku`: log reading, grepping, CI failure triage, file inventories, gathering terminal captures, comparing snapshot JSON to rendered output, running test suites and summarizing results.
  - `sonnet`: code fixes, CI config changes, implementation of each UI surface, writing tests, refreshing PRs.
  - `opus`: use only for (1) choosing the final UI layout from the candidate designs, and (2) one final review per PR before merge. Nothing else.
  - Never use `fable`. Never use `subagent_type: "fork"`, because forks inherit the parent model and ignore the `model` override.
- Run independent subagents in parallel in one message. Examples: triage each red CI job, implement each UI surface, verify each provider.
- Give every subagent a self-contained brief: the goal, the exact files, what has been ruled out, and the shape of the report it must return. Ask for short conclusions, not file dumps.
- Use isolation `worktree` for parallel implementation agents that edit code, then merge their results on the feature branch.

## Context (verified 2026-11-24)

- PR 1296 (native usage accounts) is MERGED as squash 4a2b869c on main. It needs no further work.
- Jackin generates CI with velnor-gen from `.velnor/rust-ci.toml`. Velnor Actions is retired here. Do not use velnor-actions and do not hand-edit generated `.github/` files.
- Repo root: /Users/donbeave/Projects/jackin-project/jackin. Leave the uncommitted `mise.toml` / `mise.lock` tool bumps in that checkout untouched.
- Commits: `git commit -s` as Alexey Zhokhov <alexey@zhokhov.com>, with exactly one Signed-off-by trailer.
- PR skills: `tailrocks-repository-skills:tailrocks-pr-create`, `-pr-refresh`, `-pr-status`, `-pr-threads`, `-pr-merge`, and `tailrocks-review-pr`.

## Phase 1: PR 1298 (branch ci/velnor-gen, worktree ../jackin-velnor-gen, head 059319acd, CI run 45385656546 red)

Start with one Haiku triage agent per red job, all in parallel. Each agent reads the job's log with `gh run view --log-failed` and returns the root causes, grouped. Then use Sonnet agents for the fixes.

1. Repository steps: lines 5–6 of `ci/native-libs.sh` are comments, which breaks rule D17. The worktree already holds the uncommitted deletion. Verify it with `cargo xtask ci --only comments,layers,pins,containers,errors`, then commit it.
2. Secrets scan: gitleaks reports `leaks found: 30` on the PR commit. Identify the file and rule of each finding without printing values. If the findings are generated hash data (likely `.velnor/rust-metadata.json`), fix them the way the repo already handles this: a path allowlist in the scanner config, or by not committing the generated file. Remove any real secret. Never print secret values.
3. rust: 48 tests failed and 9 timed out. Group the failures by cause and fix the CI inputs or the code. Do not ignore or skip tests to get green.
4. rust-lane-all-features: `jackin-testkit` `tests/identity/baseline.rs:19` runs `cargo metadata` under `--offline` and cannot fetch `android_system_properties`. Make the lane prefetch dependencies for all targets and features, or make the test stop needing the network.
5. Cache: velnor-ci reports "identical observed compilation unit executed 2 times". Find which jobs build the same units and fix it in `.velnor/rust-ci.toml`.
6. Run velnor-gen twice and commit only when the second run leaves no diff. Push with tailrocks-pr-refresh, wait for CI, and run tailrocks-pr-status 1298. Fix every blocker. Use one Opus agent for the final tailrocks-review-pr. When the status is READY, merge with tailrocks-pr-merge.

## Phase 2: PR 1297 (fix/session-purge-git-lock)

After 1298 is on main, refresh 1297 onto main with tailrocks-pr-refresh. Make CI green (the jackin-session `maintenance.lock` race is this PR's fix). Then run status, and merge when it is READY.

## Phase 3: usage-accounts UI/UX redesign (new branch feat/usage-accounts-ux from the new main)

Surfaces: `jackin usage` (table and TUI), `jackin usage details`, `jackin usage snapshot`, `jackin usage providers`, `jackin account usage ls`, and the TUI connections board (`crates/jackin-tui/src/connections*`, `crates/usage/jackin-usage/src/{dashboard,presentation}*`).

1. A Haiku agent inventories the current rendering code and captures every surface as it is today, saved under `docs/usage/ux-before/` or the scratchpad. A second Haiku agent summarizes the earlier uncommitted attempt in `/private/tmp/jackin-usage-ux-preview` (base 7f23709ef, 12 files) and lists what is worth keeping.
2. Three Sonnet agents in parallel each design one candidate layout as text mockups of every surface, using real data from the snapshot. One Opus agent picks the base and names the parts to graft from the others.
3. Sonnet agents implement in parallel, one per surface, in worktrees. Requirements:
   - Group accounts by provider. Each account shows its plan, then the main limit as a bar with % used and the reset as both relative and local time. Secondary windows (session / weekly / monthly / extra usage / credits / reset inventory) sit indented beneath it.
   - One normalized quota model for all providers. Antigravity `remaining_fraction`, MiniMax `remaining_percent` with `limit: 0` / `unit: count`, and MiMo raw credits (82,000,000,000) all render as human numbers (for example "82B credits, 38% used").
   - Freshness is always visible ("fetched 5m ago", stale marker). Color appears only on a TTY, and plain output pipes cleanly. `--json` is the only path that prints JSON; today `snapshot` prints raw JSON without `--json`, so fix that.
   - `account usage ls` becomes a readable table (provider, label, plan, status). IDs appear only with `--wide` or `--json`. Duplicate connections are called out.
   - Add snapshot tests for every surface. A Haiku agent runs the full test suite and reports.

## Phase 4: verify against my real accounts

Build from the branch. Do not trust the installed `~/.local/bin/jackin`, which predates main. Run `jackin usage refresh`, then every surface, against all 14 connections: claude ×2, codex ×2, muse, opencode, grok, kimi, zai, minimax, mimo, openrouter, antigravity ×2.

Run one Haiku agent per provider group, in parallel. Each checks every rendered value against the raw `jackin usage snapshot --json` data and reports mismatches. Sonnet agents fix what they find.

Known issues to resolve:

- (a) Two antigravity connections (`5fa4e27a…`, `6a4619d1…`) for the same Google account. Decide whether to dedupe or warn, and prevent duplicates at connect time.
- (b) `jackin usage providers` still lists "Gemini Code Assist" on the installed binary. It must stay removed on the new build, and Gemini app (web) remains an info line only.
- (c) Antigravity data is about 18h stale. Find out why refresh does not update it. Keep the decision that live Antigravity sign-in stops before the browser.

Never print tokens or credentials. Save before/after captures of every surface. Open the PR with tailrocks-pr-create, run status, have one Opus agent do the final review, then merge when it is READY.

## Rules for all phases

- Run tailrocks-pr-status before tailrocks-pr-merge.
- Never run `gh pr merge` and never pass `--admin`.
- Stop and ask me before any `--accept-risk`.
- Never rewrite main, force-push, or delete branches.
- Keep the earlier decisions: Gemini Code Assist and Gemini app chat limits stay removed, personal Google AI quotas stay on Antigravity, and there is no new Google Cloud project or OAuth client.
- Report each merge commit, or the exact blockers.
