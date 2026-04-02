# Jackin Workspaces And Launch Design

**Date:** 2026-04-03
**Status:** Approved

## Summary

This design adds first-class local workspaces and a fast interactive `jackin launch` flow.

`launch` becomes the human-first entrypoint for quickly starting an agent against either the current directory or a saved workspace. `load` remains the explicit terminal-first command for direct paths, saved workspaces, and fully custom one-off mount setups.

Saved workspaces are explicit local definitions stored in operator config. They describe mounts, `workdir`, and optional agent restrictions. Global mounts continue to apply on top of all workspace modes.

## Goals

- Make `jackin launch` the fastest path for everyday usage from the current directory.
- Support a workspace-first launcher TUI that clearly shows what will be mounted and where.
- Add first-class saved workspaces as local reusable runtime definitions.
- Keep `load` as the explicit non-interactive power-user path.
- Preserve custom one-off mount composition for `load` without forcing that complexity into `launch`.
- Avoid host file ownership issues by aligning runtime user identity with the current host user on macOS and Linux.

## Non-Goals

- In-TUI editing of workspace definitions in v1.
- Inline custom multi-mount authoring inside `launch`.
- A full "control room" dashboard for browsing and managing all running sessions in v1.
- Sharing workspace definitions through agent repos. Workspaces remain local operator config.

## Terminology

- **Agent** — the runtime class being launched, such as `agent-smith`.
- **Workspace** — a local saved definition of mounts, `workdir`, and optional agent affinity.
- **Current directory** — a synthetic workspace derived from `pwd` and used by `launch` and `load` fast paths.
- **Global mounts** — always-on operator-configured mounts that are applied on top of any workspace mode.

## Command Roles

### `jackin launch`

Fast interactive launcher for humans.

- Starts from workspace selection, not custom mount authoring.
- Shows the current directory and saved workspaces.
- Shows mount and `workdir` details before launch.
- Opens a searchable agent picker when more than one eligible agent is available.

### `jackin load`

Explicit non-interactive execution path.

- Supports current-directory and direct-path fast paths.
- Supports saved workspaces.
- Supports explicit custom one-off mount sets and `workdir`.

### `jackin workspace`

First-class local workspace management command group.

- Creates, lists, shows, edits, and removes saved workspaces.

## `jackin launch` User Experience

### Primary Flow

Running:

```bash
jackin launch
```

opens a Ratatui launcher with a workspace list first.

The list always includes:

- `Current directory`
- saved workspaces by name

If `pwd` exactly matches a saved workspace `workdir`, that saved workspace is preselected. The user can still move to `Current directory` to force raw same-path direct mounting.

### Workspace Details Panel

When a workspace is highlighted, the launcher shows:

- `available agents`
- `workdir`
- workspace `mounts`
- `global` mounts in a separate block

This must be explicit and concrete, for example:

```text
available agents: 3
workdir: /Users/donbeave/Projects/chainargos/myproject

mounts:
  /Users/donbeave/Projects/chainargos/myproject -> /Users/donbeave/Projects/chainargos/myproject
  /Users/donbeave/Projects/shared/lib -> /workspace/shared/lib

global:
  ~/.gradle -> /home/claude/.gradle
```

### Agent Selection

After selecting a workspace:

- if exactly one agent is eligible, skip the agent picker and launch immediately
- if multiple agents are eligible, open a searchable agent picker

The agent picker supports live typing to filter results. Typing `chainargos` should immediately narrow visible agents.

### Current Directory Semantics

The `Current directory` launcher entry behaves like a synthetic workspace:

- host `pwd` mounts to the same absolute path in the container
- `workdir` is that same absolute path
- all configured agents are eligible unless future launcher-specific filters are added

This keeps the fast path aligned with the `sbx` behavior that preserves the same in-container path.

## `jackin load` Grammar

`load` remains the explicit command and supports exactly one workspace mode at a time.

### Supported Forms

```bash
jackin load <agent>
jackin load <agent> <path>
jackin load <agent> --workspace <name>
jackin load <agent> -w <name>
jackin load <agent> --mount <src:dst[:ro]>... --workdir <dst>
```

Examples:

```bash
jackin load agent-smith
jackin load agent-smith .
jackin load agent-smith ~/Projects/chainargos/chainargos
jackin load agent-smith --workspace chainargos
jackin load agent-smith -w chainargos
jackin load agent-smith \
  --mount "$PWD/project:/workspace/project" \
  --mount "$PWD/shared:/workspace/shared" \
  --mount "/tmp/cache:/workspace/cache:ro" \
  --workdir /workspace/project
```

### Rules

- `load <agent>` with no path uses current directory mode.
- `load <agent> <path>` uses direct path mode.
- current-directory and direct-path modes mount the host path to the same absolute container path and use that path as `workdir`
- `--workspace` and `-w` resolve a saved workspace definition
- explicit `--mount ... --workdir ...` is the advanced custom mode
- exactly one workspace mode is allowed per invocation
- global mounts are always added on top

## `jackin workspace` Grammar

### Command Set

```bash
jackin workspace add <name> ...
jackin workspace list
jackin workspace show <name>
jackin workspace edit <name> ...
jackin workspace remove <name>
```

### `workspace add`

Saved workspaces are explicit and complete.

