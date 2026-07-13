# Deprecated

Tracker for deprecated APIs, CLIs, config values, usage patterns still supported for backwards compat but should **eventually be removed**. Periodically review, prune entries whose removal safe.

Deprecate something? Add here in **same commit** that introduces deprecation. See [RULES.md](RULES.md#deprecations) for rule.

While jackin pre-release (see [PRERELEASE.md](PRERELEASE.md)), schema and CLI changes made as breaking changes, not deprecations, so this file usually empty.

## How to read this file

Each entry has:

- **Item** — what deprecated (CLI verb, type, function, config field, value, behavior).
- **Type** — `cli` / `api` / `config` / `behavior`.
- **Deprecated since** — date or version deprecation landed.
- **Replacement** — what to use instead.
- **Remove when** — trigger or target for removal (date, version, or condition like "after CI no longer sees warning for two consecutive releases").
- **Where** — source files / docs implementing deprecation, so removal straightforward.

## Active deprecations

_None._

## How to add an entry

Deprecate something? Append new section to **Active deprecations** above. Use same field structure. If deprecation ships behind CLI warning, link warning's source location.

Removing entry = opposite of adding: in commit removing deprecated code/config, also delete entry from this file (or move to brief "Removed in <release>" appendix if want historical record).

## JACKIN_DEBUG as telemetry control (alias only)

`JACKIN_DEBUG=1` remains a **compat alias** for `JACKIN_TELEMETRY_LEVEL=debug` when
the latter is unset. Resolution is centralized in `jackin_diagnostics::telemetry_level`.
Host container injection still dual-sets `JACKIN_DEBUG` + `JACKIN_TELEMETRY_LEVEL`
for one capsule-image skew window; remove the dual inject after the capsule image
floor moves past this release. Prefer `JACKIN_TELEMETRY_LEVEL` / per-sink
`JACKIN_TELEMETRY_<SINK>_LEVEL` for new operator docs.
