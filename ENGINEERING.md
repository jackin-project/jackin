# Engineering rules

Cross-cutting code-craft rules for every coding session: dependency choices, DRY, telemetry, comments. Apply to Rust source, Dockerfile snippets in `docker/`, shell scripts under `docker/runtime/` and `docker/construct/`, `justfile` recipes, CI workflow steps, TypeScript helpers under `docs/scripts/`.

## Rust-first implementation default

Prefer Rust for new project-owned automation, CLIs, release tooling, parsers, state machines, and long-lived helpers. Use another language only where the surrounding ecosystem makes it the natural fit (for example docs-site TypeScript, shell inside container entrypoints, or tiny glue that must run before Rust tooling exists), and keep that exception local rather than growing a parallel implementation stack.

## Prefer libraries over hand-rolled parsers / serializers / format handlers

**Default to maintained crate. Hand-roll only when crate unmaintained, API awkward for call site, or usage trivially small.**

Must use crate, not hand-rolled:

- YAML parsing → `serde_yaml_ng` (or fork workspace depends on). No line-by-line YAML scanner.
- TOML parsing → `toml` / `toml_edit` (already in workspace).
- JSON parsing → `serde_json` (already in workspace).
- Date/time, base64, semver, URL parsing, hex, regex — pick maintained ecosystem crate.
- Cryptographic primitives — never roll own; use `ring`, `rustls`, `argon2`, etc.

"Trivially small" carve-out narrow: single five-line helper splitting one fixed-format string fine. Multi-state line-by-line scanner with quote handling, comment stripping, indent rules, or anything smelling like reimplementing a parser is not.

Choosing crate, prefer:

