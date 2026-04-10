# Custom Plugin Marketplaces In Agent Manifests

This design extends `jackin.agent.toml` so agent repos can declare Claude Code
plugin marketplaces in addition to plugin IDs.

The goal is to let `jackin` automate the same workflow users already run
manually inside the container:

```bash
claude plugin marketplace add obra/superpowers-marketplace
claude plugin install superpowers@superpowers-marketplace
```

## Goals

- Support custom Claude plugin marketplaces in `jackin.agent.toml`.
- Keep the manifest aligned with Claude Code's existing CLI model.
- Reuse `jackin`'s current runtime bootstrap path for plugin installation.
- Allow agent repos to auto-install plugins from both the official marketplace
  and additional GitHub-hosted marketplaces.

## Non-Goals

- Replacing Claude Code's own marketplace validation rules.
- Introducing local aliases or renaming marketplaces inside `jackin`.
- Reworking plugin installation into a separate build-time or image-baked
  system.

## Current Behavior

Today, `jackin` already automates Claude plugin installation, but only for the
official marketplace.

- The manifest schema only accepts `[claude].plugins`.
- Runtime state persists plugin IDs into `~/.jackin/plugins.json`.
- Container startup runs `install-plugins.sh`.
- The bootstrap script adds `anthropics/claude-plugins-official` and then runs
  `claude plugin install <plugin>` for every configured plugin.

This means plugins such as `superpowers@superpowers-marketplace` or
`jackin-dev@jackin-marketplace` cannot work unless the marketplace has already
been added manually inside the container.

## Chosen Approach

Add structured `[[claude.marketplaces]]` blocks while keeping `[claude].plugins`
as raw Claude plugin install strings.

Example:

```toml
dockerfile = "Dockerfile"

[claude]
plugins = [
  "superpowers@superpowers-marketplace",
  "jackin-dev@jackin-marketplace",
]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
sparse = ["plugins", ".claude-plugin"]

[[claude.marketplaces]]
source = "donbeave/jackin-marketplace"
```

Each marketplace block maps to a `claude plugin marketplace add` invocation.
The `source` value is passed directly to:

```bash
claude plugin marketplace add <source>
```

If `sparse` is present, `jackin` should append the corresponding CLI flags:

```bash
claude plugin marketplace add <source> --sparse <path1> <path2>
```

This keeps `jackin` as a thin wrapper around Claude Code rather than creating
new marketplace naming or install rules.

## Why This Shape

### Match The Existing Manual Workflow

Users already think in terms of the two Claude commands:

```bash
claude plugin marketplace add <source>
claude plugin install <plugin>@<marketplace-name>
```

Using raw plugin install strings in the manifest mirrors the install workflow
exactly, while marketplace blocks preserve the add workflow and its
marketplace-specific options.

### Avoid Alias Drift

`jackin` should not invent marketplace aliases in TOML because Claude plugin
installation already depends on the marketplace name declared in the
marketplace's own `.claude-plugin/marketplace.json`.

For example, the source string may be:

```text
donbeave/jackin-marketplace
```

while the plugin ID remains:

```text
jackin-dev@jackin-marketplace
```

The suffix comes from the marketplace's declared `name`, not from a local
manifest alias.

### Keep Marketplace Options Attached To The Marketplace

The `sparse` option belongs to marketplace registration, not plugin
installation. A flat `marketplaces = []` string list cannot express this cleanly
without inventing a custom mini-language.

Using `[[claude.marketplaces]]` blocks keeps marketplace-specific data together:

```toml
[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
sparse = ["plugins", ".claude-plugin"]
```

### Delegate Source Validation To Claude

Claude Code already knows how to interpret marketplace sources such as GitHub
shorthand. `jackin` should persist and pass through the source string, then let
`claude plugin marketplace add` handle validation and errors.

## Data Model Changes

### Manifest Schema

Extend `ClaudeConfig` with:

- `plugins: Vec<String>`
- `marketplaces: Vec<ClaudeMarketplaceConfig>`

Both fields default to empty lists.

`ClaudeMarketplaceConfig` should contain:

- `source: String`
- `sparse: Vec<String>`

This preserves backwards compatibility for existing agent manifests that only
declare plugins or no plugins at all.

### Runtime State

The runtime bootstrap JSON currently stores only plugins. It should store both
marketplaces and plugins so the entrypoint has all Claude plugin setup inputs in
one mounted file.

Current shape:

```json
{
  "marketplaces": [],
  "plugins": ["code-review@claude-plugins-official"]
}
```

