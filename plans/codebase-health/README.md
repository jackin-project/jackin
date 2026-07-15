# Codebase-health enforcement plans

Generated 2026-07-14 at commit `846038946` by a deep advisor audit of every section of the [codebase-health enforcement roadmap](../../docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx). The roadmap page is the acceptance contract; each plan below closes one coherent cluster of its open items. Telemetry convergence (OTEL/OTLP) is the first focus per operator instruction — plans 001–009 — and later plans assume the earlier telemetry plans have landed.

Each executor: read the plan fully before starting, run the drift check first, honor STOP conditions, and update your row here when done. One plan = one branch = one PR (repo rule: never commit `main`; sign every commit with `-s`; push after every commit).

## Execution order & status

| Plan | Title | Priority | Effort | Depends on | Status |
|------|-------|----------|--------|------------|--------|
| 001 | Typed event registry + canonical attribute schema | P1 | L | — | DONE |
| 002 | Move run/session/component identity off the OTLP Resource | P1 | M | 001 | DONE |
| 003 | Top-level `EventName` through the log bridge | P1 | M | 001 | DONE |
| 004 | Prefix-free, schema-complete capsule OTLP export | P1 | L | 001 | DONE |
| 005 | Versioned JSONL adapter + prohibited-key negative tests | P1 | M | 001 | DONE |
| 006 | `JACKIN_DEBUG` cutover: one reader + dated removal boundary | P1 | S | — | DONE |
| 007 | Trace & dimension coverage: stage registry, screen.name, missing metrics | P1 | M | 001 | DONE |
| 008 | Migrate failure-prone HTTP/Docker/attach/process paths to typed telemetry | P1 | L | 001, 004 | DONE |
| 009 | Exporter-backed host-to-capsule conformance matrix + measured volume ratchet | P1 | L | 002, 003, 004, 005 | DONE |
| 010 | Syntax-aware suppression parser; purge fake ratchet lint keys | P1 | M | — | DONE |
| 011 | Lint policy completion: `allow_attributes`, full census, restriction decisions | P2 | M | 010 | DONE |
| 012 | Advisory CI honesty: Miri per crate, hakari decision, Dylint pilot closure | P2 | S | — | DONE |
| 013 | Executable boundary gates: Turso sole-owner + forbidden-root path audit | P2 | M | — | DONE |
| 014 | Fix OSC 8 hyperlink identity repointing (bug) | P1 | M | — | DONE |
| 015 | Split `runtime/image.rs` by ownership; drop its ratchet exception | P2 | L | — | DONE |
| 016 | Launch pipeline phase contracts + `run_launch_core` harness + benchmark | P2 | L | — | DONE |
| 017 | Capsule daemon decomposition + injectable boundary ports | P2 | L | — | DONE |
| 018 | One shared command-transport model (xtask / capsule / runtime) | P3 | L | — | DONE |
| 019 | Narrow foundational `pub mod` surfaces + public-surface growth ratchet | P2 | L | — | DONE |
| 020 | Domain newtypes census + typed error taxonomy | P3 | L | — | DONE |
| 021 | TUI/console convergence: `drive_frame`, scroll classifier, editor cleanup | P3 | L | — | DONE |
| 022 | Root CLI handler split + TTY fallback + `launch` deprecation warning | P2 | M | — | DONE |
| 023 | Test infrastructure: consolidate fakes, add property tests, wire protocol fuzz | P2 | M | — | DONE |
| 024 | Spec gate: syntax-aware citations, close `MISSING` entries, snapshot review policy | P2 | L | — | DONE |
| 025 | Deterministic time: wall-clock seam + first boundary conversions | P2 | M | — | DONE |
| 026 | Measured performance completion: missing benches, allocation lane, first-frame harness | P3 | L | — | DONE |
| 027 | Ratchet & health completion: suite-time/public-surface providers, trends, JSON diagnostics | P3 | M | 010 | DONE |
| 028 | Docs integrity gates: codebase-map audit, README-freshness wiring, config-key drift | P2 | M | — | DONE |
| 029 | Brand gate completion: bare-brand prose, `plans/` tree, exemption classes | P2 | M | — | DONE |

Status values: TODO | IN PROGRESS | DONE | BLOCKED (one-line reason) | REJECTED (one-line rationale).

## Dependency notes

- 002–009 build on 001's registry: canonical spellings (`error.type`, `jackin.stage`, `expected_close`) and the fail-closed event definitions are what the later plans validate against.
- 009 (conformance matrix) is the acceptance gate for the whole telemetry section — run it last among 001–008; it also supersedes the constant-based `export-volume` ratchet family.
- 011 and 027 both regenerate `ratchet.toml` families; land 010 first so regenerated baselines are trustworthy.
- 017's daemon decomposition satisfies the precondition for splitting `crates/jackin-capsule/src/daemon/tests.rs` (mega-test rule: extract the production responsibility first).
- 016 unblocks the launch-pipeline Criterion scenario and the eventual `launch/tests.rs` split for the same reason.


## Findings considered and rejected (do not re-audit)

- **Schema-policy five-artifact gate (roadmap Characterization #7)**: already DONE — `cargo xtask schema-check` is blocking in CI, idempotence asserted by `crates/jackin/tests/migration_fixtures.rs`, corpus committed. No plan.
- **Flake quarantine / slow-test publication / chaos lane (Characterization #6, partial)**: already DONE via `.config/nextest.toml`, `rust-nextest.yml` flake gate, and the `dind-chaos` hygiene job. Remaining item 6 work (protocol fuzz wiring, mega-test splits) lives in plans 023/016/017.
- **Defect ledger (Self-measuring #2)**: `DEFECT_LEDGER.md` already satisfies the append-only symptom/root-cause/coverage/gate shape. No plan.
- **`log.category` emission**: audited; not emitted anywhere (only `jackin.category`). Compliant.
- **`Box<dyn Error>` in libraries**: zero occurrences workspace-wide. Compliant.
- **iai-callgrind adoption**: stays pinned until a Valgrind-capable CI runner exists (recorded in `plans/README.md` and `ratchet.toml`). Not actionable now.
- **Operator-guide "on the roadmap" phrasing** (`why.mdx:146`, `security-model.mdx:13`): LOW-confidence nit; arguably legitimate product-direction prose. Maintainer call; folded as an optional step into plan 029, not a standalone plan.
