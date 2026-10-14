# Pkl fixtures and a complete workflow composition

## Status and intended use

The snippets are **proposed implementation fixtures**, not a claim that Jackin already accepts these files or that pklr evaluated them during this research. T03–T05 must implement the versioned production schema and mirror examples into ordinary nextest tests. The first pair is a small language-conformance fixture; it is deliberately not the full workflow schema. The composition below defines the complete behavioral example through the normative block contracts.

No published Jackin Pkl package URL is invented here. Development fixtures use local modules from a frozen source bundle. Production package/import versions and content digests must be pinned by the resolver.

## Fixture A — Typed constraints and reusable inheritance

Prospective fixture file: `fixtures/pkl/AgentTemplate.pkl`.

```pkl
abstract module AgentTemplate

class Limits {
  cpuMillicores: Int(this > 0)
  memoryBytes: Int(this > 0)
  timeoutMs: Int(this > 0)
}

class AgentTask {
  id: String(length > 0)
  roleRef: String(length > 0)
  instructions: String(length > 0)
  limits: Limits
}

class RustTask extends AgentTask {
  roleRef = "fixture-rust-role"
  limits = new {
    cpuMillicores = 2000
    memoryBytes = 4294967296
    timeoutMs = 1800000
  }
}

task: AgentTask
```

Prospective fixture file: `fixtures/pkl/repair-task.pkl`.

```pkl
amends "./AgentTemplate.pkl"

task = new RustTask {
  id = "implement"
  instructions = "Implement the requested change; independent checks decide acceptance."
  limits {
    timeoutMs = 1200000
  }
}
```

Expected evaluated data has task ID `implement`, inherited role `fixture-rust-role`, CPU 2000 millicores, memory 4294967296 bytes and overridden timeout 1200000 ms. A sibling fixture with `memoryBytes = -1` must fail a typed constraint. A missing instruction must fail. Compare supported results/errors with a pinned official Pkl evaluator during development; production remains pklr-only. Confirm the exact constraint syntax and behavior with CERT-PKL before promoting these candidate fixtures.

## Fixture B — Frozen inputs and complete execution recipe

Use the following logical fixture values through an in-memory test resolver. They are fictional, not an existing user repository, credential or model. An actual run replaces them with approved host-resolved values before planning.

| Input | Fixture / required meaning |
|---|---|
| Workflow ID | `verified-rust-change` |
| Repository | `repo-main`, exact initial SHA-1 OID `1111111111111111111111111111111111111111` in the fake Git fixture |
| Roles | Existing-role references `dev-role`, `verify-role`, `review-role`; each resolver result has immutable manifest/image digests |
| Implementer | Certified interactive profile for one actual existing Jackin runtime in real e2e; fake profile only in pure unit tests |
| Reviewer | Separate certified real CLI instance and fresh session; may use same vendor but no shared mutable state |
| Task context | `task.md`, source bytes and digest sealed read-only |
| Worker limits | Implementer 2000 millicores/4 GiB; verifier 2000/2 GiB; reviewer 2000/4 GiB; each has finite pids/disk/output/deadline limits |
| Run budget | At most 3 concurrent instances, 6000 millicores and 12 GiB; absolute deadline and separate infra/repair counters |
| Repair | Initial iteration plus at most 2 repairs; same healthy logical session |
| Publication | Optional authorized checkpoint backup; final PR requires latest-candidate acceptance plus operator approval |
| Retention | Unique unfinished/unpushed work pinned; terminal/prompt evidence protected by declared policy |

CPU/memory values are example planning budgets, not claims that any user machine or agent needs exactly these values. Host admission may reject them or resolve an explicitly permitted range before hashing the plan.

## Node and port composition

The production Pkl schema represents this table with typed classes and explicit input bindings. Common permissions, limits, retry and completion come from versioned templates and must appear fully resolved in the plan.

