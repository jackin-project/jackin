# Architecture decisions and release-blocking certification

## Decided defaults

These are proposed implementation decisions, not unanswered product questions. Changes require an explicit architecture decision, updated tests and a new plan version where execution semantics change.

| Decision | Default and rationale | Consequence |
|---|---|---|
| ADR-01 | Native host subsystem; real agents and original Capsule PTYs | No external workflow engine or nested terminal authority is required. |
| ADR-02 | pklr evaluator-only embedding with a frozen capability provider | Configuration evaluation cannot read arbitrary host environment/files/network. |
| ADR-03 | Private clones for strict parallel workers | Worktrees remain available only under explicit shared-reference authorization. |
| ADR-04 | Restrictive workflow grant after legacy resolution | Existing manual grant semantics are preserved; workflows cannot inherit unintended widening. |
| ADR-05 | One input lease enforced by Capsule and structured-transport mediation | UI locks alone are insufficient. Old clients cannot bypass workflow control. |
| ADR-06 | Durable intent/outbox plus reconciled effects | No universal exactly-once PTY claim; ambiguous prompts require resolution. |
| ADR-07 | Host-sealed checkpoints after a certified write barrier | Git commits in a concurrently writable agent repository are not enough. |
| ADR-08 | Trusted verification harness separate from implementer state | Changed or skipped tests cannot silently satisfy acceptance. |
| ADR-09 | Latest-candidate acceptance certificate | New code, manual edits or integration commits invalidate old acceptance eligibility. |
| ADR-10 | Bounded repair scopes, not ordinary DAG cycles | Iteration/attempt counters remain durable and auditable. |
| ADR-11 | Workflow-owned embedded database through project-standard turso | No new general-purpose rusqlite/SQLx stack without a separately approved exception. |
| ADR-12 | Host-held publisher credentials; no automatic merge in v1 defaults | Checkpoint backup, PR publication and merge remain separate explicit authorities. |
| ADR-13 | Preserve-by-default workflow cleanup | Run terminal state never means unique unfinished work is disposable. |
| ADR-14 | Authenticated per-user local API; optional local web bridge | No unauthenticated daemon TCP listener or generic host-shell endpoint. |
| ADR-15 | Source-preserving editor-owned Pkl structures and safe read-only fallback | Never regenerate arbitrary hand-authored Pkl from evaluated JSON. |
| ADR-16 | Protected audit artifacts separate from governed telemetry | Replay is not raw terminal content streamed into OTLP. |
| ADR-17 | Capability certification per version/mode/backend | A vendor name or a backend family never implies all requested capabilities. |
| ADR-18 | Unknown facts are explicit unavailable states | Runtime model availability, provider quotas and untested resume support are not invented. |

## Certification records

Each certificate includes a schema version, test-suite version, source/build digest, exact runtime/backend versions, operating system and architecture, tested capabilities, evidence artifacts, result, expiry/revalidation trigger and reviewer identity. Certificates are host-trusted metadata. A worker cannot self-certify by returning JSON.

| Gate | Required proof | Release behavior before closure |
|---|---|---|
| CERT-PKL | Pinned pklr artifact, differential schema fixtures, typed constraints, deterministic frozen imports, denied IO and evaluator kill/resource limits | No untrusted workflow execution; compiler may run only in explicitly marked development mode |
| CERT-STORE | turso transaction guarantees, durable state/event/outbox atomicity, crash/reopen, migration interruption, disk-full and artifact recovery | No autonomous workflows advertised as durable |
| CERT-SANDBOX | Per-backend mount, resource, disk, network/protocol, credentials, process descendants, DinD denial and write-barrier tests | Reject any workflow requiring an unproven guarantee |
| CERT-CAPSULE | Stable epoch identity, all-ingress lease enforcement, command journal, partial-write uncertainty, observe/replay safety | No autonomous input or exclusive takeover claim |
| CERT-AGENT | Exact binary/protocol/mode: startup, long input, same-session follow-up, turn boundaries, interrupt, resume when claimed, original TUI, model/provider selection | Mark capability unavailable and fail plans that require it |
| CERT-REVISION | Checkpoint consistency, independent objects, trusted harness, stale evidence and integration-commit invalidation | No implementation acceptance/publication |
| CERT-PUBLISH | Scoped credentials, remote reconciliation, protected target policy, duplicate PR avoidance, no implicit destructive effects | Local retained artifacts only; requested publication fails admission |
| CERT-EDITOR | Supported subset edits preserve unrelated bytes; imports/inheritance/comments stay intact; source CAS and compile-before-save | Read-only graph is allowed, but final authoring acceptance is not met |
| CERT-COMPAT | Manual CLI/Console/Capsule/roles/workspaces and conditional platform lanes remain green | v1 release blocked |

## Candidate versions and unresolved compatibility

The pklr source pin in the evidence register identifies version 5.1.1 as a concrete candidate for integration. This is not a claim that its registry artifact, all language behavior or security limits were tested. Freeze that candidate or a reviewed successor in Cargo.lock after source/dependency review and CERT-PKL. Do not mix API examples from inconsistent moving `latest` documentation.

Agent models are selected by exact IDs from a versioned adapter/provider capability snapshot and rechecked for authorization/availability before launch. A model may become unavailable after planning; that is a recorded failure or an explicitly planned fallback, not permission for the agent to choose another model. No current model price or provider availability promise is embedded in this specification.

Apple Container and future VM providers implement the common execution port but must independently certify each restriction. A feature unsupported on a host is rejected with the failing capability named. Docker-first delivery does not justify pretending Apple support has passed.

## Exception policy

Host security denials, missing isolation enforcement, corrupted evidence and unknown publication targets are non-waivable within a run. Operators may approve only policy-defined soft gates or choose a new authorized plan with a different contract. Every exception records actor, exact revision, reason, expiry and affected checks. “Approved” and “all checks passed” remain different results.

## Research follow-through

The implementation sequence opens with certification scaffolding and a baseline dependency audit. It must confirm that inspected APIs have not changed since the audit pin. A later implementation team should compare its base commit to the evidence register, update changed touchpoints, and keep the conceptual components rather than mechanically copying stale paths or signatures.

## Evidence and references

- **J13 — [Docker effective grant merging](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-docker-profile/src/docker_profile/effective.rs).** Full file. Generally capability-widening and maximum-ceiling merging; role DinD None is a special narrowing case. Critical workflow policy integration gap.
- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.
- **E01 — [pklr source README](https://github.com/jdx/pklr/blob/ed746fcf308c5510e159064814fafb5dd151ac44/README.md).** Pinned source documentation: pure-Rust evaluator and eval-core/custom EvalCapabilities embedding. No conformance tests run in this audit.
- **E02 — [pklr Cargo manifest](https://github.com/jdx/pklr/blob/ed746fcf308c5510e159064814fafb5dd151ac44/Cargo.toml).** Source candidate version 5.1.1; default native IO/HTTP features versus eval-core-only embedding. Exact registry artifact must be verified before adoption.
