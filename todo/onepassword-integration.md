# 1Password Integration for Agent Secrets

**Status**: Deferred — needs design work

## Problem

Jackin does not yet have a first-class way to integrate with 1Password for secrets an agent may need at runtime, such as API tokens, cloud credentials, or project-specific environment values. Today the operator has to manage these manually through mounts, shell setup, or ad-hoc environment injection.

## Why It Matters

- 1Password is a common source of truth for developer and team secrets
- Manual secret handling is error-prone and easy to make inconsistent across agents and workspaces
- A first-class integration could reduce the need to mount broad host directories just to make credentials available
- The operator should be able to decide which secrets enter which agent environment, matching jackin's boundary model

## Options

1. **1Password CLI passthrough**: Install or expose the `op` CLI in agent classes and let operators authenticate inside the container. Simple and flexible, but pushes too much setup onto each agent session.

2. **Workspace-managed secret references**: Let workspaces or global config declare references to 1Password items, then resolve them at launch time into files, mounts, or environment variables.

3. **Ephemeral runtime injection**: Resolve 1Password secrets only at launch and inject them into the running container without persisting them into `~/.jackin/data`.

4. **Read-only secret mount generation**: Materialize selected 1Password secrets into temporary files and mount them read-only into the container.

## Related Files

- `src/config.rs` — config format would need secret references
- `src/runtime.rs` — launch-time resolution
- `src/workspace.rs` — workspace config integration
