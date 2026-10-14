# B051 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/c3cf9a0eb43a-b1b9078173`  
**Tip:** [`b1b90781739333ba7c8829c092a845f6ea82b11c`](https://github.com/jackin-project/jackin/commit/b1b90781739333ba7c8829c092a845f6ea82b11c)  
**Group:** stash-alias  
**Server protected flag:** false  
**Inspection:** PARTIAL_SEMANTIC_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Stashed runtime repo-cache extraction**

- Inspected delta adds jackin-runtime-repo-cache and extracts role-repository cache/resolution code, RepoError and RepoLock.
- Preserve same-process reentrant flock behavior, per-path gating, releasing on last handle, blocking-pool acquisition, and typed user-facing failures.
- The inspected repo_matches compares owner/repo after stripping host for HTTP/SSH forms. Security reviewer must assess cross-host identity behavior against current canonical implementation; this observation alone is not a confirmed exploit.
- Final policy manifest already lists repo-cache, but that is not proof of code, tests, or untracked stash-parent equivalence.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `Cargo.toml`
- `Cargo.lock`
- `crates/services/jackin-runtime-repo-cache`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Exact-tip aliases: [B049](b049-stash-alias-371a2456f4ff.md), [B053](b053-stash-alias-4c4fe5556b72.md), [B055](b055-stash-alias-f8acf47b8695.md), [B058](b058-stash-alias-373bb2fc05d3.md), [B061](b061-stash-alias-cf15510f83af.md), [B063](b063-stash-alias-54f51006e189.md), [B064](b064-stash-alias-abb1dffe6447.md), [B065](b065-stash-alias-0e6388966c3a.md), [B066](b066-stash-alias-b1eced4af625.md), [B067](b067-stash-alias-31cbf60dff61.md), [B071](b071-stash-alias-42c120365146.md), [B073](b073-stash-alias-2cc727bc102c.md), [B074](b074-stash-alias-bc491d591458.md), [B076](b076-stash-alias-8e34450f0fb7.md). These share object analysis but retain separate coverage rows.

Related decisions: No behavior-level approval is available; retain this object for TASK-002 detailed classification.

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.


Mandatory remainder: `REM-051` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-004 after source classification for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `b1b90781739333ba7c8829c092a845f6ea82b11c`](https://github.com/jackin-project/jackin/commit/b1b90781739333ba7c8829c092a845f6ea82b11c). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).
