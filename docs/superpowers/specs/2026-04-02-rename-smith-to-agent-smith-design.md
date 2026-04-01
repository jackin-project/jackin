# Rename smith to agent-smith & Add Identity Support

**Date:** 2026-04-02
**Status:** Approved

## Summary

Rename the default agent class from `smith` to `agent-smith`, rename the repo from `smith` to `agent-smith` (with `jackin-` GitHub prefix convention), change container naming from `agent-{name}` to `jackin-{name}`, simplify Docker network naming, and add an `[identity]` section to `jackin.agent.toml` for display names.

## Motivation

The current `agent-` container prefix doesn't work for agent names like "Neo", "Morpheus", "The Architect", "The Oracle". The `jackin-` prefix provides brand recognition across all jackin-managed containers and repos while keeping class names flexible.

The `[identity]` section allows agents to declare a human-friendly display name independent of the class selector, supporting the vision of a marketplace of pre-configured, specialized agents.

Kubernetes is planned as a future platform target (after Docker experience is mature).

## Naming Convention

| Concept | Example: agent-smith | Example: neo | Namespaced: chainargos/the-architect |
|---------|---------------------|-------------|--------------------------------------|
| Class selector | `agent-smith` | `neo` | `chainargos/the-architect` |
| GitHub repo | `jackin-agent-smith` | `jackin-neo` | `chainargos/jackin-the-architect` |
| Git URL in config | `git@github.com:donbeave/jackin-agent-smith.git` | `git@github.com:donbeave/jackin-neo.git` | `git@github.com:chainargos/jackin-the-architect.git` |
| Container (primary) | `jackin-agent-smith` | `jackin-neo` | `jackin-chainargos-the-architect` |
| Container (clone) | `jackin-agent-smith-clone-1` | `jackin-neo-clone-1` | `jackin-chainargos-the-architect-clone-1` |
| DinD sidecar | `jackin-agent-smith-dind` | `jackin-neo-dind` | `jackin-chainargos-the-architect-dind` |
| Docker network | `jackin-agent-smith-net` | `jackin-neo-net` | `jackin-chainargos-the-architect-net` |
| Docker image | `jackin-agent-smith` | `jackin-neo` | `jackin-chainargos-the-architect` |
| Display name | "Agent Smith" | "Neo" | "The Architect" |
| Data dir | `~/.jackin/data/jackin-agent-smith/` | `~/.jackin/data/jackin-neo/` | `~/.jackin/data/jackin-chainargos-the-architect/` |

## Code Changes — jackin

### instance.rs — Container naming

Change `primary_container_name()` prefix from `agent-` to `jackin-`:

```rust
pub fn primary_container_name(selector: &ClassSelector) -> String {
    match &selector.namespace {
        Some(namespace) => format!("jackin-{namespace}-{}", selector.name),
        None => format!("jackin-{}", selector.name),
    }
}
```

Clone and family matching logic unchanged (uses `primary_container_name` output).

### runtime.rs — Network naming

Simplify from `jackin-{container_name}-net` to `{container_name}-net` to avoid double `jackin-` prefix:

```rust
let network_name = format!("{container_name}-net");
```

### runtime.rs — Image naming

`image_name()` already produces `jackin-{name}` which now matches the container pattern. No logic change needed (output already correct).

### config.rs — Default agent

```rust
agents.insert("agent-smith".to_string(), AgentSource {
    git: "git@github.com:donbeave/jackin-agent-smith.git".to_string(),
});
```

### manifest.rs — Add Identity

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct AgentManifest {
    pub dockerfile: String,
    pub claude: ClaudeConfig,
    #[serde(default)]
    pub identity: Option<IdentityConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IdentityConfig {
    pub name: String,
}
```

The `identity` field is optional. When absent, the class selector name is used as the display name.

### Tests

All test references updated: `"smith"` class selector becomes `"agent-smith"`, container names from `"agent-smith"` to `"jackin-agent-smith"`, etc. Approximately 58+ occurrences across `runtime.rs`, `cli.rs`, `instance.rs`, `selector.rs`.

## Code Changes — agent-smith repo (formerly smith)

### Directory rename

`smith/` → `agent-smith/` (GitHub repo rename is a separate manual step)

### jackin.agent.toml

```toml
dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = [
  "code-review@claude-plugins-official",
  "feature-dev@claude-plugins-official",
]
```

### README.md

- Title: `# smith` → `# agent-smith`
- All mentions of `smith` → `agent-smith`
- Command: `jackin load smith` → `jackin load agent-smith`

### Dockerfile

No changes needed — does not reference agent name.

## Documentation Updates — jackin

### README.md

- All command examples updated to use `agent-smith` class and `jackin-agent-smith` container
- Note about `jackin-` repo naming convention added
- Note about Kubernetes as planned future platform (Docker-only for now)
- Section about `[identity]` in `jackin.agent.toml`

### docs/superpowers/specs/2026-04-01-jackin-v1-design.md

- Naming table updated to reflect new convention

### CLAUDE.md / AGENTS.md

- Any references to `smith` updated

## Breaking Changes

- Container names change from `agent-{name}` to `jackin-{name}`
- Default class key changes from `smith` to `agent-smith`
- Default git URL changes to `jackin-agent-smith`
- Network names change from `jackin-{container}-net` to `{container}-net`
- Existing running containers with old names won't be recognized

This is acceptable — the project is pre-v1 / PoC stage.
