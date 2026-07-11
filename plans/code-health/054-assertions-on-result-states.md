# Plan 054: Adopt `assertions_on_result_states` after mass test conversion

> Residual of plan 011: ~127 `assert!(result.is_ok())` / `assert!(…is_err())`
> sites across tests make day-one deny infeasible. Convert tests then deny.

## Status

- **Priority**: P2
- **Effort**: M
- **Depends on**: plans/code-health/011-lint-strictness-silent-failures.md
- **Category**: tech-debt
- **Planned at**: 2026-07-11 on `chore/rust-code-health-roadmap`

## Steps

1. Inventory all `assert!(….is_ok())` / `assert!(….is_err())` under `crates/`.
2. Convert: `is_ok` → unwrap/expect-with-message in tests (allow-unwrap-in-tests);
   `is_err` → `assert!(matches!(…, Err(_)))` or inspect `err.to_string()`.
3. Set `assertions_on_result_states = "deny"` in root `Cargo.toml`.
4. Verify workspace clippy `-D warnings` and nextest green.
5. Mark plan DONE; update plans/code-health/README.md.

## Done criteria

- [ ] Lint denied workspace-wide
- [ ] Zero bare `assert!(….is_ok())` / `is_err()` that fire the lint in production+tests
- [ ] README status DONE
