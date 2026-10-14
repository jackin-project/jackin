# C03 — Host authorization and capability ceilings

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Produce an unforgeable host-authorized execution contract and recheck privileged actions. Workflow definitions request capabilities; host policy grants them. Keep policy resolution in an existing or narrowly extracted host service, with serializable grant types below the orchestration layer.

## 2. Existing foundation and gap

Jackin already validates Docker profiles and grants. Its inspected `apply_grants` generally chooses the more capable network/DinD/resource setting and unions permissions; that cannot serve as a final workflow ceiling. Preserve those semantics for manual launches, then impose a distinct workflow grant after every legacy source has resolved. [J12](#evidence-and-references), [J13](#evidence-and-references).

## 3. API and data contracts

```text
authorize_plan(principal, frozen_request, policy_snapshot) -> AuthorizationCeiling
issue_execution_grant(run, attempt, ceiling, current_policy) -> AuthorizedExecutionGrant
authorize_action(principal, grant, action, target, expected_state) -> ActionPermit
revoke(grant_or_principal, reason) -> RevocationRecord
```

The execution grant includes issuer, run/attempt identity, backend requirements, mounts with exact source/destination/mode, network routes/protocols, resource maxima, credential references/scopes, allowed tool actions, Git actions, image/binary digests, policy digest, issue/expiry times and a nonce or signature authenticated to the runtime. It is not a user-editable role file.

## 4. Required functionality

Resolve requested rights against host/operator/workspace policy and the frozen plan ceiling. Required rights outside the allowed set cause rejection. Optional rights may be omitted only with an explicit diagnostic recorded in the plan; there is no silent fallback to a more permissive profile. Resource ranges include required minima and requested maxima; choose an admissible maximum and record it, or reject when minima cannot be met.

Mount authorization works on canonical, ownership-checked paths and resolved file descriptors where supported, not string prefixes alone. Prevent symlink swaps, path traversal, overlapping mounts that shadow protected directories, runtime/control socket exposure and writable shared `.git` aliases. Read-only task files remain read-only even if a broader writable parent mount was requested.

Network grants name destination rules, ports and protocols plus the required enforcement mechanism. Agent/provider endpoints, package download endpoints and GitHub publishing endpoints are separate scopes. DNS, IPv4/IPv6, redirects, direct IP routes, proxy CONNECT and cloud metadata access must be part of the backend test contract. An allowlisted destination remains a possible data-exfiltration channel; scope credentials and permitted content accordingly rather than claiming total exfiltration prevention.

Use existing account/credential resolution. Workers receive only the credentials explicitly needed for their task, preferably short-lived and narrowly scoped. GitHub publication credentials normally never enter implementer or reviewer instances. Pkl and prompts cannot read host secret values. Authorization actions use resolved argument vectors, never a free-form “run this host shell” permission.

## 5. Failure and revocation

Expiry/revocation stops new privileged actions and automated input where appropriate. Persist the denial, fence stale grants and ask the runtime to stop or reduce activity according to policy. A run may not broaden itself when current policy changes. A new broader plan requires a new authorization decision; the old plan remains immutable.

Hard security requirements are not operator acceptance waivers. A human can approve a PR gate but cannot label unenforced isolation as enforced. Generic CLI applications and agent-edited configuration are never trusted grant issuers.

## 6. Build instructions

Add separate request/ceiling/grant/permit types. Write policy algebra/property tests before wiring flags. Integrate a final grant validation point immediately before backend creation and before each sensitive effect. Thread the immutable grant through runtime launch without global config mutation. Add revocation and expiration checks to the same command/lease mediation path used by Capsule.

## 7. Dependencies and integration

C02 compiles the requested contract; C04 enforces it; C06–C07 enforce input authority; C16 enforces publication. Reuse current trust, account, env and Docker profile helpers but do not expose their mutable configuration directly to a worker.

## 8. Acceptance tests

`AUTH-01`: inherited role/global config cannot widen a workflow grant. `AUTH-02`: requested mount aliases and symlink races fail. `AUTH-03`: editing Pkl/role/prompt inside a worker never changes rights. `AUTH-04`: expired/revoked grants stop privileged dispatch. `AUTH-05`: approval cannot waive a hard denial. `AUTH-06`: publisher credentials remain absent from implementation/test/review environments.

## Evidence and references

- **J06 — [Programmatic launch options](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-launch-load-options/src/load_options.rs).** Lines 1–300. Non-interactive launch, programmatic constructor, identity sink, inputs and validation error vocabulary. Deeper launch call paths were not executed.
- **J07 — [Shared Capsule contracts](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/lib.rs).** Lines 1–280. Session capability and identity environment, scoped credential contracts, CapsuleConfig and exit preservation choices.
- **J12 — [Docker security profile facade](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-docker-profile/src/docker_profile.rs).** Full file. Existing network, grant, resource, capability, cgroup and read-only-root helpers.
- **J13 — [Docker effective grant merging](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-docker-profile/src/docker_profile/effective.rs).** Full file. Generally capability-widening and maximum-ceiling merging; role DinD None is a special narrowing case. Critical workflow policy integration gap.
- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.

## Additional enforcement details

Denying the `git` executable alone cannot prevent local Git metadata mutation when an agent can write `.git`. Enforce denied local Git rights through metadata mount/ownership controls; separately protect host/remote refs and credentials. Similarly, account “sync” must resolve a sanitized scoped credential snapshot, not copy arbitrary host home files that could include unrelated secrets or publication tokens. Inspect the effective mounted credential/config set before issuing the final grant.
