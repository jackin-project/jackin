# B12 — Publish block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Perform an explicitly authorized host publication action: checkpoint backup, accepted branch push, PR creation or artifact production.

## API: configuration and ports

`kind: publish`. Configuration: `action`, exact repository/remote and branch/base policy, permitted artifact classifications, required checks, idempotency/reconciliation policy and output contract. Action variants are `push_checkpoint`, `push_accepted`, `create_pr`, `export_artifacts`.

Inputs: sealed checkpoint plus backup authority for `push_checkpoint`; exact acceptance certificate for accepted publication; declared artifact set and export policy for artifact-only output. Output: `publication: publication/v1`, with exact OID/artifact digests, remote receipt, effect identity and state.

## Functionality

Resolve host-held scoped credentials at dispatch. Verify exact target/source and current authorization, persist the effect intent, execute the declared mutation, then verify/store the receipt. On an uncertain outcome, lookup/reconcile before retrying. Checkpoint push can occur before acceptance but never labels that work accepted or merged.

For PR creation, use a unique run/effect marker and exact head/base identity to avoid blind duplication. An artifact-only publish can produce a retained local artifact without GitHub access. A successful export does not authorize later network sharing.

## Permissions, errors and limits

No implicit force push, branch deletion, default-branch update or merge. Workers do not receive publishing credentials. Stale acceptance, remote divergence, changed required checks, forbidden classification or unresolved prior side effect stops dispatch. Publication failure retains all work and its separate acceptance state.

## Build instructions and acceptance

Use C16’s effect journal and remote adapter. Test lost responses, duplicate markers, exact accepted OID, no worker credentials and retention after failed/offline push. Test IDs: `PUB-01` through `PUB-08`, `REC-04`.

## Implementation ownership

See [C16 — Checkpoint publication and final GitHub publication](../components/16-publication.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.
