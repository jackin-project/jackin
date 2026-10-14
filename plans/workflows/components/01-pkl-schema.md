# C01 — Typed workflow language and Pkl schema

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Define the typed, reusable authoring language for all workflow blocks. Own the versioned Pkl schema package and its mapping to the canonical domain model in `jackin-workflow-spec`. Do not replace existing Jackin role manifests or workspace configuration.

## 2. Existing foundation and gap

The inspected workspace does not contain workflow crates; the existing orchestration research names typed steps but is not a Pkl implementation. Reuse role/workspace resolvers by reference. New workflow schema definitions must not become a second copy of the entire Jackin configuration schema. [J02](#evidence-and-references), [J04](#evidence-and-references).

## 3. API and data contracts

The authoring root is `Workflow { schemaVersion, id, parameters, roles, repositories, sandboxProfiles, contexts, nodes, repairScopes, budget, acceptance, retention }`.

Every node has `id`, `kind`, `inputs`, `outputs`, `dependsOn`, `permissions`, `limits`, `retry`, `completion` and optional `when`. Typed reusable templates may supply these, but the evaluated plan must contain explicit effective values. Kind-specific classes cover Agent, Context, Command, Test, Review, Condition, Parallel, Join, Feedback, Gate, Git and Publish. Review is a distinct behavior even when it uses an Agent adapter.

Expose `SchemaCatalog::version()`, `SchemaCatalog::block_types()` and `SchemaCatalog::output_contract(type_id)`. Schema errors include a Pkl source URI, span, node/field path and stable diagnostic code. These are new target APIs.

## 4. Required functionality

Support class/template reuse, amendment/inheritance, parameters, shared profiles and versioned imports for the certified language subset. Resolve local schema imports from an explicit bundle during development; publish immutable versioned schema packages only after conformance. A mutable branch URL is not a version lock.

Connections use typed input bindings rather than implicit string interpolation into shell commands. A dependency orders execution; it does not automatically copy a filesystem or share a session. Context attachments resolve to immutable artifact references. Secrets are opaque host references, never Pkl-evaluated literal values.

Conditions use a small typed expression tree over declared structured outputs: boolean operators, typed comparisons and explicit existence tests. They cannot execute shell, invoke a model, fetch a URL or access ambient state. Gates enumerate required checks and acceptance thresholds; unknown severities and result states fail validation.

Define `RepairScope` with implementer node, checkpoint/verification/review template, feedback target, `maxRepairs`, per-iteration deadline and exhaustion outcome. Reject ordinary dependency cycles. A feedback target is a controller relationship, not an unbounded graph edge.

## 5. Security and failure rules

Reject duplicate IDs, ambiguous ports, missing output contracts, unconstrained command capture, unknown block kinds, unauthorized host effects and implicit shared-write arrangements. Constraints must validate all exported workflow data; lazy Pkl evaluation must not leave unvisited invalid values in the accepted plan. Root-level unknown fields fail in strict v1 so spelling errors do not silently weaken a gate.

## 6. Build instructions

Create the schema types and Rust serde models together. Start with workflow/node/port/limit definitions, then role/repository references, then block variants and repair scopes. Add fixtures for reusable templates, inherited limits and parameter overrides. Add one valid and multiple invalid fixtures for every block kind. Mirror canonical examples into nextest-discoverable tests, rather than relying on undocumented runtime schema behavior.

Keep a documented schema migration path: old source is validated against its declared version and upgraded only through explicit tooling. Do not silently reinterpret old fields under new semantics. The compiler owns semantic validation beyond the Pkl type system.

## 7. Dependencies and integration

Depends on [C02 compiler](02-plan-compiler.md), the [domain contracts](../architecture/02-domain-contracts.md) and the [block catalog](../blocks/01-agent.md). Runtime compatibility and authorization are resolved by host adapters, not encoded as duplicated lists inside Pkl.

## 8. Acceptance tests

`PKL-01`: typed inherited workflow evaluates to the expected domain fixture. `PKL-02`: missing required field and invalid nested constraint fail with source spans. `PKL-03`: duplicate IDs, cycles and wrong port types fail. `PKL-04`: a role change cannot widen a workflow permission. `PKL-05`: all twelve block kinds and bounded repair scopes have valid/invalid fixtures. `PKL-06`: the published schema examples are exercised by ordinary project tests.

## Evidence and references

- **J02 — [Cargo workspace](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/Cargo.toml).** Workspace members and package metadata inspected. Version 0.6.4; layered apps/core/services/adapters/ui directories; no workflow crates in the inspected member list.
- **J04 — [Existing orchestration design constraints](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/docs/content/research/agents/orchestration/workflow-systems/agent-workflow-orchestration/02-design-constraints.mdx).** Full document. Proposed deterministic ledger, evidence, operator intervention and Capsule ownership.
- **E01 — [pklr source README](https://github.com/jdx/pklr/blob/ed746fcf308c5510e159064814fafb5dd151ac44/README.md).** Pinned source documentation: pure-Rust evaluator and eval-core/custom EvalCapabilities embedding. No conformance tests run in this audit.
- **E03 — [Apple Pkl CLI reference](https://pkl-lang.org/main/current/pkl-cli/index.html).** Official Pkl language/evaluation reference. Its sandbox options do not automatically exist in pklr. Moving documentation, accessed 2026-10-14.
