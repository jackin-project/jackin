# Acceptance tests and release gates

## Evidence status

No case in this pack has been executed.
Every case starts as `NOT_RUN`.
Every task assignment for a case also starts as `NOT_RUN`.
Run a shared case separately for every task that owns it.
The case-level status is only a summary of those task results.
Use `PASS`, `FAIL`, `BLOCKED`, and `NOT_RUN` as distinct test results.
Do not mark a missing credential, host, cluster, or upstream capability as PASS.

A task can pass a SOURCE or UNIT case without qualifying a runtime target.
A LIVE case requires the actual stated target, command, and observations.
The combined journey requires real agents and approved disposable accounts.
A mocked login or version command does not satisfy that journey.

## Harness interface to implement in P04

These are new v2 test commands. They are not existing Jackin commands.
P04 must implement the exact options below.
Use explicit fixture references. Do not put raw secrets in command lines.

```sh
# Check schemas, task links, test registration, and source evidence.
cargo run --locked -p jackin-xtask -- v2-check --pack docs/openshell-v2

# Run one registered case against one registered target.
cargo run --locked -p jackin-xtask -- v2-test \
  --case A66 --target linux-docker \
  --fixture-config tests/v2/fixtures.local.toml \
  --evidence evidence/openshell-v2

# Audit every old implementation disposition and runtime call boundary.
cargo run --locked -p jackin-xtask -- v2-audit \
  --inventory docs/openshell-v2/evidence/legacy-inventory.json

# Require all source, unit, build, live, and independent-repeat records.
cargo run --locked -p jackin-xtask -- v2-verify \
  --matrix tests/v2/targets.toml \
  --evidence evidence/openshell-v2 --require-all

# Run the repository-wide Rust checks on the merged v2 graph.
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features --locked --no-fail-fast
cargo test --workspace --doc --all-features --locked
```

Use `--case` repeatedly for several selected cases.
Reject an unknown case or target.
Keep local fixture configuration outside version control.
Use a new run ID and new owned resource names for each independent repeat.

Early source cases can use recorded ordinary commands before the harness exists.
P04 registers and validates those records afterwards.
Do not create fabricated command transcripts for earlier work.

## Early LIVE records and command registration

Only P02 assignments A05 and A06 use early LIVE records.
Before P03 starts, P02 records actual runs in `evidence/openshell-v2/P02/early-live-records.json`.
Store one record per case. Bind each record to case ID, task ID P02, exact target ID and complete compatibility tuple, unique run ID, input source commit, start and end times, exact argv, exit code, assertion results, tool name and version, implementation identity, and an independent reviewer identity.
Retain stdout and stderr or record explicit truncation. Hash every retained output and evidence file. Record resource ownership before effects and cleanup receipts after effects.
Do not fabricate transcripts, infer LIVE results from source assertions, or downgrade either case.
P04 validates the record fields, retained hashes, and independent review before importing each result only for P02.
T12 later runs both cases again through `v2-test`, with separate fresh run IDs. P02 evidence never satisfies T12 or counts as its independent repeat.

P04 harness command registration occurs before P04 qualification.
The Integration subagent adds only the v2 command enum, dispatch, and required command registration to [`crates/tools/jackin-xtask/src/main.rs`](../../crates/tools/jackin-xtask/src/main.rs) through a P04 patch request.
P04 owns harness implementation and tests. P05 retains workspace and build graph registration for `jackin-xtask` and test support.
P05 does not own or defer v2 command dispatch.

## Final release authorization

The independent T40 reviewer decides whether the candidate passes the release gate.
For every supported compatibility tuple that contains a patch, retain one record per tuple at `evidence/openshell-v2/T40/maintainer-approvals.json`.
Each record names the tuple ID, patch digest, maintainer identity, explicit `APPROVED` decision, and approval reference.
A125 verifies the records for supported patched tuples.
Unpatched supported tuples do not require patch-approval records.
After every finalization prerequisite passes, T40 submits T40 PASS, the final acceptance summary, completion, finalization receipt, and approval to Integration for one atomic write.
The independent T40 reviewer does not write those shared records directly.

## Required target rows

The rows below are selected plan requirements.
They refine the specification's Linux, macOS, desktop-engine, remote, VM, and Kubernetes coverage.
A required unavailable row remains BLOCKED.
Do not remove a row silently to make release approval pass.

| Target ID | Required profile and environment | Key assertion |
| --- | --- | --- |
| `linux-docker` | Linux x86_64, native Docker, OpenShell dev-container. | Local snapshot and Git workflows. |
| `macos-desktop` | macOS ARM64, Docker Desktop, OpenShell dev-container. | Desktop-engine qualification. |
| `macos-orbstack` | macOS ARM64, OrbStack, OpenShell dev-container. | Separate desktop-engine qualification. |
| `remote-linux` | Independent Linux x86_64 Docker gateway. | Registry delivery and remote storage. |
| `remote-macos` | Independent macOS ARM64 Docker gateway. | No laptop-path or local-engine assumption. |
| `live-linux` | Separate personal Linux trusted-live-local gateway. | Live Shared and Worktree behavior. |
| `live-macos-desktop` | Separate personal Docker Desktop trusted-live-local gateway. | Live edit behavior on the actual runtime host. |
| `live-macos-orbstack` | Separate personal OrbStack trusted-live-local gateway. | Live edit behavior without inferred equivalence. |
| `vm-linux-small` | Linux x86_64, isolated-vm-small. | Fixed VM resources and guest-owned storage. |
| `vm-linux-large` | Linux x86_64, isolated-vm-large. | Independent larger gateway shape. |
| `vm-macos-small` | macOS ARM64, isolated-vm-small. | ARM64 identity, storage, and mount behavior. |
| `vm-macos-large` | macOS ARM64, isolated-vm-large. | Independent larger ARM64 shape. |
| `engine-vm-linux` | Qualified Linux VM gateway with companion extension. | Complete managed Docker contract. |
| `engine-vm-macos` | Qualified macOS ARM64 VM gateway with companion extension. | Companion contract on the other advertised host family. |
| `kubernetes` | Linux x86_64 nodes, official deployment, dedicated supervisors. | Complete enforcement and persistence tuple. |
| `verification` | Qualified strict-worker profile with actual read-only root. | Independent checks without credentials or ordinary network. |

Also register `source`, `unit`, and `build` targets for non-runtime cases.
Do not use these labels to bypass a LIVE case.
Additional advertised architectures or drivers require additional qualified rows.
Do not advertise Kubernetes ARM64 or another engine driver from this matrix alone.

Each target record contains its exact compatibility tuple and approved fixture references.
Use separate gateway registrations for materially different staging and production trust classes.
Run tests only on authorized disposable equivalents, not on actual production workloads.

The independent verification target is explicit in each combined journey.
Do not silently downgrade a requested VM verification boundary to Docker.
The initial strict-worker implementation qualifies only the drivers that pass U02.

## Required case applicability

Run A117 on every advertised developer target mechanism and each required host-family row.
Run engine-specific cases only on the engine-enabled rows, plus default-off negatives on ordinary rows.
Run worker-specific cases on every driver that advertises strict verification.
Run storage, identity, transport, and lifecycle cases on every target that claims those mechanisms.
The coordinator records this complete mapping in `tests/v2/targets.toml` during P04 and qualification updates.
An unassigned required case makes the full gate fail.

## Runtime evidence record

Record each case with its case ID, task ID, target ID, run ID, input source commit, and complete tuple.
Record start and end times, exact argv, exit codes, assertion results, and output truncation.
Record resource ownership before effects and cleanup receipts after effects.
Record the actual implementation and reviewer agent identifiers.
Record whether the run is an independent fresh repeat.
Hash retained evidence files and redact known secret material.

Keep SOURCE, UNIT, BUILD, and LIVE evidence labels separate.
A source assertion can cite code. A runtime assertion needs executed evidence.
Do not infer runtime success from a helper report or subprocess exit code alone.

## Failure procedure

