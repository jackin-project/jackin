# Plan 004: Restore evidence redundancy for claude/codex — wire the OSC-133 emitter and give every evidence source a uniform freshness contract

> **Executor instructions**: Do plan 008 (test seam) first. Run every verification command; honor STOP
> conditions. Update the README row when done.
>
> **Drift check**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-capsule/src/agent_status/arbitrate.rs crates/jackin-capsule/src/agent_status/evidence.rs crates/jackin-capsule/src/session.rs docker/construct/zshrc`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: 008
- **Category**: bug (compute redundancy)
- **Planned at**: commit `5d3661cff`, 2026-07-03
- **Implementation status**: DONE in PR 714 (`OscEvidence.shell_state` is timestamped, TTL-bounded, and cleared with agent OSC signals; the construct zsh config emits OSC-133 prompt/command marks; optional physics-idle was deferred)

## Why this matters

For claude and codex — the two primary agents — the status authority advertises four evidence sources but
**three are structurally inert**, leaving one fragile screen pack as the single point of failure for all
blocked/idle/done:
1. Hooks are identity-only (Decision 0a): `gating.rs` maps every claude/codex event to `heartbeat`, and
   `heartbeat` only refreshes an *existing* authority (`session.rs:848`) — it never creates one, so
   `self.authority` stays `None`.
2. **OSC-133 has no emitter at all** — `docker/construct/zshrc` sets only OSC 0/2/7; there is no
   `precmd`/`preexec` OSC-133 mark, so `osc.shell_state` is never populated. The source is dead.
3. Physics can only ever produce `Working`/`Unknown` (`arbitrate.rs:160-174`) — never blocked/idle.
So any drift in the screen pack silently disables all non-working status for claude/codex. Compounding this,
`osc.shell_state` is a **sticky level with no TTL, never reset**, trusted at `Strong` **above** the screen
rules — so the moment OSC-133 *is* wired it becomes a permanent-pin trap. Root cause: the evidence model has
no uniform "freshness" contract — `AuthorityEvidence` carries a TTL but OSC does not, and an *edge* signal
(133 marks) is modeled as a write-once *level*.

## Current state

- `crates/jackin-capsule/src/agent_status/gating.rs:136-142` — claude/codex events → `heartbeat`/`agent-exit`.
- `crates/jackin-capsule/src/session.rs:848` — `Heartbeat => refresh_matching(&mut self.authority)` (no create).
- `docker/construct/zshrc` — no OSC-133 emitter (`grep -n "133\|precmd\|preexec" docker/construct/zshrc` → none).
- `crates/jackin-capsule/src/session.rs:1177-1185` — `scan_osc133` sets `self.osc.shell_state = Some(state)`
  as a level; **no** assignment to `None` exists anywhere (`grep -rn "shell_state = None" crates` → empty).
- `crates/jackin-capsule/src/agent_status/evidence.rs:45-51` — `clear_agent_signals` resets `title`/`progress*`
  but **omits `shell_state`**; `session.rs:167` comment "OSC 133 shell_state persists (belongs to the shell)".
- `crates/jackin-capsule/src/agent_status/arbitrate.rs:127-134` — `shell_state` returned at `Strong` **before**
  the strong-screen check (`:136`); contrast `AuthorityEvidence` which is TTL-filtered at `arbitrate.rs:73-76`.

## Scope

**In scope:** `docker/construct/zshrc` (add the emitter), `crates/jackin-capsule/src/agent_status/evidence.rs`
+ `arbitrate.rs` + `session.rs` (freshness/TTL for OSC, reset in `clear_agent_signals`), and optionally a
physics-authored low-confidence idle. **Out of scope:** the screen packs themselves (plans 005/007); the
semantic-authority promotion (plan 009).

## Steps

Do Step 1 (freshness) before Step 2 (emitter) so wiring the emitter can't introduce the pin.

### Step 1: Give OSC evidence the same freshness contract as authority (removes the sticky-pin trap)

- Store `last_mark_at: Instant` alongside `shell_state`; in `arbitrate`, filter `shell_state` by an OSC TTL
  (mirror `AUTHORITY_TTL` at `arbitrate.rs:73`) so a stale mark expires instead of pinning forever.
- Add `shell_state` reset to `clear_agent_signals` (`evidence.rs:45-51`) so authority-clear also clears it.
- Reconsider precedence: a shell-prompt mark should not outrank a *strong current-screen* blocked match
  (an approval dialog visible on screen is stronger than a stale prompt mark). Move the `shell_state` arm to
  after strong-screen-blocked, or gate it on freshness+foreground.
- **Structural framing:** every evidence source must carry a freshness bound; none may be a write-once level.
  Add a test that a `shell_state` older than the OSC TTL does not influence arbitration.

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/arbitrate|osc|shell_state|freshness/)'` → pass.

