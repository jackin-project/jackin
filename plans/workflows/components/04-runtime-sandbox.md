# C04 — Runtime provisioning and sandbox enforcement

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Provision, inspect, suspend/stop and preserve isolated workflow workers using existing Jackin execution services. Implement the workflow’s `ExecutionPlatform` port over the runtime service family; do not add a second Docker/VM manager.

## 2. Existing foundation and gap

Existing runtime exports launch, session spawn, inspect, attach and cleanup. `LoadOptions::programmatic` avoids interactive dialogs and records instance identity. Docker security helpers already emit resource/capability/read-only configuration. Extend these foundations with exact grants, workflow ownership and certified guarantees. [J05](#evidence-and-references), [J06](#evidence-and-references), [J12](#evidence-and-references).

## 3. API and data contracts

```text
capabilities(host, backend) -> BackendCapabilityReport
provision(effect_id, attempt, resolved_launch, grant) -> InstanceReceipt
inspect(instance_id, expected_owner) -> InstanceObservation
reconcile(effect_id, owner_labels) -> Absent | One(InstanceReceipt) | Conflict
exec_sandboxed(instance, ExecSpec, grant) -> CommandHandle
quiesce_writers(instance, deadline) -> WriteBarrier
release_barrier(barrier) -> Receipt
stop(instance, stop_policy) -> StopReceipt
preserve(instance, hold) -> PreservationReceipt
cleanup(instance, removal_permit) -> CleanupReceipt
```

`ExecSpec` contains an executable/argv vector, approved cwd, explicit environment references, stdin artifact, timeout and output limits. It never implies host-shell execution. `BackendCapabilityReport` distinguishes supported, unsupported and unverified guarantees, with exact certificate IDs and reasons.

## 4. Required functionality

Assign workflow/run/node/iteration/attempt/effect ownership labels before creation. Reuse a matching worker only during reconciliation of the same creation intent; do not silently reuse an unrelated manual instance. A role can provision multiple independent instances. Multiple Capsule sessions in one instance are not independent filesystem/namespace sandboxes even when per-session identities differ.

Resolve exact repository state and mounts, image digest, runtime binary digest, model/account and trusted role setup before launch. Use the existing non-interactive path without `force`-based trust bypass. Return a durable identity as soon as creation is known; readiness and session startup are separate observations.

Enforce memory, CPU, process, open-file and disk limits over the whole permitted worker workload, including descendants and applicable sidecars. Disk limits cover writable repository volumes, home/state, temporary storage and retained output. A filesystem quota or bounded dedicated volume must exist for a hard disk claim; an advisory disk monitor alone cannot satisfy it. Keep enough reserved control-plane storage to report failures.

Enforce default-deny network/protocol policy using the certified mechanism. Strict workflow mode disables privileged and rootless DinD until equivalent inner-container enforcement is demonstrated; absence of the host Docker socket is necessary but not sufficient. Host namespaces, privileged containers and broad device mounts are prohibited in strict mode.

A worker-local trusted watchdog receives the grant deadline so a disconnected host cannot leave an unlimited task running. Reconciliation may interrupt or terminate a worker whose lease/grant is no longer valid. Timeouts cover child process groups, not just the top-level agent PID.

For checkpoint barriers, quiesce all writable worker processes or obtain an atomic provider snapshot. Do not freeze the Capsule transport needed to acknowledge/release the barrier. Semantic “agent idle” alone is insufficient when background child processes can still write.

## 5. Backend admission matrix

Docker is the first certification target. Apple Container is permitted only for capabilities proven on its exact host/backend version. Future KVM/microVM providers use the same port. Each requested guarantee is checked individually; never translate “VM” into blanket “secure.” A plan asking for an unsupported disk quota or protocol restriction fails before work starts.

## 6. Build instructions

Wrap existing runtime launch options in an immutable workflow launch adapter. Add ownership labels and reconciliation lookup; then pass the final grant into existing profile/mount emitters. Add missing enforcement implementations behind capability flags, initially disabled until conformance passes. Add sandbox command execution without changing Capsule’s PTY ownership. Integrate preservation holds with every cleanup path.

## 7. Dependencies and integration

Requires C03 grants, C08 repository descriptors, C10 effect intents and C17 retention. Uses existing runtime leaf crates and image/account/isolation services. Blocking backend calls run through the project’s established process abstraction and governed telemetry, not arbitrary `std::process` copies.

## 8. Acceptance tests

`SBX-01`: per-attempt ownership/recovery yields one writable instance. `SBX-02`: cross-worker and host writes fail. `SBX-03`: CPU/memory/pids/disk limits apply to descendants. `SBX-04`: denied IPv4/IPv6/DNS/proxy destinations cannot bypass policy. `SBX-05`: strict mode rejects all uncertified DinD. `SBX-06`: Apple/VM unsupported capability fails admission. `SBX-07`: writer barrier captures consistent files while Capsule control remains available. `SBX-08`: host loss does not remove worker-local deadlines.

## Evidence and references

- **J05 — [Runtime public integration surface](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime/src/lib.rs).** Full file. Existing launch, inspect, spawn, attach and cleanup re-exports; reactive daemon feature boundary.
- **J06 — [Programmatic launch options](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-launch-load-options/src/load_options.rs).** Lines 1–300. Non-interactive launch, programmatic constructor, identity sink, inputs and validation error vocabulary. Deeper launch call paths were not executed.
- **J07 — [Shared Capsule contracts](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/lib.rs).** Lines 1–280. Session capability and identity environment, scoped credential contracts, CapsuleConfig and exit preservation choices.
- **J12 — [Docker security profile facade](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-docker-profile/src/docker_profile.rs).** Full file. Existing network, grant, resource, capability, cgroup and read-only-root helpers.
- **J13 — [Docker effective grant merging](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-docker-profile/src/docker_profile/effective.rs).** Full file. Generally capability-widening and maximum-ceiling merging; role DinD None is a special narrowing case. Critical workflow policy integration gap.
- **J14 — [Git isolation public surface](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-isolation/src/lib.rs).** Full file. Existing materialization, finalization, cleanup, state and safe-removal boundaries.
- **E08 — [Docker security](https://docs.docker.com/engine/security/).** Official namespace, daemon and capability security guidance. Not a Jackin backend certification. Accessed 2026-10-14.
- **E09 — [Docker resource constraints](https://docs.docker.com/engine/containers/resource_constraints/).** Official memory/CPU limits. Disk-volume and network guarantees require separate backend evidence. Accessed 2026-10-14.
- **E10 — [Apple Container how-to](https://github.com/apple/container/blob/main/docs/how-to.md).** Official backend usage surface. Moving main; individual guarantees must be probed on the exact OS/backend combination. Accessed 2026-10-14.

## Immutable binary and finalization details

Mount agent executables from verified sealed content-addressed copies, not a mutable host file that was merely hashed once. A host updater must not silently change an active run's executable. Support a finalization barrier that closes the worker write phase without replacing Capsule or pretending a dead process is still live. Observers can inspect the original session; reopening interactive work first invalidates the candidate's finalization/acceptance eligibility.