Preserve the original failed result.
Record a minimal reproduction and the affected contract.
Fix the cause in its owning task.
Run the affected tests again under a new run ID.
Keep both failed and corrected evidence.
Do not silently widen policy, grant root, expose a socket, or disable a negative test.

## Measurement rules

Measure each critical-path phase separately.
Record host, kernel, architecture, exact artifacts, cache conditions, and sample count.
Record at least five cold and twenty warm observations for the baseline summary.
Report median and range for these small samples.
Use at least one hundred observations before reporting p95.
Use at least one thousand observations before reporting p99.
Do not present these minimum counts as a guarantee of statistical confidence.
Report measurement uncertainty and resource constraints explicitly.

Do not invent a performance threshold from historical anecdotes.
Do not report configured RAM as host RSS or sparse capacity as allocated disk.
Do not claim support above measured and qualified resource bounds.

## Detailed cases

### A01 — Safe source freeze

**Evidence:** SOURCE. **Tasks:** P00. **Initial status:** NOT_RUN. **Initial assignment status:** P00=NOT_RUN.

**Setup:** Use a checkout with one committed file and one uncommitted sentinel.

**Action:** Create the v2 worktree and source lock. Compare the original bytes and Git status.

**Required result:** The original sentinel and status remain unchanged. All source pins and tool limits appear in the lock.

### A02 — Complete call inventory

**Evidence:** SOURCE. **Tasks:** P01. **Initial status:** NOT_RUN. **Initial assignment status:** P01=NOT_RUN.

**Setup:** Use the frozen baseline and all executable repository paths.

**Action:** Trace direct calls, wrappers, scripts, FFI, generated launch code, and inactive paths.

**Required result:** Every discovered runtime effect has a caller, owner, disposition, replacement task, and test reference.

### A03 — Product behavior coverage

**Evidence:** SOURCE. **Tasks:** P01. **Initial status:** NOT_RUN. **Initial assignment status:** P01=NOT_RUN.

**Setup:** Use the current CLI, account, role, project, Capsule, and workflow behavior register.

**Action:** Map every behavior to preservation, deliberate removal, or a visible blocker.

**Required result:** No retained behavior lacks an implementation task. Every removal names its specification basis.

### A04 — Real upstream contract map

**Evidence:** SOURCE. **Tasks:** P02. **Initial status:** NOT_RUN. **Initial assignment status:** P02=NOT_RUN.

**Setup:** Use the exact candidate SDK, protobufs, handlers, and drivers.

**Action:** Resolve each required operation to an actual symbol and field.

**Required result:** Proposed fields are marked proposed. Missing capabilities have upstream tasks. No moving source reference enters the lock.

### A05 — SDK mutual TLS

**Evidence:** LIVE. **Tasks:** P02, T12. **Initial status:** NOT_RUN. **Initial assignment status:** P02=NOT_RUN; T12=NOT_RUN.

**Setup:** Use a disposable gateway with valid, invalid, and missing client certificates.

**Action:** Connect through the selected SDK and perform a scoped read.

**Required result:** The valid identity succeeds. Invalid and missing identities fail. No insecure TLS path is used.

### A06 — Concurrent explicit targets

**Evidence:** LIVE. **Tasks:** P02, T12. **Initial status:** NOT_RUN. **Initial assignment status:** P02=NOT_RUN; T12=NOT_RUN.

**Setup:** Use two gateways with identical friendly sandbox names and different content sentinels.

**Action:** Interleave scoped read and allowed mutation requests without changing active registrations.

**Required result:** Each result matches its explicit target and workspace. No request reaches the other gateway.

### A07 — Strict v2 schemas

**Evidence:** UNIT. **Tasks:** T14. **Initial status:** NOT_RUN. **Initial assignment status:** T14=NOT_RUN.

**Setup:** Prepare valid v2 configurations and variants with unknown fields, wrong versions, and ambiguous IDs.

**Action:** Parse and resolve each variant.

**Required result:** Valid inputs resolve once. Invalid inputs fail explicitly. No old schema or first-match fallback succeeds.

### A08 — Domain identity rules

**Evidence:** UNIT. **Tasks:** T24. **Initial status:** NOT_RUN. **Initial assignment status:** T24=NOT_RUN.

**Setup:** Prepare two accounts, two policies, two trust classes, and one logical instance.

**Action:** Attempt domain reuse across each changed field.

**Required result:** Only identical admitted trust sets can reuse a domain. Human labels cannot replace stable identity.

### A09 — Operation state contract

**Evidence:** UNIT. **Tasks:** T10. **Initial status:** NOT_RUN. **Initial assignment status:** T10=NOT_RUN.

**Setup:** Enumerate every frozen phase, outcome, error, and recovery transition.

**Action:** Apply legal and illegal transitions, including cancellation after an uncertain remote effect.

**Required result:** Legal transitions preserve receipts. Illegal transitions fail. Local cancellation does not imply remote termination.

### A10 — Honest test gate

**Evidence:** UNIT. **Tasks:** P04. **Initial status:** NOT_RUN. **Initial assignment status:** P04=NOT_RUN.

**Setup:** Prepare one passing case, one failed assertion, one missing case, and one unavailable target.

**Action:** Run the harness and full-gate command.

**Required result:** Only the passing case reports PASS. The full gate fails for missing, failed, or blocked required cases.

### A11 — Disposable resource ownership

**Evidence:** LIVE. **Tasks:** P04. **Initial status:** NOT_RUN. **Initial assignment status:** P04=NOT_RUN.

**Setup:** Create owned test resources and unrelated sentinel resources in the same environment.

**Action:** Interrupt setup and run scoped cleanup.

**Required result:** Cleanup changes only recorded owned resources. Uncertain cleanup remains visible. Sentinels survive.

### A12 — Evidence secret protection

**Evidence:** UNIT. **Tasks:** P04. **Initial status:** NOT_RUN. **Initial assignment status:** P04=NOT_RUN.

**Setup:** Use synthetic credential canaries in allowed secret inputs and denied ordinary records.

**Action:** Exercise log, report, panic, transcript, and evidence serialization paths.

**Required result:** No known canary appears in ordinary artifacts. A leakage assertion fails the harness.

### A13 — Production dependency boundary

**Evidence:** SOURCE. **Tasks:** P05, T39. **Initial status:** NOT_RUN. **Initial assignment status:** P05=NOT_RUN; T39=NOT_RUN.

**Setup:** P05 inspects the newly created current graph. T39 repeats the checks on the final candidate graph after legacy deletion.

**Action:** P05 inspects dependency edges and compiles forbidden-import fixtures. T39 repeats graph and composition checks after deletion.

**Required result:** Core has no runtime or database types. Capsule has no host credential path. Forbidden fixtures fail in both the P05 graph and the final post-deletion graph.

### A14 — No production backend selection

**Evidence:** SOURCE. **Tasks:** P05, T39. **Initial status:** NOT_RUN. **Initial assignment status:** P05=NOT_RUN; T39=NOT_RUN.

**Setup:** Use the active crate graph and application composition.

**Action:** Search and inspect runtime enums, trait implementations, feature flags, and command dispatch.

**Required result:** OpenShell is the only production sandbox adapter. Mocks exist only in test scope.

### A15 — Authoritative activation observations

**Evidence:** LIVE. **Tasks:** U00, T13, T23. **Initial status:** NOT_RUN. **Initial assignment status:** U00=NOT_RUN; T13=NOT_RUN; T23=NOT_RUN.

**Setup:** Create a sandbox with forged helper claims and real gateway observations. U00 and T13 qualify LIVE observation and adapter layers. T23 tests integrated activation through StartSession.

**Action:** U00 verifies scoped observations. T13 verifies adapter binding and fail-closed behavior. T23 attempts StartSession with missing, stale, or mismatched image, policy, provider, or generation evidence. It also tests forged helper claims.

**Required result:** Only matching authoritative observations satisfy the integrated StartSession barrier. U00 and T13 results do not replace T23's full activation attempt. A helper echo cannot replace gateway evidence.

