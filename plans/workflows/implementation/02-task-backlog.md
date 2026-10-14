# Assignable implementation backlog

## How to use this file

Each row is an implementation work package. Open its component specification and required dependency tasks before coding. Every task is initially **not started**; this research did not implement product code. A task closes only with a reviewed change, exact build/source identity and passing evidence linked from the project’s approved PR/CI surfaces.

## Work packages

| Task | Deliverable / principal owner | Depends on | Required proof |
|---|---|---|---|
| T01 | Baseline source/dependency/compatibility audit and certificate format | None | Current base compared with audit pin; baseline test evidence; updated touchpoints |
| T02 | Canonical IDs, errors, plan/node/evidence/event types in workflow-spec | T01 | Round-trip, unknown version/enum, numeric bound and type tests |
| T03 | Versioned Pkl block/template schema and valid/invalid fixtures — C01 | T02 | PKL-01 through PKL-06 |
| T04 | Pinned evaluator-only pklr, frozen resolver, killable helper — C02 | T01, T03 | CERT-PKL; denied IO, import integrity, constraints and kill limits |
| T05 | Graph/port/repair validation and canonical plan hashing — C02 | T02, T04 | PLAN-01 through PLAN-07 |
| T06 | Host authorization ceiling and current-action grants — C03 | T02 | AUTH-01 through AUTH-06; no change to manual widening semantics |
| T07 | Workflow turso schema, migrations, backups and atomic transactions — C10 | T02 | DB-01, DB-03, DB-04, DB-08 |
| T08 | Protected artifacts, seal/read integrity and context resolution — C11 | T02, T07 | ART-01 through ART-07 |
| T09 | Pure state reducer, dependency scheduling and reservations — C09 | T02, T05, T07 | SCH-01 through SCH-08 against fake ports |
| T10 | Effect outbox, controller fencing, receipts and event cursors — C10 | T07, T09 | DB-01, DB-02, DB-05, DB-06; crash injection |
| T11 | Existing runtime programmatic-launch adapter and ownership labels — C04 | T06, T10 | SBX-01 and creation-after-crash reconciliation |
| T12 | Hard sandbox ceilings, missing enforcement and backend certificates — C04 | T06, T11 | SBX-02 through SBX-08; actual supported backend report |
| T13 | Exact private-clone preparation and initial preservation holds — C08 | T06, T08, T11 | GIT-01, GIT-02, initial REC-04 guards |
| T14 | Certified write barrier and immutable checkpoint sealing — C08 | T08, T12, T13 | GIT-03 through GIT-08 |
| T15 | Capsule physical epoch/session binding and protocol extension — C06 | T02, T10 | CAP-04 and backwards-compatible session fixtures |
| T16 | Capsule durable command journal and unknown-delivery handling — C06 | T08, T15 | CAP-01 through CAP-07 |
| T17 | Observe/control leases across all input ingress — C07 | T06, T15, T16 | LEASE-01 through LEASE-06 |
| T18 | Common workflow adapter capabilities and registry enumeration — C05 | T02, T11, T15 | ADP-01 through ADP-06; every Agent::ALL entry represented |
| T19 | Claude workflow profile and actual CLI certification | T12, T16, T17, T18 | CLAUDE-01 through CLAUDE-06 |
| T20 | Codex workflow profiles, correlation and TUI topology certification | T12, T16, T17, T18 | CODEX-01 through CODEX-07 |
| T21 | OpenCode structured/native-TUI profile and ingress mediation | T12, T16, T17, T18 | OPENCODE-01 through OPENCODE-06 |
| T22 | Extend existing Muse workflow profile and artifact acquisition audit | T12, T16, T17, T18 | MUSE-01 through MUSE-07 |
| T23 | Approved generic program launch path and other eight built-in profiles | T12, T16, T17, T18 | GENERIC-01 through GENERIC-06 plus every per-agent file’s tests; no invented support |
| T24 | Trusted independent test execution and evidence validation — C12 | T08, T12, T14, T10 | TEST-01 through TEST-08 |
| T25 | Independent real reviewer, findings and coverage — C13 | T14, T18, T24; one certified agent task | REV-01 through REV-07 |
| T26 | Gate evaluator, approval scope and acceptance certificate — C15 | T06, T10, T24, T25 | GATE-01 through GATE-10 |
| T27 | Finite repair scope and same-session feedback — C14 | T09, T16, T17, T24, T25, T26 | REPAIR-01 through REPAIR-08 |
| T28 | Checkpoint backup and accepted publication reconciliation — C16 | T06, T10, T14, T26 | PUB-01 through PUB-08 |
| T29 | Full host/session recovery, quarantine and all cleanup holds — C17 | T10, T11, T13, T16, T28 | REC-01 through REC-08; entire crash matrix |
| T30 | Complete Condition/Parallel/Join/Git integration and block catalog | T05, T09, T14, T27 | Every block fixture; mandatory-gate and merged-revision tests |
| T31 | Full authenticated versioned host workflow API — C18 | T05, T09, T10, T17, T26 | API-01 through API-08; early skeleton can precede full completion |
| T32 | CLI namespace, JSON/exit/help and original-session attach — C19 | T31 | CLI-01 through CLI-06 |
| T33 | Console graph/details/control/retained-work views — C20 | T31, T32 | CONSOLE-01 through CONSOLE-06 |
| T34 | Host authoring model, source map and safe edit/CAS APIs — C21 | T03, T05, T31 | EDIT-02 through EDIT-06 preservation corpus |
| T35 | Full visual block authoring and authenticated local bridge — C21 | T30, T34 | EDIT-01 through EDIT-08; actual create/save/plan demo |
| T36 | Capsule trace correlation, safe replay and telemetry governance — C22 | T08, T15, T17, T31 | OBS-01 through OBS-04, OBS-07 |
| T37 | Markdown/JSON/bundle export and hold-aware retention — C22 | T08, T28, T29, T36 | OBS-05, OBS-06; export integrity and deletion audit |
| T38 | Manual product/backward-compatibility and platform regression | T12, T17, T23, T32, T33 | CERT-COMPAT; required Docker and conditional OrbStack lanes |
| T39 | Integrated adversarial/fault campaign on actual binaries/backends | T27, T28, T29, T30, T35, T37 | All release certificates and acceptance evidence at exact release candidate |
| T40 | Final security/architecture review, docs and release capability matrix | T38, T39 | All fourteen acceptance criteria; no unverified capability advertised as supported |

