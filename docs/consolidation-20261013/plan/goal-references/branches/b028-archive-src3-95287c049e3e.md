# B028 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-6ce20ce13cb8/tip-6ce20ce13cb87c5ab89b44d72698fa0d68aacf39`  
**Tip:** [`6ce20ce13cb87c5ab89b44d72698fa0d68aacf39`](https://github.com/jackin-project/jackin/commit/6ce20ce13cb87c5ab89b44d72698fa0d68aacf39)  
**Group:** archive-src3  
**Server protected flag:** false  
**Inspection:** TIP_PARTIAL_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Typed provider error status propagation**

- ProviderProbeOutcome no longer derives rate limits or retry deadlines by searching arbitrary error text for status-like numbers.
- Claude scope restriction requires typed HTTP 403; typed 401, 429, transport/decode errors and CLI strings containing misleading status codes do not qualify.
- Codex provider/display error precedence is separated from auth error classification, retaining structured ProviderError information until presentation.
- Preserve typed status and authoritative retry metadata through policy provider-core and live-repair normalization. Text containing port 429 must not create a rate-limit deadline.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `crates/jackin-usage/src/usage/claude.rs`
- `crates/jackin-usage/src/usage/codex.rs`
- `crates/jackin-usage/src/usage/refresh.rs`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-006: COMBINE](../02-change-coverage-matrix.md#chg-006)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-006:** Port 429 text; typed 403 with message mentioning 401; typed 401/429; transport/decode text; absent authoritative retry deadline. Risk: False authorization denial, fabricated rate deadlines or incorrect inference-only classification.

Mandatory remainder: `REM-028` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-008 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `6ce20ce13cb87c5ab89b44d72698fa0d68aacf39`](https://github.com/jackin-project/jackin/commit/6ce20ce13cb87c5ab89b44d72698fa0d68aacf39). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).