New shape:

```json
{
  "marketplaces": [
    {
      "source": "obra/superpowers-marketplace",
      "sparse": ["plugins", ".claude-plugin"]
    }
  ],
  "plugins": ["superpowers@superpowers-marketplace"]
}
```

The file can keep its current location at `~/.jackin/plugins.json` to avoid a
larger naming or mount refactor in this change.

## Runtime Flow

The existing startup flow remains intact:

1. `jackin` parses `jackin.agent.toml`.
2. `jackin` writes plugin bootstrap state into `~/.jackin/plugins.json` on the
   host.
3. `jackin` mounts that file into the container.
4. The runtime entrypoint runs `install-plugins.sh`.
5. The script adds marketplaces, then installs plugins.

The updated script behavior should be:

1. Add the official marketplace:

   ```bash
   claude plugin marketplace add anthropics/claude-plugins-official
   ```

2. Read `.marketplaces[]` from `plugins.json` and run one command per entry:

   ```bash
   claude plugin marketplace add "$source"
   ```

   where each marketplace entry expands to:

   - required source argument from `.source`
   - an optional `--sparse <path1> <path2> ...` segment built from `.sparse[]`

3. Read `.plugins[]` from `plugins.json` and run:

   ```bash
   claude plugin install "$plugin"
   ```

The ordering matters: marketplaces must be added before plugin installation.

## Error Handling

`jackin` should keep error handling simple in v1.

- If a custom marketplace source is invalid or inaccessible, the bootstrap
  command should fail the same way a manual Claude command would fail.
- If a `sparse` path is invalid for a given marketplace, the Claude CLI command
  should fail and surface the Claude error directly.
- If a plugin references a marketplace name that has not been registered
  successfully, `claude plugin install` should fail and surface the Claude
  error.
- `jackin` should not try to pre-validate whether a plugin suffix matches a
  marketplace source, because that mapping is only known after Claude resolves
  the marketplace metadata.

This keeps the implementation small and avoids duplicating Claude behavior.

## Testing Strategy

Add focused regression tests at the existing state and manifest layers.

### Manifest Tests

- Parsing a manifest that omits `marketplaces` still yields an empty list.
- Parsing a manifest with marketplace blocks and plugins loads both fields.
- Parsing a marketplace block without `sparse` yields an empty sparse list.
- Parsing a marketplace block with `sparse` preserves the declared paths.

### Runtime State Tests

- `AgentState::prepare` writes `plugins.json` with marketplace objects and the
  plugin array.

### Runtime Command Tests

The existing runtime tests already verify that `plugins.json` is mounted rather
than that `claude plugin install` is run directly from Rust. Those tests should
be updated only as needed to reflect the new JSON contents, not the shell
script internals.

## Documentation Changes

Update the following docs to match the new manifest capability:

- `docs/pages/developing/agent-manifest.mdx`
- `docs/pages/developing/creating-agents.mdx`

Also resolve the tracked TODO item and keep project planning docs aligned:

- `todo/custom-plugin-marketplace.md`
- `TODO.md`
- `docs/pages/reference/roadmap.mdx`

## Alternatives Considered

### Flat `marketplaces = []` Strings

Example:

```toml
[claude]
marketplaces = ["obra/superpowers-marketplace"]
```

Rejected because it cannot naturally support marketplace-specific options such
as `sparse`.

### TOML Map Of Marketplace Aliases

Example:

```toml
[claude.marketplaces]
superpowers-marketplace = "obra/superpowers-marketplace"
```

Rejected because it duplicates marketplace naming already owned by Claude and
creates room for mismatch between TOML keys and marketplace metadata.

### Grouping Plugins Inside Marketplace Blocks

Example:

```toml
[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
plugins = ["superpowers"]
```

Rejected because `jackin` would have to derive the final install string, such as
`superpowers@superpowers-marketplace`, from marketplace metadata it does not own.
Keeping `[claude].plugins` as raw Claude install strings avoids that coupling.

### Writing Claude Settings Instead Of Using The CLI

Rejected because `jackin` already has a working runtime bootstrap path that uses
Claude CLI commands successfully. Extending that path is smaller and less risky
than introducing a second configuration mechanism.

## Rollout

This change is safe to ship as an additive manifest feature.

- Existing manifests remain valid.
- Existing official marketplace plugin installs continue to work.
- Agent repos can opt in by adding `[claude].marketplaces` entries.

The TODO item should move to resolved once the code and docs ship.