### A16 — Effective policy source

**Evidence:** LIVE. **Tasks:** U00, T13, T29. **Initial status:** NOT_RUN. **Initial assignment status:** U00=NOT_RUN; T13=NOT_RUN; T29=NOT_RUN.

**Setup:** Use a sandbox policy that differs from the gateway global policy. U00 and T13 verify accepted revisions and replacement source. T29 qualifies product presentation.

**Action:** Query actual accepted revisions and display the effective policy source. Do not infer an intersection.

**Required result:** The gateway global policy is the effective replacement. T29 displays its accepted revision and source; no task reports an imaginary policy intersection.

### A17 — Provider authority observation

**Evidence:** LIVE. **Tasks:** U00, T13. **Initial status:** NOT_RUN. **Initial assignment status:** U00=NOT_RUN; T13=NOT_RUN.

**Setup:** Use a provider binding that adds approved endpoint access.

**Action:** Attach, revoke, and change the binding while querying installed status.

**Required result:** Activation uses the installed revision. Added access is visible. Revocation cannot remain falsely ready.

### A18 — Same-name replacement fencing

**Evidence:** LIVE. **Tasks:** U01, T13. **Initial status:** NOT_RUN. **Initial assignment status:** U01=NOT_RUN; T13=NOT_RUN.

**Setup:** Record sandbox A identity and generation. Replace it with B under the same name.

**Action:** Deliver delayed stop, start, delete, exec, policy, provider, transfer, and service mutations for A.

**Required result:** Every mutation rejects the stale binding. B and its data remain unchanged.

### A19 — Mutation replay integrity

**Evidence:** LIVE. **Tasks:** U01, T13. **Initial status:** NOT_RUN. **Initial assignment status:** U01=NOT_RUN; T13=NOT_RUN.

**Setup:** Use one effect with a persisted request ID and canonical digest.

**Action:** Lose the first response. Retry the same request. Then change the payload under that ID.

**Required result:** The identical retry causes no duplicate effect. The changed payload fails. The original result stays attributable.

### A20 — Scope and permission masking

**Evidence:** LIVE. **Tasks:** U01, T13. **Initial status:** NOT_RUN. **Initial assignment status:** U01=NOT_RUN; T13=NOT_RUN.

**Setup:** Use two workspaces and a caller without authority in one scope.

**Action:** Read, mutate, and replay requests across the forbidden scope.

**Required result:** Access fails without foreign data exposure. Permission masking does not cause local data deletion.

### A21 — Lost effectful exec response

**Evidence:** LIVE. **Tasks:** U01, T13. **Initial status:** NOT_RUN. **Initial assignment status:** U01=NOT_RUN; T13=NOT_RUN.

**Setup:** Start a counted effect through the qualified exec API and drop its response.

**Action:** Recover the operation and attempt its permitted replay path.

**Required result:** The effect count stays one, or the result remains OutcomeUnknown. No new-ID blind retry occurs.

### A22 — Strict worker immutability

**Evidence:** LIVE. **Tasks:** U02, T32. **Initial status:** NOT_RUN. **Initial assignment status:** U02=NOT_RUN; T32=NOT_RUN.

**Setup:** Use a worker with immutable candidate, checks, and root plus approved scratch.

**Action:** Attempt write, chmod, chown, rename, unlink, hardlink, xattr, and symlink-based escape operations.

**Required result:** Every out-of-grant content or metadata mutation fails. Approved scratch remains usable.

### A23 — Worker resource limits

**Evidence:** LIVE. **Tasks:** U02, T32. **Initial status:** NOT_RUN. **Initial assignment status:** U02=NOT_RUN; T32=NOT_RUN.

**Setup:** Use fixed memory, CPU, PID, disk, output, and deadline limits.

**Action:** Exhaust each resource with disposable fixtures. Spawn child and grandchild processes.

**Required result:** Limits apply to the required scope. Output stays bounded. Timeout cleanup removes all owned worker processes.

### A24 — Worker authority denial

**Evidence:** LIVE. **Tasks:** U02, T32. **Initial status:** NOT_RUN. **Initial assignment status:** U02=NOT_RUN; T32=NOT_RUN.

**Setup:** Use a strict worker with no account, provider, host mount, or ordinary network grant.

**Action:** Probe credential canaries, host sockets, DNS, IPv4, IPv6, and metadata endpoints.

**Required result:** All forbidden access fails. No unapproved account or network path appears.

### A25 — Trustworthy worker completion

**Evidence:** LIVE. **Tasks:** U02, T32. **Initial status:** NOT_RUN. **Initial assignment status:** U02=NOT_RUN; T32=NOT_RUN.

**Setup:** Use success, nonzero exit, signal, OOM, timeout, and lost-status fixtures.

**Action:** Observe each result through the runtime completion contract.

**Required result:** Results remain distinct. Unknown infrastructure state cannot become a passing verdict.

### A26 — Ordinary native rebuilds

**Evidence:** LIVE. **Tasks:** U03. **Initial status:** NOT_RUN. **Initial assignment status:** U03=NOT_RUN.

**Setup:** Use an approved native Rust network test and a fixed executable path.

**Action:** Run Cargo and nextest. Edit and rebuild twice at the same path. Repeat the allowed connection.

**Required result:** All approved development cycles work under the selected contract without path changes or Docker execution substitutes.

### A27 — Replaced executable authority

**Evidence:** LIVE. **Tasks:** U03. **Initial status:** NOT_RUN. **Initial assignment status:** U03=NOT_RUN.

**Setup:** Use a protected provider route and an untrusted replacement executable.

**Action:** Replace or race the executable while it requests the protected route.

**Required result:** Replacement cannot acquire unrelated credential authority. Destination and provider restrictions remain enforced.

### A28 — Default-off engine

**Evidence:** LIVE. **Tasks:** U08, T33. **Initial status:** NOT_RUN. **Initial assignment status:** U08=NOT_RUN; T33=NOT_RUN.

**Setup:** Use a role and target with engine access disabled.

**Action:** Load, prewarm, inspect, stop, and delete the domain.

**Required result:** No engine VM, endpoint, credential, engine prewarm, or engine-owned storage appears.

### A29 — Companion ownership identity

**Evidence:** LIVE. **Tasks:** U05, T33. **Initial status:** NOT_RUN. **Initial assignment status:** U05=NOT_RUN; T33=NOT_RUN.

**Setup:** Use two domains and a restarted parent generation.

**Action:** Request each endpoint from the other domain and from an old generation.

**Required result:** Only the current owning domain can use its endpoint. Names alone cannot transfer ownership.

### A30 — Companion lifecycle

**Evidence:** LIVE. **Tasks:** U05, U08, T33. **Initial status:** NOT_RUN. **Initial assignment status:** U05=NOT_RUN; U08=NOT_RUN; T33=NOT_RUN.

**Setup:** Use an admitted domain with engine data and running Docker children.

**Action:** Stop, start, recreate, and delete the parent.

**Required result:** Stop revokes access. Start issues new access. Recreation uses a new engine. Delete confirms destruction and revocation.

### A31 — Companion failure cleanup

**Evidence:** LIVE. **Tasks:** U05, U08, T33. **Initial status:** NOT_RUN. **Initial assignment status:** U05=NOT_RUN; U08=NOT_RUN; T33=NOT_RUN.

**Setup:** Use checkpoints at every companion provisioning and cleanup transition.

**Action:** Kill the parent, gateway, or provisioning process at each checkpoint. Recover the owner.

**Required result:** Recovery does not duplicate engines. Orphans are reconciled. Uncertain cleanup never reports completed deletion.

### A32 — Confined project projection

**Evidence:** LIVE. **Tasks:** U08. **Initial status:** NOT_RUN. **Initial assignment status:** U08=NOT_RUN.

**Setup:** Place approved project files beside forbidden host and account sentinels.

**Action:** Access the projection from the daemon machine and privileged Docker children.

**Required result:** Only approved project data is reachable. Account and control-state sentinels remain outside daemon authority.

