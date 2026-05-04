# Deprecated

Tracker for deprecated APIs, CLIs, config values, and usage patterns
that are still supported for backwards compatibility but should
**eventually be removed**. Periodically review this file and prune
entries whose removal is safe.

When you deprecate something, add it here in the **same commit** that
introduces the deprecation. See [RULES.md](RULES.md#deprecations) for
the rule.

## How to read this file

Each entry includes:

- **Item** — what is deprecated (CLI verb, type, function, config field, value, behavior).
- **Type** — `cli` / `api` / `config` / `behavior`.
- **Deprecated since** — the date or version the deprecation landed.
- **Replacement** — what to use instead.
- **Remove when** — the trigger or target for removal (a date, a version,
  or a condition like "after CI no longer sees the warning for two
  consecutive releases").
- **Where** — the source files / docs that implement the deprecation,
  so removal is straightforward.

## Active deprecations

### `jackin launch` CLI verb

- **Type:** cli
- **Deprecated since:** 2026-04-23 (PR #166)
- **Replacement:** bare `jackin` (interactive terminal) or
  [`jackin console`](docs/src/content/docs/commands/console.mdx)
  (explicit; works on any terminal that can host a TUI).
- **Behavior today:** the binary still accepts `jackin launch [...]`
  and dispatches to the same console handler, but prints
  `warning: \`jackin launch\` is deprecated and will be removed in a
  future release; use \`jackin\` or \`jackin console\` instead` on
  stderr.
- **Remove when:** no operator-reported usage of `jackin launch` for
  two consecutive releases AND the docs deprecation page has been
  live for ≥ 90 days.
- **Where:**
  - `src/cli/dispatch.rs::LAUNCH_DEPRECATION_WARNING` — warning string.
  - `src/cli/dispatch.rs::Action::{RunConsole, ErrorNotTtyCapable}` —
    `deprecated_alias: bool` field; remove the field and its emission
    sites in `src/main.rs`.
  - `src/cli/agent.rs::LaunchArgs` — type alias for `ConsoleArgs`.
  - `src/cli/root.rs::Command::Launch` — clap variant wrapping
    `LaunchArgs`.
  - `docs/src/content/docs/commands/launch.mdx` — deprecation page;
    delete the page and remove its sidebar entry from
    `docs/astro.config.ts`.
  - `tests/bare_jackin_fallback.rs` — three tests pin the deprecation
    warning on stderr; delete them at removal time.

### `auth_forward = "copy"` config value

- **Type:** config
- **Deprecated since:** 2026-04-23 (auth-sync default)
- **Replacement:** `auth_forward = "sync"` (the new default).
- **Behavior today:** any `config.toml` declaring `auth_forward = "copy"`
  is silently migrated to `"sync"` on the next config write, with a
  one-line deprecation notice (`migrated auth_forward "copy" → "sync"
  in {path} (copy is deprecated)`) printed via `tui::deprecation_warning`.
- **Remove when:** rewriting any config no longer encounters the
  legacy value across collected operator telemetry / support reports
  for one full release cycle.
- **Where:**
  - `src/config/persist.rs` — migration on save.
  - `src/app/mod.rs::parse_auth_forward_mode_from_cli` — the
    `was_deprecated` return tuple element.
  - `src/config/mod.rs::AuthForwardMode::from_str` — accepts `"copy"`
    as an alias.
  - `src/tui/output.rs::deprecation_warning` — printer used by the
    migration; can stay if other deprecations need it.

### Bare `op://...` strings as env values (runtime resolution)

- **Type:** behavior
- **Deprecated since:** 2026-04-27 (PR #193)
- **Replacement:** use the structured `EnvValue::OpRef` inline-table
  form `{ op = "op://uuid/uuid/uuid", path = "Vault/Item/Field" }`.
  The operator-facing path is `jackin workspace env set <NAME>
  "op://..."` (the CLI auto-resolves to the pinned form) or via the
  TUI 1Password picker keystroke.
- **Behavior today:** workspaces and config TOMLs that contain a
  scalar `op://...` string as an env value **no longer** have it
  resolved at launch via `op read`. The string is passed to the
  container as a literal. No error is emitted; the row renders
  without the `[op]` marker in the TUI (the visual cue that
  re-picking is needed).
- **Remove when:** N/A — the legacy strings still load and flow
  through without error; only the silent runtime resolution behavior
  has been removed. There is no planned hard-removal date; the TUI
  `[op]`-marker cue and the `jackin workspace env set` auto-resolve
  path are the migration surfaces.
- **Where:**
  - `src/operator_env.rs::resolve_env_value` — variant dispatch:
    only `EnvValue::OpRef` triggers `op read`. `Plain` arm falls
    through to `dispatch_plain` which never calls the 1Password CLI.
  - `src/console/manager/render/editor.rs::render_secrets_key_line`
    — plain rows (including legacy bare `op://...`) render without
    `[op]` marker, signalling the need to re-pick.

### `/home/claude/...` mount destinations

- **Type:** config / behavior
- **Deprecated since:** 2026-05-01 (multi-agent slice)
- **Replacement:** `/home/agent/...` for workspace and global mount
  destinations, plus any agent Dockerfile paths that reference the
  runtime user home.
- **Behavior today:** jackin now runs containers as the `agent` user
  with home directory `/home/agent`. Existing `/home/claude/...`
  mount destinations point at a path that is no longer created by the
  construct image.
- **Remove when:** after one release cycle without operator reports
  that legacy `/home/claude` paths are still required.
- **Where:**
  - `docker/construct/Dockerfile` — creates the `agent` user and
    `/home/agent` state directories.
  - `src/derived_image.rs` — rewrites generated Dockerfile paths to
    `/home/agent`.
  - `src/runtime/launch.rs` — mounts agent state under
    `/home/agent`.
  - `src/cli/config.rs` and docs examples — current examples use
    `/home/agent`.

### Top-level state files in `~/.jackin/data/<container>/`

- **Type:** config / behavior
- **Deprecated since:** 2026-05-03 (PR #210)
- **Replacement:** state files now live under per-agent subdirectories.
  Claude state is grouped under `<container>/claude/` and Codex state
  under `<container>/codex/`.
- **Behavior today:** jackin no longer reads or writes the legacy
  flat layout. Existing per-container directories from before this
  change retain their old files but jackin will not consult them —
  any cached Claude session history at the old paths is invisible to
  the new layout. Operators with active state can either eject and
  re-launch (clean re-init) or move files manually:
    `<container>/.claude/ → <container>/claude/state/`
    `<container>/.claude.json → <container>/claude/account.json`
    `<container>/.jackin/plugins.json → <container>/claude/plugins.json`
    `<container>/config.toml → <container>/codex/config.toml`
- **Remove when:** never — this is a hard break, not a transitional
  deprecation. The entry exists so the rationale is recorded.
- **Where:**
  - `src/instance/mod.rs::prepare` — constructs the new per-agent
    paths.
  - `src/runtime/launch.rs::agent_mounts` — maps host paths to
    container mount destinations (container paths unchanged).
  - `docs/src/content/docs/reference/architecture.mdx` — layout
    diagram reflects the new shape.

### `--no-workdir-mount` flag on `workspace create`

- **Type:** cli
- **Deprecated since:** 2026-05-04 (PR #213)
- **Replacement:** `workspace create` requires at least one `--mount` flag.
  The workdir is not auto-mounted.
- **Behavior today:** Passing `--no-workdir-mount` to `workspace create` is
  accepted, prints a one-line stderr warning, and otherwise has no effect.
  The flag is hidden from `--help`. `workspace edit --no-workdir-mount` is
  unaffected — it still removes an existing same-path mount from the
  workspace config.
- **Remove when:** After a deprecation window; delete the `no_workdir_mount`
  field from `WorkspaceCreate` in `src/cli/workspace.rs` and the discard
  shim in `src/app/mod.rs` (`let _ = no_workdir_mount;`).
- **Where:**
  - `src/cli/workspace.rs` — `WorkspaceCreate::no_workdir_mount` field,
    `hide = true`.
  - `src/app/mod.rs` — emits the deprecation warning then ignores the flag.

## How to add an entry

When you deprecate something, append a new section to **Active
deprecations** above. Use the same field structure. If the deprecation
ships behind a CLI warning, link the warning's source location.

Removing an entry is the opposite of adding it: in the commit that
removes the deprecated code/config, also delete the entry from this
file (or move it to a brief "Removed in <release>" appendix if you
want a historical record).
