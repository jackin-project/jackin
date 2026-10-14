# A04 — Muse Code workflow adapter

**Status:** Proposed Jackin workflow adapter contract. Upstream documentation is not a passed Jackin runtime certification. Shared purpose/API semantics are defined by [C05](../components/05-agent-adapters.md), [C06](../components/06-capsule-session-control.md) and [C07](../components/07-input-leases-attachment.md).

## Purpose and observed foundation

Extend the existing `MuseRuntime` in `crates/core/jackin-core/src/agent/adapters/muse.rs`; do not add another Muse installation/auth adapter. The current fallback deliberately fails because a standalone installer was not verified in that code. Upstream now documents installation and interactive, exec and versioned serve/schema modes. Reconcile that drift through pinned-artifact review, not an unreviewed runtime download. [J16](#evidence-and-references), [E07](#evidence-and-references).

## API and capability mapping

Implement C05 workflow operations around the current runtime. The initial supported target is a certified interactive Muse process owned by Capsule. A structured `serve` integration is a separate capability profile; it must prove original-TUI equivalence before satisfying an interactive block.

| Capability | Target / unverified boundary |
|---|---|
| Binary availability | Resolve an approved artifact for the worker OS/architecture, verify digest and version. A host binary is not assumed container-compatible. |
| Auth | Reuse the current Muse auth modes and scoped credential flow; no fabricated environment/home-directory convention. |
| Initial/follow-up input | Certified interactive submission and same-session behavior through Capsule. |
| Structured protocol | Capture schema/version from the exact installed build; implement only verified methods. |
| Goal/steering/resume | Capability probe and fixtures required; do not invent commands or resume semantics. |
| Original TUI | Real Muse CLI under Capsule, not output from an exec-only process styled as a TUI. |

## Required functionality

Treat upstream Muse workflow/session-messaging features as optional vendor capabilities, not a replacement for the Jackin host DAG or independent review control. All vendor-spawned work remains inside the granted instance/resource boundary. Host policy controls which network/tool/credential paths are permitted.

The interactive startup path resolves workspace trust/auth decisions non-interactively through approved Jackin policy, or returns a precise preflight/operator-required error. Do not automatically answer every vendor permission prompt “yes.” The adapter records actual installed version, protocol schema digest where used, model/provider and selected mode.

Maintain a persistent logical/vendor session binding for healthy feedback. If a build lacks required continuation, report unsupported and reject that plan. If the original process dies, record a new physical binding only through a certified resume/recovery path; otherwise stop for operator action.

## Build instructions

Audit existing Muse prefetch, auth, role and Capsule dispatch call sites. Update binary acquisition only after reviewing supported target artifacts and checksums/signatures. Add an interactive workflow capability profile first. Add long-instruction/reference, turn boundary, interrupt and same-session fixtures. Investigate the versioned protocol with the exact build’s schema, then certify any shared TUI topology before enabling it.

## Acceptance tests

`MUSE-01`: existing runtime adapter is reused; no duplicate installation/auth path. `MUSE-02`: correct Linux/container artifact is verified or launch fails closed. `MUSE-03`: exact prompt and references arrive once. `MUSE-04`: repair feedback uses the original healthy session. `MUSE-05`: original live TUI remains attachable. `MUSE-06`: unsupported goal/steer/resume/protocol capabilities are explicit. `MUSE-07`: vendor orchestration cannot bypass host budgets/gates.

## Evidence and references

- **J16 — [Existing Muse adapter](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent/adapters/muse.rs).** Full file. Muse runtime installation/auth/path/version adapter exists. Fallback installer deliberately fails closed.
- **E07 — [Muse Code official documentation](https://dev.meta.ai/docs/muse-code).** Official interactive/exec/serve/schema modes and installer. Contrast with existing Jackin fail-closed fallback. Accessed 2026-10-14.
