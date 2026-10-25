# Specification traceability

Each row names an exact heading from the unchanged specification.
Line ranges refer to the local [SPEC.md](./SPEC.md) file.
Task assignment means planned coverage. It does not mean that implementation or runtime qualification passed.
P01 must add requirement-level records for each individual behavior and source callsite.
Do not treat section coverage as an exhaustive callsite audit.

| Specification section | Lines | Responsible tasks |
| --- | --- | --- |
| Decision and product scope | 1–10 | P00, P03, P05, T39, T40 |
| Source baseline and evidence | 11–25 | P00, P01, P02 |
| Architecture and ownership | 26–67 | P03, P05, T11, T13, T18, U04 |
| Product and domain model | 68–94 | P03, T10, T14, T24 |
| Domains and account isolation | 95–104 | T21, T22, T24 |
| OpenShell workspace authority | 105–112 | T12, T21, T37 |
| Roles and reproducible image construction | 113–128 | T14, T15, T16, T31 |
| Portable role ABI | 129–147 | T16, T35, T36 |
| Capsule v2 and persistent sessions | 148–157 | T17, T18, T19 |
| Transport | 158–165 | T12, T19 |
| Protocol and authority | 166–177 | T17, T19, T26 |
| Persistence semantics | 178–191 | T18, T19, T27, T28 |
| Workspace materialization and retention | 192–223 | T20, T27, T28 |
| Handoff | 224–227 | T28 |
| Credentials, account state and policy | 228–241 | T21, T22, T23, T25, T26 |
| Policy ownership | 242–249 | U00, T13, T21 |
| Target profiles and Kubernetes deployment | 250–265 | P02, T35, T36, T37 |
| Kubernetes ownership and deployment | 266–279 | T37 |
| Developer services | 280–285 | T30 |
| Optional Docker capability | 286–325 | U04, U05, U06, U07, U08, T33 |
| Independent verification workers | 326–335 | U02, T32 |
| Application operations and failure handling | 336–355 | P04, T10, T11, T13, T19, T26, T38 |
| Command semantics | 356–372 | T23, T24, T28, T29 |
| Fleet view and developer experience | 373–384 | T29, T31, T34 |
| Proposed command examples | 385–401 | T29 |
| Rust architecture and configuration | 402–464 | P03, P05, T10, T12, T14 |
| Performance and resource contracts | 465–476 | P04, T31, T38 |
| Dependency register and qualification gates | 477–492 | P02, P04, U00, U01, U02, U03, U04, U05, U06, U07, U08, T40 |
| Implementation sequence | 493–510 | P00, P04, P05, T23, T39, T40 |
| Primary source references | 511–549 | P00, P01, P02 |

## Source classes

The specification selects the product contract.
[CONTRACTS.md](./CONTRACTS.md) adds explicit implementation decisions where the specification leaves choices open.
[SOURCE-AUDIT.md](./SOURCE-AUDIT.md) labels direct source observations and historical regression leads.
P02 must resolve actual upstream symbols before implementation.
Acceptance cases define required future evidence. They are not existing test results.

## Changes during execution

Record new requirements before changing a contract.
Keep the original specification unchanged.
Update this map, the task graph, and the affected test cases together.
Request the planner review when a change affects authority, storage ownership, or an upstream contract.
Do not remove a mandatory requirement to close a blocker.
