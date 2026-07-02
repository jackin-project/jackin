# Plan 019: Close the verification-baseline blind spots (doctests + container shell scripts)

> **Executor instructions**: DX/tests plan reconciling the test story. Run every verification command.
> Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- TESTING.md .github/workflows/hygiene.yml .github/workflows/rust-nextest.yml docker/runtime`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: tests / dx
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

The one-command baseline (`cargo nextest run --all-features`) is solid, but has three blind spots:
1. nextest does **not** run doctests, and there are ~15 runnable doc examples under `crates/*/src`; they
   execute only in `hygiene.yml` via `cargo test --workspace --locked` — a command `TESTING.md` explicitly
   **forbids** ("Never `cargo test`"). So doctest coverage rides on a single rule-violating lane.
2. `docker/runtime/entrypoint.sh` (compiled into the image), the `report-hook.sh` agent-status hooks, and
   `scripts/*.sh` have **no** automated test except indirect Docker e2e — a bootstrap-script regression
   surfaces only in a full Docker launch.
3. The `cargo test` in hygiene is decision-drift vs the documented nextest-only invariant.

## Current state

- `TESTING.md:46` — "Never `cargo test` — always `cargo nextest run`."
- `.github/workflows/hygiene.yml:125` — `- run: cargo test --workspace --locked` (the only lane that runs
  doctests; contradicts the doc).
- Shell without automated checks: `docker/runtime/entrypoint.sh`, `docker/runtime/agent-status/hooks/{claude,codex,opencode}/report-hook.sh`, `scripts/*.sh`.

## Scope

**In scope:** `TESTING.md`, `.github/workflows/` (add an explicit doctest step + optional shellcheck),
possibly a `cargo xtask` addition if plan 031 lands first (coordinate). **Out of scope:** rewriting the
doc examples; the Docker e2e lane.

## Steps

### Step 1: Make doctests a first-class, documented step

Decide and record: either (a) add an explicit `cargo test --doc --workspace --locked` step to the main CI
test lane and **reconcile `TESTING.md`** to say "`cargo test --doc` is the one sanctioned `cargo test`
invocation — for doctests only, which nextest can't run", OR (b) if the doc examples aren't worth
maintaining, convert them to `no_run`/```text` and drop the reliance. Recommend (a). Update `TESTING.md` so
the nextest-only rule explicitly carves out `--doc`.

**Verify**: `grep -rn "cargo test --doc" TESTING.md .github/workflows` → ≥1 match;
`cargo test --doc --workspace --locked` → exit 0 (doctests pass locally).

### Step 2: Add a fast shell-script check

Add a `shellcheck` step (installed via mise, consistent with the repo's "tools via mise" rule) over
`docker/runtime/**/*.sh` and `scripts/*.sh` in the hygiene or CI workflow. Fix or `# shellcheck disable`
(with reason) any findings so the step is green. This gives entrypoint/report-hook a fast automated check
short of a full Docker launch.

**Verify**: `shellcheck docker/runtime/entrypoint.sh docker/runtime/agent-status/hooks/*/report-hook.sh scripts/*.sh`
→ exit 0 (after fixes / justified disables).

### Step 3: Remove the rule-violating lane

Replace the bare `cargo test --workspace --locked` in `hygiene.yml:125` with the explicit
`cargo test --doc --workspace --locked` (doctests only) so the workspace's "never `cargo test`" invariant
holds except for the sanctioned doctest carve-out.

**Verify**: `grep -rn "cargo test --workspace" .github/workflows/hygiene.yml` → no matches (only `--doc`).

## Done criteria

- [ ] Doctests run via an explicit, documented `cargo test --doc` step; `TESTING.md` reconciled
- [ ] `hygiene.yml` no longer runs the forbidden bare `cargo test --workspace`
- [ ] A `shellcheck` step covers the runtime/entrypoint + report-hook + scripts shell
- [ ] `actionlint` (via mise) passes on the edited workflows, or YAML parses
- [ ] `plans/README.md` row updated

## STOP conditions

- `shellcheck` surfaces a real bug in `entrypoint.sh` (not just style) — report it separately; a bootstrap
  bug is a finding, not something to silently `disable`.
- Doctests fail because an example is genuinely broken — fix the example if trivial, else report.

## Maintenance notes

- Reviewer: confirm `TESTING.md`'s carve-out is unambiguous so a future contributor doesn't "fix" the
  `cargo test --doc` line back to nextest (which can't run doctests).
- If plan 031 (`cargo xtask ci`) lands, fold the doctest + shellcheck steps into it so there's one entry point.
