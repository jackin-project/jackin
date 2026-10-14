# Final acceptance and verification matrix

## Status and execution evidence

**All product implementation tests in this file are NOT RUN in this research.** They are required tests to build and execute. Static source inspection established reuse/gap findings; documentation-generation checks verify this pack’s files/links, not Jackin behavior.

Test IDs are stable requirement identifiers, not claims that tests with those names already exist in the repository. Implement them as nextest-discoverable unit/integration/e2e tests, plus actual platform/vendor conformance scenarios where needed. Store evidence through approved PR/CI/artifact surfaces with exact source/binary/backend versions.

## Traceability to the fourteen supplied acceptance criteria

| AC | Original requirement, retained in meaning | Components / work | Mandatory proof |
|---|---|---|---|
| AC01 | Typed Pkl compiles into a validated immutable plan | C01–C03; T02–T06 | PKL-01–06, PLAN-01–07, CERT-PKL; changing active source does not change plan |
| AC02 | Real supported coding-agent CLIs launch autonomously in isolated instances | C04–C06, all adapter files; T11–T23 | Actual CLI startup per advertised certified tuple; unsupported capabilities explicit; no mock-only claim |
| AC03 | Exact instructions, references, resources and capabilities reach agents | C03–C06, C11 | AUTH, ART, ADP and CAP payload/digest/limit tests; actual mounted read-only content |
| AC04 | Parallel agents cannot interfere with unauthorized repositories/mutable state | C03–C04, C08–C09 | SBX-02–06, GIT-01–02, SCH-02–03; same-role parallel writers remain independent |
| AC05 | Tests/reviews execute against immutable revisions | C08, C12–C13 | GIT-03–06, TEST-01–07, REV-01–04; wrong-revision/harness/forged evidence negatives |
| AC06 | Failed verification produces correlated feedback to the original session | C05–C07, C14 | REPAIR-01/03/04/07, CAP-03; live original session receives exact failure bundle |
| AC07 | Repair is bounded and final gates evaluate latest revision | C09, C14–C15 | REPAIR-02/05/06/08, GATE-02/03/08/09/10; race/new candidate, unsealed changes and restart cases |
| AC08 | User can attach to original live TUI without terminating workflow | C06–C07, C19–C21 | ADP-05, LEASE-03, CLI-03, CONSOLE-02 plus actual interactive agent demonstration |
| AC09 | Manual takeover is exclusive, auditable and safely relinquished | C07, C17, C22 | LEASE-01–06 across raw/control/vendor ingress; manual edits force reconciliation |
| AC10 | States/events/checkpoints/evidence survive failures and restarts | C08, C10–C11, C17, C22 | DB-01–07, ART-03/04, REC-01–03, OBS-01; real kill/power-loss/reopen tests |
| AC11 | Unaccepted/unpushed work is not silently destroyed | C08, C16–C17, C22 | GIT-08, PUB-07, REC-04–08, OBS-06; all normal cleanup paths and disk pressure |
| AC12 | Publication and merge permissions enforced by host | C03, C15–C16 | AUTH-05/06, GATE-04/06, PUB-01–08; v1 merge denied unless separately explicit/implemented |
| AC13 | Graph visible and operable, with visual authoring | C18–C21 | API, CLI, CONSOLE and EDIT suites; create/configure/save/plan all block kinds; viewer alone fails |
| AC14 | Existing manual CLI/Console/Capsule/role/workspace behavior continues | All touched existing crates | CLI-06, CONSOLE-06, GENERIC-06, CERT-COMPAT and required project/platform CI |

## Required integrated scenarios

