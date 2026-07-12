# Agent Runtime Status — residual plans only

Structural layers **001–004, 008, 011** are fully implemented and **removed** from `plans/` (source of truth is the code: total tab glyphs, generic identify unwrap, state-tick cadence, OSC-133 freshness, `ProcessSampler` seam, `jackin-agent-status` crate).

Operator product goal (still open): **all four tab states** (🔴 blocked, 🟡 working, 🔵 done, 🟢 idle) visible zero-config for every supported agent, backed by **real** chrome — not synthetic fixtures.

## Residual plans (keep)

| Plan | Status | Residual (why kept) |
|------|--------|---------------------|
| [005](005-pack-reality-coupling.md) | PARTIAL | Mixed synthetic fixtures remain; full anti-circular live goldens incomplete |
| [006](006-detector-exhaustiveness-and-grok.md) | PARTIAL | Exhaustiveness + grok bake shipped; grok **blocked** still synthetic |
| [007](007-pack-content-rewrite.md) | PARTIAL | Some live-backed rules; fabricated kimi/amp/opencode literals remain; OSC-title only Claude |
| [009](009-semantic-authority-spike.md) | PARTIAL | Pure mappings shipped; live ordering validation not done |
| [009a](009a-codex-app-server-authority.md) | PARTIAL | Feature-gated pure map + tests only — **no production app-server status reader** |
| [009b](009b-claude-notification-authority.md) | PARTIAL | Production `enrich_event_name` path shipped; live wait-edge validation open |
| [010](010-out-of-band-pack-updates.md) | PARTIAL | Production local signed-bundle verifier shipped; live remote fetch/publish + launch-summary consent open |

## Removed as fully implemented (do not re-add without new residual)

| Plan | Why fully done |
|------|----------------|
| 001 tab glyphs | Total `VisibleAgentState` / `TabGlyph`; paint working/idle |
| 002 identify unwrap | Generic node/bun/deno/python/shell unwrap over `Agent::ALL` |
| 003 state tick | Ticker arm above PTY in biased select + tests |
| 004 OSC freshness | TTL + clear + zshrc OSC-133 emitter (physics-idle deferred by design) |
| 008 test seam | `ProcessSampler` + advance_status injectable path |
| 011 crate extract | `jackin-agent-status` workspace member + image bake |

## Licensing

**herdr** is AGPL — approach only, never code or fixtures. Clean-room packs and jackin❯-originated captures only.
