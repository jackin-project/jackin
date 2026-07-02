# Plan 002: Generic runtime/shell unwrap in agent identification (stop going dark on non-Claude wrapped agents)

> **Executor instructions**: Follow step by step; run every verification command. Honor STOP conditions.
> Update this plan's row in `plans/agent-status/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-capsule/src/agent_status/process.rs crates/jackin-core/src/agent.rs`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (false-positive identification if guards are wrong)
- **Depends on**: none
- **Category**: bug (identification)
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

`identify_agent` recognizes an agent from its process, but the node/bun/deno wrapper branch **only** matches
Claude (`@anthropic-ai/claude-code`); **every other node-wrapped agent returns `None`**, and there is no
python or POSIX-shell unwrap at all. npm-distributed CLIs (opencode, codex, amp, kimi, grok are commonly
launched via a `node`/`bun` shim) then identify as nothing. The blast radius is large: an unidentified pane
(`agent = None`) gets **no screen-pack match** (`rules.rs` keys on the agent slug) **and** its foreground
process can't validate a hook authority (`arbitrate.rs` requires `foreground_is_agent`), so it is stuck on
weak physics-only evidence — no blocked, no idle-at-prompt. The reference (herdr) unwraps generically for
*all* agents with flag guards. Root cause: identification special-cases one agent instead of modeling
"wrapped agent" as a general concept, so every non-Claude wrapped agent is a silent gap.

## Current state

`crates/jackin-capsule/src/agent_status/process.rs:216-238`:
```rust
pub fn identify_agent(info: &ProcessInfo) -> Option<Agent> {
    if let Some(ref exe) = info.exe_path {
        let exe_name = exe.file_name()?.to_string_lossy();
        if let Some(agent) = agent_from_name(exe_name.as_ref()) { return Some(agent); }
        // Node-wrapped agents: inspect argv[1] for the JS entry point.
        if matches!(exe_name.as_ref(), "node" | "bun" | "deno") {
            if let Some(script) = info.cmdline.get(1)
                && (script.contains("@anthropic-ai/claude-code") || script.contains("claude-code")) {
                return Some(Agent::Claude);        // <-- Claude only
            }
            return None;                           // <-- every other wrapped agent goes dark
        }
    }
    agent_from_name(info.comm.as_str())            // 15-char-truncated fallback
}
```
- `agent_from_name` (`process.rs:~200-212`) maps `"claude-code" → "claude"` else `Agent::from_slug(name)`.
- `Agent::from_slug` (`crates/jackin-core/src/agent.rs`) knows all six slugs (claude/codex/amp/kimi/opencode/grok).
- **Reference to mirror (approach only, AGPL — do NOT copy code):**
  `herdr/src/detect/mod.rs:276-314` `normalized_process_name` / `wrapped_agent_name_from_runtime_argv`:
  unwraps node/bun (guarding `-e`/`-p` eval), python/python3 (guarding `-c`/`-m`), POSIX shells sh/bash/zsh/fish
  (guarding `-c`), Windows cmd (`/c`) and powershell (`-File`/`-Command`); its tests
  (`herdr/src/detect/mod.rs:694-1052`) are the behavioral spec for the guards.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Build | `cargo check -p jackin-capsule --all-targets` | exit 0 |
| Test | `cargo nextest run -p jackin-capsule -E 'test(/identify|process/)'` | all pass |
| Clippy | `cargo clippy -p jackin-capsule -- -D warnings` | exit 0 |

## Scope

**In scope:** `crates/jackin-capsule/src/agent_status/process.rs` (`identify_agent` + a new unwrap helper),
its `tests.rs`. **Out of scope:** the `/proc` sampling that produces `ProcessInfo` (plan 008 adds its test
double); `Agent::from_slug` (already total over the six agents).

## Steps

### Step 1: Add a general "unwrap wrapped agent from argv" helper