```bash
jackin workspace add <name> \
  --workdir <container-path> \
  --mount <src:dst[:ro]>...
```

Optional agent-affinity flags:

```bash
jackin workspace add monorepo \
  --workdir /workspace/project \
  --mount ~/code/project:/workspace/project \
  --mount ~/code/shared:/workspace/shared \
  --allowed-agent agent-smith \
  --allowed-agent chainargos/the-architect \
  --default-agent agent-smith
```

### `workspace list`

Compact listing of saved workspaces, including:

- workspace name
- `workdir`
- mount count
- allowed-agent count or `all`
- default agent when present

### `workspace show`

Detailed inspection of one workspace, including:

- `workdir`
- mounts
- allowed agents
- default agent

### `workspace edit`

Patch-style updates.

Recommended grammar:

```bash
jackin workspace edit <name> --workdir <container-path>
jackin workspace edit <name> --mount <src:dst[:ro]>
jackin workspace edit <name> --remove-destination <dst>
jackin workspace edit <name> --allowed-agent <selector>
jackin workspace edit <name> --remove-allowed-agent <selector>
jackin workspace edit <name> --default-agent <selector>
jackin workspace edit <name> --clear-default-agent
```

Mount edit semantics:

- `--mount` is an upsert keyed by destination path
- if destination already exists, replace that mount
- if destination does not exist, add it
- duplicate destinations in the same edit command are rejected as ambiguous
- `--remove-destination <dst>` removes one existing mount by destination

### `workspace remove`

Deletes a saved workspace definition.

```bash
jackin workspace remove <name>
```

## Config Shape

Saved workspaces live in local config at `~/.config/jackin/config.toml` as a new top-level `workspaces` table.

Example:

```toml
[workspaces.chainargos]
workdir = "/Users/donbeave/Projects/chainargos/chainargos"
default_agent = "agent-smith"
allowed_agents = ["agent-smith", "chainargos/the-architect"]

[[workspaces.chainargos.mounts]]
src = "/Users/donbeave/Projects/chainargos/chainargos"
dst = "/Users/donbeave/Projects/chainargos/chainargos"

[[workspaces.chainargos.mounts]]
src = "/tmp/cache"
dst = "/workspace/cache"
readonly = true
```

### Agent Affinity

For v1:

- `allowed_agents` is optional
- `default_agent` is optional

Rules:

- if `allowed_agents` is absent, all configured agents are eligible
- if `allowed_agents` is present, only those agents may be selected
- if `default_agent` is present and `allowed_agents` is also present, the default must belong to the allowed set
- a single-item `allowed_agents` list is the pinned-agent case

## Validation Rules

Workspace definitions are validated strictly.

### Workspace Requirements

- `workdir` is required for saved workspaces
- at least one workspace mount is required
- each mount `src` must be an absolute host path after expansion
- each mount `dst` must be an absolute container path
- mount destinations must be unique
- `workdir` must be equal to, or inside, one of the workspace mount destinations

### Conflict Rules

- global mounts are applied after workspace mounts
- if a global mount conflicts with a workspace destination, launch/load fails with a clear error rather than silently overriding
- workspace-specific duplicate destinations are rejected during add/edit validation

## Runtime Resolution Model

Jackin should resolve every launch/load into a unified runtime workspace object before starting Docker.

This object may come from:

- the synthetic current-directory workspace
- a direct path
- a saved workspace definition
- an explicit custom mount set from `load`

After resolution, runtime startup logic should be shared as much as possible.

## Host User Identity

To avoid ownership problems on macOS and Linux, Jackin should run workspace-mounted sessions with a runtime user identity aligned to the current host user.

At a minimum, the design requirement is:

- files created inside mounted workspaces must remain writable by the invoking host user without chown repair steps

Exact runtime mechanics can be finalized during implementation, but this behavior is part of the workspace feature contract.

## README Documentation Requirements

`README.md` must clearly explain:

- what `jackin launch` is for
- how `launch` differs from `load`
- why `Current directory` and saved workspaces both appear
- why a saved workspace may be preselected over `Current directory`
- that custom one-off mount composition belongs in `load`, not `launch`

## Example Workflows

### Fastest Path

```bash
cd ~/Projects/chainargos/chainargos
jackin launch
```

Behavior:

- show `Current directory` and saved workspaces
- preselect saved workspace on exact `workdir` match
- show workspace details
- if one agent is eligible, launch immediately after workspace selection
- otherwise open searchable agent picker

### Explicit Saved Workspace

```bash
jackin load agent-smith -w chainargos
```

Behavior:

- uses the saved workspace exactly
- fails if the selected agent is not allowed by that workspace

### Explicit Custom Mode

```bash
jackin load agent-smith \
  --mount "$PWD/project:/workspace/project" \
  --mount "$PWD/shared:/workspace/shared" \
  --mount "/tmp/cache:/workspace/cache:ro" \
  --workdir /workspace/project
```

Behavior:

- uses the explicit mount table
- requires `--workdir`
- does not involve saved workspace resolution

## Rationale

This design keeps each command focused:

- `launch` chooses quickly
- `load` executes explicitly
- `workspace` manages reusable local definitions

It also keeps the beginner path simple while leaving room for advanced explicit runtime composition.
