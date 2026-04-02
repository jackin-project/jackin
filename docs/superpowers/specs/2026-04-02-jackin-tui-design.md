# Jackin TUI Design

## Summary

This design adds a phosphor-themed terminal user interface to `jackin`. The TUI provides visual feedback during the `load` lifecycle — from a cinematic intro sequence through numbered build steps to an animated outro when the agent exits.

The visual language is drawn from The Matrix films: green digital rain, typewriter-effect dialogue, glitch reveals, and a consistent phosphor green/dim/dark color palette. The TUI makes the `jackin load` experience feel intentional and polished while still providing useful operational information through the configuration summary table and numbered step indicators.

## Goals

- Give `jackin load` a distinctive phosphor-themed visual identity.
- Show a configuration summary (identity, container, repository, operator, image, DinD) before the build begins.
- Provide numbered step shimmer animations so the operator can track progress through the load lifecycle.
- Show styled error messages (rose-colored) when steps fail.
- Display a Matrix outro when the agent session ends, including a count of remaining running agents.
- Allow operators to skip animations with `--no-intro` for scripted or repeated use.
- Set the terminal title to the agent's display name during the session.

## Non-Goals

- Organization or agent-specific logo display (deferred — will be added later as a configurable element).
- Interactive TUI elements (progress bars, spinners with ETA, interactive menus).
- Customizable color themes or palettes.
- TUI for commands other than `load` (e.g., `eject`, `exile`, `purge`).

## Color Palette

All colors are true-color RGB values rendered via ANSI escape sequences.

| Name           | RGB             | Usage                                       |
|----------------|-----------------|---------------------------------------------|
| `PHOSPHOR_GREEN` | `(0, 255, 65)`  | Primary accent — intro text, step prefixes, config labels, deploying message |
| `PHOSPHOR_DIM`   | `(0, 140, 30)`  | Secondary — settled step text, config values, outro remaining-agents text     |
| `PHOSPHOR_DARK`  | `(0, 80, 18)`   | Tertiary — table borders, "Connection closed." text                          |
| `WHITE`        | `(255, 255, 255)` | Highlight — digital rain head character, shimmer sweep peak              |
| `DIM`          | `(120, 120, 120)` | Muted info — remaining agents in simple outro                            |
| `ROSE`         | `(210, 100, 100)` | Errors — `step_fail`, `fatal`                                            |

## Visual Elements

### Digital Rain

A terminal-rendered falling-character animation modeled on the Matrix digital rain. Characters fall in columns at varying speeds with head/trail color gradients (white → green → dim → dark). Renders to stderr at 60ms per frame.

Used in:
- **Intro**: 2000ms duration before the dialogue sequence.
- **Outro**: 1500ms duration before the exit messages.

The rain uses a deterministic xorshift PRNG seeded at `0xDEAD_BEEF_CAFE_1337` for reproducible visual patterns. The grid is 70 columns × 18 rows with 2-space left margin.

### Typed Text

Characters appear one at a time with a configurable per-character delay, rendering in a single color. Used for the intro dialogue lines and outro messages.

### Glitch Text

A reveal effect where the target text is shown with random character substitutions that resolve over 4 frames (80ms each). Random characters flash in `PHOSPHOR_GREEN` while the rest use the target color. Used for the final intro line ("Knock, knock, {name}.").

### Step Shimmer

Each numbered build step displays with a sweep animation: a bright-white highlight travels left to right across the text, leaving settled characters in `PHOSPHOR_DIM`. The prefix (step number) renders in bold `PHOSPHOR_GREEN`. Each character frame is 25ms.

Steps are numbered sequentially starting from 1.

### Configuration Table

A bordered table using Unicode box-drawing characters (`┌ ─ ┐ │ └ ┘`) rendered in `PHOSPHOR_DARK`. Labels are `PHOSPHOR_GREEN`, values are `PHOSPHOR_DIM`. Column widths auto-size to content.

Fields displayed:

| Label       | Source                                                        |
|-------------|---------------------------------------------------------------|
| `identity`  | `[identity].name` from `jackin.agent.toml`, or selector name  |
| `container` | Resolved container name (e.g., `jackin-agent-smith`)          |
| `repository`| Path to cached agent repo checkout                            |
| `operator`  | Git `user.name` and `user.email` from host                    |
| `image`     | Docker image name                                             |
| `dind`      | DinD sidecar container name                                   |

The `operator` row is omitted when `git config user.name` is empty.

## Load Flow with TUI

The `jackin load` command follows this sequence:

