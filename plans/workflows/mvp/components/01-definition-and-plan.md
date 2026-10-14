# C1: workflow definition and immutable plan

## Purpose

Convert one typed Pkl definition into a complete fixed-sequence plan.
Resolve all inputs before worker execution.
Reject a request that the selected runtime cannot support.

## Existing code and changes

Keep existing role and workspace configuration.
Add `jackin-workflow-spec` for workflow types and plan compilation.
Use `pklr` for Pkl evaluation. Do not create another Pkl parser.
Add only the fixed repair template in this release.
The full block catalog is not an MVP dependency.

## API

These are proposed interfaces, not current Rust symbols.

| Operation | Input | Output |
|---|---|---|
| `validate_definition` | Source bundle, host profile, adapter profiles | Diagnostics with file, field, code, and message. |
| `compile_plan` | Valid definition and resolved inputs | `ExecutionPlan`, canonical bytes, and `plan_hash`. |
| `read_plan` | Stored plan hash | Verified immutable plan. |
| `admit_plan` | Plan and current host authorization | Final launch grant, or a typed rejection. |

Use these core DTOs:

| DTO | Required fields |
|---|---|
| `AgentSelection` | Agent slug, role ID, role revision, account ID, auth mode, model ID, binary digest, image digest. |
| `InputFile` | Logical path, content digest, byte length, read-only mount path. |
| `RepositoryInput` | Authorized repository ID, exact base commit, source manifest digest. |
| `ResourceLimits` | Integer CPU millicores, memory bytes, process count, run seconds, disk monitor bytes. |
| `VerificationPolicy` | Test bundle digest, exact argv, test image digest, timeout, review schema version. |
| `BackupPolicy` | Mode, authorized remote ID, host credential reference, permitted ref prefix. |
| `ExecutionPlan` | Schema version, template, all DTOs above, repair limit, input digests, granted capability facts. |

Account IDs and credential references are not secret values.
Do not store a token, password, or raw key in a plan.
Store the actual model as unknown when the vendor cannot report it.
Keep the requested model separately.

## Functionality

Use the typed fields in [EXAMPLE.md](../EXAMPLE.md).
Expand them into this closed sequence:

```text
prepare -> implement -> capture -> test -> review -> decide
                                 ^                    |
                                 |-- repair turn -----|
```

The repair relation is a bounded controller action, not a general graph edge.
Assign stable internal node IDs from stage and round.
Reject unknown templates, extra fields, negative limits, missing inputs, and unsupported capabilities.

Permit the supplied local schema and approved local template modules only.
Freeze all module bytes before evaluation.
Deny remote imports, arbitrary file reads, environment reads, and external process calls.
Use `pklr` evaluator capabilities to enforce these restrictions.
Do not rely on a text scan of the Pkl source.

Run synchronous evaluation in a killable helper process.
Apply an evaluation deadline and the supported process memory limit.
Return `evaluation_limit` when either limit is reached.
Do not leave a timed-out evaluator active in a Tokio blocking thread.

Use a pinned evaluator version and a versioned local schema.
Test the language features used by the example: types, constraints, local imports, and amendments.
Do not promise full Pkl parity from these tests.

Serialize the complete plan with deterministic map ordering and integer numeric fields.
Keep array order when order has meaning.
Hash a version prefix and the exact stored plan bytes with SHA-256.
Exclude only the derived `plan_hash` field from its own hash input.
Use an existing maintained serializer and hash implementation.

Copy task, reference, and test-policy bytes into the protected input store.
The active run reads these copies, not the source Pkl or edited workspace defaults.
Recheck authorization at launch and after a host restart.
A revoked grant blocks execution. A new broader grant requires a new plan.

## Errors

Return `invalid_definition`, `missing_input`, `unsupported_capability`, `authorization_denied`, `evaluation_limit`, or `plan_digest_mismatch`.
Give the operator the field and corrective action.
Do not include credential values in diagnostics.

## Build instructions

1. Define the DTOs and fixed template version.
2. Add the local Pkl schema and minimal evaluator helper.
3. Freeze input files and resolve existing Jackin selections.
4. Compile the fixed plan.
5. Add hashing and immutable plan storage.
6. Add the thin validation and planning commands.

## Local verification

Run K3 from [CHECKS.md](../CHECKS.md).
Use one valid fixture and a small table of invalid fixtures.
Change the original Pkl after compilation and confirm that the stored plan remains unchanged.
Do not add a general graph test framework.

## Dependencies

Use C2 adapter facts and C3 enforcement facts for admission.
Use C5 to retain the plan for a run.
See [SOURCES.md](../SOURCES.md), E4 and E5, for language and evaluator references.
