# 02 — Change coverage and decision matrix

> **Planning status: PARTIAL EVIDENCE / IMPLEMENTATION BLOCKED.** All 79 discovered server branches have dossiers and all 50 distinct tips received some inspection. Complete unique-history/semantic closure, the operator's local inventory, Jackin test execution and independent review are not complete. These are hard execution gates, not silently waived work.

## How to read this matrix

The 33 rows below are **observed behavior-level decision seeds**, not an exhaustive commit-level change inventory. Every decision is proposed, subject to the stated tests and independent review. The 79 branch-remainder rows later in this document explicitly preserve the missing full-history work; no source is silently marked complete.

KEEP retains an implementation substantially as written; ADAPT ports valuable behavior; COMBINE synthesizes sources; SUPERSEDED requires a stronger identified source/destination; REJECT excludes a demonstrably inappropriate inspected change; DEFER records a blocker. Design supersession here does not assert that the replacement is already integrated. No destination commit exists yet.

## Decision index

| Change | Observed behavior | Source refs | Proposed disposition | Owner |
| --- | --- | --- | --- | --- |
| [CHG-001](#chg-001) | Foreground authentication lease lifetime | [B038](branches/b038-baseline-f921bd05e68b.md) | KEEP | [TASK-007](05-execution-tasks.md#task-007) |
| [CHG-002](#chg-002) | Layered workspace and policy extractions | [B036](branches/b036-policy-0b27f2016591.md), [B077](branches/b077-policy-517d17ddf7fc.md) | ADAPT | [TASK-005](05-execution-tasks.md#task-005) |
| [CHG-003](#chg-003) | Pinned Rust/MBX/Mise modernization | [B035](branches/b035-modernize-9949620d3954.md), [B036](branches/b036-policy-0b27f2016591.md), [B077](branches/b077-policy-517d17ddf7fc.md) | COMBINE | [TASK-006](05-execution-tasks.md#task-006) |
| [CHG-004](#chg-004) | Exact credential capability binding | [B036](branches/b036-policy-0b27f2016591.md), [B043](branches/b043-policy-rescue-cf95d97cce7c.md), [B044](branches/b044-dirty-3eaee4abbc92.md), [B077](branches/b077-policy-517d17ddf7fc.md) | COMBINE | [TASK-007](05-execution-tasks.md#task-007) |
| [CHG-005](#chg-005) | Revoked account result-free uncertainty marker | [B040](branches/b040-usage-followup-bf245b09ab75.md) | KEEP | [TASK-007](05-execution-tasks.md#task-007) |
| [CHG-006](#chg-006) | Typed provider statuses and retry authority | [B028](branches/b028-archive-src3-95287c049e3e.md), [B036](branches/b036-policy-0b27f2016591.md), [B037](branches/b037-usage-1c388dc4de71.md), [B077](branches/b077-policy-517d17ddf7fc.md) | COMBINE | [TASK-008](05-execution-tasks.md#task-008) |
| [CHG-007](#chg-007) | Claude session/weekly freshness and 100-percent remaining | [B037](branches/b037-usage-1c388dc4de71.md), [B044](branches/b044-dirty-3eaee4abbc92.md) | ADAPT | [TASK-008](05-execution-tasks.md#task-008) |
| [CHG-008](#chg-008) | Observe-only monitor authority boundary | [B037](branches/b037-usage-1c388dc4de71.md) | ADAPT | [TASK-009](05-execution-tasks.md#task-009) |
| [CHG-009](#chg-009) | Monitor schema, wire and fingerprint migration | [B036](branches/b036-policy-0b27f2016591.md), [B037](branches/b037-usage-1c388dc4de71.md), [B077](branches/b077-policy-517d17ddf7fc.md) | COMBINE | [TASK-009](05-execution-tasks.md#task-009) |
| [CHG-010](#chg-010) | Bundled usage broker executable | [B034](branches/b034-archive-src3-ac5215e0b69e.md) | ADAPT | [TASK-015](05-execution-tasks.md#task-015) |
| [CHG-011](#chg-011) | Missing-sidecar in-process fallback | [B034](branches/b034-archive-src3-ac5215e0b69e.md) | DEFER | [TASK-015](05-execution-tasks.md#task-015) |
| [CHG-012](#chg-012) | Permanent coordination inode extraction | [B036](branches/b036-policy-0b27f2016591.md), [B047](branches/b047-stash-alias-53dd4c6affcc.md), [B048](branches/b048-stash-alias-b7b19a9f3034.md), [B050](branches/b050-stash-alias-bfd7103e838f.md), [B052](branches/b052-stash-alias-01522f00a01b.md), [B054](branches/b054-stash-alias-68ff05fe0f69.md), [B056](branches/b056-stash-alias-cbbb9b2a9c6e.md), [B057](branches/b057-stash-alias-09031c14e05c.md), [B059](branches/b059-stash-alias-ebc2e2faed24.md), [B060](branches/b060-stash-alias-15ca898cb2c6.md), [B062](branches/b062-stash-alias-dca0343dfb05.md), [B068](branches/b068-stash-alias-929b5577bb5f.md), [B069](branches/b069-stash-alias-8836a20a65fb.md), [B070](branches/b070-stash-alias-f574e4820623.md), [B072](branches/b072-stash-alias-64003e4d826d.md), [B075](branches/b075-stash-alias-770a8c6e1ecf.md), [B077](branches/b077-policy-517d17ddf7fc.md) | COMBINE | [TASK-010](05-execution-tasks.md#task-010) |
| [CHG-013](#chg-013) | Repo-cache lock and identity extraction | [B036](branches/b036-policy-0b27f2016591.md), [B047](branches/b047-stash-alias-53dd4c6affcc.md), [B048](branches/b048-stash-alias-b7b19a9f3034.md), [B050](branches/b050-stash-alias-bfd7103e838f.md), [B052](branches/b052-stash-alias-01522f00a01b.md), [B054](branches/b054-stash-alias-68ff05fe0f69.md), [B056](branches/b056-stash-alias-cbbb9b2a9c6e.md), [B057](branches/b057-stash-alias-09031c14e05c.md), [B059](branches/b059-stash-alias-ebc2e2faed24.md), [B060](branches/b060-stash-alias-15ca898cb2c6.md), [B062](branches/b062-stash-alias-dca0343dfb05.md), [B068](branches/b068-stash-alias-929b5577bb5f.md), [B069](branches/b069-stash-alias-8836a20a65fb.md), [B070](branches/b070-stash-alias-f574e4820623.md), [B072](branches/b072-stash-alias-64003e4d826d.md), [B075](branches/b075-stash-alias-770a8c6e1ecf.md), [B077](branches/b077-policy-517d17ddf7fc.md) | COMBINE | [TASK-010](05-execution-tasks.md#task-010) |
| [CHG-014](#chg-014) | Purge absence guard extraction | [B036](branches/b036-policy-0b27f2016591.md), [B042](branches/b042-policy-rescue-4dc1add6a9f0.md), [B045](branches/b045-dirty-868886b2213b.md), [B077](branches/b077-policy-517d17ddf7fc.md) | COMBINE | [TASK-010](05-execution-tasks.md#task-010) |
| [CHG-015](#chg-015) | Model/effort CLI-to-runtime propagation | [B002](branches/b002-archive-docs-33af45993b41.md), [B079](branches/b079-wip-ebf572949c76.md) | COMBINE | [TASK-011](05-execution-tasks.md#task-011) |
| [CHG-016](#chg-016) | OMP/config/restore durability obligations | [B001](branches/b001-archive-docs-437a9e81ba65.md), [B002](branches/b002-archive-docs-33af45993b41.md), [B038](branches/b038-baseline-f921bd05e68b.md) | ADAPT | [TASK-013](05-execution-tasks.md#task-013) |
| [CHG-017](#chg-017) | Per-request diagnostic retry assertions | [B027](branches/b027-archive-sanitized-0739580d021c.md) | KEEP | [TASK-012](05-execution-tasks.md#task-012) |
| [CHG-018](#chg-018) | Privacy, TLS and connection-choice tests after splitting | [B002](branches/b002-archive-docs-33af45993b41.md), [B036](branches/b036-policy-0b27f2016591.md), [B041](branches/b041-policy-rescue-bb88995b68e3.md), [B046](branches/b046-dirty-fc03ff5ffa1d.md), [B077](branches/b077-policy-517d17ddf7fc.md) | COMBINE | [TASK-012](05-execution-tasks.md#task-012) |
| [CHG-019](#chg-019) | Durable push-head CI evidence qualification | [B005](branches/b005-archive-keeper-efa220a4ecda.md), [B012](branches/b012-archive-keeper-27c1d7ad2d95.md), [B020](branches/b020-archive-keeper-83afb29df592.md) | COMBINE | [TASK-006](05-execution-tasks.md#task-006) |
| [CHG-020](#chg-020) | First-parent-only post-merge denominator | [B030](branches/b030-archive-src3-841f56f9a292.md) | SUPERSEDED | [TASK-006](05-execution-tasks.md#task-006) |
| [CHG-021](#chg-021) | Installer-wide sccache/backend activation | [B032](branches/b032-archive-src3-70770d79115b.md) | REJECT | [TASK-006](05-execution-tasks.md#task-006) |
| [CHG-022](#chg-022) | Tool wrapper isolation and provisioning contracts | [B039](branches/b039-misrouted-e096ae35b5a8.md) | ADAPT | [TASK-006](05-execution-tasks.md#task-006) |
| [CHG-023](#chg-023) | Native product graph and full Apple obligations | [B015](branches/b015-archive-keeper-dc5b26803593.md), [B025](branches/b025-archive-staged-67aa054ffa3d.md), [B031](branches/b031-archive-src3-75be4f6a7848.md), [B033](branches/b033-archive-src3-9264ed8676fb.md), [B035](branches/b035-modernize-9949620d3954.md), [B036](branches/b036-policy-0b27f2016591.md), [B077](branches/b077-policy-517d17ddf7fc.md) | COMBINE | [TASK-015](05-execution-tasks.md#task-015) |
| [CHG-024](#chg-024) | Intentionally unregenerated pre-M1 state | [B029](branches/b029-archive-src3-21f83e144de4.md) | REJECT | [TASK-015](05-execution-tasks.md#task-015) |
| [CHG-025](#chg-025) | WIP musl/GNU tool lock mappings | [B078](branches/b078-wip-6b8347c05938.md) | DEFER | [TASK-006](05-execution-tasks.md#task-006) |
| [CHG-026](#chg-026) | Zig minisign provenance | [B078](branches/b078-wip-6b8347c05938.md) | KEEP | [TASK-006](05-execution-tasks.md#task-006) |
| [CHG-027](#chg-027) | Staged-only affected-crate selection | [B010](branches/b010-archive-keeper-8fd347cb734e.md) | COMBINE | [TASK-014](05-execution-tasks.md#task-014) |
| [CHG-028](#chg-028) | Preserved test identities through policy splits | [B036](branches/b036-policy-0b27f2016591.md), [B041](branches/b041-policy-rescue-bb88995b68e3.md), [B046](branches/b046-dirty-fc03ff5ffa1d.md), [B077](branches/b077-policy-517d17ddf7fc.md) | COMBINE | [TASK-014](05-execution-tasks.md#task-014) |
| [CHG-029](#chg-029) | Proof-only production debris | [B004](branches/b004-archive-keeper-5aacaad28a02.md), [B006](branches/b006-archive-keeper-856b4d85181f.md), [B007](branches/b007-archive-keeper-74cf60fb6d52.md), [B008](branches/b008-archive-keeper-77baf007acb6.md), [B009](branches/b009-archive-keeper-dd596b9a057a.md), [B011](branches/b011-archive-keeper-64de0538eaa5.md), [B013](branches/b013-archive-keeper-e118ca686ade.md), [B014](branches/b014-archive-keeper-4d364008016b.md), [B016](branches/b016-archive-keeper-f164586efdc9.md), [B017](branches/b017-archive-keeper-b3531a0bac69.md), [B018](branches/b018-archive-keeper-6176f4f904dc.md), [B019](branches/b019-archive-keeper-7cddcc8364f3.md), [B022](branches/b022-archive-keeper-0058bbcc86ff.md), [B023](branches/b023-archive-keeper-a760f85ce5ae.md), [B024](branches/b024-archive-keeper-d898a35778ef.md) | REJECT | [TASK-014](05-execution-tasks.md#task-014) |
| [CHG-030](#chg-030) | Whole synthetic conflict snapshot | [B003](branches/b003-archive-recovery-583438c051c5.md) | REJECT | [TASK-013](05-execution-tasks.md#task-013) |
| [CHG-031](#chg-031) | Historical evidence and operating records | [B001](branches/b001-archive-docs-437a9e81ba65.md), [B002](branches/b002-archive-docs-33af45993b41.md), [B021](branches/b021-archive-keeper-45846051b2ee.md), [B037](branches/b037-usage-1c388dc4de71.md), [B045](branches/b045-dirty-868886b2213b.md) | ADAPT | [TASK-016](05-execution-tasks.md#task-016) |
| [CHG-032](#chg-032) | Sanitized audit tip with no returned patch | [B026](branches/b026-archive-sanitized-0ced81950334.md) | DEFER | [TASK-002](05-execution-tasks.md#task-002) |
| [CHG-033](#chg-033) | Strict warm CI and flaky-test accounting | [B036](branches/b036-policy-0b27f2016591.md), [B077](branches/b077-policy-517d17ddf7fc.md) | KEEP | [TASK-006](05-execution-tasks.md#task-006) |

<a id="chg-001"></a>
### CHG-001 — Foreground authentication lease lifetime

**Sources:** [B038](branches/b038-baseline-f921bd05e68b.md). Exact source tips: [`1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff).

**Disposition:** KEEP. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Keep a prepared foreground broker alive while collector liveness exists; active monitors also prevent idle expiry; passive idle brokers still expire.

**Affected symbols/modules and proposed destination:** `crates/services/jackin-usage-broker/`, `crates/services/jackin-usage-coordinator/`, `crates/services/jackin-usage-discovery/`, `crates/services/jackin-usage-credential-resolver/`, `crates/services/jackin-usage-coordinator/src/tests/case_03.rs`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Apply idle timeout indiscriminately after ten minutes.

**Selection rationale:** Main tip adds has_foreground_liveness to should_exit_idle and tests post-sleep/eleven-minute behavior. This is baseline behavior, not an optional branch feature.

**Dependencies:** TASK-006; implement and review through [TASK-007](05-execution-tasks.md#task-007).

**Potential regression:** Crate extraction or sidecar fallback can accidentally sever the liveness owner.

**Required tests:** Clock jump + no provider call; active monitor; passive expiry; live foreground liveness after idle; owner termination. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-002"></a>
### CHG-002 — Layered workspace and policy extractions

**Sources:** [B036](branches/b036-policy-0b27f2016591.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** ADAPT. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** One coherent adapters/apps/core/services/testing/tools/ui graph with required features, runtime contracts and discoverable tests.

**Affected symbols/modules and proposed destination:** `Cargo.toml`, `Cargo.lock`, `crates/adapters/`, `crates/apps/`, `crates/core/`, `crates/services/`, `crates/testing/`, `crates/tools/`, `crates/ui/`, `policy/owned-packages.json`, `policy/package-groups.json`, `ratchet.toml`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Mechanical merge of all 366 policy commits, or retaining flat and layered duplicate implementations.

**Selection rationale:** The manifest confirms extensive leaf extraction but capped comparisons cannot prove every body/test is retained. Approve exact source units only after complete coverage.

**Dependencies:** TASK-004; implement and review through [TASK-005](05-execution-tasks.md#task-005).

**Potential regression:** Large move set, ownership/feature regressions, missing source tests.

**Required tests:** Metadata graph, per-package/feature catalog diff, policy/file-size/agent checks, source-to-result symbol review. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-003"></a>
### CHG-003 — Pinned Rust/MBX/Mise modernization

**Sources:** [B035](branches/b035-modernize-9949620d3954.md), [B036](branches/b036-policy-0b27f2016591.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`9ef617d014df15c6d967501bcbbdcb3689ed453c`](https://github.com/jackin-project/jackin/commit/9ef617d014df15c6d967501bcbbdcb3689ed453c), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** One reviewed toolchain and lockfile; retain missing modernization benefits including native-local task and release-preview behavior.

**Affected symbols/modules and proposed destination:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `mise.toml`, `mise.lock`, `native/mise.toml`, `scripts/release-preview-package`, `scripts/test-mbx-cargo-wrapper.py`, `.github-gen/`, `.github/workflows/`, `crates/tools/jackin-xtask/src/ci_evidence/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Prefer the newest timestamp or copy an old flat manifest over the selected graph.

**Selection rationale:** T has two exclusive commits and four target commits missing; P moves the Rust floor to 1.99. File overlap alone does not prove equivalent intent.

**Dependencies:** TASK-005; implement and review through [TASK-006](05-execution-tasks.md#task-006).

**Potential regression:** Inconsistent pins, stale wrapper contracts and accidental dependency downgrade.

**Required tests:** Pinned installation, metadata, dependency policy, native task contracts, release-preview fixture and lock reproducibility. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-004"></a>
### CHG-004 — Exact credential capability binding

**Sources:** [B036](branches/b036-policy-0b27f2016591.md), [B043](branches/b043-policy-rescue-cf95d97cce7c.md), [B044](branches/b044-dirty-3eaee4abbc92.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`3ddbc0563ed95225b0004c393b3928fb599ee066`](https://github.com/jackin-project/jackin/commit/3ddbc0563ed95225b0004c393b3928fb599ee066), [`40823a76a3e20b901d5bc410979cee0931fd5c48`](https://github.com/jackin-project/jackin/commit/40823a76a3e20b901d5bc410979cee0931fd5c48), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Exact account/surface/source/material fingerprint binding; fail closed for ambiguity, missing material and multiple candidate matches.

**Affected symbols/modules and proposed destination:** `crates/services/jackin-usage-broker/`, `crates/services/jackin-usage-coordinator/`, `crates/services/jackin-usage-discovery/`, `crates/services/jackin-usage-credential-resolver/`, `crates/services/jackin-usage-coordinator/src/tests/case_03.rs`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Broaden provider aliases or accept a matching display name as authority.

**Selection rationale:** Inspected s7 functions carry security checks that may be obscured by extraction. Preserve their semantics and negative cases.

**Dependencies:** TASK-006; implement and review through [TASK-007](05-execution-tasks.md#task-007).

**Potential regression:** Privilege expansion through permissive reconciliation.

**Required tests:** Mixed env/profile, duplicate launch-key binding, mismatched fingerprint/source/account, closed alias-set and zero-call denial tests. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-005"></a>
### CHG-005 — Revoked account result-free uncertainty marker

**Sources:** [B040](branches/b040-usage-followup-bf245b09ab75.md). Exact source tips: [`2abed4ad67d8438ab9eb124b564be6ad5b545847`](https://github.com/jackin-project/jackin/commit/2abed4ad67d8438ab9eb124b564be6ad5b545847).

**Disposition:** KEEP. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Retain the concrete state store for the persisted marker assertion after replacing corrupt/revoked account state.

**Affected symbols/modules and proposed destination:** `crates/services/jackin-usage-broker/`, `crates/services/jackin-usage-coordinator/`, `crates/services/jackin-usage-discovery/`, `crates/services/jackin-usage-credential-resolver/`, `crates/services/jackin-usage-coordinator/src/tests/case_03.rs`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Lose the assertion by moving only the trait-object handle.

**Selection rationale:** The tip directly fixes the test seam; compare the final policy test before adding a duplicate.

**Dependencies:** TASK-006; implement and review through [TASK-007](05-execution-tasks.md#task-007).

**Potential regression:** A passing coordinator test that no longer examines durable state.

**Required tests:** corrupt_revoked_account_is_replaced_with_result_free_uncertainty_marker; inspect actual persisted state after catalog rotation. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-006"></a>
### CHG-006 — Typed provider statuses and retry authority

**Sources:** [B028](branches/b028-archive-src3-95287c049e3e.md), [B036](branches/b036-policy-0b27f2016591.md), [B037](branches/b037-usage-1c388dc4de71.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7), [`6ce20ce13cb87c5ab89b44d72698fa0d68aacf39`](https://github.com/jackin-project/jackin/commit/6ce20ce13cb87c5ab89b44d72698fa0d68aacf39), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Keep structured HTTP status and retry metadata through adapters; only typed 403 establishes Claude scope restriction.

**Affected symbols/modules and proposed destination:** `crates/services/jackin-usage-provider-core/`, `crates/services/jackin-usage-provider-claude/`, `crates/services/jackin-usage-provider-codex/`, `crates/services/jackin-usage-output/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Search arbitrary display strings for 401/403/429 or Retry-After-looking text.

**Selection rationale:** The archived typed-error patch explicitly rejects misleading transport/CLI strings and distinguishes auth status from display precedence.

**Dependencies:** TASK-006; implement and review through [TASK-008](05-execution-tasks.md#task-008).

**Potential regression:** False authorization denial, fabricated rate deadlines or incorrect inference-only classification.

**Required tests:** Port 429 text; typed 403 with message mentioning 401; typed 401/429; transport/decode text; absent authoritative retry deadline. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-007"></a>
### CHG-007 — Claude session/weekly freshness and 100-percent remaining

**Sources:** [B037](branches/b037-usage-1c388dc4de71.md), [B044](branches/b044-dirty-3eaee4abbc92.md). Exact source tips: [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7), [`40823a76a3e20b901d5bc410979cee0931fd5c48`](https://github.com/jackin-project/jackin/commit/40823a76a3e20b901d5bc410979cee0931fd5c48).

**Disposition:** ADAPT. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Derive zero used from authoritative 100% remaining where supported; preserve independent window freshness and unknown reset values.

**Affected symbols/modules and proposed destination:** `crates/services/jackin-usage-provider-core/`, `crates/services/jackin-usage-provider-claude/`, `crates/services/jackin-usage-provider-codex/`, `crates/services/jackin-usage-output/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Fabricate a reset timestamp or make session fresh because weekly data is fresh.

**Selection rationale:** U records a v12 fix but explicitly does not establish live acceptance. Adapt tested semantics, not its readiness claims.

**Dependencies:** TASK-006; implement and review through [TASK-008](05-execution-tasks.md#task-008).

**Potential regression:** Misleading quota availability or fresh-looking stale data.

**Required tests:** Per-window partial observations, missing used/reset, stale session with fresh weekly, bounded percent and synthetic zero-call fixtures. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-008"></a>
### CHG-008 — Observe-only monitor authority boundary

**Sources:** [B037](branches/b037-usage-1c388dc4de71.md). Exact source tips: [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7).

**Disposition:** ADAPT. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Observe-only monitors remain goal-less/non-runnable and do not grant, revoke or exercise task dispatch authority.

**Affected symbols/modules and proposed destination:** `crates/core/jackin-protocol/`, `crates/services/jackin-usage-broker-wire/`, `crates/services/jackin-usage-snapshot-store/`, `crates/services/jackin-usage-store-backend/`, `crates/services/jackin-usage-token-monitor/`, `crates/services/jackin-usage-projection/`, `crates/apps/jackin/src/bin/usage-broker-fixture.rs`, `crates/apps/jackin/tests/usage_provider_flow/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Treat monitor presence or an active broker as authorization.

**Selection rationale:** The live handoff clearly separates observation from task policy and leaves dispatch unapproved.

**Dependencies:** TASK-007, TASK-008; implement and review through [TASK-009](05-execution-tasks.md#task-009).

**Potential regression:** Unattended activity against live credentials.

**Required tests:** Observe-only goal_id/runnable output, no launch/auth/provider dispatch, absent/denied capabilities. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-009"></a>
### CHG-009 — Monitor schema, wire and fingerprint migration

**Sources:** [B036](branches/b036-policy-0b27f2016591.md), [B037](branches/b037-usage-1c388dc4de71.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Reconcile U schema-6/wire-v9 requirements with selected versions; preserve stable binding and complete predecessor/recovery handling.

**Affected symbols/modules and proposed destination:** `crates/core/jackin-protocol/`, `crates/services/jackin-usage-broker-wire/`, `crates/services/jackin-usage-snapshot-store/`, `crates/services/jackin-usage-store-backend/`, `crates/services/jackin-usage-token-monitor/`, `crates/services/jackin-usage-projection/`, `crates/apps/jackin/src/bin/usage-broker-fixture.rs`, `crates/apps/jackin/tests/usage_provider_flow/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Blindly increment or downgrade schema integers or open real state to test.

**Selection rationale:** Migration and wire shape span multiple new leaf crates; compatibility requires explicit mapping and fixture tests.

**Dependencies:** TASK-007, TASK-008; implement and review through [TASK-009](05-execution-tasks.md#task-009).

**Potential regression:** Irreversible state changes or silently incompatible clients.

**Required tests:** All supported predecessors, interruption, malformed/ambiguous records, fingerprint stability, mixed-version rejection and recovery. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-010"></a>
### CHG-010 — Bundled usage broker executable

**Sources:** [B034](branches/b034-archive-src3-ac5215e0b69e.md). Exact source tips: [`33d88b1cf764f614d3935c11c48decd53386083e`](https://github.com/jackin-project/jackin/commit/33d88b1cf764f614d3935c11c48decd53386083e).

**Disposition:** ADAPT. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** A jackin package includes the intended broker sibling and correct shared service entry point; preserve diagnostic stderr context.

**Affected symbols/modules and proposed destination:** `crates/apps/jackin/Cargo.toml`, `crates/apps/jackin/src/bin/usage_broker.rs`, `crates/services/jackin-runtime/`, `crates/services/jackin-usage-broker/`, `crates/adapters/jackin-usage-ffi/`, `native/mise.toml`, `native/Package.swift`, `native/project.yml`, `native/Scripts/run-ui-tests.sh`, `crates/tools/jackin-xtask/src/desktop/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Assume a separately installed runtime sidecar exists.

**Selection rationale:** The source adds a jackin binary target and shared service module specifically to avoid missing-sidecar installations.

**Dependencies:** TASK-009, TASK-010, TASK-011, TASK-012, TASK-013; implement and review through [TASK-015](05-execution-tasks.md#task-015).

**Potential regression:** Package works in checkout but fails after installation.

**Required tests:** Fresh fixture prefix, both binary identities, missing/invalid executable and activation/argument error fixtures. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-011"></a>
### CHG-011 — Missing-sidecar in-process fallback

**Sources:** [B034](branches/b034-archive-src3-ac5215e0b69e.md). Exact source tips: [`33d88b1cf764f614d3935c11c48decd53386083e`](https://github.com/jackin-project/jackin/commit/33d88b1cf764f614d3935c11c48decd53386083e).

**Disposition:** DEFER. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Choose fallback only after preserving lease ownership, leader-election/CAS, capability isolation and client-lifetime semantics.

**Affected symbols/modules and proposed destination:** `crates/apps/jackin/Cargo.toml`, `crates/apps/jackin/src/bin/usage_broker.rs`, `crates/services/jackin-runtime/`, `crates/services/jackin-usage-broker/`, `crates/adapters/jackin-usage-ffi/`, `native/mise.toml`, `native/Package.swift`, `native/project.yml`, `native/Scripts/run-ui-tests.sh`, `crates/tools/jackin-xtask/src/desktop/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Always fail closed versus always enable fallback after any sidecar failure.

**Selection rationale:** The source changes negative activation behavior. Main foreground liveness and current auth architecture require explicit safety proof before choosing it.

**Dependencies:** TASK-009, TASK-010, TASK-011, TASK-012, TASK-013; implement and review through [TASK-015](05-execution-tasks.md#task-015).

**Potential regression:** In-process lifetime weaker than sidecar lifetime; hiding a packaging/security error.

**Required tests:** Missing/unusable sidecar; uncreatable data dir; owner death; concurrent activation; no capability bypass. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-012"></a>
### CHG-012 — Permanent coordination inode extraction

**Sources:** [B036](branches/b036-policy-0b27f2016591.md), [B047](branches/b047-stash-alias-53dd4c6affcc.md), [B048](branches/b048-stash-alias-b7b19a9f3034.md), [B050](branches/b050-stash-alias-bfd7103e838f.md), [B052](branches/b052-stash-alias-01522f00a01b.md), [B054](branches/b054-stash-alias-68ff05fe0f69.md), [B056](branches/b056-stash-alias-cbbb9b2a9c6e.md), [B057](branches/b057-stash-alias-09031c14e05c.md), [B059](branches/b059-stash-alias-ebc2e2faed24.md), [B060](branches/b060-stash-alias-15ca898cb2c6.md), [B062](branches/b062-stash-alias-dca0343dfb05.md), [B068](branches/b068-stash-alias-929b5577bb5f.md), [B069](branches/b069-stash-alias-8836a20a65fb.md), [B070](branches/b070-stash-alias-f574e4820623.md), [B072](branches/b072-stash-alias-64003e4d826d.md), [B075](branches/b075-stash-alias-770a8c6e1ecf.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`80ab98038e945ec48c82dad241ebf33aa0efa82d`](https://github.com/jackin-project/jackin/commit/80ab98038e945ec48c82dad241ebf33aa0efa82d), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Preserve permanent owned private regular coordination inodes, safe namespace placement and non-unlinking lock release.

**Affected symbols/modules and proposed destination:** `crates/services/jackin-runtime-coordination/`, `crates/services/jackin-runtime-repo-cache/`, `crates/services/jackin-runtime-cleanup-absent-guards/`, `crates/services/jackin-runtime/src/runtime/apple_container.rs`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Assume the crate name in P proves all stash code/tests/untracked files were incorporated.

**Selection rationale:** The stash contains specific path/ownership/link-count safeguards. Every parent payload must be reconciled.

**Dependencies:** TASK-006; implement and review through [TASK-010](05-execution-tasks.md#task-010).

**Potential regression:** Lock identity substitution or pruning active authority.

**Required tests:** Prunable overlap, parent/symlink traversal, missing/permission ambiguity, wrong owner/mode/type/link count, cleanup and lock release. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-013"></a>
### CHG-013 — Repo-cache lock and identity extraction

**Sources:** [B036](branches/b036-policy-0b27f2016591.md), [B047](branches/b047-stash-alias-53dd4c6affcc.md), [B048](branches/b048-stash-alias-b7b19a9f3034.md), [B050](branches/b050-stash-alias-bfd7103e838f.md), [B052](branches/b052-stash-alias-01522f00a01b.md), [B054](branches/b054-stash-alias-68ff05fe0f69.md), [B056](branches/b056-stash-alias-cbbb9b2a9c6e.md), [B057](branches/b057-stash-alias-09031c14e05c.md), [B059](branches/b059-stash-alias-ebc2e2faed24.md), [B060](branches/b060-stash-alias-15ca898cb2c6.md), [B062](branches/b062-stash-alias-dca0343dfb05.md), [B068](branches/b068-stash-alias-929b5577bb5f.md), [B069](branches/b069-stash-alias-8836a20a65fb.md), [B070](branches/b070-stash-alias-f574e4820623.md), [B072](branches/b072-stash-alias-64003e4d826d.md), [B075](branches/b075-stash-alias-770a8c6e1ecf.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`80ab98038e945ec48c82dad241ebf33aa0efa82d`](https://github.com/jackin-project/jackin/commit/80ab98038e945ec48c82dad241ebf33aa0efa82d), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Retain reentrant RepoLock and per-path last-handle semantics; audit host-aware repository identity against the selected contract.

**Affected symbols/modules and proposed destination:** `crates/services/jackin-runtime-coordination/`, `crates/services/jackin-runtime-repo-cache/`, `crates/services/jackin-runtime-cleanup-absent-guards/`, `crates/services/jackin-runtime/src/runtime/apple_container.rs`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Copy host-stripping repo_matches behavior without security/compatibility review.

**Selection rationale:** Inspected code normalizes owner/repo across URL forms, raising an identity question that requires actual destination comparison.

**Dependencies:** TASK-006; implement and review through [TASK-010](05-execution-tasks.md#task-010).

**Potential regression:** Wrong cached repository reuse or lock lifetime regressions; not a confirmed exploit from inspection alone.

**Required tests:** Nested same-process handles, different paths, blocking-pool acquisition/cancellation, same owner/repo on different hosts. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-014"></a>
### CHG-014 — Purge absence guard extraction

**Sources:** [B036](branches/b036-policy-0b27f2016591.md), [B042](branches/b042-policy-rescue-4dc1add6a9f0.md), [B045](branches/b045-dirty-868886b2213b.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`45e834683c8d5ca0fea1c7d3676cb136efa4acb1`](https://github.com/jackin-project/jackin/commit/45e834683c8d5ca0fea1c7d3676cb136efa4acb1), [`bff77099f8273a4d480a0e430cba66afc4197543`](https://github.com/jackin-project/jackin/commit/bff77099f8273a4d480a0e430cba66afc4197543), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Preserve ensure_absent_for_purge_with preconditions and failure behavior in the final cleanup-absent-guards owner.

**Affected symbols/modules and proposed destination:** `crates/services/jackin-runtime-coordination/`, `crates/services/jackin-runtime-repo-cache/`, `crates/services/jackin-runtime-cleanup-absent-guards/`, `crates/services/jackin-runtime/src/runtime/apple_container.rs`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Add back intermediate jackin-runtime-apple-container-ensure-absent solely to match an archived path.

**Selection rationale:** P manifest uses the cleanup-absent-guards leaf; s61 documentation records a prior intermediate extraction and unverified old quality results.

**Dependencies:** TASK-006; implement and review through [TASK-010](05-execution-tasks.md#task-010).

**Potential regression:** Safety checks lost during rename or stale baseline claims accepted as current.

**Required tests:** Guard outcomes, interrupted purge, live/container ambiguity and unchanged test identities. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-015"></a>
### CHG-015 — Model/effort CLI-to-runtime propagation

**Sources:** [B002](branches/b002-archive-docs-33af45993b41.md), [B079](branches/b079-wip-ebf572949c76.md). Exact source tips: [`b1e08b880223594cceccf3ea44322d44e3cdf984`](https://github.com/jackin-project/jackin/commit/b1e08b880223594cceccf3ea44322d44e3cdf984), [`d04467f05f1183008322047b1828c201e551e947`](https://github.com/jackin-project/jackin/commit/d04467f05f1183008322047b1828c201e551e947).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Validate model/effort and propagate them through dry-run, launch and restore while preserving old JSON fields.

**Affected symbols/modules and proposed destination:** `crates/apps/jackin/src/app/load_cmd.rs`, `crates/apps/jackin/src/cli/role.rs`, `crates/services/jackin-runtime/`, `crates/core/jackin-core/`, `crates/apps/jackin-capsule/`, `crates/services/jackin-runtime-launch-load-options/`, `crates/services/jackin-runtime-launch-dry-run/`, `crates/services/jackin-runtime-launch-restore/`, `crates/services/jackin-runtime-launch-programmatic-selection/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Keep only the parser/help or duplicate historical propagation paths.

**Selection rationale:** The reflog snapshot adds CLI behavior; historical build-split records identify complementary runtime propagation.

**Dependencies:** TASK-006; implement and review through [TASK-011](05-execution-tasks.md#task-011).

**Potential regression:** Accepted-but-ignored user options or restored sessions selecting the wrong agent.

**Required tests:** Trimmed/empty model, four effort values, invalid effort, defaults/null JSON, actual launch/restore selection. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-016"></a>
### CHG-016 — OMP/config/restore durability obligations

**Sources:** [B001](branches/b001-archive-docs-437a9e81ba65.md), [B002](branches/b002-archive-docs-33af45993b41.md), [B038](branches/b038-baseline-f921bd05e68b.md). Exact source tips: [`1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff), [`3cb98f2a634c1e660e501489d7383329569db7a3`](https://github.com/jackin-project/jackin/commit/3cb98f2a634c1e660e501489d7383329569db7a3), [`b1e08b880223594cceccf3ea44322d44e3cdf984`](https://github.com/jackin-project/jackin/commit/b1e08b880223594cceccf3ea44322d44e3cdf984).

**Disposition:** ADAPT. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Locate and verify final destinations for OMP/WAL snapshot, stored agent identity, config migration/proof and durable cleanup obligations.

**Affected symbols/modules and proposed destination:** `crates/adapters/jackin-omp-store/`, `crates/core/jackin-config/`, `crates/services/jackin-runtime/`, `crates/core/jackin-process/`, `plans/jackin-build-split/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Assume all historical open issues remain present, or that the old squash message proves they are all fixed.

**Selection rationale:** Archived reports give concrete tests and rejected intermediates but are not current test results.

**Dependencies:** TASK-010, TASK-011, TASK-012; implement and review through [TASK-013](05-execution-tasks.md#task-013).

**Potential regression:** Reopening fixed work or silently losing still-missing persistence/security behavior.

**Required tests:** Concurrent writer snapshot, WAL reset suffix, interruption/fsync cleanup, migration golden/ambiguity fixtures, proof invalidation and source discovery. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-017"></a>
### CHG-017 — Per-request diagnostic retry assertions

**Sources:** [B027](branches/b027-archive-sanitized-0739580d021c.md). Exact source tips: [`502e0627d3940e2cd68ae0078910f21452d9ccc1`](https://github.com/jackin-project/jackin/commit/502e0627d3940e2cd68ae0078910f21452d9ccc1).

**Disposition:** KEEP. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Each retry must carry one matching log event, not merely a correct flattened total.

**Affected symbols/modules and proposed destination:** `crates/services/jackin-diagnostics/`, `crates/services/jackin-telemetry/`, `crates/testing/jackin-otlp-testbed/`, `crates/adapters/jackin-docker/src/docker_client/tests.rs`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Accept a 0/1/2 split over three requests because its sum is three.

**Selection rationale:** The sanitized patch directly strengthens wire_failure_support assertions.

**Dependencies:** TASK-006; implement and review through [TASK-012](05-execution-tasks.md#task-012).

**Potential regression:** Retry-payload corruption concealed by aggregate counts.

**Required tests:** Negative 0/1/2 distribution and positive 1/1/1; traces/logs/metrics request counts. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-018"></a>
### CHG-018 — Privacy, TLS and connection-choice tests after splitting

**Sources:** [B002](branches/b002-archive-docs-33af45993b41.md), [B036](branches/b036-policy-0b27f2016591.md), [B041](branches/b041-policy-rescue-bb88995b68e3.md), [B046](branches/b046-dirty-fc03ff5ffa1d.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`b1e08b880223594cceccf3ea44322d44e3cdf984`](https://github.com/jackin-project/jackin/commit/b1e08b880223594cceccf3ea44322d44e3cdf984), [`b72046bbc9314e93cddf4d8252570dc375935f56`](https://github.com/jackin-project/jackin/commit/b72046bbc9314e93cddf4d8252570dc375935f56), [`ef238a7e4720fe1be8e8e295e1c502f2ba529e0b`](https://github.com/jackin-project/jackin/commit/ef238a7e4720fe1be8e8e295e1c502f2ba529e0b), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Retain no-private-value OTLP assertions, TLS feature guard and invalid connection-choice tests in the selected split modules.

**Affected symbols/modules and proposed destination:** `crates/services/jackin-diagnostics/`, `crates/services/jackin-telemetry/`, `crates/testing/jackin-otlp-testbed/`, `crates/adapters/jackin-docker/src/docker_client/tests.rs`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Treat removal from an old test file as either proof of loss or proof of harmless relocation.

**Selection rationale:** The source snapshots show large test-module moves and historical rejected redaction intermediates.

**Dependencies:** TASK-006; implement and review through [TASK-012](05-execution-tasks.md#task-012).

**Potential regression:** Security regressions hidden by structural test churn.

**Required tests:** Per-assertion catalog map, payload inspection, TLS compile/feature cases, rejected endpoint choices. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-019"></a>
### CHG-019 — Durable push-head CI evidence qualification

**Sources:** [B005](branches/b005-archive-keeper-efa220a4ecda.md), [B012](branches/b012-archive-keeper-27c1d7ad2d95.md), [B020](branches/b020-archive-keeper-83afb29df592.md). Exact source tips: [`14bd1471616056e07c81c59d59eafc0963b16865`](https://github.com/jackin-project/jackin/commit/14bd1471616056e07c81c59d59eafc0963b16865), [`1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d`](https://github.com/jackin-project/jackin/commit/1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d), [`c7b8a95856cce6f5e8442f0e6fafcf025a089132`](https://github.com/jackin-project/jackin/commit/c7b8a95856cce6f5e8442f0e6fafcf025a089132).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Keep explicit obligations and schema-5 predecessor, completed-run, artifact-byte/hash and workflow/Git binding proofs.

**Affected symbols/modules and proposed destination:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `mise.toml`, `mise.lock`, `native/mise.toml`, `scripts/release-preview-package`, `scripts/test-mbx-cargo-wrapper.py`, `.github-gen/`, `.github/workflows/`, `crates/tools/jackin-xtask/src/ci_evidence/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Observed-run-only denominator, fixture-derived green status or unbound success-like conclusions.

**Selection rationale:** The ledger and hardening sources deliberately make missing/conflicting/unqualified evidence fail closed.

**Dependencies:** TASK-005; implement and review through [TASK-006](05-execution-tasks.md#task-006).

**Potential regression:** False-green reliability or stale evidence reused across heads.

**Required tests:** Missing producer, missing predecessor, wrong before SHA, mismatched workflow/tree/digest, partial status, fixture and conflicting terminal observations. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-020"></a>
### CHG-020 — First-parent-only post-merge denominator

**Sources:** [B030](branches/b030-archive-src3-841f56f9a292.md). Exact source tips: [`12dea6c5fd0f9d2c594d5ad3159e6493d03352f4`](https://github.com/jackin-project/jackin/commit/12dea6c5fd0f9d2c594d5ad3159e6493d03352f4).

**Disposition:** SUPERSEDED. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Use Git first-parent history as repository evidence, not as proof of which push heads produced workflow obligations.

**Affected symbols/modules and proposed destination:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `mise.toml`, `mise.lock`, `native/mise.toml`, `scripts/release-preview-package`, `scripts/test-mbx-cargo-wrapper.py`, `.github-gen/`, `.github/workflows/`, `crates/tools/jackin-xtask/src/ci_evidence/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** 12dea6c5 first-parent model versus durable push-head/artifact model at 14bd1471.

**Selection rationale:** The stronger source candidate proves actual event grouping; negative fixtures/conflict handling from the first-parent candidate still merit retention. This is a proposed design supersession, not proof it is already integrated.

**Dependencies:** TASK-005; implement and review through [TASK-006](05-execution-tasks.md#task-006).

**Potential regression:** Counting obligations for commits that were never distinct push heads, or missing true unstarted obligations.

**Required tests:** Multi-commit push represented by one workflow head; gaps and fixture inputs stay unqualified. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-021"></a>
### CHG-021 — Installer-wide sccache/backend activation

**Sources:** [B032](branches/b032-archive-src3-70770d79115b.md). Exact source tips: [`b1dc105acffb68ba71fff86f9c18dc5ff77de924`](https://github.com/jackin-project/jackin/commit/b1dc105acffb68ba71fff86f9c18dc5ff77de924).

**Disposition:** REJECT. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Do not enable RUSTC_WRAPPER/GHA backend globally before installation.

**Affected symbols/modules and proposed destination:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `mise.toml`, `mise.lock`, `native/mise.toml`, `scripts/release-preview-package`, `scripts/test-mbx-cargo-wrapper.py`, `.github-gen/`, `.github/workflows/`, `crates/tools/jackin-xtask/src/ci_evidence/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Global shared-workflow environment versus task-scoped guarded successor.

**Selection rationale:** The inspected source exports wrapper settings before setup; successor 41dd148e explicitly confines them. Current selected cache policy may remove the wrapper altogether.

**Dependencies:** TASK-005; implement and review through [TASK-006](05-execution-tasks.md#task-006).

**Potential regression:** Bootstrap failures and unprovisioned/global backend assumptions.

**Required tests:** Installer env clean; absent wrapper/backend does not fake validation; current cache contract tests. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-022"></a>
### CHG-022 — Tool wrapper isolation and provisioning contracts

**Sources:** [B039](branches/b039-misrouted-e096ae35b5a8.md). Exact source tips: [`41dd148e514d1bf3f84401f0ecbf8e87b58bb261`](https://github.com/jackin-project/jackin/commit/41dd148e514d1bf3f84401f0ecbf8e87b58bb261).

**Disposition:** ADAPT. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Retain installer isolation and guarded backend availability checks where applicable to the selected toolchain.

**Affected symbols/modules and proposed destination:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `mise.toml`, `mise.lock`, `native/mise.toml`, `scripts/release-preview-package`, `scripts/test-mbx-cargo-wrapper.py`, `.github-gen/`, `.github/workflows/`, `crates/tools/jackin-xtask/src/ci_evidence/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Restore old sccache workflows merely to preserve their literal string assertions.

**Selection rationale:** The successor improves scope but contains tool-dependent smoke skips and old generated contract assumptions.

**Dependencies:** TASK-005; implement and review through [TASK-006](05-execution-tasks.md#task-006).

**Potential regression:** Stale tests forcing obsolete architecture or early-return passes misreported as validation.

**Required tests:** Provision-before-use, wrapper absence, endpoint/token availability, actual executed smoke count. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-023"></a>
### CHG-023 — Native product graph and full Apple obligations

**Sources:** [B015](branches/b015-archive-keeper-dc5b26803593.md), [B025](branches/b025-archive-staged-67aa054ffa3d.md), [B031](branches/b031-archive-src3-75be4f6a7848.md), [B033](branches/b033-archive-src3-9264ed8676fb.md), [B035](branches/b035-modernize-9949620d3954.md), [B036](branches/b036-policy-0b27f2016591.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`0437497d7f22fbdb7c8aad986be932704d707fe9`](https://github.com/jackin-project/jackin/commit/0437497d7f22fbdb7c8aad986be932704d707fe9), [`432e0108ce90b64b428c69c27d4a2ac23e559379`](https://github.com/jackin-project/jackin/commit/432e0108ce90b64b428c69c27d4a2ac23e559379), [`7646c61bd4261a5908693bdaf0a6ecd9e9643448`](https://github.com/jackin-project/jackin/commit/7646c61bd4261a5908693bdaf0a6ecd9e9643448), [`9ef617d014df15c6d967501bcbbdcb3689ed453c`](https://github.com/jackin-project/jackin/commit/9ef617d014df15c6d967501bcbbdcb3689ed453c), [`d16cd795d4a539b84ba5f44290663b2363d49350`](https://github.com/jackin-project/jackin/commit/d16cd795d4a539b84ba5f44290663b2363d49350), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Keep one producer-consumer XCFramework graph, profile/floor, harnesses, UI count proof, stamping, dSYM and native format/lint/dead-code gates.

**Affected symbols/modules and proposed destination:** `crates/apps/jackin/Cargo.toml`, `crates/apps/jackin/src/bin/usage_broker.rs`, `crates/services/jackin-runtime/`, `crates/services/jackin-usage-broker/`, `crates/adapters/jackin-usage-ffi/`, `native/mise.toml`, `native/Package.swift`, `native/project.yml`, `native/Scripts/run-ui-tests.sh`, `crates/tools/jackin-xtask/src/desktop/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Duplicate synthetic Swift units or a partial generic migration that drops task-owned obligations.

**Selection rationale:** Several archived candidates preserve different portions of native generation; selected P/T native Mise flow must carry the complete contract.

**Dependencies:** TASK-009, TASK-010, TASK-011, TASK-012, TASK-013; implement and review through [TASK-015](05-execution-tasks.md#task-015).

**Potential regression:** Swift resolves a missing/wrong product or validates fewer behaviors after migration.

**Required tests:** Fresh native build, correct producer paths, CI/merge/scheduled commands and macOS OrbStack release lane. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-024"></a>
### CHG-024 — Intentionally unregenerated pre-M1 state

**Sources:** [B029](branches/b029-archive-src3-21f83e144de4.md). Exact source tips: [`8fb49688a6408627e22118f92d24f47785564979`](https://github.com/jackin-project/jackin/commit/8fb49688a6408627e22118f92d24f47785564979).

**Disposition:** REJECT. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Do not integrate mismatched new inputs and old generated outputs.

**Affected symbols/modules and proposed destination:** `crates/apps/jackin/Cargo.toml`, `crates/apps/jackin/src/bin/usage_broker.rs`, `crates/services/jackin-runtime/`, `crates/services/jackin-usage-broker/`, `crates/adapters/jackin-usage-ffi/`, `native/mise.toml`, `native/Package.swift`, `native/project.yml`, `native/Scripts/run-ui-tests.sh`, `crates/tools/jackin-xtask/src/desktop/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** 8fb49688 inputs-only snapshot versus complete later generation plus current selected contract.

**Selection rationale:** The commit explicitly expects policy --check to fail and says not to land before M1 regeneration.

**Dependencies:** TASK-009, TASK-010, TASK-011, TASK-012, TASK-013; implement and review through [TASK-015](05-execution-tasks.md#task-015).

**Potential regression:** Known-invalid CI state entering main.

**Required tests:** Byte-identical generation from the final reviewed input tree and selected generator product. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-025"></a>
### CHG-025 — WIP musl/GNU tool lock mappings

**Sources:** [B078](branches/b078-wip-6b8347c05938.md). Exact source tips: [`ef40fea11a54ade7911de7622e6e011707241dd4`](https://github.com/jackin-project/jackin/commit/ef40fea11a54ade7911de7622e6e011707241dd4).

**Disposition:** DEFER. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Regenerate and validate supported platform mappings instead of copying saved lock rows.

**Affected symbols/modules and proposed destination:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `mise.toml`, `mise.lock`, `native/mise.toml`, `scripts/release-preview-package`, `scripts/test-mbx-cargo-wrapper.py`, `.github-gen/`, `.github/workflows/`, `crates/tools/jackin-xtask/src/ci_evidence/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Hand-retain linux-musl aliases pointing at GNU binaries.

**Selection rationale:** The patch shows those mappings but planning did not execute platform compatibility checks.

**Dependencies:** TASK-005; implement and review through [TASK-006](05-execution-tasks.md#task-006).

**Potential regression:** Wrong libc/architecture artifact selected by an apparently valid lock.

**Required tests:** Platform-specific resolution, checksums/signatures and actual binary/runtime compatibility. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-026"></a>
### CHG-026 — Zig minisign provenance

**Sources:** [B078](branches/b078-wip-6b8347c05938.md). Exact source tips: [`ef40fea11a54ade7911de7622e6e011707241dd4`](https://github.com/jackin-project/jackin/commit/ef40fea11a54ade7911de7622e6e011707241dd4).

**Disposition:** KEEP. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Preserve verified minisign provenance when producing the selected lockfile.

**Affected symbols/modules and proposed destination:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `mise.toml`, `mise.lock`, `native/mise.toml`, `scripts/release-preview-package`, `scripts/test-mbx-cargo-wrapper.py`, `.github-gen/`, `.github/workflows/`, `crates/tools/jackin-xtask/src/ci_evidence/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Drop provenance while reconciling platform changes.

**Selection rationale:** The saved lock explicitly adds a provenance field. Keeping a field is conditional on valid generated/signature evidence.

**Dependencies:** TASK-005; implement and review through [TASK-006](05-execution-tasks.md#task-006).

**Potential regression:** Supply-chain evidence silently weakened.

**Required tests:** Pinned tool signature/provenance and reproducible lock generation. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-027"></a>
### CHG-027 — Staged-only affected-crate selection

**Sources:** [B010](branches/b010-archive-keeper-8fd347cb734e.md). Exact source tips: [`6d65e649855f98f133f58f605727b098b5d4eabb`](https://github.com/jackin-project/jackin/commit/6d65e649855f98f133f58f605727b098b5d4eabb).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Select the union of HEAD↔index and index↔worktree dependency changes; widen safely on structural/parse ambiguity.

**Affected symbols/modules and proposed destination:** `crates/tools/jackin-xtask/src/clippy_affected.rs`, `crates/tools/jackin-xtask/src/clippy_affected/`, `goal-references/02-change-coverage-matrix.md`, `.config/nextest.toml`, `flaky-tests.toml`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Compare only HEAD to working tree and miss staged-only changes.

**Selection rationale:** Inspected source and tests address staged changes even when working bytes are reverted to HEAD.

**Dependencies:** TASK-009, TASK-013, TASK-015; implement and review through [TASK-014](05-execution-tasks.md#task-014).

**Potential regression:** Pre-commit checks silently skip affected crates.

**Required tests:** Comment-only no-op, staged version/structure changes, two-pair union, either-pair parse failure. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-028"></a>
### CHG-028 — Preserved test identities through policy splits

**Sources:** [B036](branches/b036-policy-0b27f2016591.md), [B041](branches/b041-policy-rescue-bb88995b68e3.md), [B046](branches/b046-dirty-fc03ff5ffa1d.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`b72046bbc9314e93cddf4d8252570dc375935f56`](https://github.com/jackin-project/jackin/commit/b72046bbc9314e93cddf4d8252570dc375935f56), [`ef238a7e4720fe1be8e8e295e1c502f2ba529e0b`](https://github.com/jackin-project/jackin/commit/ef238a7e4720fe1be8e8e295e1c502f2ba529e0b), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** COMBINE. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Preserve assertion-level coverage while splitting large tests and adjusting imports/helpers.

**Affected symbols/modules and proposed destination:** `crates/tools/jackin-xtask/src/clippy_affected.rs`, `crates/tools/jackin-xtask/src/clippy_affected/`, `goal-references/02-change-coverage-matrix.md`, `.config/nextest.toml`, `flaky-tests.toml`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Trust total test counts or file-presence alone.

**Selection rationale:** Comparison responses are capped and some moved-file stats are zero despite actual content.

**Dependencies:** TASK-009, TASK-013, TASK-015; implement and review through [TASK-014](05-execution-tasks.md#task-014).

**Potential regression:** False confidence from inflated counts or undetected orphan modules.

**Required tests:** Per-package/feature/test identity map, body review, actual nonzero selection and platform disposition. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-029"></a>
### CHG-029 — Proof-only production debris

**Sources:** [B004](branches/b004-archive-keeper-5aacaad28a02.md), [B006](branches/b006-archive-keeper-856b4d85181f.md), [B007](branches/b007-archive-keeper-74cf60fb6d52.md), [B008](branches/b008-archive-keeper-77baf007acb6.md), [B009](branches/b009-archive-keeper-dd596b9a057a.md), [B011](branches/b011-archive-keeper-64de0538eaa5.md), [B013](branches/b013-archive-keeper-e118ca686ade.md), [B014](branches/b014-archive-keeper-4d364008016b.md), [B016](branches/b016-archive-keeper-f164586efdc9.md), [B017](branches/b017-archive-keeper-b3531a0bac69.md), [B018](branches/b018-archive-keeper-6176f4f904dc.md), [B019](branches/b019-archive-keeper-7cddcc8364f3.md), [B022](branches/b022-archive-keeper-0058bbcc86ff.md), [B023](branches/b023-archive-keeper-a760f85ce5ae.md), [B024](branches/b024-archive-keeper-d898a35778ef.md). Exact source tips: [`07beb1c705ee8f813d2c8f5a438291cc031c9d1a`](https://github.com/jackin-project/jackin/commit/07beb1c705ee8f813d2c8f5a438291cc031c9d1a), [`0a898cd246fbbf62f3b463f0097ffd6a368b1e07`](https://github.com/jackin-project/jackin/commit/0a898cd246fbbf62f3b463f0097ffd6a368b1e07), [`1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d`](https://github.com/jackin-project/jackin/commit/1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d), [`256b88d1a7e85b4855d1e64399e2af4482026e17`](https://github.com/jackin-project/jackin/commit/256b88d1a7e85b4855d1e64399e2af4482026e17), [`5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e`](https://github.com/jackin-project/jackin/commit/5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e), [`5a8264746286a981debb099646a6354d9b10e0cb`](https://github.com/jackin-project/jackin/commit/5a8264746286a981debb099646a6354d9b10e0cb), [`5c4aa5eba5aaea2f2bfa2339f91066567a8e9979`](https://github.com/jackin-project/jackin/commit/5c4aa5eba5aaea2f2bfa2339f91066567a8e9979), [`631d734b99ee9b413230ddb210698a5c80f41d70`](https://github.com/jackin-project/jackin/commit/631d734b99ee9b413230ddb210698a5c80f41d70), [`66227a0033a6f8b917e684de1776753e33978590`](https://github.com/jackin-project/jackin/commit/66227a0033a6f8b917e684de1776753e33978590), [`77508d454bfe9a903043586db2c8c5faa1f9e960`](https://github.com/jackin-project/jackin/commit/77508d454bfe9a903043586db2c8c5faa1f9e960), [`a28ab0a27eccc892decf8a4df55244b70096f631`](https://github.com/jackin-project/jackin/commit/a28ab0a27eccc892decf8a4df55244b70096f631), [`b930e3658c671ca21ab7c25ea1a40349bb69284a`](https://github.com/jackin-project/jackin/commit/b930e3658c671ca21ab7c25ea1a40349bb69284a), [`df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb`](https://github.com/jackin-project/jackin/commit/df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb), [`e978dda9643079998f342380ccd123562139858a`](https://github.com/jackin-project/jackin/commit/e978dda9643079998f342380ccd123562139858a), [`ffe6d6767bf2be30f54df7d245574b5980c05eac`](https://github.com/jackin-project/jackin/commit/ffe6d6767bf2be30f54df7d245574b5980c05eac).

**Disposition:** REJECT. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Exclude inspected hook-proof functions, duplicate definitions, cache comments, empty probe commits and malformed untracked production source.

**Affected symbols/modules and proposed destination:** `crates/tools/jackin-xtask/src/clippy_affected.rs`, `crates/tools/jackin-xtask/src/clippy_affected/`, `goal-references/02-change-coverage-matrix.md`, `.config/nextest.toml`, `flaky-tests.toml`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Merge every archived tip regardless of intentional test damage.

**Selection rationale:** Visible patches/messages identify proof-only or deliberately malformed payloads. This rejection does not cover unseen history or parent trees.

**Dependencies:** TASK-009, TASK-013, TASK-015; implement and review through [TASK-014](05-execution-tasks.md#task-014).

**Potential regression:** Accidentally rejecting useful ancestral changes along with a probe tip.

**Required tests:** Isolated hook/stale-head/cache regression fixtures; no probe debris in final source. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-030"></a>
### CHG-030 — Whole synthetic conflict snapshot

**Sources:** [B003](branches/b003-archive-recovery-583438c051c5.md). Exact source tips: [`fea3ade6830912017f2ad80c0c5a9d1acfbde0b7`](https://github.com/jackin-project/jackin/commit/fea3ade6830912017f2ad80c0c5a9d1acfbde0b7).

**Disposition:** REJECT. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Do not merge the fea3ade unresolved conflict tree or infer historical ancestry from its synthetic parent.

**Affected symbols/modules and proposed destination:** `crates/adapters/jackin-omp-store/`, `crates/core/jackin-config/`, `crates/services/jackin-runtime/`, `crates/core/jackin-process/`, `plans/jackin-build-split/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Wholesale import to preserve every byte.

**Selection rationale:** The source explicitly disclaims buildability; useful individual hunks remain separately subject to salvage review.

**Dependencies:** TASK-010, TASK-011, TASK-012; implement and review through [TASK-013](05-execution-tasks.md#task-013).

**Potential regression:** Broken source and fabricated history entering the integration.

**Required tests:** Per-conflict-side audit, meaningful hunk dispositions and no unintended merge markers. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-031"></a>
### CHG-031 — Historical evidence and operating records

**Sources:** [B001](branches/b001-archive-docs-437a9e81ba65.md), [B002](branches/b002-archive-docs-33af45993b41.md), [B021](branches/b021-archive-keeper-45846051b2ee.md), [B037](branches/b037-usage-1c388dc4de71.md), [B045](branches/b045-dirty-868886b2213b.md). Exact source tips: [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7), [`3cb98f2a634c1e660e501489d7383329569db7a3`](https://github.com/jackin-project/jackin/commit/3cb98f2a634c1e660e501489d7383329569db7a3), [`45e834683c8d5ca0fea1c7d3676cb136efa4acb1`](https://github.com/jackin-project/jackin/commit/45e834683c8d5ca0fea1c7d3676cb136efa4acb1), [`b1e08b880223594cceccf3ea44322d44e3cdf984`](https://github.com/jackin-project/jackin/commit/b1e08b880223594cceccf3ea44322d44e3cdf984), [`d0bdbc021350dbd7b9f5f011b8e96d2fa3858f78`](https://github.com/jackin-project/jackin/commit/d0bdbc021350dbd7b9f5f011b8e96d2fa3858f78).

**Disposition:** ADAPT. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Retain provenance, rejected alternatives and unresolved obligations with accurate dates and verification scope.

**Affected symbols/modules and proposed destination:** `GOAL.md`, `GOAL_PROMPT.md`, `goal-references/`, `TESTING.md`, `BRANCHING.md`, `plans/claude-unattended-monitor/`, `plans/jackin-build-split/`, `plans/cicd-reliability/`, `native/mise.toml`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Convert historical run claims into present PASS or overwrite current commands with obsolete paths.

**Selection rationale:** The records contain important constraints but reference other hosts, older revisions and pending live actions.

**Dependencies:** TASK-014, TASK-015; implement and review through [TASK-016](05-execution-tasks.md#task-016).

**Potential regression:** An executable runbook accidentally performs a live cutover or claims unearned readiness.

**Required tests:** Link/source SHA checks, exact current task commands and NOT_RUN/live distinction. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-032"></a>
### CHG-032 — Sanitized audit tip with no returned patch

**Sources:** [B026](branches/b026-archive-sanitized-0ced81950334.md). Exact source tips: [`2d97bba390864fbdc79aa6566e3394df938408af`](https://github.com/jackin-project/jackin/commit/2d97bba390864fbdc79aa6566e3394df938408af).

**Disposition:** DEFER. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Inspect actual tree/parents and retained documentation before closing the source.

**Affected symbols/modules and proposed destination:** `goal-references/branches/`, `goal-references/02-change-coverage-matrix.md`, `goal-references/03-overlaps-and-conflicts.md`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Infer the entire branch is empty from null diff/files.

**Selection rationale:** Only metadata is available for the sanitized tip; the missing-patch condition is a specific evidence gap.

**Dependencies:** TASK-001; implement and review through [TASK-002](05-execution-tasks.md#task-002).

**Potential regression:** Sanitized or empty-looking tips concealing meaningful ancestry.

**Required tests:** Tree equality, parent comparison, direct target diff and historical documentation closure. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

<a id="chg-033"></a>
### CHG-033 — Strict warm CI and flaky-test accounting

**Sources:** [B036](branches/b036-policy-0b27f2016591.md), [B077](branches/b077-policy-517d17ddf7fc.md). Exact source tips: [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f).

**Disposition:** KEEP. **Approval:** PROPOSED; independent review NOT_RUN. **Destination revision:** not created.

**Required behavior:** Preserve documented offline fan-out, exact semantic cache keys, canonical writer, first-attempt accounting, shrink-only quarantine and warm one-minute contract.

**Affected symbols/modules and proposed destination:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `mise.toml`, `mise.lock`, `native/mise.toml`, `scripts/release-preview-package`, `scripts/test-mbx-cargo-wrapper.py`, `.github-gen/`, `.github/workflows/`, `crates/tools/jackin-xtask/src/ci_evidence/`. These are the owning task's scope; source-specific paths appear in the linked dossiers. TASK-004 resolves renamed child files before writes.

**Alternatives considered:** Inflate timeouts, broaden cache reuse or conceal reruns to claim success.

**Selection rationale:** Pinned P TESTING.md states these requirements, but no planning run established achieved timing.

**Dependencies:** TASK-005; implement and review through [TASK-006](05-execution-tasks.md#task-006).

**Potential regression:** Performance or correctness degraded under a superficially faster cache.

**Required tests:** Cold/exact-warm/changed-source/changed-contract runs, forbidden download/build markers, retry/quarantine and per-job timing evidence. Use the owning task's exact pinned commands; zero matches or a tool-dependent early return cannot satisfy acceptance.

**Independent reviewer:** NOT_RUN. Assign a reviewer other than the implementer and record an immutable reviewed SHA. **Evidence to close:** full source/hunk mapping, actual destination symbols/tests, command results, reviewer verdict and final integration SHA. A KEEP/REJECT label alone does not close the source history.

## Mandatory unexamined-remainder ledger

Each row below is an **open coverage obligation**, not an approved omission of important functionality. It covers source history/parent/file portions not fully examined in planning. TASK-002 must enumerate and classify the remainder, cross-review it and replace this open obligation with exact change rows. Final integration cannot proceed while any material remainder is merely unexamined.

| Remainder ID | Source ref | Pinned tip | State / disposition | Closure owner |
| --- | --- | --- | --- | --- |
| REM-001 | [B001](branches/b001-archive-docs-437a9e81ba65.md) | `3cb98f2a634c1e660e501489d7383329569db7a3` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-002 | [B002](branches/b002-archive-docs-33af45993b41.md) | `b1e08b880223594cceccf3ea44322d44e3cdf984` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-003 | [B003](branches/b003-archive-recovery-583438c051c5.md) | `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-004 | [B004](branches/b004-archive-keeper-5aacaad28a02.md) | `0a898cd246fbbf62f3b463f0097ffd6a368b1e07` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-005 | [B005](branches/b005-archive-keeper-efa220a4ecda.md) | `1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-006 | [B006](branches/b006-archive-keeper-856b4d85181f.md) | `1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-007 | [B007](branches/b007-archive-keeper-74cf60fb6d52.md) | `5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-008 | [B008](branches/b008-archive-keeper-77baf007acb6.md) | `5a8264746286a981debb099646a6354d9b10e0cb` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-009 | [B009](branches/b009-archive-keeper-dd596b9a057a.md) | `5c4aa5eba5aaea2f2bfa2339f91066567a8e9979` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-010 | [B010](branches/b010-archive-keeper-8fd347cb734e.md) | `6d65e649855f98f133f58f605727b098b5d4eabb` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-011 | [B011](branches/b011-archive-keeper-64de0538eaa5.md) | `07beb1c705ee8f813d2c8f5a438291cc031c9d1a` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-012 | [B012](branches/b012-archive-keeper-27c1d7ad2d95.md) | `14bd1471616056e07c81c59d59eafc0963b16865` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-013 | [B013](branches/b013-archive-keeper-e118ca686ade.md) | `256b88d1a7e85b4855d1e64399e2af4482026e17` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-014 | [B014](branches/b014-archive-keeper-4d364008016b.md) | `631d734b99ee9b413230ddb210698a5c80f41d70` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-015 | [B015](branches/b015-archive-keeper-dc5b26803593.md) | `7646c61bd4261a5908693bdaf0a6ecd9e9643448` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-016 | [B016](branches/b016-archive-keeper-f164586efdc9.md) | `66227a0033a6f8b917e684de1776753e33978590` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-017 | [B017](branches/b017-archive-keeper-b3531a0bac69.md) | `77508d454bfe9a903043586db2c8c5faa1f9e960` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-018 | [B018](branches/b018-archive-keeper-6176f4f904dc.md) | `a28ab0a27eccc892decf8a4df55244b70096f631` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-019 | [B019](branches/b019-archive-keeper-7cddcc8364f3.md) | `b930e3658c671ca21ab7c25ea1a40349bb69284a` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-020 | [B020](branches/b020-archive-keeper-83afb29df592.md) | `c7b8a95856cce6f5e8442f0e6fafcf025a089132` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-021 | [B021](branches/b021-archive-keeper-45846051b2ee.md) | `d0bdbc021350dbd7b9f5f011b8e96d2fa3858f78` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-022 | [B022](branches/b022-archive-keeper-0058bbcc86ff.md) | `df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-023 | [B023](branches/b023-archive-keeper-a760f85ce5ae.md) | `e978dda9643079998f342380ccd123562139858a` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-024 | [B024](branches/b024-archive-keeper-d898a35778ef.md) | `ffe6d6767bf2be30f54df7d245574b5980c05eac` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-025 | [B025](branches/b025-archive-staged-67aa054ffa3d.md) | `0437497d7f22fbdb7c8aad986be932704d707fe9` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-026 | [B026](branches/b026-archive-sanitized-0ced81950334.md) | `2d97bba390864fbdc79aa6566e3394df938408af` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-027 | [B027](branches/b027-archive-sanitized-0739580d021c.md) | `502e0627d3940e2cd68ae0078910f21452d9ccc1` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-028 | [B028](branches/b028-archive-src3-95287c049e3e.md) | `6ce20ce13cb87c5ab89b44d72698fa0d68aacf39` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-029 | [B029](branches/b029-archive-src3-21f83e144de4.md) | `8fb49688a6408627e22118f92d24f47785564979` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-030 | [B030](branches/b030-archive-src3-841f56f9a292.md) | `12dea6c5fd0f9d2c594d5ad3159e6493d03352f4` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-031 | [B031](branches/b031-archive-src3-75be4f6a7848.md) | `432e0108ce90b64b428c69c27d4a2ac23e559379` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-032 | [B032](branches/b032-archive-src3-70770d79115b.md) | `b1dc105acffb68ba71fff86f9c18dc5ff77de924` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-033 | [B033](branches/b033-archive-src3-9264ed8676fb.md) | `d16cd795d4a539b84ba5f44290663b2363d49350` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-034 | [B034](branches/b034-archive-src3-ac5215e0b69e.md) | `33d88b1cf764f614d3935c11c48decd53386083e` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-035 | [B035](branches/b035-modernize-9949620d3954.md) | `9ef617d014df15c6d967501bcbbdcb3689ed453c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-036 | [B036](branches/b036-policy-0b27f2016591.md) | `fba69d60c2373156dc0dcecee8e085c83776966f` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-037 | [B037](branches/b037-usage-1c388dc4de71.md) | `2e4a62da4bc4406764162a267163389a5fb513a7` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-038 | [B038](branches/b038-baseline-f921bd05e68b.md) | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-039 | [B039](branches/b039-misrouted-e096ae35b5a8.md) | `41dd148e514d1bf3f84401f0ecbf8e87b58bb261` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-040 | [B040](branches/b040-usage-followup-bf245b09ab75.md) | `2abed4ad67d8438ab9eb124b564be6ad5b545847` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-041 | [B041](branches/b041-policy-rescue-bb88995b68e3.md) | `ef238a7e4720fe1be8e8e295e1c502f2ba529e0b` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-042 | [B042](branches/b042-policy-rescue-4dc1add6a9f0.md) | `bff77099f8273a4d480a0e430cba66afc4197543` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-043 | [B043](branches/b043-policy-rescue-cf95d97cce7c.md) | `3ddbc0563ed95225b0004c393b3928fb599ee066` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-044 | [B044](branches/b044-dirty-3eaee4abbc92.md) | `40823a76a3e20b901d5bc410979cee0931fd5c48` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-045 | [B045](branches/b045-dirty-868886b2213b.md) | `45e834683c8d5ca0fea1c7d3676cb136efa4acb1` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-046 | [B046](branches/b046-dirty-fc03ff5ffa1d.md) | `b72046bbc9314e93cddf4d8252570dc375935f56` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-047 | [B047](branches/b047-stash-alias-53dd4c6affcc.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-048 | [B048](branches/b048-stash-alias-b7b19a9f3034.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-049 | [B049](branches/b049-stash-alias-371a2456f4ff.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-050 | [B050](branches/b050-stash-alias-bfd7103e838f.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-051 | [B051](branches/b051-stash-alias-7512abdb59f0.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-052 | [B052](branches/b052-stash-alias-01522f00a01b.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-053 | [B053](branches/b053-stash-alias-4c4fe5556b72.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-054 | [B054](branches/b054-stash-alias-68ff05fe0f69.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-055 | [B055](branches/b055-stash-alias-f8acf47b8695.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-056 | [B056](branches/b056-stash-alias-cbbb9b2a9c6e.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-057 | [B057](branches/b057-stash-alias-09031c14e05c.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-058 | [B058](branches/b058-stash-alias-373bb2fc05d3.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-059 | [B059](branches/b059-stash-alias-ebc2e2faed24.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-060 | [B060](branches/b060-stash-alias-15ca898cb2c6.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-061 | [B061](branches/b061-stash-alias-cf15510f83af.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-062 | [B062](branches/b062-stash-alias-dca0343dfb05.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-063 | [B063](branches/b063-stash-alias-54f51006e189.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-064 | [B064](branches/b064-stash-alias-abb1dffe6447.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-065 | [B065](branches/b065-stash-alias-0e6388966c3a.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-066 | [B066](branches/b066-stash-alias-b1eced4af625.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-067 | [B067](branches/b067-stash-alias-31cbf60dff61.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-068 | [B068](branches/b068-stash-alias-929b5577bb5f.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-069 | [B069](branches/b069-stash-alias-8836a20a65fb.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-070 | [B070](branches/b070-stash-alias-f574e4820623.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-071 | [B071](branches/b071-stash-alias-42c120365146.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-072 | [B072](branches/b072-stash-alias-64003e4d826d.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-073 | [B073](branches/b073-stash-alias-2cc727bc102c.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-074 | [B074](branches/b074-stash-alias-bc491d591458.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-075 | [B075](branches/b075-stash-alias-770a8c6e1ecf.md) | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-076 | [B076](branches/b076-stash-alias-8e34450f0fb7.md) | `b1b90781739333ba7c8829c092a845f6ea82b11c` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-077 | [B077](branches/b077-policy-517d17ddf7fc.md) | `fba69d60c2373156dc0dcecee8e085c83776966f` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-078 | [B078](branches/b078-wip-6b8347c05938.md) | `ef40fea11a54ade7911de7622e6e011707241dd4` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
| REM-079 | [B079](branches/b079-wip-ebf572949c76.md) | `d04467f05f1183008322047b1828c201e551e947` | OPEN / DEFER pending full source audit | TASK-002; independent reviewer NOT_RUN |
