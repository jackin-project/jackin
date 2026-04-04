# Interactive Env Vars and Resolution

**Status**: Deferred — future enhancement to env var system

## Problem

When an agent manifest declares interactive environment variables with `depends_on` chains, the dependent variable's `title` and `default_value` fields are static strings. This means prompts can't reference the value the user just selected in a previous step.

## Why It Matters

- A prompt like "Branch name for project2:" is more contextual than "Branch name for this project:"
- Default values that derive from prior selections (e.g., `feature/${PROJECT_TO_CLONE}`) reduce typing and enforce conventions
- The dependency chain already implies a relationship — interpolation makes it explicit in the UI

## Proposed Design

Allow `${VAR_NAME}` syntax in `title` and `default_value` fields of `[env.*]` entries in `jackin.agent.toml`:

```toml
[env.PROJECT_TO_CLONE]
interactive = true
options = ["project1", "project2"]
title = "Select a project:"

[env.BRANCH_TO_CREATE]
interactive = true
depends_on = ["env.PROJECT_TO_CLONE"]
title = "Branch name for ${PROJECT_TO_CLONE}:"
default_value = "feature/${PROJECT_TO_CLONE}"
```

Interpolation would be limited to `title` and `default_value` fields only. Options arrays remain static.

## Operator-Side Resolution and Overrides

Env vars declared in the agent manifest need to be overridable at multiple levels. Proposed resolution order (highest priority wins):

1. **Workspace config** (`config.toml`):
   ```toml
   [workspaces.my-project.env]
   POSTGRESQL_DB_HOST = "10.0.0.5"
   ```
2. **Operator global config** (`config.toml`):
   ```toml
   [env]
   CLAUDE_ENV = "docker-staging"
   CONTEXT7_API_KEY = "$CONTEXT7_API_KEY"   # host env passthrough
   ```
3. **Agent manifest default** (`jackin.agent.toml`):
   ```toml
   [env.CLAUDE_ENV]
   default_value = "docker"
   ```

The `$VAR` syntax (single dollar, no braces) means "resolve from host environment at launch time," mirroring `docker run -e` behavior.

### Secret Resolution (Future)

The resolution layer should eventually support pluggable backends beyond literal values and host env passthrough:

- 1Password (`op read op://vault/item/field`)
- Other secret managers (Vault, AWS SSM, etc.)

This needs its own design pass — see [1Password Integration](onepassword-integration.md) for initial thinking.

## Related Files

- `src/manifest.rs` — env var declaration parsing
- `src/runtime.rs` — launch-time env var resolution
- `src/config.rs` — operator config and workspace config
- `src/workspace.rs` — workspace-level overrides
