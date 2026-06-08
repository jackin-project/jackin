# Goal — Phase 6: Roadmap status reconcile & deferred work

Parent index: [`../PR-495-FIXES.md`](../PR-495-FIXES.md). HEAD baseline: `f920b29a`.

These items were surfaced by the PR-495 audit (the now-removed `PR-495-REVIEW.md`, Part 2 sections A, C, E). The recurring finding: several roadmap acceptance items are marked `[x]` (done) while their own notes say *deferred*, *Partial*, *requires hardware*, or *benchmark numbers required*. None of these are code merge blockers — but the **roadmap-freshness hard rule** (`AGENTS.md`) makes honest re-statusing a pre-merge requirement: a PR that ships/advances/defers a roadmap item must move that item in the same PR. So this phase is roadmap hygiene, not feature work.

For each task: confirm the current code state, then either complete it (only if cheap and in-scope for PR #495) or re-status the roadmap item honestly (`[~]` / Partial / Planned) with the exact remaining scope. After any status/file move, run the sidebar + overview audits in `docs/AGENTS.md`.

## Tasks

| ID | Status | Source / evidence | Action | Acceptance |
|---|---|---|---|---|
| `RMP-1` | pending | Diagnostics JSONL is written directly; `tracing` is additive (`crates/jackin-diagnostics/src/run.rs`). Roadmap (Defect 47 / `structured-tracing-metrics`) claimed it is "span-sourced with a real `span_id`"; only `span_id` is real. | Verify state, then either build the inversion (spans authoritative, a `JackinDiagnosticsLayer` emits the JSONL) or correct the status line to "JSONL direct, tracing additive, `span_id` only." | Roadmap status matches code; no contradictory "span-sourced" claim. |
| `RMP-2` | deferred | Observability metrics surface (stage-duration histograms + cache hit/miss counters, `structured-tracing-metrics` PR 5) not built; only a `duration_ms` JSONL field was added. | Re-status the roadmap item as Planned/Partial. | The metrics surface is not claimed done anywhere. |
| `RMP-3` | deferred | `jackin-term` zero-alloc tail: Ghostty `PageList` arena, `RefCountedSet` interning, multi-session slab, and `dirty_spans()` emit-path integration deferred; `Vec<Vec<Cell>>` + dirty-spans `Vec` still allocate. | Re-status the "zero per-frame allocation" acceptance as partial; complete only if `present_frame`/`dhat` numbers justify it now. | No roadmap line claims zero-alloc achieved; bench evidence linked if claimed. |
| `RMP-4` | deferred | Real PTY conformance corpus (`claude`/`codex`/`vim`/`htop`/asciinema captures) absent; the differential harness runs inline fixtures only. | Re-status as "remaining next step"; keep the fixture dir noted as sparse. | Roadmap shows the corpus as outstanding, not done. |
| `RMP-5` | pending | Capsule chrome still emits VT100/ANSI rather than `jackin-tui` Ratatui primitives (audit Part 2 E) — the largest remaining "two implementations" risk. `CAP-1` migrates the pane border palette; the broad chrome migration is bigger. | Ensure a roadmap item explicitly tracks "capsule ANSI → Ratatui migration" with remaining scope; cross-link `CAP-1`/`CAP-3`. | A named roadmap item exists with concrete remaining files/behaviors. |
| `RMP-6` | pending | Stale roadmap acceptance notes (audit Part 2 A, Part 4): "Green everywhere", `cargo fmt`, `nextest`, and "clippy blocked by capsule test compile" were `[x]` while gates were red. Those gates are now green at HEAD; the notes still mislead. Also: the dispatch exception arms (`agent_binary.rs`, `multiplexer_utils.rs`). | Update the acceptance lines to reflect the now-green cargo gates; re-state any still-deferred `[x]` as `[~]`; collapse or `#[expect]`-justify the two exception arms (the `ARCH-2` residual). | No roadmap page claims a gate green that is red, or done that is deferred. |

### Optional

| ID | Status | Evidence | Action |
|---|---|---|---|
| `RMP-7` | deferred | God files flagged by the audit have shrunk but remain large: `console/tui/input/global_mounts.rs` 1407, `capsule/.../dialog.rs` 1425, `console/.../op_picker.rs` 1197 LOC. "Not urgent" per the audit. | Split along input/state/render seams when next touching these files; not a PR-495 blocker. |

## Done definition
- Every roadmap item this PR ships/advances/defers has a status that matches the code (no premature `[x]`).
- `RMP-1` and `RMP-5`/`RMP-6` resolved (built or honestly re-statused); `RMP-2`/`RMP-3`/`RMP-4` re-statused as deferred with named remaining scope.
- `docs/AGENTS.md` sidebar + overview audits pass after any status/file move.
- The roadmap index reflects each item in exactly one section.