- **popular, canonical option** — check crates.io download counts (recent + total), GitHub stars, breadth of ecosystem dependents. Famous crates get most bug reports, fixes, security review. Niche / low-download only when no maintained alternative;
- **active recent maintenance** — commits / releases within ~12 months, ideally less. Open issues triaged. Multiple contributors, not single-person;
- **stable major version** (1.x+) where possible — pre-1.0 acceptable when crate still canonical (e.g. `clap`'s subcommand derive history) but flag in PR;
- **continuity with workspace** — if sibling dependency already in `Cargo.lock`, prefer over alternative adding new transitive tree;
- **panic-free / error-result-returning APIs** over panic-on-bad-input (matters at trust boundaries — host config, network responses, untrusted user input).

Anti-pattern: pulling fresh-but-obscure crate just from search results. Crate with 30 stars, no recent commits, one author is *worse* than canonical-but-deprecated alternative — deprecated one is battle-tested. Prefer (in order): popular + maintained → popular but stale → write the few lines yourself. No fringe crates.

When canonical crate *deprecated* but no clear successor: document choice in PR — name deprecation, candidate forks evaluated, criterion that picked winner. Eliminates re-debating the choice later.

Rationale: Rust ecosystem is a leverage point. Pulling a crate is usually 50–200 KB and one `Cargo.toml` line. Reinventing parsers wastes review attention, multiplies bug surface, misses upstream fixes.

When you do hand-roll something this rule covers, comment why (crate unavailable, scope tiny, dependency cost rejected) so a later maintainer can replace without re-debating.

## Reuse before writing — DRY (hard rule)

**Before writing new code, check whether something close enough exists. If yes, extend/parameterise/wrap it, not a parallel copy. If no, write it in a shape future callers can reuse.**

Applies to *every* layer: render helpers, state-derivation functions, parsing/validation, CLI argument structs, docker mount-list builders, TUI block layout, dialog dispatch, OS abstraction, hook scripts, build scripts. About to write a function "mostly same as `<other_function>` but with one branch flipped" — stop, refactor existing one to accept the difference, use it.

Checks before adding new code:

1. **`grep` for the verb, the noun, the surrounding nouns.** "render global mounts" → `rg 'global_mount' src/`. "derive cwd from a manifest" → `rg 'fn .*cwd|manifest.*cwd' src/`. Multi-noun phrases catch helpers named for adjacent concepts. One match — read it before writing new function; multiple — duplication already started, flag in PR even if your change is narrow.
2. **Walk call sites of closest match.** Existing function with two-three callers passing different args — usually add a parameter (or small enum) and route every caller through it. If call sites would grow ugly to share, *say so in a comment* on the new function, keep duplication explicit.
3. **Look one directory up.** Helpers often live in `<feature>/mod.rs`, `console/manager/render/mod.rs`, `runtime/mod.rs`, `instance/mod.rs`. If `<feature>/sub.rs` about to grow a private helper not depending on `sub.rs`-only state, helper belongs in parent `mod.rs` (or sibling `helpers.rs`).
4. **Symmetric variants demand symmetric implementations.** Two functions handling "current dir" vs "saved workspace" — or "agent" vs "shell" — or "Linux" vs "macOS" — per-variant deltas should be data, not control flow. Both paths run `f()` + `g()` + `h()` in slightly different order or with one missing call — missing call is almost always a bug waiting to surface (one variant extended, other not). Pull shared sequence into one function, pass variant-specific bits as args.
5. **Constraints / extension points beat copies.** New caller needing *slightly* different behaviour, prefer (in order): (a) new parameter with sensible default; (b) small `enum` matched inside existing function; (c) trait taken by reference. Forking into `do_foo_for_x` and `do_foo_for_y` is last resort, only when divergence structural enough that shared body confuses more than two siblings.

Why: every parallel implementation is a future bug. Extend one path, forget the other — divergence surfaces later as "feature works on workspace screen but not current-directory screen", a class of bug this project hit before. Adding a parameter advances both paths together; adding a second function makes them drift.

Patterns this blocks (real findings):

- `sidebar_inputs_for_workspace` and `sidebar_inputs_for_current_dir` build the same `SidebarInputs` struct with overlapping body. Extending one while leaving the other untouched is the bug. Fix: factor divergent piece (picker-role resolution, role-binding presence) into helpers both call, not another sibling function for a third selection kind.
- `focused_block_still_scrollable` matching only `ManagerListRow::SavedWorkspace` for global-mounts focus while render path also accepts `ManagerListRow::CurrentDirectory`. Render and scrollability checks must read from same selection-to-rows helper, else focus calculation lags visible content.
- Adding a per-agent `LAUNCH=` block to `docker/runtime/entrypoint.sh` when an existing block handles "agent X with optional credential mount" via a `case`. Extend the `case`, not duplicate the surrounding `seed_home_dir` / chmod / exec scaffolding.

When you do duplicate (deltas too structural for shared body, or shared body defers divergent decision to a runtime branch hurting readability), leave a one-line comment on each copy naming the sibling and the *reason* divergence is preserved.

## Telemetry must be debuggable on demand without becoming noisy by default (hard rule)

**Standard log output (no debug flag) must be compact: lifecycle events, action breadcrumbs, error paths only. Debug-flag output must be a firehose detailed enough to reconstruct every operator keystroke, protocol frame, dispatch decision, render boundary. Both surfaces live in same code, gated on same flag — no `// TODO: remove debug logging` smell, no "rebuild with extra logging" round trip.**

Two-tier:

- **`clog!` (compact, always on).** Daemon start, session spawn/exit, child reap, PTY mutex poison, attach handshake outcomes, dialog dispatch arms that act (`Command`, `SpawnAgent`, `RenameTab`, `Dismiss`), pane/tab close, focus swap, error paths with underlying errno. Quiet enough a multi-hour session yields a scrollable log. Operators paste these into bug reports for the timeline of *what happened*.
- **`cdebug!` (verbose, gated on `JACKIN_DEBUG=1`).** Every byte from client, every parser event with dispatch state (dialog open / focused pane / prefix awaiting), every PTY write with bytes and destination session, every render frame size and reason, every dialog redraw, every per-tick state ticker. Macro skips format + write entirely when flag off, so production pays nothing. Flag on, trace localizes "key X produced no visible effect" from log alone — chunk line proves byte reached daemon, parser line proves classification, dispatch line proves routing, PTY-write line proves byte hit slave fd.

Flag is same `JACKIN_DEBUG` host's `--debug` sets — flows into container via `env_passthrough` in `daemon.rs`, captured once at `logging::init()` time. New verbose sites branch on `cdebug!`, not `clog!`. New compact sites branch on `clog!`. Anything firing more than ~10 times/minute under normal operation belongs on `cdebug!`.

Adding "TEMPORARY logging to triage a regression" — stop, convert to `cdebug!` — next bug report needs the same telemetry, removing-and-readding-it each regression cycle is the loop this rule breaks. Same for any surface growing a telemetry / tracing layer (host CLI's `tui::tprintln`, docs site render warnings, `runtime::launch` path): two tiers, debug-gated firehose, default compact.

When current logs insufficient to explain complex or inconsistent behaviour, do not guess. First add durable `cdebug!` telemetry capturing missing state, ask operator to rerun repro with `--debug`, then fix from that evidence. Only exception: missing state obtainable safely from live process or container without code change — inspect directly and keep going.

Reason: operators rarely reproduce on demand. They need to paste a log that already has the answer — no rebuild, no extra instrumentation forgotten at ship, no "now run it again with this added line". Host's `--debug` is the single switch; everything downstream honours it.

## Code comments — explain only what is not obvious

**Comments earn their place by encoding non-obvious WHY, not narrating WHAT.** Well-named identifiers, type signatures, surrounding code already say what code does; a repeating comment is noise that pushes signal off-screen and rots faster than the code.

Comment when, and only when, one is true:

- Code looks suspicious/weird/wrong on first read but is intentional. Name the constraint forcing it (TOCTOU, parser-bypass safety, ordering invariant, race window, kernel quirk, upstream bug).
- A non-local invariant is preserved. Point at invariant and dependent call site.
- Shape could reasonably be written differently. Name the trade-off that picked it.
- Code interacts with externally documented behaviour an unfamiliar reader won't predict (POSIX edge case, Docker daemon quirk, library footgun).

Do not comment when:

- Identifier name already says it (`fn provision_amp_auth` needs no `// Provision Amp auth`).
- Signature already says it (`Result<T, io::Error>` needs no `// returns an io::Error on failure`).
- Control flow says it (`for x in items { … }` needs no `// loop over items`).
- Diff says it (`// renamed from foo`, `// added in PR #N`, `// previously did X`).

Style:

- One sentence over a paragraph. Trim until removing one more word breaks clarity.
- Lead with the constraint, not the code. "TOCTOU on settings.json: …" beats "We do this thing because there is a TOCTOU…".
- Drop "mirrors X" / "matches Y" parallel-structure narration — parallel code structure already encodes it, the cross-reference dates the moment one side drifts.
- Code blocks, function names, error strings, CLI flag names exact, never abbreviated; prose around them as terse as possible.

Applies to inline `//`, multi-line `/** */` / `///` / `//!` doc comments, test-method docstrings. Operator-facing surfaces (`clap` `--help` text, `eprintln!` lines, README prose) follow docs split rules loading under `docs/` — not "comments" here.

Do not memorialize old shapes in code comments ("formerly named X", "old location was Y"). Git history is the record; code describes only the current shape.
