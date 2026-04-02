# Logo, Global Mounts, Debug Output & UX Polish

**Date:** 2026-04-02
**Status:** Draft

## Overview

Four improvements to the `jackin load` lifecycle:

1. Agent repo logo display
2. Configurable Docker mounts with global, wildcard, and per-agent scoping
3. Debug flag wiring and suppressed git noise
4. Deploying message timing fix

## 1. Logo Support

Agent repos may include a `logo.txt` file at the repo root. During `jackin load`, after the Matrix intro and before the config table, jackin reads and displays it in `MATRIX_GREEN` with 2-space indent per line.

- If `logo.txt` is missing or empty, skip silently
- No manifest declaration needed — convention over configuration
- No size validation — trust the agent repo author

### Display sequence (updated)

```
Matrix intro (if --no-intro not set)
  -> clear screen
  -> logo (if logo.txt exists)
  -> config table
  -> steps 1..5
  -> "Deploying {name} into the Matrix..."
  -> agent session
  -> outro
```

## 2. Configurable Docker Mounts

### Config format

In `~/.config/jackin/config.toml`:

```toml
# Global — applies to all agents
[docker.mounts]
gradle-cache = { src = "~/.gradle/caches", dst = "/home/claude/.gradle/caches" }
gradle-wrapper = { src = "~/.gradle/wrapper", dst = "/home/claude/.gradle/wrapper", readonly = true }

# Wildcard — applies to all agents from namespace "chainargos"
[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "~/.chainargos/secrets", dst = "/secrets", readonly = true }

# Exact — applies only to chainargos/agent-brown
[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "~/.chainargos/brown", dst = "/config", readonly = true }
```

### Mount fields

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `src` | yes | — | Host path. Supports `~` expansion. |
| `dst` | yes | — | Container path. Must be absolute. |
| `readonly` | no | `false` | If `true`, mount is read-only (`:ro`). |

### Matching and precedence

When loading an agent, mounts are collected from all matching scopes:

1. Global (`[docker.mounts]`)
2. Wildcard (`[docker.mounts."namespace/*"]`) if the agent's namespace matches
3. Exact (`[docker.mounts."namespace/name"]`) if the agent's full selector matches

All matching mounts are collected. If the same mount **name** appears in multiple scopes, the most specific scope wins (exact > wildcard > global).

### Validation

At load time, before building the Docker image:

- `src` (after `~` expansion) must exist on the host. If not, `jackin load` fails with a clear error message naming the mount and the missing path.
- `dst` must be an absolute path.
- Duplicate `dst` values across all resolved mounts is an error.

### CLI management

```
jackin config mount add <name> --src <path> --dst <path> [--readonly] [--scope <pattern>]
jackin config mount remove <name> [--scope <pattern>]
jackin config mount list
```

Examples:

```bash
# Add a global mount
jackin config mount add gradle-cache --src ~/.gradle/caches --dst /home/claude/.gradle/caches

# Add a scoped mount
jackin config mount add chainargos-secrets --src ~/.chainargos/secrets --dst /secrets --readonly --scope "chainargos/*"

# Remove by name (global scope)
jackin config mount remove gradle-cache

# Remove scoped mount
jackin config mount remove chainargos-secrets --scope "chainargos/*"

# List all configured mounts
jackin config mount list
```

When no `--scope` is given, the command operates on global mounts.

### Config struct changes

`AppConfig` gains a new `docker` field:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerConfig {
    #[serde(default)]
    pub mounts: BTreeMap<String, BTreeMap<String, MountConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
}
```

The outer `BTreeMap` key is the scope: empty string `""` for global, `"chainargos/*"` for wildcard, `"chainargos/agent-brown"` for exact. The inner `BTreeMap` key is the mount name.

## 3. Debug Output & Suppressed Git Noise

### Debug flag

When `--debug` is set on `jackin load`:

- Docker build runs with inherited stdout/stderr (user sees full build log)
- Docker network creation output is shown
- DinD start and readiness polling output is shown

When not set (default), these commands capture and suppress output (current behavior).

### Suppressed git pull

The `git pull --ff-only` during repo sync currently leaks output like "Already up to date." to the terminal. Fix: capture stdout/stderr from git clone/pull. Only surface output on error.

### New step: "Resolving agent identity"

Add a new first step before "Building Docker image" that covers the git clone/pull operation. This replaces the raw git output with a clean shimmer step.

Updated step sequence:

```
1. Resolving agent identity     (git clone or pull)
2. Building Docker image
3. Creating Docker network
4. Starting Docker-in-Docker container
5. Mounting volumes
   -> "Deploying {name} into the Matrix..."
```

## 4. Deploying Message Timing

The "Deploying {name} into the Matrix..." message currently pauses 800ms then immediately clears the screen. Increase the pause to 1500ms so the user can read it.

## Files to modify

| File | Changes |
|------|---------|
| `src/config.rs` | Add `DockerConfig`, `MountConfig`, mount resolution logic |
| `src/runtime.rs` | Logo display, new step 1, debug flag wiring, deploying timing, capture git output |
| `src/tui.rs` | Add `print_logo(lines, color)` function |
| `src/cli.rs` | Add `config mount` subcommands |
| `src/lib.rs` | Route `config mount` commands |

## Out of scope

- Branch display in config table (deferred to workspace projects)
- Agent data path in config table
- TLS-enabled DinD (deferred to Kubernetes support)
- Profile system