Write a helper `wrapped_agent_from_argv(exe_name: &str, cmdline: &[String]) -> Option<Agent>` that:
- For `node|bun|deno`: scan `cmdline` for the first argument that is **not** a flag and **not** an eval flag's
  operand (`-e`, `-p`, `--eval`, `--print`), take its basename (strip path + `.js`/`.mjs`/`.cjs`), and map it
  through `Agent::from_slug` (also handle the `@scope/name` npm form → last path segment; e.g.
  `@sourcegraph/amp` → `amp`, `@openai/codex` → `codex`, `opencode` → `opencode`). Keep the existing explicit
  `@anthropic-ai/claude-code → Claude` mapping.
- For `python|python3`: skip if `-c`/`-m` (module/eval), else basename of the first script arg → `from_slug`.
- For `sh|bash|zsh|fish`: skip if `-c` (inline command; too ambiguous to trust — return `None`), else the
  script basename → `from_slug`.
Return `None` when nothing maps. Mirror herdr's guard set exactly (it exists to avoid mis-identifying
`node -e "…amp…"` as amp).

### Step 2: Route identification through the helper

Replace the Claude-only node branch: after the exe-basename check, if `exe_name` is a known runtime/shell,
`return wrapped_agent_from_argv(...)`. Keep the truncated-`comm` fallback last (it correctly catches native
binaries named `codex`/`opencode`/etc.).

**Verify**: `cargo check -p jackin-capsule --all-targets` → exit 0.

### Step 3: Tests (author jackin's own tests covering the same guard behavior — not copied from herdr)

In `process/tests.rs`, add cases (build a `ProcessInfo` with `exe_path` + `cmdline`):
- `node /usr/lib/node_modules/opencode/bin/opencode.js` → `Some(Opencode)`.
- `node …/@openai/codex/…` → `Some(Codex)`; `bun …amp… ` → `Some(Amp)`; grok shim → `Some(Grok)`.
- `node -e "console.log('amp')"` → `None` (eval guard — must not false-identify).
- `python -m http.server` → `None`; `python /path/kimi.py` → `Some(Kimi)` (only if kimi ships that way — else omit).
- `bash -c "grok --help"` → `None` (inline `-c` not trusted).
- Native binary `comm = "opencode"` (no exe unwrap) → `Some(Opencode)` (fallback still works).
- Existing Claude cases still pass.

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/identify|process/)'` → all pass incl. new.

## Done criteria

- [ ] Node/bun-wrapped opencode/codex/amp/kimi/grok identify correctly (tests prove each)
- [ ] Eval/inline guards prevent false identification (`node -e …`, `bash -c …` → `None`, tests prove)
- [ ] The Claude-only special case is gone; Claude still identifies (regression test passes)
- [ ] `cargo clippy -p jackin-capsule -- -D warnings` exits 0
- [ ] `plans/agent-status/README.md` row updated

## STOP conditions

- **Verify the real launch shape first.** Read `docker/runtime/entrypoint.sh` and the construct image to see
  how each agent is actually launched in-container (native binary vs node shim). If codex/opencode/etc. run as
  **native binaries** (exe basename already matches `agent_from_name`), the wrapped-unwrap is defense-in-depth,
  not the active fix — say so in the row note, but still land it (herdr shows wrappers are common and future
  npm installs will hit this). If a shell `-c` launch is the *only* way an agent starts (so the `-c` guard
  would make it undetectable), report — that agent needs a different identity signal.
- A guard can't distinguish a legitimate wrapped agent from an eval that mentions the agent name — prefer
  `None` (herdr does) and report the ambiguous case.

## Maintenance notes

- Reviewer: scrutinize the flag guards — a too-loose unwrap that identifies `node -e` as an agent is worse
  than `None` (it pins a wrong pack). herdr's tests are the spec.
- This removes the "one special-cased agent" condition; adding a new agent to `Agent::from_slug` now works
  through the wrapper automatically. Pairs with plan 006's exhaustiveness test.
