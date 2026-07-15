# Plan 022: Root CLI decomposition — handler splits, TTY fallback, `launch` deprecation, module contracts

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin/src/app.rs crates/jackin/src/app/ crates/jackin/src/cli/status.rs crates/jackin-capsule/src/session.rs`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: M (fallback+warning) / L (handler splits — sliceable)
- **Risk**: MED (CLI behavior must not change except the two additive behaviors)
- **Depends on**: none
- **Category**: tech-debt + feature-completion
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Ownership item 7 requires splitting "the large `load`, Claude-token, and workspace command handlers around setup, dispatch, and effect boundaries without changing CLI behavior", with each `too_many_lines` expectation removed "only after its narrow behavioral coverage remains green", plus "explicit characterization for the bare-command TTY-capability fallback and `launch` deprecation warning in `app.rs`". The twist found in audit: those two behaviors are NOT YET IMPLEMENTED — `app.rs:108-114`'s comment says they "land in a follow-up commit" that never landed, and bare `jackin` unconditionally routes to the console handler with no TTY check. Item 6 additionally wants every substantial module to carry a leading `//!` contract; the audit's notable miss is `crates/jackin-capsule/src/session.rs` (1689 lines, plain `//` comments).

## Current state

- `crates/jackin/src/app.rs:108-114`:

```rust
    // Resolve the subcommand. Bare `jackin` currently routes to the same
    // console handler as `jackin console`; the TTY-capability fallback and
    // the deprecation warning for `launch` land in a follow-up commit.
    let command = match cli.command {
        Some(cmd) => cmd,
        None => Command::Console(cli.console_args),
    };
```

  No `launch`-deprecation code exists (`grep 'launch.*deprecat' crates/jackin/src` → nothing).
- `too_many_lines` expectations on handlers: `crates/jackin/src/app/load_cmd.rs:163` (714 lines), `app/workspace_cmd.rs:18` (662), `app/token_cmd.rs:13` (632), `app/config_cmd.rs:61`, `cli/status.rs:93,346`.
- Docs contract for bare `jackin`: `docs/content/docs/commands/console.mdx` is the ONLY page explaining the bare shortcut (docs/CLAUDE.md rule) — the fallback behavior must be reflected there when it lands.
- Deprecation policy: `DEPRECATED.md` tracks active deprecations; a `launch` deprecation warning entry belongs there.
- Docs-command drift gate: `crates/jackin/tests/docs_commands.rs` parses every fenced `jackin …` invocation in docs against the clap tree — CLI surface changes must keep it green.
- Module contracts: `session.rs:4-5` uses `//` not `//!`; sampled peers carry `//!`. The structural check is deliberately deferred by the roadmap ("add a structural check only after it can distinguish meaningful contracts from boilerplate") — this plan only fixes the misses.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Root-crate tests | `cargo nextest run -p jackin` | pass |
| Docs-command gate | `cargo nextest run -p jackin -E 'test(docs_commands)'` | pass |
| Lint | `cargo clippy -p jackin --all-targets -- -D warnings` | exit 0 |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `crates/jackin/src/app.rs` + `app/` handler files + their tests; `cli/status.rs`; the console command docs page (bare-command behavior sentence) + `DEPRECATED.md` (launch warning entry); `//!` contract for `session.rs` and any other misses found by a quick sweep (`for f in $(find crates -name '*.rs' -path '*/src/*' -size +40k); do head -1 $f | grep -L '^//!' … ` — scriptable; keep the sweep to substantial modules).

**Out of scope**: console lifecycle changes; removing the `launch` command (warning only — removal is a separate deprecation decision); the structural `//!` check (deferred by roadmap); daemon internals (017).

## Git workflow

Branch `feat/cli-tty-fallback` (behaviors) then `refactor/cli-handler-split` (splits); Conventional Commits; `git commit -s`; push per commit.

## Steps

### Step 1: Implement + characterize the TTY fallback

