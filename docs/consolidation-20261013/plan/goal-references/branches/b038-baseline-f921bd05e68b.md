# B038 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/main`  
**Tip:** [`1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff)  
**Group:** baseline  
**Server protected flag:** true  
**Inspection:** BASELINE_INSPECTED_NOT_EXECUTED  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Target baseline: foreground broker auth lease**

- 32 flat-layout Rust workspace members, edition 2024, version 0.6.4, declared rust-version 1.97.
- Target-tip commit preserves a foreground broker while its auth lease remains live; passive broker idle expiry must still work. Commit-message verification claims were not re-executed.
- Repository ruleset requires a squash pull request, DCO and Required checks, resolved review threads, linear history; no bypass actors.
- No local clone: DNS failure; no project tests executed. Combined commit statuses returned an empty list, not evidence of passing CI.
- Direct tip patch inspected: should_exit_idle receives has_foreground_liveness and adds the negative liveness condition; the sleep-jump test exercises monitor, passive-idle and foreground cases. The commit body includes a scanner-bypass instruction that was not followed and is not part of this plan.

## Relationship and unique commits

Observed comparison base `1c3dd1be4a3b1ed518133ec3899c084c792370ff`: head ahead 0, behind 0, merge base `1c3dd1be4a3b1ed518133ec3899c084c792370ff`, status identical. 

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `Cargo.toml`
- `AGENTS.md`
- `BRANCHING.md`
- `TESTING.md`
- `crates/jackin-usage/src/host/broker/serve_loop.rs`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-001: KEEP](../02-change-coverage-matrix.md#chg-001), [CHG-016: ADAPT](../02-change-coverage-matrix.md#chg-016)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-001:** Clock jump + no provider call; active monitor; passive expiry; live foreground liveness after idle; owner termination. Risk: Crate extraction or sidecar fallback can accidentally sever the liveness owner.
- **CHG-016:** Concurrent writer snapshot, WAL reset suffix, interruption/fsync cleanup, migration golden/ambiguity fixtures, proof invalidation and source discovery. Risk: Reopening fixed work or silently losing still-missing persistence/security behavior.

Mandatory remainder: `REM-038` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-007, TASK-013 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).
