# C02 — Configuration compiler and immutable plan

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Turn Pkl plus explicitly resolved inputs into a validated immutable execution plan. Own dependency resolution, canonicalization, diagnostics and source mapping in `jackin-workflow-spec`, with host IO supplied through a narrow resolver port.

## 2. Existing foundation and gap

pklr’s source supports evaluator-only embedding with custom capabilities, but Jackin integration and language/security conformance have not been tested. Adopt candidate source version 5.1.1 only after dependency review and certification. No official Pkl CLI fallback is allowed in production. [E01](#evidence-and-references), [E02](#evidence-and-references).

## 3. API and data contracts

```rust
// Proposed interfaces; surrounding domain types are specified separately.
fn validate(request: ValidateRequest, resolver: &dyn InputResolver)
    -> Result<ValidationReport, WorkflowError>;
fn compile(request: CompileRequest, resolver: &dyn InputResolver)
    -> Result<PlanEnvelope, WorkflowError>;
fn verify_plan(envelope: &PlanEnvelope) -> Result<(), WorkflowError>;
```

`CompileRequest` names a source bundle, explicit parameter values, schema version, repository/role selectors and host policy context. `InputResolver` returns content-addressed bytes, exact Git and image/binary digests, adapter capability snapshots and non-secret authorization facts. Planning may read/fetch only approved inputs; it never starts an agent, runs repository hooks or publishes code.

`ValidationReport` separates syntax/type/graph errors, policy denials, missing runtime certificates and warnings. `PlanEnvelope` uses the canonical model; source spans and a projection of editable fields are stored outside its execution semantics unless explicitly hash-covered as input content.

## 4. Required functionality

Resolve and freeze the transitive import closure, source files, attachments, role/workspace inputs, starting repository OIDs, images and runtime binary identities. HTTP/package fetches go through a host allowlisted resolver with redirect/origin limits, content-size budgets and checksum verification. Evaluation sees only the frozen snapshot and allowed non-secret parameters; ambient environment and current filesystem state are absent.

Use pklr with default native IO/HTTP disabled and custom capabilities. Run evaluation in a killable restricted helper process with memory, CPU, output and wall-clock bounds; this helper is still Rust/pklr, not a separate orchestration service. A Tokio timeout around `spawn_blocking` cannot guarantee a runaway evaluator stops. Reject a backend/platform that cannot establish the selected evaluator restrictions.

Evaluate, deserialize strictly, validate all typed outputs, validate graph/ports/repair scopes, resolve actual required capabilities, authorize, then canonicalize. Stable-sort maps and order-insensitive sets; preserve semantically ordered arrays. Use a reviewed canonical JSON implementation and normalized integer units. Hash `"jackin-workflow-plan/v1\0" || canonical_bytes` with SHA-256. Store those exact bytes, source bundle and import lock before returning a plan ID.

For each dependency-free parallel branch, check that requested writable instance/repository identities do not alias. Check required gates are reachable on every successful path, joins account for skipped outputs, and no condition can bypass a mandatory acceptance check.

## 5. Immutability and errors

An active run uses only the stored plan and frozen artifacts. Reopening the original Pkl is a source-view operation, not reconfiguration. Different effective defaults, role revisions, runtime binary digests, attachments or permissions must produce a different plan hash. Secret rotation does not change the secret reference’s identity, but action authorization is rechecked.

Unknown model availability is a named preflight result, not an invented success. Explicit fallback models must already be enumerated and authorized in the plan. An expired adapter or backend certificate blocks launch even when syntax validation passes.

## 6. Build instructions

Implement a pure canonical-domain compiler against an in-memory resolver first. Add graph diagnostics and fixture hashes. Implement the frozen-bundle resolver and evaluator helper separately; test denied file/env/network reads, redirect escapes, package traversal and exhausted budgets. Integrate role/workspace resolution without mutating those resources. Add source-span mappings only after byte/hash invariants are stable.

## 7. Dependencies and integration

Requires C01, C03, C05 and C11 interfaces. Store the plan through C10. Use existing role/image/credential metadata services through ports, not service imports that create a core-to-runtime dependency cycle.

## 8. Acceptance tests

`PLAN-01`: identical frozen inputs yield identical bytes/hash across independent compilations. `PLAN-02`: every execution-relevant input mutation changes the hash. `PLAN-03`: active source edits have no effect. `PLAN-04`: transitive mutable import changes are detected. `PLAN-05`: forbidden host IO and runaway evaluation terminate safely. `PLAN-06`: unsupported runtime/isolation capabilities fail before provisioning. `PLAN-07`: required gates cannot be bypassed by conditions or skip semantics.

## Evidence and references

- **E01 — [pklr source README](https://github.com/jdx/pklr/blob/ed746fcf308c5510e159064814fafb5dd151ac44/README.md).** Pinned source documentation: pure-Rust evaluator and eval-core/custom EvalCapabilities embedding. No conformance tests run in this audit.
- **E02 — [pklr Cargo manifest](https://github.com/jdx/pklr/blob/ed746fcf308c5510e159064814fafb5dd151ac44/Cargo.toml).** Source candidate version 5.1.1; default native IO/HTTP features versus eval-core-only embedding. Exact registry artifact must be verified before adoption.
- **E03 — [Apple Pkl CLI reference](https://pkl-lang.org/main/current/pkl-cli/index.html).** Official Pkl language/evaluation reference. Its sandbox options do not automatically exist in pklr. Moving documentation, accessed 2026-10-14.
- **J06 — [Programmatic launch options](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-launch-load-options/src/load_options.rs).** Lines 1–300. Non-interactive launch, programmatic constructor, identity sink, inputs and validation error vocabulary. Deeper launch call paths were not executed.
- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.
