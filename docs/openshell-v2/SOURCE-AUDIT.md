# Source audit and planning basis

## Scope

The complete supplied specification was read, including its source references and dependency register.
The copied [SPEC.md](./SPEC.md) is unchanged.

```text
File: Jackin v2 OpenShell Architecture Specification.md
Physical lines: 549
SHA-256: 857e2bf002676af9fbc2b8eac0b0aea4d7657d873b90b3a64e8abfe767523a3e
```

The source review used the connected GitHub read tools.
The current Jackin main query returned the specification's pinned commit.
This audit examined the files and ranges listed below.
It is not a full repository audit or a runtime qualification report.

No Jackin build, OpenShell deployment, authenticated agent task, or historical experiment was executed.
No subagent runner was available in this planning session.
The execution pack requires actual independent workers and reviews when the execution runner supplies them.

## Verified current Jackin findings

| Finding | Source inspected | Rewrite consequence |
| --- | --- | --- |
| The queried main commit matches `faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e`. | GitHub commit API for `main`. | P00 must still detect later drift before execution. |
| The workspace package version is `0.6.4`. Its member list contains many runtime-prefix crates and Apple-specific crates. | Root [Cargo.toml](../../Cargo.toml), member list and package settings. | This is not one backend replacement. P01 must find all consumers. |
| Runtime orchestration includes Docker calls, Apple clients, mounts, DinD, attach, and cleanup. | [crates/services/jackin-runtime/README.md](../../crates/services/jackin-runtime/README.md), complete file. | Replace the orchestration model, not only a Docker API type. |
| Capsule is a PID1 multiplexer with runtime setup, auth handoff, firewall, and sudo helpers. | [crates/apps/jackin-capsule/README.md](../../crates/apps/jackin-capsule/README.md), complete file. | Rewrite the service around unprivileged sessions. Review any reused terminal code. |
| Capsule control checks use peer UID, session identity, and a root-specific ExecCommand path. | [crates/apps/jackin-capsule/src/daemon/control/auth.rs](../../crates/apps/jackin-capsule/src/daemon/control/auth.rs), complete file. | Do not carry the old privilege assumptions into same-UID OpenShell workloads. |
| The image subsystem handles recipes, binary artifacts, cache decisions, and versions. | [crates/adapters/jackin-image/README.md](../../crates/adapters/jackin-image/README.md), complete file. | Keep build behavior, but separate artifacts, delivery, and runtime probes. |
| A real version path runs `docker run --rm --entrypoint ... --version`. | [crates/services/jackin-runtime-image/src/image/version.rs](../../crates/services/jackin-runtime-image/src/image/version.rs), complete file. | T16 and T39 must replace and remove this hidden execution path. |
| Workflow verification creates a disposable Docker worker and invokes Docker cleanup. | [crates/services/jackin-workflow/src/verify.rs](../../crates/services/jackin-workflow/src/verify.rs), lines 1–200 and 340–570. | The verifier needs its own OpenShell replacement. A main-agent adapter swap would miss it. |
| Verification has bounded host-process helper code for Docker operations. | [crates/services/jackin-workflow/src/verify/process.rs](../../crates/services/jackin-workflow/src/verify/process.rs), complete file. | Review pure bounded-output helpers separately from prohibited execution. |
| Workflow state is host-owned and currently uses a dedicated local run database. | [crates/services/jackin-workflow/README.md](../../crates/services/jackin-workflow/README.md), complete file. | Preserve host verdict authority. Do not preserve the old durable schema. |

These findings do not prove every described path is reachable in every configuration.
P01 must record reachability and callers before the final removal audit.

## Verified OpenShell source distinction

The development SDK configuration defines `ClientIdentity` and `ClientConfig.client_identity`.
The release comparator does not define that mTLS field and describes the earlier transport limitation.
These are direct source observations, not a live mTLS test.

The first 230 lines of the pinned public protobuf were inspected.
They include workspace-authorized sandbox, provider, template, lifecycle, SSH, and service operations.
This limited read does not prove that every request has the required identity fence.
P02 and U01 must inspect every effectful request, handler, replay path, and asynchronous effect boundary.

The release listing observed during research identifies `v0.1.5` as the latest published release.
The execution plan does not assume that this remains true when implementation starts.
It uses immutable pins rather than a latest-version claim.

## Specification-derived blockers

The following findings come from [SPEC.md](./SPEC.md), not from new runtime experiments in this session.

| Contract | Specification basis | Required task |
| --- | --- | --- |
| Authoritative activation observations | Lines 150–176 and 244–248. | U00 and T23. |
| Identity and generation safety for mutations | Lines 348–354 and 484. | U01 and T13. |
| Strict read-only worker root and metadata | Lines 326–334 and 483. | U02 and T32. |
| Ordinary rebuilt network programs | Lines 485–486. | U03. |
| Managed engine companion | Lines 286–324 and 482. | U04 through U08 and T33. |
| Complete account/session behavior | Lines 95–111 and 487. | T21 through T28 and A117. |
| VM and Kubernetes qualification | Lines 250–278 and 488–489. | T35 through T38. |

A capability flag does not close one of these blockers.
A proposed upstream extension does not exist merely because Jackin has an adapter for it.
The final gate requires actual runtime evidence.

## Historical experiments

The historical README, readiness report, and refactor plan were read at the supplied experiment commit.
The readiness report uses an older Jackin source commit, `6e266406b49da2ab3c55355f3b7d33d1643ee6cd`.
Do not transfer every historical path or result to the current baseline without verification.

The report describes these regression leads:

