# Deprecated

Tracker for deprecated APIs, CLIs, config values, and usage patterns
that are still supported for backwards compatibility but should
**eventually be removed**. Periodically review this file and prune
entries whose removal is safe.

When you deprecate something, add it here in the **same commit** that
introduces the deprecation. See [RULES.md](RULES.md#deprecations) for
the rule.

While jackin is pre-release (see [AGENTS.md](AGENTS.md#project-status-pre-release-agent-only)),
schema and CLI changes are made as breaking changes rather than
deprecations, so this file is typically empty.

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

_None._

## How to add an entry

When you deprecate something, append a new section to **Active
deprecations** above. Use the same field structure. If the deprecation
ships behind a CLI warning, link the warning's source location.

Removing an entry is the opposite of adding it: in the commit that
removes the deprecated code/config, also delete the entry from this
file (or move it to a brief "Removed in <release>" appendix if you
want a historical record).
