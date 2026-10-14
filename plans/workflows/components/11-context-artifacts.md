# C11 — Context preparation and artifact storage

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Resolve exact task inputs, mount them read-only and retain verifiable outputs. Implement a workflow artifact service with content-addressed immutable objects, protected metadata and explicit retention. It is not a second general-purpose filesystem exposed to agents.

## 2. Existing foundation and gap

Reuse Jackin’s existing mount, credential and diagnostics boundaries. The new work is workflow-specific content provenance, immutable reference resolution and durable artifact/evidence lifecycle. General telemetry must remain separate from raw prompts, documents and terminal content. [J07](#evidence-and-references), [J15](#evidence-and-references), [J18](#evidence-and-references).

## 3. API and data contracts

```text
resolve_context(context_spec, authorized_sources) -> ContextBundle
begin_upload(owner, media_type, expected_size, policy) -> UploadHandle
append_chunk(upload, offset, bytes) -> ChunkReceipt
seal(upload, expected_digest) -> ArtifactRef
read(artifact_ref, principal, range?) -> VerifiedContent
materialize_readonly(bundle, instance, grant) -> MountManifest
retain(artifact_ref, hold) -> Receipt
export(artifact_refs, export_policy, principal) -> ExportReceipt
```

`ArtifactRef` contains opaque ID, content SHA-256, byte length, media/schema type, provenance, producer identity, classification, encryption/key metadata, sealed state and references to owner/retention records. Hashes identify bytes, not authorization. `ContextBundle` has ordered entries with logical name, source locator/version, digest, in-sandbox path and intended use. Sensitive source locators are protected metadata.

## 4. Required functionality

Freeze task documents and reference files before launch. A Context block may consume declared earlier node outputs at runtime, but those resolved input artifact digests are persisted for that attempt; they do not mutate the original plan. Capture fetch time and source identity for remote content. Read only host-authorized files or allowlisted URLs; block SSRF, redirects to private services, path traversal and unbounded downloads.

Mount references read-only outside writable repository parents or ensure nested mount enforcement cannot be shadowed. Deduplicate immutable bytes across authorized owners without leaking whether another user/workspace has a matching private artifact. Agents cannot read host artifact paths by guessing content hashes. Output collection uses allowlisted relative paths and bounded traversal, never arbitrary worker-chosen host paths.

Seal an artifact by writing a staging object, computing/checking its digest, completing encryption where required, flushing the file and parent metadata, then atomically renaming into immutable storage. Only then insert the database reference. A crash creates a recoverable staging/orphan record, not a valid missing blob. Reads verify authenticated encryption and content digest before serving evidence.

Prompts, terminal captures, private repository diffs and credentials-adjacent data are sensitive by default. Use an approved authenticated-encryption implementation and a host key-provider boundary, not custom cryptography or keys in Pkl. The key provider must reuse or integrate with approved host credential facilities. If encryption/key storage is unavailable, fail the requested sensitive-retention contract rather than store plaintext silently.

## 5. Limits and retention

Every input/output has max bytes, media/schema type and retention class. Exceeding a limit fails collection with a named reason; it does not silently truncate an acceptance artifact. Diagnostic streams may be capped only if the cap and missing range are visible. Do not include raw credentials in artifacts by design; redact/quarantine suspicious content and document that automatic redaction is not perfect.

Content needed for active runs, pending effects, acceptance proofs or unique-work preservation stays pinned beyond ordinary TTL. Cleanup releases references only through C17/C22. Artifact export is an authorized operation with safe filenames, no path traversal and a manifest of included/omitted/redacted content.

## 6. Build instructions

Implement storage and key-provider ports with test adapters. Add upload/seal/read integrity tests, then database reference transactions and orphan recovery. Integrate context resolution through approved mount/path resolvers. Add output collectors for logs, JSON, Git bundles and terminal traces. Keep private content out of telemetry at every stage.

## 7. Dependencies and integration

C02 freezes source bundles; C04 mounts data; C08 seals Git objects; C10 records references; C12–C13 consume verification artifacts; C17/C22 enforce retention and export. Use common context and output contracts across Agent, Context, Command and Review blocks.

## 8. Acceptance tests

`ART-01`: context bytes/digests match the plan or recorded node inputs. `ART-02`: worker cannot modify references or escape output collection. `ART-03`: kill during seal/DB insertion recovers without dangling accepted artifacts. `ART-04`: corruption and wrong keys fail closed. `ART-05`: limits never cause silent acceptance truncation. `ART-06`: protected holds prevent TTL deletion. `ART-07`: remote resolution rejects SSRF/redirect/path escapes.

## Evidence and references

- **J07 — [Shared Capsule contracts](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/lib.rs).** Lines 1–280. Session capability and identity environment, scoped credential contracts, CapsuleConfig and exit preservation choices.
- **J15 — [Git mount materialization](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-isolation/src/materialize.rs).** Lines 1–280. Both clone and worktree path layouts; worktree auxiliary mount intentionally exposes shared host .git read-write. Full clone implementation not audited.
- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.
