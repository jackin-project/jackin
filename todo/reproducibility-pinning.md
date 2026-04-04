# Reproducibility and Provenance Pinning

**Status**: Deferred — implement after agent source trust model

## Problem

The current agent repo flow tracks moving branches (typically `main`) by default. There is no mechanism to pin to a specific commit, verify provenance, or control when updates are pulled.

## Why It Matters

- An agent's behavior can change between runs without the operator's knowledge
- No way to reproduce a previous run's exact environment
- No audit trail of which commit was used for a given session

## Desired Behavior

- Support lockfile-like pinning to commit SHAs in agent config
- Display the resolved commit SHA during agent launch
- Introduce explicit `--update` flag to pull latest (rather than auto-updating)
- Record the commit SHA used in runtime state for audit/debugging
- Integrate with the trust model: trust is granted at a specific commit, `--update` re-evaluates trust

## Related Files

- `src/runtime.rs` — `resolve_agent_source()`, repo checkout logic
- `src/config.rs` — agent config would need commit SHA field
- `src/manifest.rs` — version/provenance metadata
