# B034 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/unreachable-33d88b1cf764/tip-33d88b1cf764f614d3935c11c48decd53386083e`  
**Tip:** [`33d88b1cf764f614d3935c11c48decd53386083e`](https://github.com/jackin-project/jackin/commit/33d88b1cf764f614d3935c11c48decd53386083e)  
**Group:** archive-src3  
**Server protected flag:** false  
**Inspection:** TIP_PARTIAL_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Bundled usage broker sidecar and in-process fallback**

- Adds a shared runtime service entry point and ships the usage-broker executable with jackin; CLI/console callers resolve the bundled sibling binary.
- Missing or unusable sidecar falls back to an in-process broker while retaining leader-election/CAS reconciliation and stderr context in the intended design.
- Negative activation tests are changed to use an uncreatable data directory, since missing sidecar no longer guarantees failure.
- Preserve installation completeness and diagnostics. In-process fallback remains a gated design decision: prove ownership, client-death/lease lifetime and capability isolation against main foreground-auth behavior before selecting it.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `crates/jackin-runtime/src/usage_broker.rs`
- `crates/jackin-runtime/src/lib.rs`
- `crates/jackin-runtime/src/bin/usage-broker/main.rs`
- `crates/jackin-usage/src/host/broker`
- `crates/jackin/Cargo.toml`
- `crates/jackin/src/bin/usage_broker.rs`
- `crates/jackin-usage-ffi/src/bridge/tests.rs`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-010: ADAPT](../02-change-coverage-matrix.md#chg-010), [CHG-011: DEFER](../02-change-coverage-matrix.md#chg-011)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-010:** Fresh fixture prefix, both binary identities, missing/invalid executable and activation/argument error fixtures. Risk: Package works in checkout but fails after installation.
- **CHG-011:** Missing/unusable sidecar; uncreatable data dir; owner death; concurrent activation; no capability bypass. Risk: In-process lifetime weaker than sidecar lifetime; hiding a packaging/security error.

Mandatory remainder: `REM-034` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-015 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `33d88b1cf764f614d3935c11c48decd53386083e`](https://github.com/jackin-project/jackin/commit/33d88b1cf764f614d3935c11c48decd53386083e). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).
