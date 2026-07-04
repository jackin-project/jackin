# Plan 009: Spike — promote Claude Notification hook + Codex app-server to graded semantic authority; lean into container-owned reporters

> **Executor instructions**: This is a **design/spike** plan, not build-everything. The deliverable is a
> validated design + one prototype authority behind a flag + follow-up plans. Do plan 008 first (you need the
> test seam). Update the README row when done.
>
> **Drift check**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-capsule/src/agent_status/gating.rs crates/jackin-capsule/src/agent_status/arbitrate.rs docker/runtime/agent-status`

## Status

- **Implementation status**: BLOCKED/PARTIAL in PR #714. Official docs validation is recorded and the pure
  gating prototype is landed: Claude `Notification:permission_prompt`/`idle_prompt`/`elicitation_*` can author
  partial authority, and a feature-gated `codex-app-server-authority` prototype maps Codex app-server
  `turn/started`/`turn/completed` events. Full completion is blocked on live in-container ordering validation
  and a real Codex app-server reader; ordinary Claude/Codex lifecycle hooks remain heartbeat-only.
- **Priority**: P2 (highest reliability ceiling; direction)
- **Effort**: L
- **Risk**: MED
- **Depends on**: 008 (test seam), and conceptually 004 (freshness model)
- **Category**: direction
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

jackin❯ owns the image and container, so it can install a
first-party semantic reporter that gives authoritative state — herdr can only *offer* optional integrations
the user installs. Today that advantage is used for only 2 of 6 agents (opencode plugin, amp event mapping);
claude/codex/kimi/grok ride the version-fragile screen path. Meanwhile the two flagship agents now expose
**reliable semantic surfaces** jackin❯ leaves unused: Claude Code's **Notification hook** emits typed events
(`permission_prompt`, `idle_prompt`, `elicitation_*`), and Codex ships an **app-server** with `turn/started`
(inProgress) and `turn/completed` (completed|interrupted|failed). These are exactly the blocked/idle/done edges
the tab needs. Decision 0a made claude/codex hooks identity-only because *completion-class* events
(`Stop`/`SubagentStop`) are order-unreliable — but the Notification subtypes and the app-server `turn` status
are **not** subject to that reordering. This spike scopes promoting them to graded authority without
resurrecting the Decision-0a hazard.

## Current state / grounding

- `crates/jackin-capsule/src/agent_status/gating.rs:125-143` — every claude/codex hook event → `heartbeat`
  (identity-only, "never author working/blocked/idle"). Only opencode/amp get real event→state maps
  (`gating.rs:144-152`).
- `crates/jackin-capsule/src/agent_status/arbitrate.rs:73-124` — authority-wins path (TTL + identity gated),
  with graded confidence; opencode graded `Complete`/`Authoritative` (`arbitrate.rs:181-184`).
- Design doc rationale for Decision 0a: `docs/content/docs/reference/research/agent-orchestration/terminal-observation/agent-runtime-status-design.mdx:215`
  (post-Stop revive hazard).
- External surfaces (verify current at implementation time):
  - Claude Code hooks — Notification event subtypes `permission_prompt` / `idle_prompt` / `elicitation_*`
    (`https://code.claude.com/docs/en/hooks`).
  - Codex app-server — `turn/started` (`status: inProgress`), `turn/completed` (`completed|interrupted|failed`),
    `command/exec` streaming (`https://developers.openai.com/codex/app-server`).
- PR #714 doc validation (2026-07-04): Claude's hooks guide/reference documents `Notification` as firing when
  Claude Code waits for input or permission, with `permission_prompt`, `idle_prompt`, and `elicitation_*`
  matchers. OpenAI's Codex app-server docs document `turn/start`, streamed turn/item notifications, and
  `turn/completed` as the final turn status event. This validates the event names for a prototype but does not
  replace a live in-container ordering run.
- jackin❯ already TTL- and identity-guards authority (`arbitrate.rs:73-87`), so a stale reporter can't pin.

## Steps (spike — produce a design + one prototype + follow-up plans; do NOT wire all agents)

### Step 1: Validate the event surfaces and their ordering guarantees

Confirm (from the live CLIs in-container or their docs) the exact payloads and ordering of: Claude
`Notification: permission_prompt/idle_prompt/elicitation_*`; Codex app-server `turn/started`/`turn/completed`.
Specifically confirm they are **not** subject to the Stop/SubagentStop reordering that Decision 0a guards
against. Record the finding — this is the gate for promotion.

### Step 2: Define the graded promotion (map to the existing canonical vocabulary)

Extend `gating.rs` so the **safe** subset authors state at an appropriate grade, while keeping order-fragile
events (`Stop`/`SubagentStop`) as `heartbeat`:
- Claude `Notification: permission_prompt` → blocked; `idle_prompt` → idle/turn-complete; `elicitation_*` →
  blocked — at `Partial`/graded confidence, TTL-guarded exactly like opencode.
- Codex app-server `turn/started` → working, `turn/completed` → idle/done — at `Complete` grade.
Keep the strong on-screen blocked match as an override safety net (`arbitrate.rs:103`), so a mis-timed report
can't hide a visible dialog.

### Step 3: Prototype ONE authority behind a flag (Codex app-server — cleanest lifecycle)

Prototype the Codex app-server reader as a container-local reporter (jackin❯ owns the launch, so it can start
codex via app-server) feeding the existing `ReportRuntimeEvent` → gating → authority path, behind a feature
flag. Prove it with plan 008's test seam (injected `turn/*` events → published state). Do **not** also wire
Claude/kimi/grok in this spike — those become follow-ups.

### Step 4: Write follow-up plans

`009a-codex-app-server-authority.md` (productionize the prototype), `009b-claude-notification-authority.md`
(promote the Notification subtypes), and note kimi/grok reporter feasibility (do their CLIs expose a
plugin/MCP surface? amp does not support plugins.json — its reporter needs the MCP/toolbox surface, tracked as
remaining work). Defer anything whose event surface Step 1 shows is unreliable.

## Done criteria

- [ ] Step 1 finding: which events are safe to promote (ordering-verified) recorded in the row note — docs
  validation is recorded; live ordering validation is still blocked
- [x] `gating.rs` maps the safe subset to state at a graded confidence; order-fragile events stay `heartbeat`
- [x] A flagged Codex app-server authority prototype compiles and is proven via the plan-008 seam at the pure
  event-mapping/session-authority layer
- [x] Screen-blocked override safety net preserved (test: a visible dialog overrides a stale reported idle)
- [x] Follow-up plans (009a/009b) written; unreliable surfaces explicitly deferred with rationale
- [x] `plans/agent-status/README.md` row updated

## STOP conditions

- Step 1 shows the Notification/app-server events are *also* order-unreliable in practice — then Decision 0a
  stands; keep them identity-only and report. Do not promote an event you can't order-guarantee.
- The app-server launch changes how codex is started in a way that affects other subsystems — report the blast
  radius before productionizing; the prototype stays flagged until that's scoped.

## Maintenance notes

- This is the real fix for claude/codex redundancy (plan 004 only removes the OSC landmine and revives the
  shell source). A first-party semantic reporter beats any screen heuristic — but only for events with
  guaranteed ordering; the grading + TTL + screen-override is what keeps it honest.
- Keep the herdr-style observer (screen packs) as the zero-config floor for every agent; reporters are the
  authority ceiling, not a replacement — a reviewer should ensure the floor still works with the reporter off.
