# Plan 043: Track — finish the security-hardening cluster (compat→standard default flip)

> **Executor instructions**: This is a **tracking/sequencing** plan for a cluster of hardening finish-work.
> The keystone (sudo audit) gates the rest. Do the sequencing + the keystone audit; each downstream item
> becomes its own plan. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- docker/construct/Dockerfile crates/jackin-runtime/src/runtime/docker_profile.rs crates/jackin-runtime/src/runtime/launch.rs TODO.md`

## Status

- **Priority**: P2
- **Effort**: L (cluster; individual items S-M)
- **Risk**: MED
- **Depends on**: plan 003 (host.sock auth is one item in this cluster)
- **Category**: direction (DIRECTION-03)
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

The isolation/security boundary is the product's entire reason to exist, and its hardening is repeatedly
"wired but not defaulted/validated." The roadmap's "Partially implemented" wall clusters on security
finish-work: the `docker-runtime-hardening-contract` (flip `compat`→`standard` default; Tier-2 real-workload
matrix), backed by three `TODO.md` sub-tasks (the `NOPASSWD:ALL` sudo audit at `docker/construct/Dockerfile:113`,
`no-new-privileges` on `standard`, rootless DinD); the `jackin-exec` host.sock gap (plan 003);
`network-egress-policy` DinD-inner coverage; and signed-releases in-process verification never run
end-to-end. Closing this cluster advances jackin❯ from "functional proof of concept" toward a boundary
an operator can trust by default — higher-value than any new feature at this stage. **The `compat` default
itself is a maintainer-decided tradeoff (not re-litigated here); the direction is finishing the audit that
unblocks flipping it.**

## Current state (the cluster — verify each)

- **Keystone — sudo audit**: `docker/construct/Dockerfile` (~line 113) grants `agent ALL=(ALL) NOPASSWD:ALL`,
  incompatible with `--security-opt no-new-privileges`. `TODO.md` "audit `NOPASSWD:ALL` sudo in base image"
  has findings-so-far (2026-06-04: zero sudo calls in jackin❯-controlled runtime code; the entry exists for
  role-authored hooks / agent `apt install`). This gates the default flip.
- **Default flip**: `TODO.md` "flip default from `compat` to `standard`" — one line in
  `impl Default for DockerSecurityProfile` (`crates/jackin-runtime/src/runtime/docker_profile.rs`) + a
  `profile_base_grants(Standard).sudo` field flip + enabling `no-new-privileges` for `standard`. Blocked by
  the sudo audit.
- **Rootless DinD**: `TODO.md` "rootless DinD requires cgroup v2 confirmation" — `launch.rs` DinD start uses
  `docker:dind --privileged` regardless; `standard` wants `docker:dind-rootless` on cgroup v2.
- **host.sock auth**: plan 003.
- **network-egress DinD-inner + signed-releases e2e verification**: roadmap "Partially implemented" items.

## Steps

### Step 1: Sequence the cluster (the deliverable)

Write the dependency order explicitly (in this plan's row note and, if the roadmap doc lacks it, in the
`docker-runtime-hardening-contract` roadmap page): **sudo audit (keystone) → `no-new-privileges` on
`standard` → default flip → rootless DinD (cgroup v2)**; in parallel, host.sock auth (plan 003) and
network-egress DinD-inner coverage; signed-releases e2e verification independently.

### Step 2: Execute the keystone — complete the sudo audit

Finish the `NOPASSWD:ALL` audit per `TODO.md`'s resolution path: enumerate every privileged operation the
base image + built-in agent images invoke at runtime via `sudo`, and for each, decide: replace with a file
capability (`setcap`) on the specific binary, restructure to run before `USER agent`, or require the role to
declare `min_profile = "standard"`. Produce the audit result as a doc + a concrete list. Do **not** flip the
default in this plan — that's the next plan once the audit clears.

### Step 3: Write per-item follow-up plans

For each cluster item, write a scoped next-numbered plan: `044-sudo-audit-resolution.md` (execute the
setcap/restructure from Step 2), `045-nnp-standard.md` (enable `no-new-privileges` on `standard`),
`046-default-flip.md` (the one-line default flip + `profile_base_grants` + docs table update),
`047-rootless-dind.md` if those are the next available numbers; otherwise use the current next numbers in
order. Reference plan 003 for host.sock. Each carries its own verification (compat matrix under `standard`
with `no-new-privileges`).

## Done criteria

- [ ] The cluster's dependency order is documented (row note + roadmap page)
- [ ] The sudo audit is completed: a concrete list of every runtime `sudo` operation + a per-item resolution
      (setcap / restructure / min_profile)
- [ ] Per-item follow-up plans written as next-numbered `plans/NNN-*.md` files, each with its own verification
- [ ] Roadmap `docker-runtime-hardening-contract` Status/Related-Files updated (docs gate)
- [ ] `plans/README.md` row updated
- [ ] (Confirms TODO.md's `TODO(docker-security-profile-*)` markers — coordinate with plan 036 which adds them)

## STOP conditions

- The sudo audit finds a privileged runtime operation with **no** setcap/restructure path (genuinely needs
  full sudo) — report it; that operation defines whether `standard`-with-`no-new-privileges` is achievable
  at all, and the maintainer must decide (e.g. keep it under `compat`-only).
- Flipping any default is tempting mid-audit — do NOT; the default flip belongs in the next-numbered
  default-flip follow-up after the audit clears. This plan produces the audit + the plan chain, not the flip.

## Maintenance notes

- The `compat` default is a deliberate decision (documented); this track finishes the audit that lets it be
  reconsidered — a reviewer should keep the two separate (audit ≠ re-deciding the default).
- Each downstream flip must pass the compatibility test matrix for the built-in roles (`the-architect`,
  `agent-smith`) under `standard` + `no-new-privileges` before landing — that gate is non-negotiable.
