# Plan 009a: Productionize Codex app-server authority

## Status

- **Priority**: P2
- **Effort**: M-L
- **Risk**: MED
- **Depends on**: 009 live ordering validation
- **Category**: direction / semantic authority

## Why this matters

Plan 009 proved the pure event-mapping layer can accept a flagged Codex app-server source without changing the
default Codex hook path. Productionizing it requires a real reader that starts or connects to `codex
app-server`, preserves the normal operator launch model, and forwards only verified turn lifecycle events into
the existing `ReportRuntimeEvent` path.

## Scope

In scope: a flagged container-local app-server reader, launch integration that does not disturb the default
Codex TUI path, live ordering validation, tests proving `turn/started` and `turn/completed` publish
working/idle, and a screen-blocked override regression.

Out of scope: Claude Notification promotion, remote app-server exposure, or replacing screen packs.

## STOP conditions

- Starting Codex through app-server changes TUI behavior, auth, sandboxing, or session persistence in a way that
  affects other capsule systems.
- Live app-server notifications arrive out of order or miss turn completion under ordinary interactive use.
- A screen-visible blocked prompt can be hidden by a stale app-server idle report.
