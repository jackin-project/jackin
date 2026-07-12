# Plan 067: thiserror mid-tranche — jackin-docker

## Status

- **Priority**: P2
- **Effort**: S-M
- **Depends on**: 037
- **Category**: tech-debt
- **Planned at**: 2026-07-12 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Work

- `DockerError` covering shell timeout/failure shapes, docker build, exec,
  HTTP client/status, prefetch, range, download panic/timeout
- Convert shell_runner, docker_client, net stringly sites
- Public APIs remain `anyhow::Result` at trait edges

## Done criteria

- [x] Typed errors in-tree
- [x] jackin-docker tests green
- [x] Residual ledger updated