T23 must be subdivided into one reviewable change per remaining built-in profile when implementation begins; the separate Amp, Kimi, Grok, Antigravity, Gemini, Cursor, OMP and Hermes documents are the handoff contracts. Grouping them in one planning row does not authorize one unreviewed bulk implementation.

## Required pull-request handoff

Each implementation change records: task IDs; source/base commit; component/API changed; existing code reused; database/protocol migrations; feature/default behavior; permission/retention impact; exact tests and platforms executed; evidence references; unsupported capabilities; and safe rollback behavior. A “not run” test is not a pass. Existing failing tests must be distinguished from regressions.

For an agent implementing a task, the instruction is: read this task, its component, canonical domain/state contracts and relevant existing code first; implement the smallest dependency-safe change; do not broaden scope or relax a gate to obtain green tests; run the specified project tests; report the exact diff, evidence and remaining capability limitations. Agents must not approve or publish their own work unless a separate host-authorized workflow action permits it.

## Definition of done

A work package is complete when its code and schemas exist, tests cover positive/negative/crash paths as applicable, required real-environment tests ran, documentation matches the code, telemetry/privacy rules hold, compatibility remains green and review approves the change. A mock adapter, prose design, unchecked feature flag or successful build alone does not close an enforcement or recovery task.

## Evidence and references

- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.
- **J19 — [Testing and CI rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/TESTING.md).** Lines 1–200. Pinned mise/mbx/nextest commands, e2e and conditional OrbStack lane, documentation examples and crate gates. Examples here are not evidence of execution.
- **J22 — [Built-in agent identity and dispatch registry](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent.rs).** Lines 1–240. Complete Agent enum, Agent::ALL, canonical slugs, auth modes and runtime dispatch. Twelve registered built-in agents; not a workflow capability certificate.
