# DinD Hostname Env Var Design

## Summary

Add a jackin-managed runtime environment variable, `JACKIN_DIND_HOSTNAME`, so agents can reach services started inside the Docker-in-Docker sidecar without parsing `DOCKER_HOST`.

## Goals

- Expose the DinD sidecar hostname directly to the agent runtime.
- Keep runtime-owned metadata env vars reserved from `jackin.agent.toml`.
- Document what jackin-managed env vars mean for agent authors.
- Resolve the existing TODO item and align roadmap/docs with shipped behavior.

## Non-Goals

- No new port env vars such as `JACKIN_DIND_PORT`.
- No broader networking refactor.
- No domain-specific service env vars like `POSTGRESQL_DB_HOST`.

## Design

`src/runtime.rs` already derives the DinD sidecar name when constructing `DOCKER_HOST=tcp://{dind}:2375`. The runtime should reuse that same value to inject `JACKIN_DIND_HOSTNAME={dind}` into the agent container alongside `DOCKER_HOST`, `GIT_AUTHOR_NAME`, and `GIT_AUTHOR_EMAIL`.

`src/manifest.rs` should treat `JACKIN_DIND_HOSTNAME` the same way it treats `JACKIN_CLAUDE_ENV`: reserved jackin runtime metadata that cannot be declared under `[env]` in `jackin.agent.toml`.

The runtime injection point should include a concise comment explaining that these `JACKIN_*` variables are jackin-owned runtime metadata, not agent-defined manifest values.

## Documentation

`docs/pages/developing/agent-manifest.mdx` should document both reserved runtime-managed env vars and their meaning:

- `JACKIN_CLAUDE_ENV=jackin` marks that the process is running inside a jackin-managed runtime.
- `JACKIN_DIND_HOSTNAME=<container>-dind` is the network hostname agents should use to reach services launched inside the DinD sidecar.

The roadmap and TODO index should move this item from planned/open to resolved/completed.

## Testing

- Add a runtime regression test asserting the generated `docker run` command includes `-e JACKIN_DIND_HOSTNAME=...`.
- Add a manifest validation test asserting `JACKIN_DIND_HOSTNAME` is rejected under `[env]`.

## Files

- `src/runtime.rs`
- `src/manifest.rs`
- `docs/pages/developing/agent-manifest.mdx`
- `docs/pages/reference/architecture.mdx`
- `docs/pages/reference/roadmap.mdx`
- `todo/dind-hostname-env-var.md`
- `TODO.md`
