# Design: `jackin help` Command (cargo-style)

**Date:** 2026-05-01
**Status:** Approved

## Summary

Add `jackin help [COMMAND]...` that mirrors cargo's `help` subcommand exactly:
long-form man page output via `man` -> `less`/`more` -> raw stdout fallback chain.
This is distinct from `jackin <cmd> --help`, which continues to show clap's short summary.

## Motivation

`jackin <cmd> --help` gives a compact option listing. Operators who want the full reference
(long descriptions, examples, all flags) currently have no single entry point.
The cargo pattern (`cargo help install`) is well-understood by Rust developers and sets
the right precedent for jackin's CLI surface.

## Behaviour

```
jackin help                   # man page for jackin itself
jackin help config            # man page for jackin-config
jackin help config auth       # man page for jackin-config-auth
jackin help workspace env     # man page for jackin-workspace-env
jackin help unknowncmd        # error: no such subcommand `unknowncmd`, exit 1
```

`jackin --help` (the flag) is unchanged -- still shows clap's short summary.

The root `Cli` gains an `after_help` footer:
```
Run 'jackin help <command>' for more detailed information.
```

## Architecture

### 1. Build-time man page generation (`build.rs`)

New build dependency: `clap_mangen`.

`build.rs` calls `Cli::command()` (via `CommandFactory`) to obtain the full clap
command tree, then walks every command and subcommand recursively. For each node it generates:

- **`<name>.1`** -- roff format, via `clap_mangen::Man::new(cmd).render(writer)`
- **`<name>.txt`** -- plain text, via `cmd.render_long_help()`

Naming convention: the root is `jackin`; subcommands are joined with `-`:
`jackin-config`, `jackin-config-auth`, `jackin-workspace-env`, etc.

All files are packed into a deterministic gzipped tar archive written to
`$OUT_DIR/man.tgz` (sorted, fixed timestamp, maximum compression), matching
cargo's approach for reproducible builds.

The archive is embedded in the binary at compile time:
```rust
static MAN_ARCHIVE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/man.tgz"));
```

### 2. `Help` variant in `Command` enum (`src/cli/mod.rs`)

The variant is visible (not hidden) so it appears in `jackin --help`'s command list.
`command: Vec<String>` uses `trailing_var_arg = true` to capture multi-word paths like
`config auth`.

### 3. Dispatch wiring (`src/cli/dispatch.rs`)

Add `Action::PrintHelp { command: Vec<String> }`.

In `classify`, `Command::Help { command }` maps to `Action::PrintHelp { command }`.

### 4. Runtime help display (`src/cli/help.rs`, new file)

Public entry point: `pub fn exec(command: &[String]) -> anyhow::Result<()>`

Logic:
1. Build archive key: `"jackin"` if `command` is empty, else `format!("jackin-{}", command.join("-"))`.
2. Decompress embedded `MAN_ARCHIVE` (flate2 + tar).
3. Try `.1` entry -> write to temp file -> invoke `man <path>`. Fallback only if `man` is not found in PATH (not on exit code -- user quitting with 'q' exits 1 but the page was shown).
4. Try `.txt` entry -> write to temp file -> try `less -R <path>`, then `more <path>`. If either succeeds, return.
5. Final fallback: print `.txt` bytes directly to stdout.
6. If neither entry found -> `anyhow::bail!("no help available for `{key}`")`.

`main.rs` maps `Action::PrintHelp { command }` to `help::exec(&command)`,
propagating errors to the existing fatal handler.

### 5. `after_help` footer on `Cli`

Add `after_help = "Run 'jackin help <command>' for more detailed information."` to the
root `#[command(...)]` attribute on `Cli`.

## New Dependencies

| Crate | Where | Purpose |
|-------|-------|---------|
| `clap_mangen` | `[build-dependencies]` | roff man page generation |
| `flate2` | `[dependencies]` | gzip decompression at runtime |
| `tar` | `[dependencies]` | tar extraction at runtime |

`tempfile` is already a dev-dependency; it needs promotion to a full runtime dependency.

## Files Changed

| File | Change |
|------|--------|
| `Cargo.toml` | Add `flate2`, `tar` deps; promote `tempfile`; add `clap_mangen` build-dep |
| `build.rs` | Add man page generation and archive packing |
| `src/cli/mod.rs` | Add `Help` variant; add `after_help` footer to `Cli` |
| `src/cli/help.rs` | New file -- runtime archive extraction and display chain |
| `src/cli/dispatch.rs` | Add `Action::PrintHelp`; classify `Command::Help` |
| `src/main.rs` | Handle `Action::PrintHelp` |

## Error Handling

- Unknown command -> `anyhow::bail!` propagated to `tui::fatal`, exit 1.
- `man`/`less`/`more` not found -> silently skip to next fallback (no error).
- Corrupt archive (embedded at compile time, should not happen) -> propagated as error.

## Testing

- Unit: `jackin help` parses to `Command::Help { command: [] }`.
- Unit: `jackin help config auth` parses to `Command::Help { command: ["config", "auth"] }`.
- Unit: `classify(Help { command: [] })` -> `Action::PrintHelp { command: [] }`.
- Integration (assert_cmd): `jackin help` exits 0, stdout non-empty.
- Integration: `jackin help config auth` exits 0, stdout contains "auth".
- Integration: `jackin help unknowncmd` exits non-zero.
- Build-time assertion: archive entry count > 0.

## Out of Scope

- Shell completions for `jackin help` arguments.
- Installing man pages to the system man path.
- Markdown source files (cargo has these; jackin generates from clap doc comments).
