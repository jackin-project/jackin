# Reproducibility and Provenance Pinning

**Status**: Needs design — brainstorming required before implementation

## Problem

The current agent repo flow tracks moving branches (typically `main`) by default. There is no mechanism to pin to a specific version, verify provenance, or control when updates are pulled.

## Why It Matters

- An agent's behavior can change between runs without the operator's knowledge
- No way to reproduce a previous run's exact environment
- No audit trail of which version was used for a given session

## Desired Behavior

- Pin agents to a specific version so behavior is reproducible across runs
- Display the resolved version during agent launch
- Introduce explicit `--update` flag to pull latest (rather than auto-updating)
- Record the version used in runtime state for audit/debugging
- Integrate with the trust model: trust is granted at a specific version, `--update` re-evaluates trust

## Open Questions

- What is the right unit of pinning — git tags (semantic versions) or raw commit SHAs?
- Should `--update` advance to the latest tag, or to HEAD of the default branch?
- How does this interact with built-in agents vs third-party agents?
- Should the operator be able to pin to a specific version from the CLI (e.g. `--version v1.2.0`)?

## Options

### Option 1: Git Tag Versioning

Agent repos use git tags (e.g. `v1.0.0`, `v1.2.3`) as the release mechanism. jackin' resolves and pins to tags rather than branches.

- First load: resolve the latest tag, record it in config
- Subsequent loads: check out the pinned tag (no auto-update)
- `--update`: fetch tags, resolve latest, re-pin
- Config: `version = "v1.2.3"` in `AgentSource`
- Agents without tags fall back to branch tracking (current behavior)

Pros:
- Familiar model (Cargo, npm, Docker tags)
- Human-readable versions in config and launch output
- Agent authors can publish releases with changelogs
- Natural integration with semver constraints in the future (e.g. `version = "^1.0"`)

Cons:
- Requires agent authors to tag releases — adds process
- Need to decide how to handle repos with no tags
- Tag-based resolution adds complexity (latest tag selection, pre-release handling)

### Option 2: Commit SHA Pinning

Lockfile-style pinning to raw commit SHAs, similar to `flake.lock` or Go module checksums.

- First load: clone/pull, record HEAD SHA in config
- Subsequent loads: fetch + checkout pinned SHA
- `--update`: pull latest, re-pin to new HEAD
- Config: `commit = "a1b2c3d4e5f6..."` in `AgentSource`

Pros:
- Works with any repo, no tagging discipline required
- Exact reproducibility (SHAs are immutable)
- Simple implementation

Cons:
- SHAs are opaque — no sense of "which version am I on" or "how far behind am I"
- No concept of releases, changelogs, or upgrade paths
- Harder for operators to reason about

### Option 3: Hybrid

Support both — prefer tags when available, fall back to commit SHAs.

- If the repo has tags: resolve latest tag, pin by tag name, record the tag's SHA for verification
- If the repo has no tags: fall back to commit SHA pinning
- Config could store both: `version = "v1.2.3"` and `commit = "a1b2c3d..."` (tag + verification SHA)

Pros:
- Best of both worlds
- Graceful degradation for untagged repos

Cons:
- More complex config and resolution logic
- Two code paths to maintain

## Related Files

- `src/config.rs` — `resolve_agent_source()`, `AgentSource` would need version/commit fields
- `src/runtime.rs` — repo checkout logic, calls `resolve_agent_source()`
- `src/manifest.rs` — version/provenance metadata