### A33 — Live Docker bind behavior

**Evidence:** LIVE. **Tasks:** U08. **Initial status:** NOT_RUN. **Initial assignment status:** U08=NOT_RUN.

**Setup:** Use Compose binds at the agreed absolute project path.

**Action:** Edit, rename, create, and delete files from both sides. Observe file watches.

**Required result:** Both sides see the specified changes and ownership. A streamed build context is not accepted as a bind substitute.

### A34 — Projection path attacks

**Evidence:** LIVE. **Tasks:** U08. **Initial status:** NOT_RUN. **Initial assignment status:** U08=NOT_RUN.

**Setup:** Add escaping links, traversal paths, and unsupported special-file fixtures.

**Action:** Attempt projection and child-container access through each fixture.

**Required result:** The projection rejects or confines each case. It never widens the host source mount.

### A35 — Complete Docker API behavior

**Evidence:** LIVE. **Tasks:** U08. **Initial status:** NOT_RUN. **Initial assignment status:** U08=NOT_RUN.

**Setup:** Use the authenticated managed engine endpoint.

**Action:** Run online pull, build, run, exec, logs, events, and concurrent long-lived streams.

**Required result:** Required API behavior works without a host socket. Stream cancellation does not corrupt ownership.

### A36 — Outer engine egress

**Evidence:** LIVE. **Tasks:** U08. **Initial status:** NOT_RUN. **Initial assignment status:** U08=NOT_RUN.

**Setup:** Define one allowed destination and denied destinations for the companion.

**Action:** Connect from the daemon, a build step, and a child container.

**Required result:** All three layers obey outer egress. Agent binary policy alone does not satisfy this assertion.

### A37 — Dynamic ports and callbacks

**Evidence:** LIVE. **Tasks:** U08. **Initial status:** NOT_RUN. **Initial assignment status:** U08=NOT_RUN.

**Setup:** Use concurrent Testcontainers services with dynamic published ports.

**Action:** Discover service hosts. Test callbacks, collisions, exhaustion, release, and reuse.

**Required result:** Each route reaches its owner. Failures are bounded. Revoked routes cannot be reused through stale credentials.

### A38 — Aggregate engine budgets

**Evidence:** LIVE. **Tasks:** U08. **Initial status:** NOT_RUN. **Initial assignment status:** U08=NOT_RUN.

**Setup:** Set one domain budget for the engine and all children.

**Action:** Start multiple containers that jointly exceed CPU, RAM, PID, disk, and network limits.

**Required result:** The aggregate admitted budget remains enforced. Per-child limits alone do not pass.

### A39 — Real Compose and Testcontainers

**Evidence:** LIVE. **Tasks:** U08, T33. **Initial status:** NOT_RUN. **Initial assignment status:** U08=NOT_RUN; T33=NOT_RUN.

**Setup:** Use two managed companions, a live project, and an ordinary native test project.

**Action:** Run Compose and Testcontainers after two source edits. Repeat from fresh resources independently.

**Required result:** Real service data exchange succeeds. Ownership, isolation, cleanup, and native rebuild assertions also pass.

### A40 — Journal crash boundaries

**Evidence:** UNIT. **Tasks:** T10. **Initial status:** NOT_RUN. **Initial assignment status:** T10=NOT_RUN.

**Setup:** Use fault points before and after each local commit and simulated external effect.

**Action:** Crash and restart the state coordinator at every point.

**Required result:** Request identities and receipts survive. Recovery follows the declared state table without duplicate effects.

### A41 — Unknown outcome recovery

**Evidence:** LIVE. **Tasks:** T38. **Initial status:** NOT_RUN. **Initial assignment status:** T38=NOT_RUN.

**Setup:** Lose responses from create, exec, stop, delete, and publication effects.

**Action:** Restart host control and inspect each operation.

**Required result:** Each operation resolves through qualified replay or stays explicitly uncertain. No guessed success appears.

### A42 — Fresh v2 state

**Evidence:** UNIT. **Tasks:** T10. **Initial status:** NOT_RUN. **Initial assignment status:** T10=NOT_RUN.

**Setup:** Provide a clean directory, a valid v2 database, and an old-format database.

**Action:** Initialize and open each state location.

**Required result:** Fresh and valid v2 state work. Old state is rejected with fresh-setup guidance and remains unchanged.

### A43 — Local IPC authentication

**Evidence:** LIVE. **Tasks:** T11. **Initial status:** NOT_RUN. **Initial assignment status:** T11=NOT_RUN.

**Setup:** Use the host service, another local user, and unsafe socket-path fixtures.

**Action:** Attempt connection through valid and forged peer identities or symlink paths.

**Required result:** Only the authorized local peer succeeds. Socket replacement cannot grant host authority.

### A44 — Host service death

**Evidence:** LIVE. **Tasks:** T19. **Initial status:** NOT_RUN. **Initial assignment status:** T19=NOT_RUN.

**Setup:** Run a remote Capsule with a live PTY and a durable operation record.

**Action:** Kill the local host service. Restart it and reconnect.

**Required result:** Remote execution continues. Recovery preserves the binding and does not replay uncertain agent startup.

### A45 — Approval-service outage

**Evidence:** LIVE. **Tasks:** T26. **Initial status:** NOT_RUN. **Initial assignment status:** T26=NOT_RUN.

**Setup:** Run a domain that requests a host-approved action. Stop the host control service.

**Action:** Attempt the action and then restore host control.

**Required result:** The action stays unavailable or pending. The remote coding session does not die or gain default approval.

### A46 — Authentication expiry

**Evidence:** LIVE. **Tasks:** T12. **Initial status:** NOT_RUN. **Initial assignment status:** T12=NOT_RUN.

**Setup:** Use qualified mTLS and intended token-based routes with invalid and expired credentials.

**Action:** Rotate or expire credentials during reads and an operation boundary.

**Required result:** Errors and supported refresh follow the upstream contract. Scope and server verification remain intact.

### A47 — Connection secret redaction

**Evidence:** UNIT. **Tasks:** T12. **Initial status:** NOT_RUN. **Initial assignment status:** T12=NOT_RUN.

**Setup:** Construct transport errors and Debug output with synthetic certificates, keys, and tokens.

**Action:** Serialize diagnostics and error reports.

**Required result:** Secret bytes stay absent from ordinary output. Target attribution remains useful.

### A48 — Partial inventory and false absence

**Evidence:** LIVE. **Tasks:** T13. **Initial status:** NOT_RUN. **Initial assignment status:** T13=NOT_RUN.

**Setup:** Use paginated resources and inject permission, authentication, and transport failures.

**Action:** List, refresh, and run cleanup preflight.

**Required result:** Partial results remain partial. No retained row or checkpoint is purged from ambiguous absence.

### A49 — Complete role cache identity

**Evidence:** UNIT. **Tasks:** T14. **Initial status:** NOT_RUN. **Initial assignment status:** T14=NOT_RUN.

**Setup:** Vary each declared build input independently.

**Action:** Resolve the role and compare cache keys.

**Required result:** Every meaningful input changes the identity. Unchanged valid inputs can reuse the exact artifact.

### A50 — Complete installed agent set

**Evidence:** BUILD. **Tasks:** T16. **Initial status:** NOT_RUN. **Initial assignment status:** T16=NOT_RUN.

**Setup:** Declare two agents but provide a candidate image that lacks one.

**Action:** Resolve selection and validate the advertised executable inventory.

**Required result:** Validation fails for the missing agent. The image cannot be reused under a different selected-agent assumption.

### A51 — Ignore rules and secret-dependent builds

**Evidence:** BUILD. **Tasks:** T15. **Initial status:** NOT_RUN. **Initial assignment status:** T15=NOT_RUN.

**Setup:** Use ignored files, included files, and an explicitly secret-dependent build fixture.

**Action:** Change each input and compare build identity and logs.

**Required result:** Ignore behavior matches the actual build context. Secret bytes do not leak. Secret-output caching follows its declared policy.

### A52 — Explicit builder routing

