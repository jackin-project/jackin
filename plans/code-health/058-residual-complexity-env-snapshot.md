# Plan 058: Residual R1 — complexity floor, env WorkspaceName doctor, snapshot helpers

## Status

- **Priority**: P2
- **Effort**: S-M
- **Depends on**: 038 pattern, 025 test-support
- **Category**: tech-debt
- **Planned at**: 2026-07-11 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Residuals advanced

| Residual ID | Work |
|-------------|------|
| R-complexity-threshold | Census: cognitive max **58**; ratcheted `clippy.toml` 60 → **58** |
| R-038-env-console-tail | Env slice: `run_doctor` / `run_doctor_with_runner` take `&WorkspaceName` |
| R-snapshot-helpers | `jackin_test_support::snapshot::{redact_digit_runs, normalize_snapshot_text}` |

## Done criteria

- [x] `cognitive-complexity-threshold = 58` with workspace clippy green at that floor
- [x] Doctor API typed at workspace name boundary; CLI parses via `WorkspaceName::parse`
- [x] Snapshot helpers exported from `jackin-test-support` with unit tests
- [x] Residual ledger updated (complexity CLOSED; 038 partially advanced; snapshot CLOSED)