| Historical result | Required use |
| --- | --- |
| The old Capsule root wrapper does not fit the released workload identity. | Test the new unprivileged Capsule ABI. |
| Rebuilt native executables at the same path lose protected network access. | Re-run ordinary native rebuild cycles in U03. |
| External dedicated engines survive parent sandbox deletion. | Test OpenShell-owned companion lifecycle and cleanup. |
| Content denial still permits a metadata change on an owned inode. | Test real root and input metadata immutability. |
| A delete-only patch does not fix other same-name mutation races. | Fence every relevant effect, not only delete. |
| A selected agent can reuse an image without that agent installed. | Validate the full declared agent set and cache identity. |
| Real accounts and several targets lack complete evidence. | Keep those rows blocked until real combined journeys pass. |

These results are historical, not newly reproduced findings.
The newer specification selects the architecture but preserves its qualification gates.
This plan permits work on a separate v2 branch. It does not grant full replacement or release approval early.

## Removal seed map

Use this table as a seed, not an exhaustive deletion script.
P01 must identify every actual path and caller at the execution commit.

| Current path or family | Required disposition |
| --- | --- |
| `crates/services/jackin-runtime*` | Replace required behavior in the new services and adapter. Delete the old implementations. |
| [crates/adapters/jackin-docker](../../crates/adapters/jackin-docker) | Remove agent/runtime use. Move only reviewed build-only behavior to `jackin-buildx`. |
| [crates/adapters/jackin-image](../../crates/adapters/jackin-image) | Replace with role resolution, build, delivery, and validation modules. |
| [crates/services/jackin-isolation](../../crates/services/jackin-isolation) | Replace target-specific execution assumptions. Review pure project logic separately. |
| [crates/services/jackin-workflow/src/verify.rs](../../crates/services/jackin-workflow/src/verify.rs) | Replace direct Docker worker execution. Review frozen-policy and verdict logic separately. |
| Capsule root, PID1, firewall, sudo, process-isolation, and control-auth paths | Delete old privilege and authority implementation. Rewrite unprivileged session behavior. |
| Old instance manifests, runtime enums, config parsers, and migration code | Delete. Use fresh v2 state and configuration. |
| Old attach, snapshot, usage relay, cleanup, image-prune, and version-probe consumers | Replace by responsibility. Do not leave hidden Docker calls. |
| Docker build recipes and Buildx infrastructure | Keep only the new role ABI and explicitly trusted image operations. |
| Generic terminal parsers, hashes, command parsing, and product rules | Port only after explicit contract review and tests. |
| CI generators, package manifests, scripts, man pages, and completions | Regenerate for the new graph. Remove stale backend assets. |

Do not delete original project repositories or pre-existing runtime resources through this seed map.
Do not keep an old crate merely because some functions in it are reusable.

## Primary source index

All Jackin paths below use commit `faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e`.
All development OpenShell paths use commit `c4688aa8715ea75d6c39fe468361dbbc16c691fa`.

```text
J0 https://api.github.com/repos/jackin-project/jackin/commits/main
J1 https://github.com/jackin-project/jackin/blob/faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e/Cargo.toml
J2 https://github.com/jackin-project/jackin/blob/faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e/crates/services/jackin-runtime/README.md
J3 https://github.com/jackin-project/jackin/blob/faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e/crates/apps/jackin-capsule/README.md
J4 https://github.com/jackin-project/jackin/blob/faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e/crates/apps/jackin-capsule/src/daemon/control/auth.rs
J5 https://github.com/jackin-project/jackin/blob/faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e/crates/adapters/jackin-image/README.md
J6 https://github.com/jackin-project/jackin/blob/faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e/crates/services/jackin-runtime-image/src/image/version.rs
J7 https://github.com/jackin-project/jackin/blob/faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e/crates/services/jackin-workflow/src/verify.rs
J8 https://github.com/jackin-project/jackin/blob/faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e/crates/services/jackin-workflow/src/verify/process.rs
J9 https://github.com/jackin-project/jackin/blob/faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e/crates/services/jackin-workflow/README.md
O1 https://github.com/NVIDIA/OpenShell/blob/c4688aa8715ea75d6c39fe468361dbbc16c691fa/crates/openshell-sdk/src/config.rs
O2 https://github.com/NVIDIA/OpenShell/blob/bdb956f2cd65a635f1294fccf1d138d263bf650f/crates/openshell-sdk/src/config.rs
O3 https://github.com/NVIDIA/OpenShell/blob/c4688aa8715ea75d6c39fe468361dbbc16c691fa/proto/openshell.proto
O4 https://github.com/NVIDIA/OpenShell/releases
H1 https://github.com/donbeave/openshell-jackin-poc/blob/a7689b48e2b6ef2ca5a4367ee27527887cf45ce8/README.md
H2 https://github.com/donbeave/openshell-jackin-poc/blob/a7689b48e2b6ef2ca5a4367ee27527887cf45ce8/READINESS_REPORT.md
H3 https://github.com/donbeave/openshell-jackin-poc/blob/a7689b48e2b6ef2ca5a4367ee27527887cf45ce8/REFACTOR_PLAN.md
L1 https://www.asd-ste100.org/about_STE.html
L2 https://www.asd-ste100.org/STE_faq.html
L3 https://www.asd-ste100.org/assets/files/ASD-STE100_ISSUE9.pdf
```

The specification contains additional immutable primary references.
Their presence in the attachment does not mean this audit independently read every linked file.
P02 must read the required sources before implementing their contracts.

## Language-review boundary

The official STE overview identifies technical nouns and technical verbs as controlled terminology categories.
The official standard PDF could not be fetched in this session.
The pack uses short imperative task sentences and a defined technical glossary.
The included validator checks structure and task-step length, not full dictionary conformity.
An independent ASD-STE100 review remains necessary before any claim of complete conformance.