1. **Resolve git identity** — Read `git config user.name` and `user.email` from the host.
2. **Matrix intro** (skipped with `--no-intro`) — Clear screen → digital rain (2s) → clear → typed dialogue:
   - "Wake up, {operator_name}..." (65ms/char)
   - "The Matrix has you..." (55ms/char)
   - "Follow the white rabbit." (50ms/char)
   - "Knock, knock, {operator_name}." (glitch effect)
   - Clear screen.
   If the operator name is empty, "Neo" is used as the fallback.
3. **Set terminal title** — `\x1b]0;{agent_display_name}\x07`
4. **Print configuration table** — Bordered summary of the session parameters.
5. **Step 1: Building Docker image** — Shimmer → run `docker build`.
6. **Step 2: Creating Docker network** — Shimmer → run `docker network create`.
7. **Step 3: Starting Docker-in-Docker container** — Shimmer → run DinD container → wait for readiness.
8. **Step 4: Mounting volumes** — Shimmer → print "Deploying {name} into the Matrix..." → clear → run `docker run -it` (attached).
9. **Agent session** — Operator interacts with Claude Code inside the container.
10. **Exit / outro** — On container exit:
    - Clear screen.
    - If `--no-intro` was NOT set: full outro with digital rain (1.5s), typed messages ("{name} has left the Matrix.", remaining agent count, "Connection closed.").
    - If `--no-intro` was set: simple text-only outro with the same information, no animations.

If any step fails, a rose-colored `step_fail` message appears below the failed step, and the load is aborted with cleanup.

## CLI Flags

Two new flags on the `load` subcommand:

| Flag         | Default | Effect                                                |
|--------------|---------|-------------------------------------------------------|
| `--no-intro` | `false` | Skip intro/outro animations, show simple text outro   |
| `--debug`    | `false` | Reserved for future use — show verbose Docker output  |

## Error Display

Two levels of error presentation:

- **`step_fail(msg)`** — Indented rose-colored text below a failed step shimmer. Used for errors during the load lifecycle.
- **`fatal(msg)`** — Bold rose "error:" prefix followed by the message. Used for top-level errors in `main()`.

## Implementation

### Module: `src/tui.rs`

A standalone module with no dependencies on `runtime.rs` internals. All functions write to stderr and are pure side-effects (no return values beyond `()`).

Public API:

```rust
pub fn matrix_intro(operator_name: &str);
pub fn matrix_outro(agent_name: &str, remaining: &[String]);
pub fn simple_outro(agent_name: &str, remaining: &[String]);
pub fn print_config_table(rows: &[(String, String)]);
pub fn step_shimmer(n: u32, text: &str);
pub fn step_fail(msg: &str);
pub fn print_deploying(agent_name: &str);
pub fn fatal(msg: &str);
pub fn set_terminal_title(title: &str);
pub fn clear_screen();
```

### Integration: `src/runtime.rs`

The `load_agent` function accepts a `LoadOptions` struct:

```rust
pub struct LoadOptions {
    pub no_intro: bool,
    pub debug: bool,
}
```

TUI calls are interleaved with the existing Docker lifecycle. Tests use `LoadOptions::default()` which sets `no_intro: true` to avoid animations during test runs.

Git identity is resolved once at the start of `load_agent` via direct `git config` subprocess calls (not through the `CommandRunner` trait, since this is host-side metadata, not a Docker operation).

### Dependencies

- `owo-colors` (v4, with `supports-colors` feature) — RGB color rendering.
- `ctrlc` (v3) — Signal handling for cleanup on Ctrl+C (available for future integration).

## Testing

The TUI module is intentionally side-effect-heavy (terminal escape sequences, sleeps, stderr output) and is not directly unit-tested. Correctness is verified through:

- All existing 39 unit tests continue to pass with `LoadOptions::default()` (animations suppressed).
- Manual verification of the visual output during `jackin load` runs.
- The `--no-intro` flag provides a fast path that skips all animations for CI or scripted environments.

## Future Considerations

- **Organization logo**: A configurable ASCII art logo can be displayed after the intro and before the config table. This is deferred and will be added as a separate feature.
- **Spinner during DinD wait**: The `wait_for_dind` loop currently sleeps silently. A future pass could add a Matrix-styled spinner or progress dots.
- **Color detection**: The `owo-colors` `supports-colors` feature enables runtime detection of terminal color support. A future pass could gracefully degrade to plain text on terminals without true-color support.
- **`--debug` flag integration**: Currently reserved. Will control visibility of Docker build output (piped vs. visible) in a future enhancement.
