# C08 — Repository isolation and immutable checkpoints

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Give each mutable attempt an isolated exact Git starting state and produce immutable, independently verifiable checkpoints. Extend `jackin-isolation` and its existing state/safe-removal boundaries; keep workflow coordination in the orchestrator.

## 2. Existing foundation and gap

The inspected materializer has both clone and worktree layouts. Worktree auxiliary mounts deliberately bind the host’s complete `.git` directory read-write. That arrangement is explicit shared-reference behavior, not strict worker independence. Reuse clone support but audit its object-sharing and exact-revision behavior before certification. [J14](#evidence-and-references), [J15](#evidence-and-references).

## 3. API and data contracts

```text
prepare_repository(effect_id, RepositoryInput, grant) -> RepositoryHandle
checkpoint(effect_id, repository, writer_barrier, capture_policy) -> Checkpoint
materialize_checkpoint(checkpoint, consumer_attempt, mode) -> RepositoryHandle
compare(base_checkpoint, head_checkpoint) -> DiffArtifact
integrate(ordered_checkpoints, base_checkpoint, strategy, grant) -> Checkpoint | Conflict
pin(checkpoint_or_repository, hold) -> Receipt
```

A checkpoint records exact base/head/tree OIDs, capture method and barrier proof, producer attempt, candidate ordinal, source manifest, submodule/LFS references, immutable object bundle/archive digest and preservation hold. Host-owned metadata is outside worker-writable mounts.

## 4. Required functionality

Resolve all initial Git refs to exact OIDs at planning/preparation. Private clones must not expose another mutable `.git`, share writable alternates, depend on another worker’s object GC, or accidentally copy remote credentials. Verify independence; use existing clone primitives with reviewed options such as independent transfer/dissociation rather than assuming a path named “clone” proves it. Pin submodules and LFS content or explicitly prohibit them for that plan.

For a checkpoint, first acquire the write barrier supplied by the runtime. It must cover background writers, not merely wait for an idle agent screen. Capture a consistent repository snapshot into a host-owned staging area, validate its content/path manifest, import or create a checkpoint commit under sanitized Git configuration, and seal the resulting object set durably. Release the barrier only after the source state can be recovered. An atomic backend snapshot can replace a process freeze when certified.

Do not run untrusted repository hooks, credential helpers, filters or arbitrary Git configuration as a privileged host process. Use a sanitized environment/configuration, disable hooks and unsafe protocols, and perform untrusted checkout/filter operations inside a dedicated sandbox. Validate symlinks, hardlinks, special files and size limits when importing artifacts. The agent’s branch pointer can move later; the host-sealed checkpoint cannot.

Define capture treatment explicitly: tracked edits and intended untracked source files enter the code checkpoint; ignored/build outputs are excluded by declared rules; non-code unfinished material receives a separate protected working-state archive when retention permits. Secret scanning/quarantine is advisory defense, not a guarantee; suspected secrets block external checkpoint push until authorized remediation. Never discard the only original copy because an archive exceeded a limit.

## 5. Concurrent review and integration

An implementer may continue after a checkpoint is sealed while independent test/review clones evaluate it. Every result retains that checkpoint ID. Late results never rebind to the newest branch head. Candidate ordinals provide total ordering within the implementation scope.

A Join only joins structured outputs. Combining code requires an explicit Git integration operation with a defined base, ordered heads and strategy. Conflicts become structured evidence/operator or bounded repair work. The combined commit is a new candidate requiring verification; individually accepted branches do not automatically make the merge accepted.

## 6. Build instructions

Add exact-revision repository descriptors and private-clone conformance tests to existing materialization. Add host-owned checkpoint state and a two-phase capture/seal journal. Implement checkpoint consumers as independent clones/RO sources with separate writable build directories. Add integration as a separate operation and wire preservation holds before any cleanup.

## 7. Dependencies and integration

C04 provides barriers and consumers, C10 records intents/ordinals, C11 seals artifacts, C12/C13 consume checkpoints, C16 publishes sealed OIDs and C17 protects copies. Existing `safe_remove` remains the removal primitive.

## 8. Acceptance tests

`GIT-01`: exact start OID and object independence. `GIT-02`: worktree mode is refused under strict isolation. `GIT-03`: active/background writer race cannot produce a mixed checkpoint. `GIT-04`: worker ref rewriting cannot alter a sealed checkpoint. `GIT-05`: hooks/filters/symlinks cannot execute or escape on host. `GIT-06`: stale evidence stays attached to old revision. `GIT-07`: integration commit invalidates prior branch acceptance. `GIT-08`: excluded/oversized unfinished state is preserved or blocks cleanup, never silently lost.

## Evidence and references

- **J14 — [Git isolation public surface](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-isolation/src/lib.rs).** Full file. Existing materialization, finalization, cleanup, state and safe-removal boundaries.
- **J15 — [Git mount materialization](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-isolation/src/materialize.rs).** Lines 1–280. Both clone and worktree path layouts; worktree auxiliary mount intentionally exposes shared host .git read-write. Full clone implementation not audited.
- **E11 — [Git worktree reference](https://git-scm.com/docs/git-worktree).** Official shared and per-worktree reference behavior. Accessed 2026-10-14.

## Checkpoint cadence and final candidate

The frozen `CheckpointPolicy` defines turn-end/pause/cancel captures plus optional finite periodic capture and a maximum count/byte budget. Long agent turns therefore need not leave all work uncheckpointed until completion. Every capture still requires a certified barrier or snapshot. An interval firing while a barrier is unavailable records a delayed/failed checkpoint and may stop work at the declared maximum uncheckpointed interval; it does not create an inconsistent snapshot.

Before final acceptance, acquire a finalization barrier and compare the current source manifest to the candidate under review. Seal and verify a new candidate if unsealed changes exist. Record a `FinalizationRecord` and keep its writer fence effective through the authorized publication boundary. Early checkpoint backup remains separate from that finalization and acceptance.
