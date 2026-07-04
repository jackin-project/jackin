# Plan 041: Document the shipped `grok` agent and the `backend` config across user surfaces

> **Executor instructions**: Docs-parity fix for shipped-but-undocumented capability. Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-core/src/agent.rs crates/jackin/src/cli/role.rs 'docs/content/docs/(public)/commands/load.mdx' crates/jackin-config/src/schema.rs docs/content/docs/reference/runtime/configuration.mdx README.md`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: docs (DOCS-07 + DIRECTION-04)
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`grok` is a **fully wired** agent runtime (`Agent` enum + `ALL`, config auth, roadmap says all six runtimes
"ship today") but it's **undersold**: `jackin load --help` and the `load` command docs list only
"claude, codex, amp, kimi, or opencode" — omitting Grok — while `prewarm.mdx` correctly lists all six, so
the flagship `load` surface is the outlier. Similarly, the `RuntimeConfig.default_backend` / per-workspace
`backend` selector (`docker`|`apple-container`) is live and read, but the config reference documents no
`backend` field. Operators reading `--help`/the load page can't discover `--agent grok`, and the config
reference never mentions the backend selector — shipped capability that looks unsupported. Cheapest possible
"feature": the capability exists; only discovery is missing.

## Current state

- `crates/jackin-core/src/agent.rs:20-39` — `Agent` enum + `ALL` include `Grok` (six runtimes).
- `crates/jackin-config/src/app_config.rs:42` — `grok` auth.
- `crates/jackin/src/cli/role.rs:53-55` — `--agent` help: "claude, codex, amp, kimi, or opencode" (**no grok**).
- `docs/content/docs/(public)/commands/load.mdx:47` — same five-agent list (no grok).
- `docs/content/docs/(public)/commands/prewarm.mdx:36` — correctly lists all six.
- `crates/jackin-config/src/schema.rs:159-182` — `RuntimeConfig.default_backend` + per-workspace
  `RuntimeConfig.backend` (`docker`|`apple-container`), read at `mounts.rs:196-198`.
- `docs/content/docs/reference/runtime/configuration.mdx` — no `backend` entry.
- README `:12` lists five agents (no Grok).

## Scope

**In scope:** `crates/jackin/src/cli/role.rs` (help text), `docs/.../commands/load.mdx`, `README.md`,
`docs/.../reference/runtime/configuration.mdx` (backend field). **Out of scope:** the agent/backend *logic*
(already shipped); the Apple backend's completeness (plan 024).

## Steps

### Step 1: Add `grok` to the `--agent` help and the load docs

- `crates/jackin/src/cli/role.rs:53-55` — add `grok` to the `--agent` help string ("claude, codex, amp,
  kimi, grok, or opencode"). Better: derive the list from `Agent::ALL` so it can't drift again (if feasible
  without churn — see Step 3).
- `docs/content/docs/(public)/commands/load.mdx:47` — add `grok` to the agent list, matching `prewarm.mdx:36`.
- `README.md:12` — add Grok to the agents listed.

**Verify**: `grep -rn "grok" crates/jackin/src/cli/role.rs 'docs/content/docs/(public)/commands/load.mdx' README.md`
→ ≥1 match each; `cargo run --bin jackin -- load --help 2>&1 | grep -i grok` → shows grok.

### Step 2: Document the `backend` config field

Add a `[runtime].backend` / `default_backend` entry to
`docs/content/docs/reference/runtime/configuration.mdx` describing the `docker` (default) and
`apple-container` values. Mark `apple-container` as **experimental / Phase 0** (per plan 024 / the roadmap)
so operators know its status.

**Verify**: `grep -rn "backend\|apple-container" docs/content/docs/reference/runtime/configuration.mdx` → ≥1 match.

### Step 3 (recommended): make the agent list single-source

To prevent this drift recurring, derive the `--agent` help and any doc-check from `Agent::ALL` where
practical (a clap `value_parser` already exists — `parse_agent`). If a help-text derivation is clean, do it;
if it would churn widely, at minimum add a test asserting the help string contains every `Agent::ALL`
variant, so a future new agent can't be omitted silently.

**Verify**: `cargo nextest run -p jackin -E 'test(/agent_help|all_agents/)'` → passes (if you added the test).

## Done criteria

- [x] `--agent` help, `load.mdx`, and `README.md` all list `grok`
- [x] `configuration.mdx` documents the `backend`/`default_backend` field (apple-container marked experimental)
- [x] `jackin load --help` output includes grok
- [x] Either the help is derived from `Agent::ALL`, or a test asserts help ⊇ `Agent::ALL`
- [x] `plans/README.md` row updated

## STOP conditions

- Grok is actually gated/incomplete behind a feature (not truly shipped) — verify it works end-to-end before
  advertising it; if it's not really ready, report rather than documenting a non-functional agent.

## Maintenance notes

- Root cause is a hand-maintained agent list that drifted from `Agent::ALL`; the Step-3 single-source (or
  test) is the durable fix. A reviewer should require new agents to update all surfaces (or rely on the derivation).
- The load/prewarm agent lists should always match — they diverged here.
