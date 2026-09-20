# Jackin and Velnor CI reliability execution record

Status: active. Started 2026-09-21. This record tracks the current cross-repository
specification: prevent known post-merge failures, enforce staged formatting and
Clippy, expose ordered Rust phases, distribute immutable runtimes, and measure
complete pipelines against 120 seconds. Neither performance nor reliability is
accepted without fresh evidence. The reliability objective is 99.9999%; a small
passing sample cannot establish it.

## Authoritative starting state

- Jackin remote main: `fce94cea8a15de0c2db3bb4ff880d741baf5c00a`.
- Jackin integration: `codex/ci-performance-campaign`, PR #1007, head
  `f4054488919e3267bfe2eef2d7d92ada25e288f5`; initially clean.
- Jackin input schema: 1. Generator source pin:
  `4fa7a3a85f141a6bb95bc9bdf0eef9e3ddde165d`.
- Velnor remote main: `97bac4c4582bbe18ee607a1dd7a41b4854345c7e`.
- Velnor integration: `fix/ci-validation-contract`, PR #979, head
  `39b67ecd9ecc47462a32b337c056f8eabe28080e`; initially clean.
- Velnor PR #978 has related typed Mise prerequisite and telemetry work at
  `ce75f7c322cbb427d4aea2138abdb9fc5e0f5ed9`; not yet adopted or accepted.
- Existing unrelated Jackin feature work and the dirty Velnor migration checkout
  are preserved. The Velnor integration uses a separate worktree because that
  checkout contains unrelated uncommitted source and generated changes.
- Authenticated `gh` is available; RTK 0.49.0 is verified. Initial API ref reads
  and Git fetch agree on both main revisions.

## Ownership and dependencies

The parent owns both integration indexes, commits, pushes, generated output,
merges, and the final deterministic gate. No agent may change those independently.
Writers must stop while a staged snapshot is validated.

| Agent | Bounded assignment | State |
| --- | --- | --- |
| `/root/failure_inventory` | Retained runs, attempts, failed jobs, evidence coverage | collecting |
| `/root/parity_protection` | PR/main equivalence, actual rules, gate semantics | read-only audit |
| `/root/hooks_research` | Current hk/prek correctness and staged snapshot design | read-only research |
| `/root/generator_architecture` | Typed Rust phase model and all execution paths | read-only design |
| `/root/bootstrap_performance` | Task expansion, tool installs, complete elapsed time | read-only audit |
| `/root/runtime_parity` | Candidate/pinned runtime bootstrap and main parity | read-only diagnosis |
| `/root/diagnostics_root` | Diagnostics partial-success root cause and regression | isolated crate work |
| `/root/independent_verifier` | Independent upstream failure and change review | read-only verification |

Dependency chain: diagnose and reproduce; implement a bounded structural fix;
independently review; run deterministic checks; commit with DCO and co-author;
push; validate current head/base; merge through normal protection; inspect the
resulting main and runtime products; adopt the verified immutable version.

## Starting incident ledger

| Occurrence | Observed evidence | Current disposition |
| --- | --- | --- |
| [35521080097 / 106105226160](https://github.com/jackin-project/jackin/actions/runs/35521080097/job/106105226160), attempt 1 | Main `fce94cea`, diagnostics unit, Ubuntu 26.04; failed `Run unit checks`; job 15:56:03–15:56:59 UTC (56 seconds). Runtime/cache setup succeeded. | Actual failure is a test failure; partial-success isolation diagnosis underway. |
| [35515575859 / 106090835001](https://github.com/jackin-project/jackin/actions/runs/35515575859/job/106090835001), attempt 1 | Main `0163d1b7`, macOS 26 desktop merge; cancelled during `desktop-merge` after more than 35 minutes. | Full timestamps, cancellation cause, implicit installations, and task graph under investigation. |
| [Velnor 35525244762 / 106116179376](https://github.com/tailrocks/velnor/actions/runs/35525244762/job/106116179376) | PR #979 current generator job failed; final gates failed. | Independent log/root-cause review underway; merge is not authorized by prior green revisions. |

Raw collection is initially staged outside tracked source under
`/Users/donbeave/Projects/work/ci-evidence/`. Durable, reviewed summaries and
machine-readable evidence will be incorporated at material checkpoints.
Existing Velnor evidence under `plans/ci-performance/` is historical input, not
proof of current acceptance. Its separate Parallax/100-experiment scope is not
part of this specification.

## Acceptance status

All nine acceptance groups remain unproven: complete failure dispositions;
candidate and main parity; safe mandatory hooks; explicit ordered Rust phases;
published upstream adoption; complete pipelines within 120 seconds; trustworthy
measured reuse; reviewed merged slices and fresh main verification; independent
negative-case coverage and first-attempt reliability reporting.

No observed cache hit is treated as validation evidence. No cancelled or retried
run is erased. No narrow timing interval substitutes for complete elapsed time.

## Immediate WIP checkpoint — 2026-09-21

User requested immediate commit and push. This execution record is saved;
no hook installer or desktop task changes have been implemented. Prior
main-sync code passed pinned Rust 1.97.1 formatting and strict Clippy.
The overall CI reliability and 120-second performance goal remains open.
Related partial Velnor implementation and authored audit reports are saved
on fix/ci-validation-contract; that checkpoint is not merge-ready.

## Pinned regeneration after task changes

Policy run 35528422285 at aef775a2 rejected only generated-tree state.
The exact published 4fa7a3a8 runtime scanned 40 units and 146 edges, then
updated only the scan fingerprint (79aad67cd9452b50 → 4af3f105caf6426a).
All generated workflow and runtime-contract bytes stayed identical. A second
`--plain --check .` passed. Live policy verification follows this commit.
