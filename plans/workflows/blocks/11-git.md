# B11 — Git block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Perform declared repository operations and produce traceable immutable revisions. Local preparation/checkpoint/integration are separate from remote publication.

## API: configuration and ports

`kind: git`. Configuration is an operation union: `prepare`, `checkpoint`, `compare`, `create_branch` or `integrate`. Each variant defines repository identity, exact base/head inputs, branch/strategy when applicable, capture policy, expected ref state and output contract. Remote push/PR/merge operations belong to Publish and host authorization, not a generic arbitrary Git command.

Inputs: repository/context/session barrier or checkpoint sets depending on operation. Outputs: `repository-ref/v1`, `checkpoint/v1`, `diff: artifact-set/v1` or a typed conflict result. A checkpoint variant requires certified write-barrier proof before capture.

## Functionality

Prepare exact private clones, create host-sealed checkpoint commits, compare exact revisions or integrate an ordered set of declared checkpoints. Disable untrusted hooks/filters and unsafe host Git configuration. Host-created commits and working-state archives have recorded provenance and preservation holds.

Integration conflicts produce evidence and explicit policy/operator or bounded repair handling. The integrated result is a new candidate requiring independent verification. A mutable branch name is only a locator, never acceptance identity.

## Permissions, errors and limits

Validate repository/ref namespaces and prohibit implicit destructive operations. Permission to write `.git` cannot be meaningfully restricted only by blocking the `git` executable; use mount/metadata ownership enforcement when local mutation is denied. Private working repositories may permit local Git operations without permitting host or remote mutation. Every unique unpushed state remains protected.

## Build instructions and acceptance

Extend C08 and existing isolation/safe-removal helpers. Test exact refs, independent objects, hooks/config escapes, consistent capture, conflicts and new-revision invalidation. Test IDs: `GIT-01` through `GIT-08`, `AUTH-02`.

## Implementation ownership

See [C08 — Repository isolation and immutable checkpoints](../components/08-repository-checkpoints.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.