**Evidence:** BUILD. **Tasks:** T15. **Initial status:** NOT_RUN. **Initial assignment status:** T15=NOT_RUN.

**Setup:** Configure builder A and an ambient DOCKER_HOST for builder B.

**Action:** Build a sentinel role and inspect authorized build records.

**Required result:** Only builder A receives source and approved secrets. Unexpected ambient routing cannot override it.

### A53 — Remote immutable image delivery

**Evidence:** BUILD. **Tasks:** T15. **Initial status:** NOT_RUN. **Initial assignment status:** T15=NOT_RUN.

**Setup:** Use a local image ID and a reachable registry artifact with an immutable digest.

**Action:** Deliver to a remote target reference and resolve pull authorization.

**Required result:** The remote plan uses the registry digest. A local image ID is never treated as a pullable reference.

### A54 — Platform artifact identity

**Evidence:** BUILD. **Tasks:** T15. **Initial status:** NOT_RUN. **Initial assignment status:** T15=NOT_RUN.

**Setup:** Use a multi-platform OCI index and mismatched architecture fixtures.

**Action:** Select the target platform and record both index and platform manifest.

**Required result:** The exact platform artifact is explicit. An unsupported architecture fails before activation.

### A55 — Separate builder outage

**Evidence:** LIVE. **Tasks:** T35. **Initial status:** NOT_RUN. **Initial assignment status:** T35=NOT_RUN.

**Setup:** Run a healthy remote gateway and then stop only its independent builder.

**Action:** Inspect, attach, stop, and attempt a new uncached build.

**Required result:** Management still works. Only builder-dependent work fails. Shared physical-engine loss is not misreported.

### A56 — Unprivileged role ABI

**Evidence:** LIVE. **Tasks:** T16. **Initial status:** NOT_RUN. **Initial assignment status:** T16=NOT_RUN.

**Setup:** Build the complete role with the C08 layout.

**Action:** Start it on the qualified target and attempt normal setup.

**Required result:** Setup needs no root, sudo, chown, setuid, firewall, or namespace operation.

### A57 — Advertised agent validation

**Evidence:** LIVE. **Tasks:** T16. **Initial status:** NOT_RUN. **Initial assignment status:** T16=NOT_RUN.

**Setup:** Use the built role and its complete executable inventory.

**Action:** Check each executable through OpenShell and run its declared startup route.

**Required result:** Every advertised binary exists at its recorded version. Runtime identity matches the qualified image evidence.

### A58 — Empty canonical Capsule startup

**Evidence:** LIVE. **Tasks:** T16. **Initial status:** NOT_RUN. **Initial assignment status:** T16=NOT_RUN.

**Setup:** Create a domain with canonical restart policy Never.

**Action:** Inspect Capsule before activation and after a new boot.

**Required result:** Each fresh boot has zero agent sessions. Setup cannot start a default agent implicitly.

### A59 — Frame allocation limits

**Evidence:** UNIT. **Tasks:** T17. **Initial status:** NOT_RUN. **Initial assignment status:** T17=NOT_RUN.

**Setup:** Feed maximum, excessive, truncated, and nested protocol frames.

**Action:** Decode both protocol directions under bounded memory assertions.

**Required result:** Valid limits work. Invalid frames fail before unbounded allocation or arithmetic overflow.

### A60 — Protocol compatibility

**Evidence:** UNIT. **Tasks:** T17. **Initial status:** NOT_RUN. **Initial assignment status:** T17=NOT_RUN.

**Setup:** Use golden v2 frames and incompatible major-version peers.

**Action:** Negotiate capabilities and request an effect.

**Required result:** Compatible frames retain meaning. Incompatible major versions fail before the effect.

### A61 — Protocol fuzzing

**Evidence:** UNIT. **Tasks:** T17. **Initial status:** NOT_RUN. **Initial assignment status:** T17=NOT_RUN.

**Setup:** Use bounded random frames and valid messages with mutated identifiers.

**Action:** Fuzz encode/decode, state transitions, and diagnostics separation.

**Required result:** No panic, unsafe allocation, cross-session input, or diagnostic-as-command behavior occurs.

### A62 — Nonroot Capsule process

**Evidence:** UNIT. **Tasks:** T18. **Initial status:** NOT_RUN. **Initial assignment status:** T18=NOT_RUN.

**Setup:** Run the Capsule test fixture as an ordinary user with no agent credentials.

**Action:** Start serve, inspect its boot, and request deterministic fixture sessions.

**Required result:** No root or PID1 assumption appears. Production agent execution is not part of this host unit fixture.

### A63 — Session child lifetime

**Evidence:** UNIT. **Tasks:** T18. **Initial status:** NOT_RUN. **Initial assignment status:** T18=NOT_RUN.

**Setup:** Use deterministic PTY fixture children and grandchildren.

**Action:** Exit one child, detach one client, resize, interrupt, and stop another session.

**Required result:** Capsule and unrelated sessions survive. Normal child groups are reaped. No security-boundary claim follows from process groups.

### A64 — Bounded replay and snapshots

**Evidence:** UNIT. **Tasks:** T18. **Initial status:** NOT_RUN. **Initial assignment status:** T18=NOT_RUN.

**Setup:** Produce output beyond retention with one stalled viewer.

**Action:** Drain the PTY and request replay plus a snapshot watermark.

**Required result:** Memory stays bounded. The stalled viewer receives a gap. Snapshot and later output have a consistent sequence boundary.

### A65 — Binary relay transport

**Evidence:** LIVE. **Tasks:** T19. **Initial status:** NOT_RUN. **Initial assignment status:** T19=NOT_RUN.

**Setup:** Use a real OpenShell exec stream without an outer PTY.

**Action:** Send all byte values, large frames, diagnostics, resize, and terminal input.

**Required result:** Protocol bytes are unchanged. Diagnostics stay on stderr. Inner PTY behavior remains correct.

### A66 — Same-PTY reconnect

**Evidence:** LIVE. **Tasks:** T19. **Initial status:** NOT_RUN. **Initial assignment status:** T19=NOT_RUN.

**Setup:** Record a live inner process identity and output sequence.

**Action:** Detach, break the network, create a fresh relay, and reconnect.

**Required result:** The same surviving PTY continues. Reconnect uses a new exec request ID and application replay, not admission replay.

### A67 — Single input writer

**Evidence:** LIVE. **Tasks:** T19. **Initial status:** NOT_RUN. **Initial assignment status:** T19=NOT_RUN.

**Setup:** Attach two viewers and one writer to a session.

**Action:** Transfer the writer lease. Send stale-generation and unacknowledged input during disconnect.

**Required result:** Only the current writer input succeeds. Stale input is discarded. Lost keystrokes are not replayed automatically.

### A68 — Portable worktree and unpushed commits

**Evidence:** LIVE. **Tasks:** T20. **Initial status:** NOT_RUN. **Initial assignment status:** T20=NOT_RUN.

**Setup:** Use a linked worktree with unpushed commits and dirty selected files.

**Action:** Export and materialize it on an independent remote filesystem.

**Required result:** The checkout and Git base are valid without host .git pointers. Selected unpushed and dirty content survives.

### A69 — Atomic materialization

**Evidence:** LIVE. **Tasks:** T20. **Initial status:** NOT_RUN. **Initial assignment status:** T20=NOT_RUN.

**Setup:** Import a multi-file source into a unique staging epoch.

**Action:** Interrupt transfer, corrupt one chunk, and retry through the allowed operation contract.

**Required result:** Partial or corrupt staging never becomes active. Only a verified complete epoch is published.

### A70 — Archive and path safety

**Evidence:** UNIT. **Tasks:** T20. **Initial status:** NOT_RUN. **Initial assignment status:** T20=NOT_RUN.

**Setup:** Prepare traversal, escaping-link, special-file, excessive-size, and credential-store fixtures.

**Action:** Validate and extract them into a disposable staging directory.

**Required result:** Unsafe inputs fail without outside writes. Known credential stores are excluded. Budget violations remain explicit.