| Scenario | Fault / adversarial input | Expected observable result |
|---|---|---|
| Happy path | One task, exact repo, trusted tests and independent review pass | Latest checkpoint accepted; original session inspectable; optional authorized PR created |
| Repair path | Test and review fail the initial checkpoint | One correlated follow-up to the original healthy session; new checkpoint; required checks rerun |
| Repair exhaustion | Every iteration fails | Exactly configured repair count; failed/operator-required state; all work retained |
| Host crash after provision | Instance exists but receipt was not recorded | Adopt matching ownership once; no duplicate writable worker |
| Host crash after prompt intent | Prompt not yet sent | Dispatch once from durable outbox after reconciliation |
| Capsule crash during input | Partial/complete bytes possible; receipt incomplete | Delivery unknown, no blind resend; explicit operator/vendor-proof resolution |
| Input takeover race | Operator requests control while prompt chunks are in flight | Serialized completed/partial boundary; one lease owner; no mixed input |
| Stale control client | Old lease sends raw bytes or structured vendor input | Rejected by authoritative ingress, not merely hidden in UI |
| Observer attack | Resize, OSC clipboard, URL/file transfer from observer/replay | No unauthorized terminal/host action |
| Moving worktree | Background process writes while checkpoint is captured | Certified barrier/snapshot yields consistent capture or fails; no mixed tree accepted |
| Late review | Findings arrive for checkpoint N after N+1 exists | Findings retained as historical; current candidate not accepted/failed using stale identity |
| Test forgery | Candidate emits “all passed” JSON or changes tests to skip | Trusted harness/collector policy rejects insufficient or forged evidence |
| Integrated branches | Two independently passing branches conflict or create a new merge | Explicit conflict or new checkpoint; combined result reverified |
| Approval race | User approves while a new checkpoint is sealed | Expected-candidate CAS fails; old approval cannot publish new code |
| Policy revocation | Grant revoked after approval but before remote dispatch | Dispatch denied; earlier decision retained; any in-flight effects reconciled |
| Lost push response | Remote updated, local client timed out | Exact remote ref lookup; no blind duplicate/force push |
| Lost PR response | PR created but ID not persisted | Marker/head/base lookup; adopt exact PR or expose duplicate conflict |
| Storage full | DB/artifact journal cannot durably record input/effect | New input/effects stop; retained work remains; health failure explicit |
| Offline cleanup | Unpushed/untracked code exists; prune/purge invoked | Workflow hold blocks destructive cleanup; export/recovery options shown |
| Unsupported sandbox | Required disk/protocol guarantee absent | Plan/admission denial naming missing guarantee; no permissive fallback |
| Source edit conflict | Visual edit races manual Pkl change | Source-hash CAS fails; unrelated source remains intact |
| Binary drift | Agent executable changes without image digest changing | Binary/certificate mismatch blocks launch; no silent capability reuse |
| All built-ins | Registry includes twelve agents with different feature support | Every registry entry represented; only actually certified workflow features advertised |

## Test layers

Pure tests cover canonicalization, graph reduction, policy algebra, schema/error handling, budgets and acceptance. Property tests explore dependency order, stale versions, skip/repair bounds and lease fencing. Real-engine tests exercise turso transactions/migrations/backups and artifact commit boundaries. Real-backend tests verify enforced sandbox capabilities and cleanup. Real-agent tests prove original TUI/session/feedback behavior. UI fixtures test rendering, source preservation and API races; they do not substitute for runtime tests.

Fault injection must use deterministic named killpoints, including before/after each durable commit, process creation, PTY write, checkpoint seal, remote mutation and cleanup decision. Test both host-daemon and Capsule restart independently. Add power-loss/abrupt-filesystem scenarios for durability claims; a graceful process shutdown is not sufficient evidence.

## Release evidence format

Every AC row records `status: passed|failed|not_run|unsupported`, exact source commit, binary/image digests, test-suite version, OS/backend, certificate IDs, evidence artifact digests and reviewer decision. “Unsupported” is valid capability reporting but does not pass an acceptance criterion that the release claims to support. All advertised final-v1 guarantees must have passing evidence. Source-preserving visual authoring and manual compatibility remain mandatory final gates.

## Evidence and references

- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.
- **J19 — [Testing and CI rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/TESTING.md).** Lines 1–200. Pinned mise/mbx/nextest commands, e2e and conditional OrbStack lane, documentation examples and crate gates. Examples here are not evidence of execution.
- **J22 — [Built-in agent identity and dispatch registry](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent.rs).** Lines 1–240. Complete Agent enum, Agent::ALL, canonical slugs, auth modes and runtime dispatch. Twelve registered built-in agents; not a workflow capability certificate.
