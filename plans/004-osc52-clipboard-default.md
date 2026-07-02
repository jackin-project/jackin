# Plan 004: Default-deny OSC 52 clipboard-write passthrough from container to host terminal

> **Executor instructions**: Security-hardening plan. Follow step by step, run every verification
> command, honor STOP conditions. Update `plans/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 46511939d..HEAD -- crates/jackin-capsule/src/session/osc_policy.rs crates/jackin-capsule/src/session.rs`
> On change, compare against "Current state"; mismatch → STOP.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: security
- **Planned at**: commit `46511939d`, 2026-07-03
- **Operator decision**: default-deny + `JACKIN_OSC52=allow` opt-in (advisor recommendation; operator did
  not override within the session). **If the maintainer prefers convenience-first (keep allow), skip
  Step 1 and do only Step 3's documentation** — see "Escape hatch" below.

## Why this matters

OSC 52 is the escape sequence by which a terminal *program* sets the *terminal emulator's* system
clipboard. In jackin the container is the untrusted side. The byte path: an agent emits OSC 52 →
capsule re-encodes it (`session.rs:1247`) → the host writes it straight to the operator's real terminal
(`host_attach.rs`), setting the **host** clipboard. The only gate is capsule's `allow_osc52()`, which
**defaults ON**. So a compromised/prompt-injected agent can silently overwrite the operator's clipboard
with arbitrary bytes; the operator's next paste into a host shell/editor runs attacker-chosen text —
the classic clipboard-poisoning primitive. xterm ships this write-path OFF by default for exactly this
reason. This plan flips the default to deny and makes it opt-in via `JACKIN_OSC52=allow`, matching the
untrusted-agent threat model that is the product's whole premise.

## Current state

- `crates/jackin-capsule/src/session/osc_policy.rs:56-66` — the default includes `ALLOW_OSC52`:
  ```rust
  const ALLOW_TITLE: u8 = 1 << 0;
  const ALLOW_OSC52: u8 = 1 << 1;
  const ALLOW_NOTIFY: u8 = 1 << 2;
  const ALLOW_HYPERLINK: u8 = 1 << 3;

  impl Default for OscPolicy {
      fn default() -> Self {
          Self { flags: ALLOW_TITLE | ALLOW_OSC52 | ALLOW_NOTIFY | ALLOW_HYPERLINK }
      }
  }
  ```
- `crates/jackin-capsule/src/session/osc_policy.rs:69+` — policy is read from the environment and cached
  at `Session::spawn` time (comment: "so a background pane cannot toggle the gate at runtime by
  `export`ing"). There is an existing env parse (find it: `grep -n "JACKIN_OSC52\|from_env\|parse" crates/jackin-capsule/src/session/osc_policy.rs`).
- `crates/jackin-capsule/src/session.rs:1247-1252` — the forward gate:
  ```rust
  PassthroughEvent::ClipboardWrite(_) => {
      if self.osc_policy.allow_osc52() && let Some(bytes) = event.encode() {
          self.pending_passthrough.push(bytes);
      }
  }
  ```

Note the env var is read **inside the container** (capsule), so the operator sets `JACKIN_OSC52` as part
of the role/launch env that flows into the container — verify how `JACKIN_*` flags reach the capsule
(the `JACKIN_DEBUG` flag uses the same `env_passthrough` mechanism, per `ENGINEERING.md`).

## Scope

**In scope:** `crates/jackin-capsule/src/session/osc_policy.rs` (default + env semantics),
its `tests.rs`, and the user-facing docs page for the security model.

**Out of scope:**
- `ALLOW_TITLE` / `ALLOW_HYPERLINK` defaults — leave ON (title/hyperlink are lower-risk; changing them is
  a separate call). **`ALLOW_NOTIFY`**: the advisor flagged it as the same class but lower severity —
  leave its default unchanged in this plan and note it in maintenance for a follow-up decision.
- The host-side write path (`host_attach.rs`) — the capsule policy remains the single gate; do not add a
  second host-side filter here.

## Steps

### Step 1: Flip the OSC 52 default to deny; keep it opt-in

Change `Default for OscPolicy` so `ALLOW_OSC52` is **not** in the default flag set. Then ensure the env
reader turns `ALLOW_OSC52` back **on** when `JACKIN_OSC52=allow` (and keep any existing `=deny` handling
as an explicit deny). Keep the existing "cached at spawn" semantics — do not make it runtime-toggleable.
The resulting matrix: unset → deny; `allow` → allow; `deny` → deny.

**Verify**: `cargo check -p jackin-capsule --all-targets` → exit 0.

### Step 2: Tests

In `osc_policy/tests.rs` (create if absent, per the tests-in-own-file rule), assert:
- default policy (no env) has `allow_osc52() == false`;
- `JACKIN_OSC52=allow` → `allow_osc52() == true`;
- title/hyperlink defaults are unchanged (`allow_title()`/`allow_hyperlink()` still true).
Use the injected/env-parse seam the module already uses; do not mutate process env in a way that races
other tests (follow the module's existing test pattern).

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/osc_policy/)'` → all pass.

### Step 3: Document the new default (required by the repo's docs gate)

Update the security-model docs page — `docs/content/docs/(public)/guides/security-model.mdx` — to state:
OSC 52 clipboard-write from the container is **denied by default** to prevent host-clipboard poisoning;
set `JACKIN_OSC52=allow` (per role/launch) to enable agent "copy to clipboard". Cross-check the exact
page path with `grep -rl "OSC 52\|clipboard" docs/content/docs`.

**Verify**: `grep -rn "JACKIN_OSC52" docs/content/docs` → at least 1 match after your edit.

## Escape hatch (if maintainer chooses convenience-first)

If the maintainer decides to keep OSC 52 **on** by default: skip Steps 1–2, and in Step 3 document the
poisoning risk plus the `JACKIN_OSC52=deny` opt-out instead. Mark this plan `REJECTED (kept allow by
operator decision)` in `plans/README.md` with that one-line rationale.

## Done criteria

- [ ] `cargo check -p jackin-capsule --all-targets` exits 0
- [ ] Default policy denies OSC 52; `JACKIN_OSC52=allow` re-enables (tests prove both)
- [ ] Title/hyperlink defaults unchanged (test asserts)
- [ ] `docs/.../guides/security-model.mdx` documents the default + opt-in
- [ ] `cargo nextest run -p jackin-capsule` green
- [ ] `plans/README.md` row updated

## STOP conditions

- The `JACKIN_OSC52` env value does not actually reach the capsule (passthrough not wired): report how
  `JACKIN_*` flags are meant to flow before changing the default, so opt-in isn't a dead switch.
- Flipping the default breaks existing capsule render-conformance tests that assumed OSC 52 forwards:
  update those tests to the new default (they are testing the old behavior), but if the break is in an
  unrelated subsystem, STOP.

## Maintenance notes

- `ALLOW_NOTIFY` (OSC 9 desktop notifications) is the same default-on host-surface-write class; a
  follow-up should decide whether it also defaults deny. Recorded here so it isn't forgotten.
- Reviewer should confirm the gate is still cached-at-spawn (not runtime-toggleable by a background pane).
- If a future "trusted role" concept lands (see plan 041/manifest), the per-role trust flag could drive
  this instead of a global env var — note the migration path.