### A71 — Source mode and host Git safety

**Evidence:** LIVE. **Tasks:** T20. **Initial status:** NOT_RUN. **Initial assignment status:** T20=NOT_RUN.

**Setup:** Use Shared, Worktree, and Clone fixtures with malicious repository command configuration.

**Action:** Materialize through live and snapshot profiles and inspect host effects.

**Required result:** Requested live behavior stays live. Snapshot behavior is explicit. Repository settings cannot execute unapproved host commands.

### A72 — Stable account selection

**Evidence:** LIVE. **Tasks:** T24. **Initial status:** NOT_RUN. **Initial assignment status:** T24=NOT_RUN.

**Setup:** Save an instance with account A and later change the user's default to B.

**Action:** Reconnect and create an approved session from the saved instance.

**Required result:** Account A remains selected. A revision change requires re-admission instead of silent reassignment.

### A73 — Private authority scope

**Evidence:** LIVE. **Tasks:** T21. **Initial status:** NOT_RUN. **Initial assignment status:** T21=NOT_RUN.

**Setup:** Use a private account, personal workspace, and team workspace with another member.

**Action:** Attempt private-account admission to both scopes.

**Required result:** Only a scope authorized for that account passes. Separate sandboxes alone do not imply account privacy.

### A74 — Account-provider activation binding

**Evidence:** LIVE. **Tasks:** T23. **Initial status:** NOT_RUN. **Initial assignment status:** T23=NOT_RUN.

**Setup:** Use a saved grant and change its provider or policy revision before activation.

**Action:** Attempt StartSession with the stale activation inputs.

**Required result:** Activation fails until the current authorized binding and effective policy are observed and admitted.

### A75 — Native credential isolation

**Evidence:** LIVE. **Tasks:** T24. **Initial status:** NOT_RUN. **Initial assignment status:** T24=NOT_RUN.

**Setup:** Use two approved native-login accounts and one generic shell domain.

**Action:** Inspect permitted account files and attempt cross-domain access.

**Required result:** Each domain receives only its admitted files. The generic shell cannot read either account store.

### A76 — Atomic rotating refresh state

**Evidence:** LIVE. **Tasks:** T22. **Initial status:** NOT_RUN. **Initial assignment status:** T22=NOT_RUN.

**Setup:** Use a native-login route with rotating tokens and explicit single-writer scope.

**Action:** Interrupt publication and attempt concurrent refresh from another writer or device.

**Required result:** One valid revision survives. Unsupported cross-device sharing fails explicitly. No generic refresh broker appears.

### A77 — Real advertised agent routes

**Evidence:** LIVE. **Tasks:** T23. **Initial status:** NOT_RUN. **Initial assignment status:** T23=NOT_RUN.

**Setup:** Use approved disposable credentials for every advertised authentication route.

**Action:** Perform an actual authenticated coding task through each agent adapter.

**Required result:** Each advertised route completes its declared task. Version output or mocked login cannot substitute.

### A78 — Activation barrier negatives

**Evidence:** LIVE. **Tasks:** T23. **Initial status:** NOT_RUN. **Initial assignment status:** T23=NOT_RUN.

**Setup:** Prepare mismatched image, generation, account grant, materialization epoch, and missing policy observations.

**Action:** Attempt launch activation for each mismatch.

**Required result:** Every mismatch blocks StartSession. SandboxReady and helper receipts alone are insufficient.

### A79 — Launch interruption and replay

**Evidence:** LIVE. **Tasks:** T23. **Initial status:** NOT_RUN. **Initial assignment status:** T23=NOT_RUN.

**Setup:** Inject loss after build, create, publication, activation receipt, and StartSession.

**Action:** Restart host control and recover the original operation.

**Required result:** Work resumes only through qualified recovery. No duplicate agent or silent replacement appears.

### A80 — Complete native vertical path

**Evidence:** LIVE. **Tasks:** T23. **Initial status:** NOT_RUN. **Initial assignment status:** T23=NOT_RUN.

**Setup:** Use one actual role, project, target, account, and fresh configuration.

**Action:** Load, produce useful agent output, detach, reconnect, inspect, and stop.

**Required result:** Every phase has matching receipts. Reconnect preserves a surviving PTY. No direct Docker agent execution occurs.

### A81 — Domain split and collaboration

**Evidence:** LIVE. **Tasks:** T24. **Initial status:** NOT_RUN. **Initial assignment status:** T24=NOT_RUN.

**Setup:** Use two accounts with separate materializations and a proposed shared-write option.

**Action:** Create sessions and then request shared writable source.

**Required result:** Accounts remain separate domains. Shared writes require explicit collaboration authorization and visible risk.

### A82 — Default and in-domain shells

**Evidence:** LIVE. **Tasks:** T24. **Initial status:** NOT_RUN. **Initial assignment status:** T24=NOT_RUN.

**Setup:** Use an account domain and its logical instance.

**Action:** Open the default shell, then an explicitly selected in-domain shell.

**Required result:** The default shell is credential-free and separate. The in-domain shell has a trusted inherited-authority label.

### A83 — Session and instance lifecycle

**Evidence:** LIVE. **Tasks:** T24. **Initial status:** NOT_RUN. **Initial assignment status:** T24=NOT_RUN.

**Setup:** Use several sessions across two domains.

**Action:** Interrupt one, detach another, exit one agent, and restart Capsule explicitly.

**Required result:** Unrelated sessions survive appropriate actions. A new Capsule boot cannot claim the old PTY still exists.

### A84 — Usage cache identity

**Evidence:** UNIT. **Tasks:** T25. **Initial status:** NOT_RUN. **Initial assignment status:** T25=NOT_RUN.

**Setup:** Use two accounts, two credential revisions, and stale or failed provider responses.

**Action:** Resolve and display usage snapshots.

**Required result:** Entries remain isolated by stable identity and revision. Stale or unavailable usage does not become zero.

### A85 — Ended-run usage denial

**Evidence:** LIVE. **Tasks:** T25. **Initial status:** NOT_RUN. **Initial assignment status:** T25=NOT_RUN.

**Setup:** End an account grant while a delayed usage request remains queued.

**Action:** Deliver that request and attempt unrelated account enumeration.

**Required result:** Both unauthorized requests fail. No global account catalog reaches Capsule.

### A86 — Host affordance injection

**Evidence:** LIVE. **Tasks:** T26. **Initial status:** NOT_RUN. **Initial assignment status:** T26=NOT_RUN.

**Setup:** Emit hostile terminal escape sequences and unsolicited helper clipboard, URL, and export requests.

**Action:** Observe the desktop without a matching operator action.

**Required result:** No host action occurs. Untrusted output cannot forge trusted labels or approval state.

### A87 — Bound approval semantics

**Evidence:** LIVE. **Tasks:** T26. **Initial status:** NOT_RUN. **Initial assignment status:** T26=NOT_RUN.

**Setup:** Approve one fixed tool request, then vary domain, image, argv, inputs, destination, expiry, or replay identity.

**Action:** Execute each changed request and replay the original effect.

**Required result:** Every changed or replayed unauthorized request fails. Only the exact admitted operation can run.

### A88 — Isolated secret consumer

**Evidence:** LIVE. **Tasks:** T26. **Initial status:** NOT_RUN. **Initial assignment status:** T26=NOT_RUN.

**Setup:** Use a coding domain and a separately approved tool domain with a synthetic secret.

**Action:** Run the fixed consumer and inspect the coding PTY, logs, and host command path.

**Required result:** Only the approved consumer receives the secret. No arbitrary host shell or coding-session injection occurs.

### A89 — Independent checkpoint restore

**Evidence:** LIVE. **Tasks:** T27. **Initial status:** NOT_RUN. **Initial assignment status:** T27=NOT_RUN.

**Setup:** Create an accepted checkpoint outside source-owned storage.

**Action:** Delete the source sandbox and owned storage. Restore into a new admitted domain.

**Required result:** Required project and approved conversation state survive. The checkpoint does not depend on the deleted resource.