Implement: bare `jackin` on a non-TTY stdout/stdin (define capability check per what the console already requires to run — find the console's own TTY probe, reuse it) falls back to help/error output instead of attempting the console. Read the operator-visible intent from `docs/content/docs/commands/console.mdx` first; behavior must match what that page promises (or the page is updated in the same PR). Characterization tests in `app/tests.rs`: bare command with TTY → console route; without TTY → fallback (assert exact user-visible output shape).

**Verify**: `cargo nextest run -p jackin -E 'test(/app::tests/)'` → new tests pass; docs gate green.

### Step 2: Implement + characterize the `launch` deprecation warning

Emit a deprecation warning when `Command::Launch` is invoked (locate the variant; the roadmap and app.rs comment call it `launch`), pointing at the replacement (`jackin load` — confirm against DEPRECATED.md/docs). Add the DEPRECATED.md row. Characterization: invoking launch prints the warning once to stderr and proceeds unchanged.

**Verify**: tests pass; `DEPRECATED.md` row present; docs gates green.

### Step 3: Handler splits (repeatable slice; one handler per slice)

For each of `load_cmd.rs`, `token_cmd.rs`, `workspace_cmd.rs`, `config_cmd.rs`, `cli/status.rs`: (a) confirm/extend narrow behavioral coverage for the handler's observable behavior (exit codes, output shape, side-effect dispatch — mine existing tests first); (b) split along setup (arg/config resolution) / dispatch (choosing the operation) / effect (doing it) boundaries into functions or submodules; (c) remove the `too_many_lines` expectation ONLY when the slice's tests are green without it.

**Verify per slice**: `cargo nextest run -p jackin` → pass; the slice's `#[expect(clippy::too_many_lines…)]` deleted; `cargo clippy -p jackin --all-targets -- -D warnings` → exit 0.

### Step 4: Module `//!` contracts

Convert `session.rs`'s header to a `//!` contract (what it owns, invariants, boundary — follow the pattern in `crates/jackin-runtime/src/runtime/attach.rs` or `crates/jackin-term/src/grid.rs`); sweep other substantial modules missing `//!` and fix the misses (list them in the PR).

**Verify**: `cargo nextest run -p jackin-capsule` → pass (comment-only); sweep list in PR.

## Test plan

Steps 1–2 add the two characterization suites (these are the roadmap's named asks); step 3 extends handler coverage before each split. Model CLI tests on existing `app/tests.rs` cases.

## Done criteria

- [x] TTY fallback implemented + characterized; console docs page consistent
- [x] `launch` deprecation: STOP — command already removed (drift); documented, no warning shipped
- [x] All five `too_many_lines` expectations removed, each after green narrow coverage
- [x] `session.rs` (+ sweep misses) carry `//!` contracts
- [x] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- The intended TTY-fallback UX is genuinely ambiguous (docs promise nothing; multiple defensible behaviors) — implement nothing; present the options (help text vs error vs auto-`load`) to the operator.
- A handler split forces observable output changes (ordering of prints, exit codes) — stop that slice; behavior preservation is the acceptance bar.
- `launch` turns out to already NOT exist as a command (drift) — the deprecation half is moot; report.

## Maintenance notes

- Future handlers follow setup/dispatch/effect from birth; `too_many_lines` expectations on new code should be rejected in review.
- If the bare-command fallback changes again, `docs/content/docs/commands/console.mdx` is the single page that documents it (docs rule).

## Execution notes

- **Launch deprecation (STOP)**: `Command::Launch` / `jackin launch` no longer exists in the CLI (`app.rs` note: deprecation N/A). No stderr warning and no DEPRECATED.md row for a removed command — reporting only, per plan STOP.
- **TTY fallback**: implemented + characterized in `cli/dispatch` tests (bare TTY → console; non-TTY → silent help; explicit `console` without TTY errors).
- **Handler splits (too_many_lines)**: removed `#[expect(clippy::too_many_lines)]` from `token_cmd`, `config_cmd`, `workspace_cmd`, and `load_cmd::handle_console` by extracting setup/dispatch/effect helpers (token setup/rotate/revoke/doctor; config mount/trust/auth/env/git; workspace create/list/show/edit/prune/remove/env with edit prep/summary/apply; console outcome dispatch via `ConsoleLaunchCtx`). `cli/status.rs` had no remaining expects at execution time.
- `session.rs` `//!` contract and module sweep left as previously landed on this branch.
