# Plan 068: thiserror mid-tranche — jackin-image

## Status

- **Priority**: P2
- **Effort**: M
- **Depends on**: 037
- **Category**: tech-debt
- **Planned at**: 2026-07-12 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Work

- `ImageError` with structured variants (sha, archive member, hooks, json fields,
  SAN) plus `Message` for multi-line remediation text
- Convert agent_binary, binary_artifact, derived_image, capsule_binary production
  `bail!`/`ensure!`/`anyhow!` sites
- Tests may still construct `anyhow::bail!` for mock failures

## Done criteria

- [x] Production stringly sites drained
- [x] jackin-image tests green
- [x] Residual ledger notes image closed
