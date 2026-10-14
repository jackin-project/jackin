# 09 — Evidence register and limitations

## What was actually done

Read the full user-provided 772-line repository-analysis specification. Used authenticated GitHub GET APIs to inspect repository metadata, enumerate all advertised server heads, pin main and candidate SHAs, inspect selected comparisons, read manifests/governance/testing guidance, and inspect all 50 distinct tip objects at varying depth. The 79 exact-ref dossiers preserve aliases and distinguish visible first-parent/tip evidence from complete historical/behavioral closure. No source-level consolidation was executed.

Repository identity is jackin-project/jackin; the snapshot is dated 13 October 2026 in Asia/Singapore. Server branch pagination returned 20, 20, 20 and 19 heads on pages 1–4, then an empty page 5. Those counts sum to 79, with 50 unique tips. This is server scope only, not the user's local checkout. Server pages are not an atomic transaction; execution must refresh and pin them and compare any drift.

### Evidence source index

- [Repository and default branch](https://api.github.com/repos/jackin-project/jackin)
- [Remote head pages 1–5](https://api.github.com/repos/jackin-project/jackin/branches?per_page=20&page=1)
- [Target M commit](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff)
- [Main Cargo manifest](https://github.com/jackin-project/jackin/blob/1c3dd1be4a3b1ed518133ec3899c084c792370ff/Cargo.toml)
- [Policy Cargo manifest](https://github.com/jackin-project/jackin/blob/fba69d60c2373156dc0dcecee8e085c83776966f/Cargo.toml)
- [Main testing guide](https://github.com/jackin-project/jackin/blob/1c3dd1be4a3b1ed518133ec3899c084c792370ff/TESTING.md)
- [Policy testing guide](https://github.com/jackin-project/jackin/blob/fba69d60c2373156dc0dcecee8e085c83776966f/TESTING.md)
- [Policy agent rules](https://github.com/jackin-project/jackin/blob/fba69d60c2373156dc0dcecee8e085c83776966f/AGENTS.md)
- [Branch policy](https://github.com/jackin-project/jackin/blob/1c3dd1be4a3b1ed518133ec3899c084c792370ff/BRANCHING.md)
- [Main active ruleset](https://api.github.com/repos/jackin-project/jackin/rulesets/14746904)
- [M versus P](https://api.github.com/repos/jackin-project/jackin/compare/1c3dd1be4a3b1ed518133ec3899c084c792370ff...fba69d60c2373156dc0dcecee8e085c83776966f)
- [M versus U](https://api.github.com/repos/jackin-project/jackin/compare/1c3dd1be4a3b1ed518133ec3899c084c792370ff...2e4a62da4bc4406764162a267163389a5fb513a7)
- [M versus T](https://api.github.com/repos/jackin-project/jackin/compare/1c3dd1be4a3b1ed518133ec3899c084c792370ff...9ef617d014df15c6d967501bcbbdcb3689ed453c)

The [branch inventory](01-branch-inventory.md) and each dossier provide source-specific commit URLs. [observations.json](evidence/observations.json) is a **curated transcription/synthesis of observed API material**, not a raw lossless API export. [remote-branches.json](evidence/remote-branches.json), [decisions.json](evidence/decisions.json) and [tasks.json](evidence/tasks.json) are normalized planning records. Re-fetch source objects for execution and independent verification.

## Access failure

A local clone was attempted; the captured output was:

```text
Cloning into '/mnt/data/jackin-analysis/repo'...
fatal: unable to access 'https://github.com/jackin-project/jackin.git/': Could not resolve host: github.com
```

The failure is environmental; the repository itself was accessible through the GitHub connector. No usable local Jackin clone, local refs/worktree inventory, full history or executable source was established. No branch/source merge, cherry-pick, push, remote file write, provider request, credential inspection or live-state operation was performed. No independent subagent was available; no subagent findings or approvals are claimed.

## Limits that prevent full planning approval

| Evidence gap | Why it matters | Closure |
| --- | --- | --- |
| Operator local refs/worktrees/upstreams and dirtiness unknown | Server rescue/local names do not reveal actual local-only work or uncommitted state. | TASK-001 in the real checkout. |
| Full unique history and all stash/merge payloads not reviewed | First-parent tip patches may omit meaningful earlier changes, merge resolutions and third-parent files. | TASK-002 complete object and semantic inventory; 79 remainder rows remain OPEN. |
| Broad compare file limits | Some returned comparisons cap file lists at 300; truncated patches/stat omissions are not evidence of absence. | Full local direct-tree/rename/commit analysis; no extrapolated complete file count. |
| Only selected graph comparisons | All 1,225 unordered pairs of 50 tips were not evaluated here; patch IDs/range-diff were not run on Jackin. | Collector can supply graph/patch aids, followed by focused semantic cross-comparison. |
| Candidate source files partially inspected | Manifest presence proves package membership, not every function or regression moved correctly. | TASK-004 source-to-symbol/test ownership map and independently approved source units. |
| Historical validation claims | Earlier source records include test counts, reviews, installations, timings and live/CI observations from different revisions/hosts. | Treat as source-reported leads; re-execute necessary checks at exact current heads. |
| Source instructions and archived defects | Probe branches deliberately include invalid code; runbooks can request actions not authorized now. | Reject proved debris/state, inspect unseen parents separately, do not execute embedded operational directions. |
| Live and platform acceptance absent | U's handoff says not ready for live use; Linux/mock evidence is not macOS OrbStack or credential proof. | Disposable test lanes and separately authorized live work where needed. |
| Independent plan review absent | Coordinator synthesis and self-check are not independent review. | Separate reviewer before implementation, task reviews and final independent audit. |
| Remote writes disabled | Read capabilities/rules metadata do not grant permission to push/PR/resolve/merge. | TASK-019 authorization gate; otherwise retain a local reviewable result. |

The 33 decisions are a researched seed set, **not a claim to enumerate every meaningful unique change**. Every branch has an explicit unresolved remainder so missing coverage cannot disappear behind a blanket “superseded” label. Source-unit selection for the 366-commit policy branch is not yet approved. The generated goal is executable as a staged investigation-and-integration workflow, but source implementation must wait for the initial closure gates.

## Observed comparison semantics

Ahead/behind numbers are relative to the named base/head pair. API comparison patches may represent merge-base-to-head rather than a complete direct base-tree/head-tree replacement. Do not infer removed target behavior from an absent API patch. Equal tip IDs prove the same commit object; equal patch IDs, where computed later, are syntactic aids and do not prove behavior or test equivalence. A lack of common ancestry can be legitimate for recovery objects. Synthetic parents are not recovered historical ancestry.

## Tool-only verification

The bundle's Python collector was executed against disposable synthetic Git fixtures. Eight tests passed: alias/patch-equivalence and dirty-work preservation; no overwrite; no output inside repository; linked common Git directory exclusion; rename source/destination preservation; multi-parent tree retention; changed refs flagged unstable; shallow history flagged incomplete. See [actual output](evidence/collector-test-results.txt).

The collector does not fetch, execute project code, inspect credentials or claim semantic approval. Its threads are concurrent Git reads, not delegated analysis agents. Artifact structural validation and source-code syntax compilation are recorded separately in [10-artifact-integrity.md](10-artifact-integrity.md). Neither establishes Jackin correctness, security, performance or integration readiness.