### A90 — Dirty untracked and unpushed retention

**Evidence:** LIVE. **Tasks:** T27. **Initial status:** NOT_RUN. **Initial assignment status:** T27=NOT_RUN.

**Setup:** Use tracked changes, selected untracked files, deletions, and unpushed commits.

**Action:** Checkpoint and restore the project.

**Required result:** All selected changes and the Git base match the manifest. No category silently disappears.

### A91 — Checkpoint credential exclusion

**Evidence:** LIVE. **Tasks:** T27. **Initial status:** NOT_RUN. **Initial assignment status:** T27=NOT_RUN.

**Setup:** Place known account and token canaries beside allowlisted conversation data.

**Action:** Create the checkpoint and inspect its manifest and content.

**Required result:** Known credentials and unrequested host data are absent. The report does not claim a scan proves arbitrary secrecy.

### A92 — Conflict-aware export

**Evidence:** LIVE. **Tasks:** T27. **Initial status:** NOT_RUN. **Initial assignment status:** T27=NOT_RUN.

**Setup:** Launch from baseline A and change the host checkout to B before return.

**Action:** Apply a returned patch based on A.

**Required result:** Conflicting content is preserved and reported. Host work is not overwritten blindly.

### A93 — Failed handoff safety

**Evidence:** LIVE. **Tasks:** T28. **Initial status:** NOT_RUN. **Initial assignment status:** T28=NOT_RUN.

**Setup:** Use an accepted checkpoint and independently authorized destination.

**Action:** Fail transfer, admission, resume, and source retirement at separate points.

**Required result:** The last accepted checkpoint remains recoverable. The source retires only after destination acceptance.

### A94 — Scoped eject and exile

**Evidence:** LIVE. **Tasks:** T28. **Initial status:** NOT_RUN. **Initial assignment status:** T28=NOT_RUN.

**Setup:** Use instances across two projects and two targets.

**Action:** Eject one instance and bulk-exile one explicit scope.

**Required result:** Only selected domains stop. Retained state follows its documented contract. Other scopes remain unchanged.

### A95 — Purge ownership guards

**Evidence:** LIVE. **Tasks:** T28. **Initial status:** NOT_RUN. **Initial assignment status:** T28=NOT_RUN.

**Setup:** Use original checkouts, owned runtime data, external PVCs, shared providers, and masked absence.

**Action:** Request purge under valid and uncertain ownership conditions.

**Required result:** Only confirmed selected owned data can be removed. Original and unrelated resources survive.

### A96 — CLI and JSON contract

**Evidence:** UNIT. **Tasks:** T29. **Initial status:** NOT_RUN. **Initial assignment status:** T29=NOT_RUN.

**Setup:** Use all frozen command forms, ambiguous selectors, and old backend flags.

**Action:** Parse commands and compare JSON output with v2 golden schemas.

**Required result:** Valid commands preserve semantics. Ambiguous and obsolete forms fail clearly. No v1 output compatibility path appears.

### A97 — Partial fleet visibility

**Evidence:** LIVE. **Tasks:** T29. **Initial status:** NOT_RUN. **Initial assignment status:** T29=NOT_RUN.

**Setup:** Use three targets, paginated scopes, and one unreachable target.

**Action:** Refresh the Console while one target fails mid-list.

**Required result:** Reachable targets stay usable. Stale rows remain. Coverage and completeness are explicit.

### A98 — Trusted status presentation

**Evidence:** LIVE. **Tasks:** T29. **Initial status:** NOT_RUN. **Initial assignment status:** T29=NOT_RUN.

**Setup:** Make an agent print false account labels, done events, and a fabricated passing result.

**Action:** Inspect Console state and attempt cleanup or acceptance.

**Required result:** Trusted host labels remain distinct. Agent output cannot grant authority, delete data, or set a verification verdict.

### A99 — Private developer forward

**Evidence:** LIVE. **Tasks:** T30. **Initial status:** NOT_RUN. **Initial assignment status:** T30=NOT_RUN.

**Setup:** Run a private sandbox service and an authorized local client.

**Action:** Create a loopback-bound authenticated forward and attempt external access.

**Required result:** The intended client works. Unapproved public access fails. Forward ownership and cleanup stay scoped.

### A100 — Public exposure separation

**Evidence:** LIVE. **Tasks:** T30. **Initial status:** NOT_RUN. **Initial assignment status:** T30=NOT_RUN.

**Setup:** Request a public service and a Capsule control endpoint.

**Action:** Inspect grants, listener TLS, application authentication, and exposure refusal.

**Required result:** Public exposure needs its own grant and authentication contract. Capsule control is never public.

### A101 — Artifact liveness under outage

**Evidence:** LIVE. **Tasks:** T31. **Initial status:** NOT_RUN. **Initial assignment status:** T31=NOT_RUN.

**Setup:** Reference an image from an active domain and a retained checkpoint on an unavailable target.

**Action:** Run cache and image-prune planning.

**Required result:** Required or uncertain artifacts remain live. Only confirmed owned unused artifacts are eligible.

### A102 — Safe prewarm

**Evidence:** LIVE. **Tasks:** T31. **Initial status:** NOT_RUN. **Initial assignment status:** T31=NOT_RUN.

**Setup:** Use a role with an optional account and optional engine capability.

**Action:** Prewarm tools and images without launch.

**Required result:** No account grant, credential exposure, companion, or engine prewarm occurs.

### A103 — Diagnostics and resource attribution

**Evidence:** LIVE. **Tasks:** T31. **Initial status:** NOT_RUN. **Initial assignment status:** T31=NOT_RUN.

**Setup:** Vary builder, gateway, host, and companion availability independently.

**Action:** Run doctor and inspect resource summaries.

**Required result:** Failures identify the correct owner. Requested, enforced, reserved, and measured resources stay distinct.

### A104 — Frozen verification binding

**Evidence:** LIVE. **Tasks:** T32. **Initial status:** NOT_RUN. **Initial assignment status:** T32=NOT_RUN.

**Setup:** Change the candidate, image, check bundle, argv, or budget after a prior passing check.

**Action:** Attempt to reuse the prior evidence for acceptance.

**Required result:** The host rejects stale evidence. Every verdict matches the complete frozen input identity.

### A105 — Verification outcome classes

**Evidence:** LIVE. **Tasks:** T32. **Initial status:** NOT_RUN. **Initial assignment status:** T32=NOT_RUN.

**Setup:** Run successful, failing, timed-out, cancelled, killed, and disconnected checks.

**Action:** Record verdict and cleanup results.

**Required result:** Each class remains distinct. Unknown completion and failed cleanup never count as a passing check.

### A106 — No verification fallback

**Evidence:** LIVE. **Tasks:** T32. **Initial status:** NOT_RUN. **Initial assignment status:** T32=NOT_RUN.

**Setup:** Make the OpenShell verifier unavailable while host Python and Docker remain available.

**Action:** Request independent verification.

**Required result:** The request is blocked. No host or direct Docker verification process starts.

### A107 — Engine capability change

**Evidence:** LIVE. **Tasks:** T33. **Initial status:** NOT_RUN. **Initial assignment status:** T33=NOT_RUN.

**Setup:** Use a running engine-disabled domain with retained work.

**Action:** Request engine enablement on supported and unsupported targets.

**Required result:** The supported route checkpoints and reprovisions. Unsupported targets reject before launch. No hidden sidecar appears.

### A108 — Full retained product coverage

**Evidence:** LIVE. **Tasks:** T34. **Initial status:** NOT_RUN. **Initial assignment status:** T34=NOT_RUN.

**Setup:** Use the P01 behavior register and the integrated v2 application.

**Action:** Execute each retained user journey and inspect its replacement path.

**Required result:** Every retained journey has real evidence or a release blocker. No old runtime helper serves a feature.

### A109 — External metadata outage

**Evidence:** LIVE. **Tasks:** T34. **Initial status:** NOT_RUN. **Initial assignment status:** T34=NOT_RUN.

