# Implementation Plans

Plans hold **unfinished** multi-step work. Fully shipped plan bodies are removed after source audit; code and git history are the source of truth.

## Active unfinished

| Path | Scope | Status |
|------|--------|--------|
| [agent-status/](agent-status/) | Product detection (live goldens, pack rewrite, live authority, remote packs) | Deferred / open residuals |
| [unified-otel-observability/](unified-otel-observability/) | Full implementation of the [Unified OpenTelemetry observability](../docs/content/docs/roadmap/unified-otel-observability.mdx) roadmap item — 15 ordered plans (schema crate → OTLP runtime → facade → propagation → identities → boundaries → TUI/capsule → call-site migration → cutover → verification → docs) | Completed |
| [codebase-health/](codebase-health/) | Deep advisor gap-audit of the codebase-health enforcement roadmap (2026-07-14, commit 846038946): 27 unfinished plans, telemetry/OTLP first (001–009), then lints/CI/ownership/testing/perf/docs | Open / in progress |
| [shared-tui-extraction/](shared-tui-extraction/) | Full implementation of the Shared TUI Extraction roadmap item (2026-07-15, commit 03928e9dd): 9 stage plans executing the research dossier — freeze, filtered history, TermRock bootstrap/publish, catalog, jackin❯ migration, donor retirement, first tag — one branch (`feature/shared-tui-extraction`, PR #794) | Open / not started |

## Removed (shipped)

These program tracks shipped on PR #759 (`chore/rust-code-health-roadmap`) and were deleted after multi-agent verification (2026-07-13):

- Code-health numbered plans **003–069** + residual ledger (waves 0–6 drained)
- Launch-speed **001–008** (including 008c early restore-scan reuse)
- Goal prompts: `GOAL-CODE-HEALTH-AND-LAUNCH-SPEED`, `GOAL-CLOSE-ALL-REMAINING`

Hard external pin only (no plan file): **iai-callgrind** — project CI has no valgrind; re-evaluate when a valgrind-capable runner exists.

Do not re-add numbered plan files without new residual evidence large enough for a dedicated PR.
