# Custom Plugin Marketplace Support in Agent Config

## Problem

`jackin.agent.toml` only supports plugins from `claude-plugins-official`. There is no way to declare custom or GitHub-hosted plugin marketplaces, so agents like `the-architect` cannot auto-install project-specific plugins like `jackin-dev`.

Currently, users must manually run `/plugin marketplace add` and `/plugin install` inside the container after launch.

## Desired Behavior

Support a marketplace declaration in `jackin.agent.toml` so custom plugins are installed automatically at agent construction time:

```toml
[claude.marketplaces]
jackin-marketplace = "donbeave/jackin-marketplace"

[claude]
plugins = [
  "superpowers@claude-plugins-official",
  "jackin-dev@jackin-marketplace",
]
```

## Current Workaround

From inside the container, run these Claude Code slash commands:

```
/plugin marketplace add donbeave/jackin-marketplace
/plugin install jackin-dev@jackin-marketplace
/reload-plugins
```

## Marketplace Repository

The marketplace is at [donbeave/jackin-marketplace](https://github.com/donbeave/jackin-marketplace). It contains a `.claude-plugin/marketplace.json` that points to plugin repos via GitHub URLs, following the same pattern as [obra/superpowers-marketplace](https://github.com/obra/superpowers-marketplace).

## Related Files

- `src/agent.rs` — agent config parsing
- `src/construct.rs` — agent image construction (where plugins are installed)
- `jackin-the-architect` repo — `jackin.agent.toml` would be first consumer