### Step 2: Wire a real OSC-133 emitter for shell panes (restores a live second source)

Add OSC-133 prompt/command marks to the container shell config so `shell_state` becomes a genuine signal for
**shell** panes (agent panes are one long-lived process and rarely cycle 133 — that's expected; this helps the
"agent returned to shell" transition and shell-only panes). In `docker/construct/zshrc`, add `precmd`/`preexec`
hooks emitting OSC 133 `A`/`B`/`C`/`D` (Final Term markers). Keep it minimal and container-local (never touch
host shell rc). Confirm `scan_osc133` (`session.rs:1177`) already parses the marks you emit.

**Verify**: after Step 1's TTL, `grep -n "133\|precmd\|preexec" docker/construct/zshrc` → present; the shell
now drives `shell_state` and it expires per TTL (no pin).

### Step 3 (optional, evaluate): let physics author a low-confidence Idle for identity-only agents

Today physics never yields idle, so claude/codex idle/done depends entirely on the screen pack. Consider a
`Weak` physics-idle when: foreground *is* the agent, CPU quiet for a hold window, and no active children — so
idle/done is reachable even if the pack under-matches. The existing debounce (`IDLE_CONFIRMATIONS`,
`IDLE_HOLD_CAP` in `policy.rs`) guards against flicker. This is a redundancy floor, not the primary path; if it
risks false-idle during a long model "think", scope it conservatively or defer (name it in the row).

**Verify**: if implemented, `cargo nextest run -p jackin-capsule -E 'test(/physics_idle|policy/)'` → pass.

## Done criteria

- [x] `shell_state` is TTL-bounded and reset in `clear_agent_signals`; a stale mark can't pin state (test proves)
- [x] OSC-133 emitter present in `docker/construct/zshrc`; `scan_osc133` consumes it; expiry works
- [x] Precedence: a strong on-screen blocked match is not overridden by a stale prompt mark (test proves)
- [x] (If Step 3) physics-idle not shipped; deferred to avoid false idle during long model thinking
- [x] `cargo nextest run -p jackin-capsule` green; clippy clean
- [x] `plans/agent-status/README.md` row updated

## STOP conditions

- Plan 008's seam isn't in place — do 008 first (you need a full-tick test to prove no pin/regression).
- The OSC-133 emitter interferes with an agent TUI (some agents disable app-shell integration) — scope the
  emitter to shell panes and confirm it's inert while an agent owns the foreground; report if it leaks.
- Step 3's physics-idle causes false idle during long model thinking in testing — scope harder or defer it,
  and name the deferral; do not ship a flickering idle.

## Maintenance notes

- The durable win is the **uniform freshness contract** — a reviewer should reject any new evidence field that
  is a write-once level without a TTL, matching how `AuthorityEvidence` already works.
- Plan 009 (semantic authority for claude/codex) is the *real* redundancy restoration; this plan removes the
  sticky-pin landmine and revives the shell source so 009 lands on a sound evidence model.
