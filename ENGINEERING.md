# Engineering rules

Cross-cutting code-craft rules that apply in every coding session: dependency choices, DRY, telemetry, and comments. These apply to Rust source, Dockerfile snippets in `docker/`, shell scripts under `docker/runtime/` and `docker/construct/`, `justfile` recipes, CI workflow steps, and TypeScript helpers under `docs/scripts/`.

## Prefer libraries over hand-rolled parsers / serializers / format handlers

**Default to a maintained crate. Only hand-roll when the crate is unmaintained, the API is awkward for the call site, or the usage is so trivially small that adding a dependency is overkill.**

Concrete examples that must use a crate, not a hand-rolled implementation:

- YAML parsing → `serde_yaml_ng` (or whichever fork the workspace already depends on). Do not write a line-by-line YAML scanner.
- TOML parsing → `toml` / `toml_edit` (already in the workspace).
- JSON parsing → `serde_json` (already in the workspace).
- Date/time, base64, semver, URL parsing, hex, regex — pick the maintained ecosystem crate.
- Cryptographic primitives — never roll your own; use `ring`, `rustls`, `argon2`, etc.

The "trivially small" carve-out is real but narrow: a single five-line helper that splits one fixed-format string is fine. A multi-state line-by-line scanner with quote handling, comment stripping, indent rules, or anything that smells like "I am reimplementing a parser" is not.

When choosing a crate, prefer:

