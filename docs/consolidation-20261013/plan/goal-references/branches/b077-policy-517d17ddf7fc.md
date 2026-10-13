# B077 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/rust-policy-rollout`  
**Tip:** [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f)  
**Group:** policy  
**Server protected flag:** false  
**Inspection:** PARTIAL_SEMANTIC_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Rust policy rollout / CI ledger (two exact-tip aliases)**

- GitHub comparison: 366 commits ahead of the pinned target, zero behind; target is the merge base.
- Cargo manifest introduces category directories adapters/apps/core/services/testing/tools/ui and extensive runtime and usage leaf crates; declared Rust floor is 1.99.
- The comparison returns a capped 300-file view; full file and commit coverage has NOT been established.
- A zero-stat added run_loop.rs in the comparison is a nonempty event loop when fetched directly. Never equate missing/zero patch stats with empty or removed behavior.
- The latest tip is docs: record Velnor d35 lock audit (2026-10-11T06:19:43Z). Documentation of a check is not an independently observed check result.
- Preferred architecture candidate, not an approved blanket merge of all 366 commits. Complete mapping and regression gates precede implementation.
- TESTING.md at the pinned policy SHA requires mise exec -- mbx +1.99.0 for Rust checks and mise -C native run ci/merge/scheduled for native checks. Public rustdoc examples are mirrored into nextest; ci-doc-examples rejects runnable rustdoc fences.
- Policy documentation defines a warm required-job/pipeline one-minute target, offline fan-out, one canonical artifact writer, semantic input-closure keys, first-attempt and flaky-test accountability. These are required contracts, not achieved performance measurements.
- The same TESTING.md still references historical ci-pr.yml/project.toml and flat Capsule fixture paths; exact current command/path consistency must be checked against the selected tree.

## Relationship and unique commits

Observed comparison base `1c3dd1be4a3b1ed518133ec3899c084c792370ff`: head ahead 366, behind 0, merge base `1c3dd1be4a3b1ed518133ec3899c084c792370ff`, status ahead. 

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `Cargo.toml`
- `Cargo.lock`
- `.github/workflows/ci.yml`
- `crates/apps/jackin-capsule/src/daemon/run_loop.rs`
- `crates/services/jackin-usage-broker`
- `crates/services/jackin-usage-provider-claude`
- `crates/services/jackin-usage-coordinator`
- `crates/services/jackin-runtime-coordination`
- `crates/services/jackin-runtime-repo-cache`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Exact-tip aliases: [B036](b036-policy-0b27f2016591.md). These share object analysis but retain separate coverage rows.

Related decisions: [CHG-002: ADAPT](../02-change-coverage-matrix.md#chg-002), [CHG-003: COMBINE](../02-change-coverage-matrix.md#chg-003), [CHG-004: COMBINE](../02-change-coverage-matrix.md#chg-004), [CHG-006: COMBINE](../02-change-coverage-matrix.md#chg-006), [CHG-009: COMBINE](../02-change-coverage-matrix.md#chg-009), [CHG-012: COMBINE](../02-change-coverage-matrix.md#chg-012), [CHG-013: COMBINE](../02-change-coverage-matrix.md#chg-013), [CHG-014: COMBINE](../02-change-coverage-matrix.md#chg-014), [CHG-018: COMBINE](../02-change-coverage-matrix.md#chg-018), [CHG-023: COMBINE](../02-change-coverage-matrix.md#chg-023), [CHG-028: COMBINE](../02-change-coverage-matrix.md#chg-028), [CHG-033: KEEP](../02-change-coverage-matrix.md#chg-033)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-002:** Metadata graph, per-package/feature catalog diff, policy/file-size/agent checks, source-to-result symbol review. Risk: Large move set, ownership/feature regressions, missing source tests.
- **CHG-003:** Pinned installation, metadata, dependency policy, native task contracts, release-preview fixture and lock reproducibility. Risk: Inconsistent pins, stale wrapper contracts and accidental dependency downgrade.
- **CHG-004:** Mixed env/profile, duplicate launch-key binding, mismatched fingerprint/source/account, closed alias-set and zero-call denial tests. Risk: Privilege expansion through permissive reconciliation.
- **CHG-006:** Port 429 text; typed 403 with message mentioning 401; typed 401/429; transport/decode text; absent authoritative retry deadline. Risk: False authorization denial, fabricated rate deadlines or incorrect inference-only classification.
- **CHG-009:** All supported predecessors, interruption, malformed/ambiguous records, fingerprint stability, mixed-version rejection and recovery. Risk: Irreversible state changes or silently incompatible clients.
- **CHG-012:** Prunable overlap, parent/symlink traversal, missing/permission ambiguity, wrong owner/mode/type/link count, cleanup and lock release. Risk: Lock identity substitution or pruning active authority.
- **CHG-013:** Nested same-process handles, different paths, blocking-pool acquisition/cancellation, same owner/repo on different hosts. Risk: Wrong cached repository reuse or lock lifetime regressions; not a confirmed exploit from inspection alone.
- **CHG-014:** Guard outcomes, interrupted purge, live/container ambiguity and unchanged test identities. Risk: Safety checks lost during rename or stale baseline claims accepted as current.
- **CHG-018:** Per-assertion catalog map, payload inspection, TLS compile/feature cases, rejected endpoint choices. Risk: Security regressions hidden by structural test churn.
- **CHG-023:** Fresh native build, correct producer paths, CI/merge/scheduled commands and macOS OrbStack release lane. Risk: Swift resolves a missing/wrong product or validates fewer behaviors after migration.
- **CHG-028:** Per-package/feature/test identity map, body review, actual nonzero selection and platform disposition. Risk: False confidence from inflated counts or undetected orphan modules.
- **CHG-033:** Cold/exact-warm/changed-source/changed-contract runs, forbidden download/build markers, retry/quarantine and per-job timing evidence. Risk: Performance or correctness degraded under a superficially faster cache.

Mandatory remainder: `REM-077` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-005, TASK-006, TASK-007, TASK-008, TASK-009, TASK-010, TASK-012, TASK-014, TASK-015 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).
