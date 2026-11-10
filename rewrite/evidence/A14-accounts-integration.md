# A14 accounts integration

Base: `e275950ca3f5d82d17dfa49789f9582d3487088b`.

Worktree: `/tmp/jackin-a14-accounts-integrated` on `wt/a14-accounts-integrated`.

## Source units

- The tracked uncommitted role-binding diff from `/tmp/jackin-a14-role-bindings`, excluding `.audit`. Its existing independent approval is `/tmp/jackin-a14-role-bindings/.audit/a14-role-bindings-review-verdict.md`.
- The production, test, and lockfile changes from A11b commit `a935999a424e945d8f9ddd84cd06d49c1b9d73ae`, excluding `.audit`. Its existing independent approval is `/tmp/a11b-independent-review.md`.

No new independent review is claimed here. A fresh integration review follows this unit.

## Conflict decisions

Two files had four textual conflict regions.

- `crates/jackin-tui/tests/console/accounts.rs` keeps the base's `InstanceChoice` and launch imports alongside the role unit's `ConsoleAgent` import. The picker and baseline APIs remain those of the base.
- `crates/jackin-session/src/accounts.rs` keeps the base's no-provider guard in launch cleanup. A11b supplies exact-provider deletion, catalog-derived per-agent removal, and a restore journal entry for every held agent provider.

The agent-keyed schema required fixture alignment in five test files. Codex launch fixtures now bind `Agent::Codex`, not `Agent::Omp`. The OmpHost launch test names `zai-work` explicitly so it tests home injection without relying on account fallback. `save_warning.rs` reads role bindings by `CoreAgent::Codex`.

The integrated code preserves agent-keyed role bindings with exact provider matching. Explicit CLI `--role` requires `--agent`. Workspace bindings remain provider-keyed. Tuple availability and baseline visuals did not change.

## Verification

Focused nextest runs used `rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 nextest` from the worktree root.

- `jackin-core`: 179 passed, 0 skipped.
- `jackin --test cli`: 176 passed, 0 skipped.
- `jackin-session --test accounts --test launch --test plan`: 314 passed, 0 skipped.
- `jackin-tui --test console accounts`: 16 passed, 0 skipped.
- The edited console save-warning test: 1 passed, 0 skipped.
- `jackin-tui --test launch account`: 6 passed, 0 skipped.

The final full gate command was:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 xtask ci --fast
```

It exited 0 in 1047.630 seconds. Nextest ran 2,249 tests in 904.453 seconds, with 2,249 passed and 0 skipped. The run passed fmt, clippy, nextest, deny, shear, comments, layers, pins, containers, errors, baseline, screen, demo-gate, and reuse. The fast gate skipped the docs build as defined by the repository gate.

`git diff --check` passed. No `.audit` artifact is included in the commit.
