# Implementation Plans

Plans hold **unfinished** multi-step work. Fully shipped plan bodies are removed after source audit; code and git history are the source of truth.

## Active unfinished

| Path | Scope | Status |
|------|--------|--------|
| [agent-status/](agent-status/) | Product detection (live goldens, pack rewrite, live authority, remote packs) | Deferred / open residuals |
| [codebase-health/](codebase-health/) | Deep advisor gap-audit of the codebase-health enforcement roadmap (2026-07-14, commit 846038946): 29 plans, telemetry/OTLP first (001–009), then lints/CI/ownership/testing/perf/docs | Open / TODO |

## Removed (shipped)

These program tracks shipped on PR #759 (`chore/rust-code-health-roadmap`) and were deleted after multi-agent verification (2026-07-13):

- Code-health numbered plans **003–069** + residual ledger (waves 0–6 drained)
- Launch-speed **001–008** (including 008c early restore-scan reuse)
- Goal prompts: `GOAL-CODE-HEALTH-AND-LAUNCH-SPEED`, `GOAL-CLOSE-ALL-REMAINING`

Routine code-health roadmap (ongoing quality program, not a residual ledger): [codebase-health-enforcement](../docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx).

Hard external pin only (no plan file): **iai-callgrind** — project CI has no valgrind; re-evaluate when a valgrind-capable runner exists.

Do not re-add numbered plan files without new residual evidence large enough for a dedicated PR.