**Setup:** Run a useful local session and make pull-request or CI metadata unavailable.

**Action:** Refresh status and perform local project work.

**Required result:** Local state remains useful. External data is marked unavailable without ending the session.

### A110 — Local container target matrix

**Evidence:** LIVE. **Tasks:** T35. **Initial status:** NOT_RUN. **Initial assignment status:** T35=NOT_RUN.

**Setup:** Prepare native Linux, macOS Docker Desktop, and macOS OrbStack rows.

**Action:** Run the required container journeys separately on each tuple.

**Required result:** Every claimed row has its own evidence. Live source behavior, identity, and cleanup match that row's contract.

### A111 — Remote container target matrix

**Evidence:** LIVE. **Tasks:** T35. **Initial status:** NOT_RUN. **Initial assignment status:** T35=NOT_RUN.

**Setup:** Prepare independent remote Linux and required remote macOS gateways.

**Action:** Build locally, deliver by registry, materialize remotely, attach, checkpoint, and clean up.

**Required result:** No laptop path or local image-ID assumption leaks into remote execution. Authorization and retention remain correct.

### A112 — VM identity and resource shapes

**Evidence:** LIVE. **Tasks:** T36. **Initial status:** NOT_RUN. **Initial assignment status:** T36=NOT_RUN.

**Setup:** Prepare required architectures and separate small and large VM gateways.

**Action:** Run role, project, account, shell, and resource tests on each row.

**Required result:** Actual identity works. Fixed gateway shapes are visible. Ignored per-sandbox limits are not claimed as enforced.

### A113 — VM storage and shutdown

**Evidence:** LIVE. **Tasks:** T36. **Initial status:** NOT_RUN. **Initial assignment status:** T36=NOT_RUN.

**Setup:** Use guest-owned project data, restricted projections, and an independent checkpoint.

**Action:** Test mount limits, guest access, orderly stop, interrupted shutdown, and flush failure.

**Required result:** No raw host-bind escape or widened mount occurs. Flush failures remain visible. Accepted checkpoints survive deletion.

### A114 — Coherent Kubernetes tuple

**Evidence:** LIVE. **Tasks:** T37. **Initial status:** NOT_RUN. **Initial assignment status:** T37=NOT_RUN.

**Setup:** Deploy official resources with dedicated supervisors and a fully pinned tuple.

**Action:** Inspect actual versions, ownership, developer permissions, and service accounts.

**Required result:** The deployment matches the lock. Jackin owns no Pod controller. Agents have no privileged cluster authority.

### A115 — Kubernetes network and control denial

**Evidence:** LIVE. **Tasks:** T37. **Initial status:** NOT_RUN. **Initial assignment status:** T37=NOT_RUN.

**Setup:** Use the real CNI and scoped developer identity.

**Action:** Probe denied ingress and egress. Attempt to weaken NetworkPolicy and bootstrap resources.

**Required result:** Traffic is actually blocked. Untrusted callers cannot weaken enforcement. Policy-object presence alone cannot pass.

### A116 — Kubernetes storage and image delivery

**Evidence:** LIVE. **Tasks:** T37. **Initial status:** NOT_RUN. **Initial assignment status:** T37=NOT_RUN.

**Setup:** Use automatic /sandbox storage, explicit StorageClass, quotas, and scoped pull credentials.

**Action:** Test Pod replacement, stop/start, role-image change, Sandbox deletion, and external PVC ownership.

**Required result:** Storage follows its actual owner. Image change does not imply PVC refresh. No child mount silently disables persistence.

### A117 — Combined real-agent journey

**Evidence:** LIVE. **Tasks:** T35, T36, T37, T38. **Initial status:** NOT_RUN. **Initial assignment status:** T35=NOT_RUN; T36=NOT_RUN; T37=NOT_RUN; T38=NOT_RUN.

**Setup:** Use one qualified target mechanism, two approved accounts, a real role, and a generic shell.

**Action:** Complete coding work, attach control, host action, checkpoint, failure recovery, verification, and cleanup. Repeat independently from fresh state.

**Required result:** The combined journey passes with actual credentials and intended isolation. Separate demos cannot substitute.

### A118 — Cross-area failure sweep

**Evidence:** LIVE. **Tasks:** T38. **Initial status:** NOT_RUN. **Initial assignment status:** T38=NOT_RUN.

**Setup:** Use the full product and every recorded external-effect boundary.

**Action:** Inject response loss, identity replacement, wrong scopes, storage faults, and process or gateway death.

**Required result:** No silent duplicate effect, authority expansion, unrelated deletion, or false persistence receipt occurs.

### A119 — Honest performance and resource report

**Evidence:** LIVE. **Tasks:** T38. **Initial status:** NOT_RUN. **Initial assignment status:** T38=NOT_RUN.

**Setup:** Use declared hosts, cache states, fixed workloads, and approved resource budgets.

**Action:** Measure one and two logical instances plus each critical-path phase. Record every sample.

**Required result:** Median, range, counts, overhead, and limits are explicit. Percentiles require the declared minimum samples. Unsupported scale is not claimed.

### A120 — Complete legacy deletion

**Evidence:** SOURCE. **Tasks:** T39. **Initial status:** NOT_RUN. **Initial assignment status:** T39=NOT_RUN.

**Setup:** Use the frozen P01 inventory and final source tree.

**Action:** Resolve each inventory record to a deleted path or reviewed pure/build-only replacement.

**Required result:** No obsolete executable implementation, hidden feature flag, old parser, or backend manifest remains.

### A121 — Semantic runtime-call audit

**Evidence:** SOURCE. **Tasks:** T39. **Initial status:** NOT_RUN. **Initial assignment status:** T39=NOT_RUN.

**Setup:** Inspect production subprocess sites, APIs, scripts, transitive imports, and packaged assets.

**Action:** Trace each possible Docker, Apple, host-worker, and Kubernetes execution route.

**Required result:** Only approved image infrastructure uses Docker directly. All agent and independent-worker execution uses OpenShell.

### A122 — Clean packaging and installation

**Evidence:** BUILD. **Tasks:** T39. **Initial status:** NOT_RUN. **Initial assignment status:** T39=NOT_RUN.

**Setup:** Use a fresh checkout and empty v2 configuration and state directories.

**Action:** Build packages, generate completions, install, and perform the fresh-start smoke tests.

**Required result:** No old binary, crate, fixture, parser, configuration, or state is required or shipped as a fallback.

### A123 — Complete release gate

**Evidence:** SOURCE. **Tasks:** T40. **Initial status:** NOT_RUN. **Initial assignment status:** T40=NOT_RUN.

**Setup:** Use every task report, target row, acceptance result, and independent review.

**Action:** Run missing, failed, blocked, and unreviewed prerequisite refusal assertions. Inspect the separate finalization contract.

**Required result:** Every refusal prevents approval. Finalization requires all three actual T40 case results and every fixed prerequisite.

### A124 — Accurate operational documentation

**Evidence:** SOURCE. **Tasks:** T40. **Initial status:** NOT_RUN. **Initial assignment status:** T40=NOT_RUN.

**Setup:** Use fresh-install, account, target, checkpoint, recovery, and capability documentation.

**Action:** Compare every supported claim with the qualified matrix and changed command contracts.

**Required result:** Documentation states actual limits and fresh setup. It includes no importer, automatic migration, or unsupported security promise.

### A125 — Release provenance and authorization

**Evidence:** BUILD. **Tasks:** T40. **Initial status:** NOT_RUN. **Initial assignment status:** T40=NOT_RUN.

**Setup:** Use the final source commit, approved tuples, patch records, and package digests.

**Action:** Build the release candidate and inspect its provenance, conditional maintainer-approval records for supported patched tuples, and publication actions.

**Required result:** Artifacts match the approved source. For each supported patched tuple, evidence/openshell-v2/T40/maintainer-approvals.json records maintainer approval bound to tuple ID and patch digest. Unpatched tuples need no approval record. No unauthorized publication or production change occurs.
