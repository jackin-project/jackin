# B02 — Context block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Prepare exact reference documents and structured task inputs for later blocks. Context resolution is deterministic or explicitly provenance-recorded; it is not a hidden agent research task.

## API: configuration and ports

`kind: context`. Configuration: ordered `sources`, permitted `sourceKinds`, expected content/version/digests where known, output names/media types, schema, byte limits, classification and retention. Sources may be frozen plan attachments or typed outputs of completed upstream blocks. Authorized remote refresh must be explicit and records new immutable content for this attempt.

Inputs: declared artifact/text/JSON outputs. Output: `bundle: context-bundle/v1`, containing immutable entries, provenance and intended in-worker paths. Derived manifests are `artifact-set/v1` only when declared. No output grants broader filesystem access.

## Functionality

Resolve approved source references, validate schema/type/size, normalize only declared transformations and seal immutable artifacts. Ordered concatenation must specify separators and encoding. Source text that looks like instructions remains untrusted task content and is not interpreted as host policy.

The block may run as a host-local read-only data operation with zero command-execution rights. Any transformation requiring code, shell or network beyond the approved resolver becomes an explicit Command or Agent block in a sandbox. A remote URL fetch is not an excuse to expose arbitrary host HTTP access.

## Permissions, errors and limits

Declare host file reads and URL destinations separately. Reject unauthorized paths, redirects, SSRF, oversized or missing references and checksum mismatches. Read-only mounts must not be shadowed by writable parent mounts. Credentials embedded in source URLs are stripped from ordinary display and forbidden in exported provenance.

## Build instructions and acceptance

Implement on C11’s resolver/sealer and use its mount manifest. Test immutable content, same ordered bundle digest on retry, upstream artifact provenance, readonly access, unsafe source denial and explicit failure on truncation. Test IDs: `ART-01`, `ART-02`, `ART-05`, `ART-07`.

## Implementation ownership

See [C11 — Context preparation and artifact storage](../components/11-context-artifacts.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.
