# jackin-workflow-spec

Typed repair-workflow definitions (Step 3 of the workflows MVP).

## What this crate owns

- Frozen Pkl evaluation (`eval`): two supplied local modules (the
  versioned schema plus one workflow file) are frozen to bytes, then
  evaluated inside a killable helper process (`jackin-workflow-eval`)
  spawned through `jackin-process` with a deadline. A counting
  allocator inside the helper caps evaluation memory; the parent maps
  a timeout or a helper death to `evaluation_limit`. The evaluator
  permits only the supplied modules via `pklr` capabilities
  (`eval-core`, no native IO): ambient file reads, environment reads,
  remote imports, temp dirs, and globs are all denied. No text scan.
- Plan compilation (`rules`, `model`): the evaluated JSON is checked
  against the closed repair template (fixed node IDs from stage and
  round), cross-field role rules (implementer is claude, reviewer is
  codex), host-grant maxima, and the unsupported-capability list.
- Input resolution (`resolve`): task, reference, and test-bundle bytes
  are read relative to the workflow file's directory and hashed; the
  repository `HEAD` selector is resolved to an exact commit once via
  `git` before hashing.
- Plan digests (`digest`, `store`): canonical JSON bytes (sorted map
  order, integer numerics, meaningful array order preserved) hashed
  with SHA-256 over a version prefix. The derived `plan_hash` is
  excluded from its own hash. The protected input store keeps
  read-only hash-addressed copies; `read_plan` re-verifies the digest.
- Typed errors (`error`): `invalid_definition`, `missing_input`,
  `unsupported_capability`, `authorization_denied`, `evaluation_limit`,
  and `plan_digest_mismatch`, each with file, field, code, message,
  and a corrective action. No credential values appear in diagnostics.

## Structure

| Module | Owns | Tests |
|---|---|---|
| `error` | Error codes, diagnostics, `WorkflowError` | — |
| `model` | Plan DTOs (`AgentSelection`, `InputFile`, `RepositoryInput`, `ResourceLimits`, `VerificationPolicy`, `BackupPolicy`, `ExecutionPlan`) | — |
| `eval` | Frozen bundle, helper protocol, bounded evaluation | — |
| `resolve` | File and repository input resolution | — |
| `rules` | Definition validation and plan compilation | — |
| `digest` | Canonical bytes and SHA-256 plan hash | — |
| `store` | Protected input store and verified reads | — |
| `jackin-workflow-eval` (bin) | Sandboxed evaluation helper | — |

## Memory-limit note

The helper enforces its memory cap with the allocation-counting `cap`
allocator rather than `RLIMIT_AS`: macOS refuses to lower the address
space limit, so an rlimit-based cap cannot work on every host this
repository targets. The counting cap is portable and fail-closed, and
keeps `unsafe` out of this crate.

## Non-goals

No capsule, runtime launch, isolation, daemon, package manager,
general graph language, or full Pkl parity. Later steps own launch
admission (`admit_plan`), plan retention for runs, and the rest of
the CLI.
