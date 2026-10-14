# C4: checkpoints, backup, and work preservation

## Purpose

Create immutable source revisions while the original agent remains alive.
Keep incomplete work and publish authorized backup without accepting the code.

## Existing code and changes

Extend existing clone materialization and cleanup protection.
Do not create another container manager or Git workspace system.
Keep a separate host-owned checkpoint repository outside worker mounts.

## API

| Operation | Input | Output |
|---|---|---|
| `prepare_clone` | Approved source repository, base commit, run and worker IDs | Independent clone and ownership record. |
| `capture_checkpoint` | Worker identity, current candidate generation, operation ID | `Checkpoint` or a preserved capture error. |
| `compare_source` | Worker identity and checkpoint manifest | Match, changed source, or unknown. |
| `backup_checkpoint` | Checkpoint, authorized remote ID, mode, operation ID | Exact remote ref and commit, or explicit failure. |
| `hold_work` | Owned source or checkpoint ID and reason | Durable preservation hold. |
| `check_removal` | Owned resource ID | Allowed removal or blocking hold. |

`Checkpoint` contains the run ID, round, candidate generation, commit ID, tree ID, source digest, parent, and manifest path.
The source manifest contains sorted paths, file types, modes, content digests, and explicit exclusions.
The host assigns checkpoint identity. Agent-written metadata cannot assign it.

## Private clone

Start from the exact approved base commit.
A local clone must not retain writable object links or alternates to the source repository.
Do not use Git worktrees for the MVP.
Keep each worker's Git state independent.

The first release supports regular files, executable files, and safe in-tree symlinks.
Reject required submodules or LFS content until their exact-input handling is implemented.
Do not silently omit them.
Do not use the host's uncommitted changes unless they are separate frozen input files.

## Consistent capture procedure

1. Confirm automatic input ownership and the expected candidate generation.
2. Wait for the active implementer turn to end.
3. Save the capture intent and a preservation hold.
4. Prevent new automatic or operator input during capture.
5. Pause the exact implementation container through C3.
6. Confirm that the backend reports all worker processes paused.
7. Drain pending source writes through the supported filesystem boundary.
8. Check source stability before copying.
9. Copy source bytes to an empty host-owned staging directory.
10. Check source stability again before ending the pause.
11. Discard an incomplete staging copy when source stability is not proven.
12. Create the host-owned Git tree and commit from the copied bytes.
13. Save the checkpoint and its manifest before use by verification.
14. Save the required resume action.
15. Resume the same container and confirm its state.
16. Record the unchanged physical session key.

Use host-side access to the private bind-mounted source.
Do not use `docker exec` to run capture inside a paused container.
Do not copy live writable source without a proven write barrier.
A quiet terminal is not a write barrier.
A matching file count is not proof of a stable source tree.

The selected local Docker and filesystem combination must pass K2.
If the backend cannot establish source stability, reject capture and preserve work.
Do not claim general filesystem snapshot support from one tested combination.

Exclude `.git` from worker content capture.
Include task source and untracked source files unless the frozen plan explicitly excludes them.
Do not derive exclusions from an agent-edited ignore file.
Do not follow symlinks outside the source root.
Reject device files, sockets, traversal paths, and unsupported file types.
Apply the configured copy-size limit.

Use trusted Git plumbing with a sanitized environment and exact source bytes.
Do not run worker hooks, filters, helper programs, or worker Git configuration on the host.
Do not let `.gitattributes` transform the captured bytes.
Build the checkpoint's parent relation from host-owned history.

A short full-container pause also pauses Capsule output.
Display `checkpointing` until output resumes.
Do not promise continuous TUI output during this interval.

## Acceptance source check

Before acceptance, repeat the source barrier and manifest comparison.
Compare the actual current source with the candidate manifest.
The host must still own input and the same candidate generation.
Record acceptance in the database while the source remains stable.

When the source differs, invalidate old eligibility.
Start a new verification round within the remaining limit, or request operator action.
Never attach passing evidence to a different source tree.

## Early backup

Keep every checkpoint locally before remote backup.
Use a unique run-specific ref, such as `refs/heads/jackin/workflows/<run>/<checkpoint>`.
The host selects the remote from an authorized registry.
The Pkl file cannot supply an arbitrary credential or expand the permitted ref prefix.

Use an expected-absent ref creation at the Git transport boundary.
If the ref already names the same commit, record success.
If it names another commit, record a conflict.
Do not emulate this operation with an unchecked read followed by an unconditional ref write.
Never force-update, delete, or merge an existing ref.

After a lost response, query the exact ref before retrying.
A matching ref completes the saved operation.
An unknown remote state remains pending or blocked.
No worker receives the publication credential.

`required` backup blocks progress on failure.
`best_effort` backup retains local work and shows a warning.
`off` records the explicit local-only choice.
Do not add PR creation or final publication in this component.

## Preservation

Normal cancellation, failure, retention expiry, and prune must obey preservation holds.
Protect the implementation clone, checkpoint repository, and required evidence.
Check holds in existing cleanup paths, not only in the new workflow CLI.
Do not remove source because it has no uncommitted changes.
A clean repository can still contain the only unpushed commit.

If the host crashes during capture, recover the pending pause or resume action first.
Keep the source even when the staging copy is incomplete.
An instance pause cannot prevent the host from reporting a durable workflow failure.

## Local verification

Run K2 with a child writer, a rejected cleanup, and an exact-ref backup conflict.
Use the same fixture in K7.
Do not add snapshot providers or object-cache optimization.

## Dependencies and sources

C3 owns pause and resume. C5 owns intent records and holds. C6 consumes checkpoints.
See [SOURCES.md](../SOURCES.md), E3, and the review's checkpoint and backup findings.
