# Plan 042: Spike — scope the reactive-daemon program

> **Executor instructions**: This is a **design/spike** plan, not a build-everything plan. The deliverable
> is a scoped design + a prototype of ONE narrow adapter, plus follow-up plans — NOT a shipped daemon.
> Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- docs/content/docs/roadmap crates/jackin-runtime/src/runtime/backend_foundation`

## Status

- **Priority**: P3
- **Effort**: L
- **Risk**: MED
- **Depends on**: none
- **Category**: direction (DIRECTION-02)
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

The reactive daemon is the project's stated next chapter and is still **all design, no code**. The roadmap
has a whole `(reactive-daemon-program)` group — an umbrella `/roadmap/jackin-daemon/` plus Phase-2/3
adapters (`live-auth-sync`, `agent-attention-prompts`, `host-bridge`), all "open — design proposal" — and
the user-facing Vision routes toward "Kubernetes-backed debug workflows". Several **already-shipped**
features have a visible ceiling they can't cross without it: bidirectional auth sync
(`auth-overwrite-on-new-tab` — "live cross-session sync remains deferred to the daemon program"), attention
notifications, mid-session host secret/command requests (`console-agent-session-control` Phase 4). It
targets the maintainer's stated top operator drag — idle wall-clock waiting on agents that don't surface
their state. Highest-leverage greenfield because it unblocks a cluster of half-features, not one.

## Current state (grounding — verify each before designing)

- `docs/content/docs/roadmap/(reactive-daemon-program)/` — the roadmap group; `index.mdx:110-115` names the
  umbrella + adapters, all "open — design proposal"; `index.mdx:142` routes Vision to Kubernetes debug workflows.
- Deferrals to it: `auth-overwrite-on-new-tab` (live sync deferred), `console-agent-session-control` Phase 4
  (live session reconciliation).
- Commit `8e50267ce` "feat(runtime): add backend foundation scaffolding (#527)" — check whether this
  scaffolding is related infrastructure or unrelated (the audit flagged it as possibly premature
  abstraction; confirm).
- **No long-running host-process code exists yet** — the abstraction is named but unbuilt (verify:
  `grep -rn "daemon\|control.sock\|reactive" crates/jackin-runtime/src crates/jackin-host/src` and inspect
  what's real vs roadmap-only).

## Steps

### Step 1: Resolve the umbrella design decisions (design doc, not code)

Read `/roadmap/jackin-daemon/` and produce/refine a design doc answering the load-bearing questions:
lifecycle (how the daemon starts/stops/updates), install method, control-socket protocol + **security
posture** (this is a new host-side attack surface — apply the same rigor as plan 003's host.sock),
log/secret redaction, and how it relates to the existing capsule daemon (in-container) vs this new host-side
daemon. Write it as an ADR/roadmap update, not inline code.

### Step 2: Prototype ONE narrow adapter — agent-attention-prompts

Prototype only the adapter with the smallest surface: **agent-attention-prompts** — it merely *consumes*
the existing agent-runtime status authority (it doesn't need bidirectional auth or a host bridge). Build a
minimal host-side listener + one notification path, behind a feature flag, proving the control-socket shape
from Step 1. Do **not** build auth-sync or host-bridge (they're higher-risk and depend on the daemon shape
being proven first).

### Step 3: Write follow-up plans

Based on what the prototype teaches, write `plans/042a-daemon-lifecycle.md`, `042b-attention-adapter.md`,
etc. — each a scoped build plan. Defer `live-auth-sync` and `host-bridge` explicitly until the daemon shape
is proven, and say so.

## Done criteria

- [ ] A design doc / ADR answering lifecycle + install + control-socket + security-posture + redaction, and
      the in-container-vs-host daemon relationship
- [ ] A feature-flagged prototype of the attention-prompts adapter that compiles and demonstrates the socket
      shape (not production-wired)
- [ ] Follow-up build plans written; auth-sync/host-bridge explicitly deferred with rationale
- [ ] Roadmap items updated (Status/Related Files) per the docs gate
- [ ] `plans/README.md` row updated

## STOP conditions

- The security posture of a host-side control socket can't be settled cheaply (it's a real new attack
  surface) — that's the crux; report it as the gating decision rather than shipping a prototype with an open
  security question.
- The prototype reveals the attention adapter actually needs bidirectional state (not just consuming status)
  — re-scope; pick a genuinely one-directional first adapter.

## Maintenance notes

- This is deliberately a spike: the value is de-risking the shape, not shipping the daemon. A reviewer should
  reject scope creep into auth-sync/host-bridge in this plan.
- Ties to plan 003 (host.sock auth) — the daemon migration is meant to subsume that listener; the security
  design here should account for absorbing it.