- **the popular, canonical option** — check crates.io download counts (recent + total), GitHub stars, and how widely the crate is depended on by other ecosystem crates. Famous, broadly-used crates get the most bug reports, the most fixes, and the most security review. Niche / low-download crates only when there is no maintained alternative;
- **active recent maintenance** — commits / releases within the last ~12 months, ideally less. Open issues being triaged. Multiple contributors, not a single-person effort;
- **a stable major version** (1.x or higher) where possible — pre-1.0 is acceptable when the crate is still the canonical choice (e.g. `clap`'s subcommand derive history) but flag it in the PR;
- **continuity with the workspace** — if a sibling dependency is already in `Cargo.lock`, prefer it over an alternative that adds a new transitive tree;
- **panic-free / error-result-returning APIs** over panic-on-bad-input ones (matters at trust boundaries — host config, network responses, untrusted user input).

Anti-pattern to avoid: pulling in a fresh-but-obscure crate just because it appeared in search results. A crate with 30 GitHub stars, no recent commits, and one author is *worse* than the canonical-but-deprecated alternative — at least the deprecated alternative is battle-tested. Prefer (in order): popular + maintained → popular but stale → write the few lines yourself. Do not pick fringe crates.

When the canonical crate is *deprecated* but no clear successor has emerged, document the choice in the PR: name the deprecation, name the candidate forks evaluated, name the criterion that picked the winner. Future-you re-debating the same crate choice 6 months later is a tax this short paragraph eliminates.

Rationale: Rust's ecosystem is one of the project's leverage points. The community ships small, focused, well-tested crates; pulling one in is usually 50–200 KB of compiled code and a single `Cargo.toml` line. Reinventing parsers and format handlers wastes review attention, multiplies bug surface, and creates code paths that don't get the upstream's bug fixes.

When you do hand-roll something this rule covers, leave a comment explaining why (crate unavailable, scope tiny, dependency cost specifically rejected) so a later maintainer can replace it without re-debating the decision.

## Reuse before writing — DRY (hard rule)

**Before writing new code, check whether something close enough already exists. If yes, extend, parameterise, or wrap it instead of writing a parallel copy. If no, write the new thing in a shape future callers can reuse.**

This applies to *every* layer of the codebase: render helpers, state-derivation functions, parsing/validation, CLI argument structs, docker mount-list builders, TUI block layout, dialog dispatch, OS abstraction, hook scripts, build scripts. Whenever you are about to write a function whose behaviour is "mostly the same as `<other_function>` but with one branch flipped" — stop, refactor the existing one to accept the difference, and use it.

Concrete checks before adding new code:

1. **`grep` for the verb, the noun, and the surrounding nouns.** "I need to render global mounts" → `rg 'global_mount' src/`. "I need to derive cwd from a manifest" → `rg 'fn .*cwd|manifest.*cwd' src/`. Multi-noun phrases catch helpers named for adjacent concepts. If the search returns one match, read it before writing a new function; if it returns multiple, the duplication this rule prevents has already started — flag it in the PR even if your change is narrow.
2. **Walk the call sites of the closest match.** If the existing function has two or three callers that pass different arguments, the right move is usually to add a parameter (or a small enum) and route every caller through the same function. If existing call sites would have to grow ugly to share, *say so in a comment* on the new function and keep the duplication explicit so the next reader can decide.
3. **Look one directory up.** Helpers often live in `<feature>/mod.rs`, `console/manager/render/mod.rs`, `runtime/mod.rs`, `instance/mod.rs`. If `<feature>/sub.rs` is about to grow a private helper that doesn't depend on `sub.rs`-only state, the helper belongs in the parent `mod.rs` (or in a sibling `helpers.rs`) where the next feature in the same family can use it.
4. **Symmetric variants demand symmetric implementations.** When two functions handle "current dir" vs "saved workspace" — or "agent" vs "shell" — or "Linux" vs "macOS" — the per-variant deltas should be data, not control flow. If both paths run `f()` + `g()` + `h()` but in slightly different order or with one missing call, the missing call is almost always a bug waiting to surface (one of the variants got extended, the other didn't). Pull the shared sequence into a single function and pass the variant-specific bits as arguments.
5. **Constraints / extension points beat copies.** If a new caller needs *slightly* different behaviour, prefer (in order): (a) a new parameter on the existing function with a sensible default; (b) a small `enum` whose match lives inside the existing function; (c) a trait the existing function takes by reference. Forking the function into `do_foo_for_x` and `do_foo_for_y` is the last resort, and only when the divergence is structural enough that a shared body would be more confusing than two siblings.

Why this rule exists: every parallel implementation is a future bug. When the operator (or an agent) extends one of the two paths and forgets the other, the divergence shows up later as "feature works on workspace screen but not current-directory screen" — exactly the class of bug this project has hit before. Adding a parameter to one function makes both paths advance together; adding a second function makes them drift.

Examples of the kind of pattern this rule blocks (drawn from real findings):

- `sidebar_inputs_for_workspace` and `sidebar_inputs_for_current_dir` build the same `SidebarInputs` struct with overlapping body. Extending one to surface a new field while leaving the other untouched is the bug. The fix is to factor the divergent piece (picker-role resolution, role-binding presence) into helpers both functions call, not to add another sibling function for a third selection kind.
- `focused_block_still_scrollable` matching only `ManagerListRow::SavedWorkspace` for the global-mounts focus while the corresponding render path also accepts `ManagerListRow::CurrentDirectory`. The render and scrollability checks must read from the same selection-to-rows helper, otherwise the focus calculation lags behind the visible content.
- Adding a per-agent `LAUNCH=` block to `docker/runtime/entrypoint.sh` when an existing block already handles "agent X with optional credential mount" via a `case`. The new agent should extend the `case`, not duplicate the surrounding `seed_home_dir` / chmod / exec scaffolding.

When you do choose to duplicate (because the deltas are too structural for a shared body, or the shared body would defer the divergent decision to a runtime branch that hurts readability), leave a one-line comment on each copy naming the sibling and the *reason* divergence is preserved.

## Telemetry must be debuggable on demand without becoming noisy by default (hard rule)

**The standard log output (no debug flag) must be compact: lifecycle events, action breadcrumbs, and error paths only. The debug-flag log output must be a firehose detailed enough to reconstruct every operator keystroke, every protocol frame, every dispatch decision, and every render boundary. Both surfaces live in the same code, gated on the same flag — no `// TODO: remove debug logging` smell and no "rebuild with extra logging" round trip when an operator reports an issue.**

The shape is two-tier:

- **`clog!` (compact, always on).** Daemon start, session spawn/exit, child reap, PTY mutex poison, attach handshake outcomes, dialog dispatch arms that act (`Command`, `SpawnAgent`, `RenameTab`, `Dismiss`), pane/tab close, focus swap, error paths with the underlying errno. Quiet enough that a multi-hour session produces a log a human can scroll. Operators pasting these into bug reports get the timeline of *what happened*.
- **`cdebug!` (verbose, gated on `JACKIN_DEBUG=1`).** Every byte arriving from the client, every parser event with its dispatch state (dialog open / focused pane / prefix awaiting), every PTY write with the bytes and the destination session, every render frame size and reason, every dialog redraw, every per-tick state ticker. The macro skips the format + write entirely when the flag is off, so production runs pay nothing. With the flag on, the trace is detailed enough to localize "key X produced no visible effect" from the log alone — chunk line proves the byte reached the daemon, parser line proves it classified, dispatch line proves the routing decision, PTY-write line proves the byte hit the slave fd.

The flag is the same `JACKIN_DEBUG` the host's `--debug` flag sets — it flows into the container via `env_passthrough` in `daemon.rs` and is captured once at `logging::init()` time. New verbose telemetry sites should branch on `cdebug!`, not `clog!`. New compact telemetry sites should branch on `clog!`. Anything that fires more than ~10 times per minute under normal operation belongs on `cdebug!`.

When you find yourself adding "TEMPORARY logging to triage a regression", stop and convert it to `cdebug!` instead — the next bug report needs the same telemetry, and removing-and-readding-it on every regression cycle is exactly the loop this rule exists to break. The same applies to any other surface that grows a telemetry / tracing layer (the host CLI's `tui::tprintln`, the docs site's render warnings, the `runtime::launch` path): two tiers, debug-gated firehose, default compact.

When the current logs are insufficient to explain a complex or inconsistent behaviour, do not guess at the fix. First add durable `cdebug!` telemetry that captures the missing state, ask the operator to rerun the repro with `--debug`, and then make the fix from that new evidence. The only exception is when the missing state can be obtained safely from the live process or container without changing code; in that case inspect it directly and keep going.

The reason: operators can rarely reproduce on demand. When they hit something weird, they need to be able to paste a log that already has the answer — without rebuilding, without enabling extra instrumentation we forgot to ship, and without an extra round of "now please run it again with this added line". The host's `--debug` flag is the single switch that turns the firehose on; everything downstream honours it.

## Code comments — explain only what is not obvious

**Comments earn their place by encoding non-obvious WHY, not by narrating WHAT.** Well-named identifiers, type signatures, and surrounding code already say what the code does; a comment that repeats them is noise that pushes real signal off the screen and rots faster than the code it describes.

Comment when, and only when, one of these is true:

- The code looks suspicious, weird, or wrong on first read but is intentional. Name the constraint that forced it (TOCTOU, parser-bypass safety, ordering invariant, race window, kernel quirk, upstream bug).
- A non-local invariant is being preserved. Point at the invariant and the call site that depends on it.
- The shape could reasonably be written a different way. Name the trade-off that picked the current shape.
- The code interacts with an externally documented behaviour an unfamiliar reader would not predict (POSIX edge case, Docker daemon quirk, library footgun).

Do not comment when:

- The identifier name already says it (`fn provision_amp_auth` does not need `// Provision Amp auth`).
- The function signature already says it (`Result<T, io::Error>` does not need `// returns an io::Error on failure`).
- The control flow says it (`for x in items { … }` does not need `// loop over items`).
- The diff says it (`// renamed from foo`, `// added in PR #N`, `// previously did X`).

Style:

- Prefer one sentence to a paragraph. Trim until removing one more word would make the comment unclear.
- Lead with the constraint, not the code. "TOCTOU on settings.json: …" beats "We do this thing because there is a TOCTOU…".
- Drop "mirrors X" / "matches Y" parallel-structure narration — the parallel code structure already encodes that, and the cross-reference dates the moment one side drifts.
- Code blocks, function names, error strings, and CLI flag names are exact and never abbreviated; English prose around them is as terse as possible.

This rule applies to inline `//` comments, multi-line `/** */` / `///` / `//!` doc comments, and to test-method docstrings. Operator-facing surfaces (`clap` `--help` text, `eprintln!` lines the operator sees, README prose) follow the docs split rules that load under `docs/` instead — those are not "comments" in the sense above.

Do not memorialize old shapes in code comments ("formerly named X", "old location was Y"). The git history is the record of what changed; the code should describe only the current shape.
