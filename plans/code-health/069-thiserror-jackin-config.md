# Plan 069: thiserror mid-tranche — jackin-config

## Status

- **Priority**: P2
- **Effort**: M
- **Depends on**: 037
- **Category**: tech-debt
- **Planned at**: 2026-07-12 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Work

- `ConfigError` with structured workspace/mount/version variants + `Message`
- Convert production `bail!`/`ensure!`/`anyhow!` across validation, schema,
  editor, migrations, resolve, mounts, persist, workspaces, planner
- Keep public APIs as `anyhow::Result` (absorb via `.into()`)
- Existing `CollapseError` unchanged

## Done criteria

- [x] Production stringly sites drained
- [x] jackin-config tests green (263)
- [x] R-thiserror-mid-tranches residual closed (all mid-tranche crates done)