| Node | Type | Inputs / dependencies | Outputs / completion |
|---|---|---|---|
| `context` | Context | Frozen task/reference sources | Immutable `context-bundle/v1` |
| `prepare` | Git prepare | Exact repository input and grant | Private `repository-ref/v1` |
| `implement` | Agent | context.bundle, prepare.repository | Retained session/ref, repository/ref, bounded turn result; not accepted code |
| `checkpoint` | Git checkpoint | Implementer repository and certified writer barrier | Host-sealed checkpoint with ordinal/base/head/tree |
| `tests` | Test | Exact checkpoint plus trusted test harness | Validated test result and evidence |
| `review` | Review | Same exact checkpoint, task context and rubric | Independent review/finding set; own original TUI |
| `assess` | Evidence Gate | tests.result and review.findings for that checkpoint | Satisfied, repairable failure, hard failure or operator required |
| `repair` | Feedback | Original implementer session plus current correlated failures | Durable submission and bounded repair-turn result |
| `publish-approval` | Operator Gate | Selected accepted scope checkpoint and evidence digest | Scoped exact-revision publication decision |
| `publish` | Publish | Final certificate plus publication decision | Verified pushed ref/PR receipt |

`implement`, `checkpoint`, `tests`, `review`, `assess` and the `repair` template form the bounded scope. They do not form an ordinary dependency cycle. T05/T27 lower the scope according to C14. The final gate/publication consume the **scope output**, never an unassessed internal checkpoint. Tests and review can run concurrently if the run budget admits both.

## Pkl repair-scope authoring fragment

The following fragment targets the production `Workflow.pkl` schema to be created by T03; it is not a standalone module and does not imply that the import exists today. Field names specify the intended schema contract.

```pkl
repairScopes = new {
  new RepairScope {
    id = "implementation-cycle"
    initialAgent = "implement"
    checkpointTemplate = "checkpoint"
    testTemplates = new { "tests" }
    reviewTemplates = new { "review" }
    assessmentGate = "assess"
    feedbackTemplate = "repair"
    maxRepairs = 2
    iterationTimeoutMs = 1200000
    overallTimeoutMs = 3600000
    exhaustion = "request_operator"
  }
}
```

Compiler validation must resolve every template ID, ensure exactly one original session owner, prevent internal outputs bypassing the scope, and reject a normal `dependsOn` back-edge from `repair` to `implement`. Input/session reuse occurs through typed scope bindings rather than a graph cycle.

## Fixture C — Required negative cases

Mutate one variable at a time: negative limit; duplicate node ID; wrong output type; ordinary cycle; unpinned import; unauthorized writable mount; unsupported backend disk guarantee; missing live-TUI capability; unavailable exact model; stale checkpoint evidence; missing required review; changed harness; zero tests; exhausted repair; missing operator approval; unknown prompt delivery; and changed source hash during editor save. Every fixture names its expected diagnostic/state and asserts that no forbidden worker/prompt/publication effect was emitted.

## Promotion to runnable examples

T03 implements the complete typed module for all twelve execution kinds. T04 validates the candidate Pkl syntax/semantics and IO boundary. T05 compiles the complete recipe above against a fake resolver and verifies canonical output. T39 replaces fake repositories/profiles with exact real fixtures and runs the end-to-end flow. Only after those tests pass should the source snippets be presented in Jackin’s user documentation as runnable product commands/configuration.

## Evidence and references

- **E01 — [pklr source README](https://github.com/jdx/pklr/blob/ed746fcf308c5510e159064814fafb5dd151ac44/README.md).** Pinned source documentation: pure-Rust evaluator and eval-core/custom EvalCapabilities embedding. No conformance tests run in this audit.
- **E02 — [pklr Cargo manifest](https://github.com/jdx/pklr/blob/ed746fcf308c5510e159064814fafb5dd151ac44/Cargo.toml).** Source candidate version 5.1.1; default native IO/HTTP features versus eval-core-only embedding. Exact registry artifact must be verified before adoption.
- **E03 — [Apple Pkl CLI reference](https://pkl-lang.org/main/current/pkl-cli/index.html).** Official Pkl language/evaluation reference. Its sandbox options do not automatically exist in pklr. Moving documentation, accessed 2026-10-14.
- **J19 — [Testing and CI rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/TESTING.md).** Lines 1–200. Pinned mise/mbx/nextest commands, e2e and conditional OrbStack lane, documentation examples and crate gates. Examples here are not evidence of execution.
