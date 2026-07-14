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

### JACKIN_DEBUG as telemetry control (alias + dual inject)

- **Item**: `JACKIN_DEBUG` env as a telemetry control / dual host→container inject.
- **Type**: `behavior`.
- **Deprecated since**: 2026-07-14 (plan 006).
- **Replacement**: `JACKIN_TELEMETRY_LEVEL` / per-sink `JACKIN_TELEMETRY_<SINK>_LEVEL`. The alias remains in `jackin_diagnostics::telemetry_level` until removal.
- **Remove when**: capsule package version floor exceeds `0.6.0-dev` (guarded by `jackin_debug_dual_inject_boundary_holds` in `jackin-runtime`). Then delete dual inject at `launch_runtime.rs` / `apple_container.rs`, their presence tests, this row, and the boundary test.
- **Where**: `crates/jackin-diagnostics/src/logging.rs` (alias), `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs`, `crates/jackin-runtime/src/runtime/apple_container.rs`.

## How to add an entry

Deprecate something? Append new section to **Active deprecations** above. Use same field structure. If deprecation ships behind CLI warning, link warning's source location.

Removing entry = opposite of adding: in commit removing deprecated code/config, also delete entry from this file (or move to brief "Removed in <release>" appendix if want historical record).
