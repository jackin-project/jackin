# Plan 007: Rewrite the kimi/amp/opencode/claude pack matchers from real chrome + add OSC-title spinner rules

> **Executor instructions**: Do plan 005 (real goldens) and plan 002 (identification) first — this plan is only
> verifiable against real captured screens. Run every verification command. Update the README row.
>
> **Drift check**: `git diff --stat 5d3661cff..HEAD -- docker/runtime/agent-status/packs`

## Status

- **Implementation status**: IN PROGRESS in PR #714. The operator supplied live jackin❯ screenshots for several
  affected states, and the PR now rewrites the corresponding narrow matchers: Codex stale working after a newer
  prompt, Kimi live `working...` and prompt-box idle, and OpenCode 1.17 `esc interrupt` footer. Full pack rewrite
  remains incomplete until real blocked/working/idle captures exist for each affected agent.
- **Priority**: P2
- **Effort**: M
- **Risk**: MED (broaden-to-match can add false positives)
- **Depends on**: 005 (goldens), 002 (identification)
- **Category**: bug (detection content)
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

The pack rule literals for kimi/amp/opencode (and Claude's idle caret) are **fabricated placeholders** that do
not match the real TUIs — so those agents can never fire blocked/idle even when identification and cadence work.
Because the screen pack is the sole blocked authority for reporter-less agents (kimi, amp), a wrong blocked
literal means that agent **can never show blocked**. Separately, the dominant working/idle signal the reference
uses for claude/codex — the **agent's own OSC-title spinner** — is parsed by jackin (`osc_title` virtual region)
but the claude pack keys working off body strings ("esc to interrupt") instead, leaving working detection
dependent on version-fragile body chrome. This plan fixes the content against real goldens and adds the
version-stable OSC-title rules.

## Current state (fabricated vs real; verified against the herdr reference)

- `docker/runtime/agent-status/packs/kimi.toml:11,27` — blocked requires `"kimi wants permission"`, idle requires
  `"kimi ready"`. Neither appears in the real Kimi TUI (herdr's kimi manifest uses `"↵ confirm"` /
  `"run this command?"` / moon-braille spinners).
- `docker/runtime/agent-status/packs/amp.toml:11,19,34` — blocked `"amp wants to execute"`, working `"running tool:"`,
  idle `requires_all=[">"]` (matches any line with `>`). herdr amp: blocked `"waiting for approval"` /
  `"run this command?"`, working `"esc to cancel"`.
- `docker/runtime/agent-status/packs/opencode.toml:19,27` — working requires **both** `"processing..."` AND
  `"ctrl+c to cancel"` (conjunction can't match real chrome); idle `"opencode ready"`. herdr opencode working:
  `"esc to interrupt"` / `"ctrl+c to interrupt"` / a progress-bar regex.
- `docker/runtime/agent-status/packs/claude.toml:44` — idle requires `["╭", "│ >"]` with **ASCII `>`**; real
  Claude renders the prompt caret `❯` (U+276F), so the substring never matches.
- `docker/runtime/agent-status/packs/codex.toml:33` — working `"• working ("` (herdr detects Codex working via
  the OSC braille title, not that screen literal).
- jackin❯ already captures `osc_title` / `osc_progress` into virtual regions (`rules.rs:266-268`,
  `evidence.rs:32-42`) — available to rules but unused by the claude/codex packs for working/idle.
- The rule engine supports `any` / `line_regex` / nested gates (`rules.rs`), so it can express herdr's
  disambiguators.

## Scope

**In scope:** `docker/runtime/agent-status/packs/{claude,codex,amp,kimi,opencode}.toml` and their goldens.
**Out of scope:** grok.toml (plan 006); the engine itself; the render layer.

## Steps

### Step 1: For each agent, rewrite blocked/working/idle rules against the captured golden (plan 005)

Working from the **real captured screens** (plan 005's goldens — never from the old pack strings), rewrite each
pack's rules so blocked/working/idle match the current TUI. Use the engine's `any`/`line_regex`/nested-gate
grammar for disambiguation (herdr's rule *shapes* are the guide — approach only, AGPL). Tighten the loose idle
rules (amp/kimi `requires_all=[">"]`) so they don't false-idle on any `>`-containing line. Fix the Claude idle
caret to the real `❯` (U+276F), and prefer a `line_regex` anchored caret (`^\s*❯`) over a fragile substring.

### Step 2: Add OSC-title working/idle rules for claude/codex/amp (version-stable signal)

Add rules keyed on the `osc_title` virtual region: a braille/spinner glyph in the title → working; a
non-spinner/idle title → idle. This gives working/idle a source independent of body chrome (herdr's primary
signal for claude/codex). Confirm each agent actually emits a title spinner in the container before relying on
it (some may not — keep the body rules as fallback, priority-ordered).

### Step 3: Verify every pack against its golden

Every pack must match its plan-005 captured goldens for blocked/working/idle. Un-`ignore` the goldens that plan
005 left pending for these agents.

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/packs_load_and_match_fixtures/)'` → all pass
(including the previously-failing kimi/amp/opencode/claude-idle cases); `cargo clippy -p jackin-capsule -- -D warnings` → exit 0.

## Done criteria

- [ ] kimi/amp/opencode/claude/codex packs match real captured goldens for blocked, working, and idle — PARTIAL:
  Codex/Kimi/OpenCode have targeted live-capture-backed fixes; full state coverage remains open
- [ ] The Claude idle caret uses the real `❯` (U+276F) via an anchored `line_regex` — BLOCKED until the real
  Claude idle capture exists
- [ ] Loose idle rules (`requires_all=[">"]`) are tightened; no false-idle on arbitrary `>` lines — BLOCKED
  until real captures prove the replacement rules
- [ ] OSC-title working/idle rules added where the agent emits a title spinner — BLOCKED until real captures
  prove which agents emit title state in-container
- [ ] `packs_load_and_match_fixtures` passes with no `#[ignore]` remaining for these agents — BLOCKED until
  plan 005 real goldens exist
- [x] `plans/agent-status/README.md` row updated

## STOP conditions

- Plan 005's goldens aren't captured yet — you'd be guessing again; do 005 first. This is the whole reason 007
  depends on 005.
- A broadened rule starts matching a false state in the goldens (e.g. working chrome also present on an idle
  screen) — narrow with `forbids_regex`/`not` gates; if the real TUI genuinely can't be disambiguated by screen
  alone for that agent, note it and lean on OSC-title (Step 2) or defer that agent to a reporter (plan 009).

## Maintenance notes

- Reviewer: every rule change must cite the golden line it matches — no rule may be added that isn't verified
  against a real capture (the anti-circularity contract from plan 005).
- When an agent CLI restyles, this is now a *data* update (re-capture golden + adjust rule), verifiable by the
  harness — not a silent break. Plan 005's advisory version-drift note tells you when a restyle happened.
