# Implementation Plans

Plans hold **unfinished** multi-step work. Fully shipped plan bodies are removed after source audit; code and git history are the source of truth.

## Active unfinished

| Path | Scope | Status |
|------|--------|--------|
| [agent-status/](agent-status/) | Product detection (live goldens, pack rewrite, live authority, remote packs) | Deferred / open residuals |
| [codebase-health/](codebase-health/) | Deep advisor gap-audit of the codebase-health enforcement roadmap (2026-07-14, commit 846038946): 27 unfinished plans, telemetry/OTLP first (001–009), then lints/CI/ownership/testing/perf/docs | Open / in progress |
| [native-macos-usage-menu-bar/](native-macos-usage-menu-bar/) | Finish the shipped menu-bar app with a universal static package, Developer ID notarized release asset, supply-chain evidence, and Homebrew cask | Open — operator policy and Apple credentials gate publication |

## Removed (shipped)

These program tracks shipped on PR #759 (`chore/rust-code-health-roadmap`) and were deleted after multi-agent verification (2026-07-13):

- Code-health numbered plans **003–069** + residual ledger (waves 0–6 drained)
- Launch-speed **001–008** (including 008c early restore-scan reuse)
- Goal prompts: `GOAL-CODE-HEALTH-AND-LAUNCH-SPEED`, `GOAL-CLOSE-ALL-REMAINING`

Individually verified codebase-health plans removed on 2026-07-15:

- **014** — OSC 8 hyperlink identity repointing fix
- **025** — deterministic-time seam and first boundary conversions

Shared TUI extraction plans **001–009** and their follow-through roadmap item were removed after the standalone TermRock repository, canonical-API migration, neutral-duplication cleanup, immutable latest-reviewed dependency, donor retirement, and ownership/test-boundary audit shipped. Durable boundaries live in the TUI reference documentation.

Application observability plans **001–016** and their roadmap item were removed after the complete direct-OTLP implementation, exact legacy-site and artifact-removal audits, real-receiver conformance, privacy/cardinality/volume/soak/performance proof, canonical documentation cutover, and green PR #793 checks (2026-07-18). Durable behavior lives in the application observability reference and run-telemetry guide.

Completed routine code-health implementation archive: [codebase-health](codebase-health/).

Hard external pin only (no plan file): **iai-callgrind** — project CI has no valgrind; re-evaluate when a valgrind-capable runner exists.

Do not re-add numbered plan files without new residual evidence large enough for a dedicated PR.
