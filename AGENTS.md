# AGENTS.md

This repository uses `main` as its primary branch. This file is the canonical home for rules and restrictions that apply only to AI agents. Rules that apply equally to human contributors and agents live in topic-specific files linked under **Shared conventions** below.

## Branch discipline: stay on the active branch (hard rule, agent-only)

**Never create a new branch when an existing feature branch or open PR is already in scope for the session.**

- At session start, check `git branch --show-current` and `gh pr list --head <current-branch>`. If there is an open PR, all work goes on that branch for the entire session.
- If the current branch is `main`, work must not be committed there. Before making any change, choose a short, lowercase, hyphen-separated branch name that follows [`BRANCHING.md`](BRANCHING.md) (`feature/`, `fix/`, `refactor/`, or `chore/`), then ask the operator to confirm it: "This is on `main`. I suggest `<branch-name>` for this work. Should I create it?" Do not proceed until the operator confirms that name or provides a replacement.
- If you believe a piece of work belongs on a *different* branch from the active one, stop and ask: "This feels like it belongs on a separate branch — should I create one, or keep it on `<current-branch>`?" Default to staying on the active branch unless the operator says otherwise.
- Never push to a remote branch other than the one the active local branch should track. If the local branch name differs from the remote PR branch (e.g. local `pr-435` vs remote `fix/capsule-agent-wheel-scroll`), resolve the tracking with `git push origin HEAD:<remote-branch-name>` — do not create a new remote branch.
- If you accidentally create a wrong remote branch, delete it with `git push origin --delete <wrong-branch>` immediately after correcting the push.

## Brand spelling (agent-only)

In prose, the product and project are always spelled `jackin'`: lowercase with the trailing apostrophe. Do not write `Jackin`, `Jackin'`, or bare `jackin` when referring to the brand, the product, or the project in normal text. Use the no-apostrophe spelling only for literal commands, binaries, crates, packages, environment variables, config keys, file paths, labels, selectors, URLs, and code identifiers, such as `jackin`, `jackin-capsule`, `JACKIN_DEBUG`, `~/.jackin/`, and `jackin.role.toml`. If the apostrophe makes a possessive or sentence awkward, rewrite the sentence instead of dropping it.

## Project staffing: solo maintainer (agent-only)

jackin' has exactly one human contributor — the operator. There is no second reviewer available, and GitHub does not let a PR author approve their own pull request. This shapes several rules and tooling choices that an agent might otherwise expect to default differently:

- Branch protection on `main` does **not** require an approving review (`required_approving_review_count = 0` in `jackin-github-terraform`). Do not propose raising it without a concrete plan for how a second human will review every PR.
- "Get a second pair of eyes" is not an available pre-merge step. Pre-merge confidence comes from CI, the path-aware aggregator status checks, the strict up-to-date branch policy, and the agent following the rules in `PULL_REQUESTS.md` — not from a human reviewer the operator does not have.
- Multi-agent review (running `code-reviewer` / `comment-analyzer` / `silent-failure-hunter` / etc. in parallel before requesting merge) is the substitute for the missing second human. Treat those review passes as load-bearing rather than optional polish.
- For irreversible or high-blast-radius changes, prefer asking the operator to confirm one more time over assuming the green CI run is sufficient. The cost of pausing 30 seconds is much lower than the cost of a bad merge that an absent second reviewer would have caught.

This rule retires when the project gains additional human reviewers.

## Project status: pre-release (agent-only)

jackin' has no released version — it is a proof-of-concept. **Breaking changes are expected and acceptable.** When schemas change (on-disk state layout, CLI flags, role/agent shapes outside the three versioned files listed below), do not write migration code, compatibility shims, fallback parsers for old field names, "tolerant ignore + warn" handlers, or deprecation warnings. Make the new shape the only shape; let stale data fail with the standard parser error.

`config.toml`, per-workspace files at `~/.config/jackin/workspaces/<name>.toml`, and `jackin.role.toml` are exceptions: all three are versioned schemas (`CURRENT_CONFIG_VERSION`, `CURRENT_WORKSPACE_VERSION`, `CURRENT_MANIFEST_VERSION` in `src/config/migrations.rs` and `src/manifest/migrations.rs`). Any PR that touches `AppConfig`, `WorkspaceConfig`, `RoleManifest`, `HooksConfig`, or any other type whose serde representation lives in one of those three files must ship with five artifacts:

1. Bump of the relevant `CURRENT_*_VERSION`.
2. A migration step in the corresponding registry (`CONFIG_MIGRATIONS`, `WORKSPACE_MIGRATIONS`, `MANIFEST_MIGRATIONS`).
3. A new fixture directory under `tests/fixtures/migrations/<file-kind>/from-<predecessor-version>/` containing `meta.toml`, `before.toml`, and `after.toml`. The fixture harness in `tests/migration_fixtures.rs` walks every supported `from_version` on every CI run and asserts that the migrated output (a) parses successfully against the current serde schema, (b) carries the declared `target_version` stamp, and (c) that `after.toml` itself parses and carries the same stamp. This guarantees a delayed operator landing on the current version after several bumps can still load their config — the chain is the regression guard.
4. Re-bake of every existing fixture's `after.toml` so it walks through the new step too. The fixture for the oldest supported `from_version` is the load-bearing test for users delayed by months — its diff is the proof the new chain is composable.
5. A new entry at the top of the **Timeline** section in `docs/src/content/docs/reference/schema-versions.mdx` with date, predecessor, fixture link, summary, and a before/after example.

A non-additive change (renamed field, removed field, type change, added enum variant, restructured table) without these five artifacts is incomplete; reviewers block merge until they appear or the change is reshaped to be additive (new optional field with a serde default). Operator config and per-workspace files migrate automatically during `AppConfig::load_or_init` at startup; role authors migrate local manifests on a desktop with `jackin role migrate <role-repo-path>`, while CI and Renovate-style automation migrate manifests with the small standalone `jackin-role migrate <role-repo-path>` binary.

Do not memorialize old shapes in code comments ("formerly named X", "old location was Y") or in documentation files outside the changelog. The git history is the record of what changed; the code should describe only the current shape.

**One schema version bump per PR, targeting the next version after `main`.** A PR that touches versioned schemas must introduce exactly one version bump — the version immediately following the current `CURRENT_*_VERSION` on `main` at the time the PR is opened. A single PR may add multiple fields, rename multiple fields, and affect multiple file kinds (config, workspace, manifest), but all of those changes land under that one version bump. Adding a second bump inside the same PR is a sign the changes should be in separate PRs, not stacked versions. If `main` advances while the PR is in flight and claims the PR's target version, the PR must rebase to use the new next version — never introduce a gap or a skip. This rule prevents the pattern where a PR introduces `v1alpha5` (with partial changes) and `v1alpha6` (with the remainder): that forces operators through two sequential migrations for what is logically one PR's worth of work and creates a stale intermediate version that no one ever ships at.

This rule retires when jackin' ships its first tagged release.

## Never mutate the host machine silently (hard rule)

**The operator's host machine is their property. jackin' must never write to host-side state — files, git config, repo `.git/config`, `.git/refs`, `~/.gitconfig`, `~/.config/gh/`, `~/.claude/`, `~/.codex/`, the host's git remotes, or any user repository — without an explicit, opt-in, surfaced-in-the-launch-summary action. All "smoothing" jackin' does to make a container work belongs *inside the container*.**

This is non-negotiable across schemas, design proposals, roadmap items, runtime behavior, and PR descriptions. Examples of what this rule blocks:

- Rewriting a host repository's `origin` remote from SSH to HTTPS because "the container can't push via SSH." The fix belongs in the container's `--global` git config and credential helper, not the host repo's `.git/config`.
- Running `gh auth setup-git` on the host as part of a `jackin` command. The container can run it; the host stays untouched.
- Editing `~/.gitconfig`, `~/.ssh/config`, or any user dotfile during a launch, refresh, or "fix it for me" path. Suggest the change in the launch summary; do not apply it.
- Force-pushing, fetching, pulling, or pruning on the host's git repo as a side effect of provisioning. The only host-side git commands the CLI runs today are the ones the operator explicitly opted into (`git_pull_on_entry`, `worktree add` under `isolation = "worktree"`), and those stay scoped to the workspace's mounted repos.
- Writing the host's `~/.config/gh/hosts.yml` from the container's in-session `gh auth login`. In-container token rotation must not flow back to the host without an explicit operator-controlled bidirectional-sync opt-in (tracked under the [GitHub CLI auth strategy](docs/src/content/docs/reference/roadmap/github-cli-auth-strategy.mdx) follow-ups).

**Read paths against the host are fine.** `gh auth token --hostname github.com`, parsing `~/.config/gh/hosts.yml`, reading `~/.claude.json`, looking up the host's git user.email — all read-only. The forbidden direction is host-side *writes* triggered by jackin' without explicit operator opt-in.

When a design proposal or roadmap item mentions doing anything to the host, the proposal must call it out under a "Host-side effects" section, the implementing PR must surface the action in the launch summary, and the change must be opt-in (config flag, CLI flag, or operator confirmation prompt). PRs that touch the host silently must be rejected at review.

The reason: the host machine is where the operator works. Surprise mutations break their flow, surface as inexplicable bugs in terminals outside jackin', and erode trust in the orchestrator. The whole point of jackin' is to absorb the messiness inside containers so the host stays clean.

## Container path convention: everything jackin' owns lives under `/jackin/` (hard rule)

**Every path jackin' creates, mounts, or owns inside a role container must live under `/jackin/`.** No FHS-borrowed top-level directories (`/run/jackin/`, `/var/lib/jackin/`, `/opt/jackin/`, `/etc/jackin/`), no scattered locations the operator has to discover one-by-one. An operator who runs `ls /jackin/` inside any role container must see the complete map of jackin-owned state in one place.

Concrete layout (current and going forward):

- `/jackin/runtime/` — entrypoint script, hooks, agent-launch scaffolding (read-only image content).
- `/jackin/state/` — runtime markers (`hooks/setup-once.done`, etc.) written during first-boot.
- `/jackin/default-home/` — image-baked default home contents copied into `/home/agent/` on first boot.
- `/jackin/run/` — runtime sockets, pidfiles, and other ephemeral runtime state. The jackin-capsule daemon socket lives at `/jackin/run/jackin.sock`.
- `/jackin/{claude,codex,amp,kimi,opencode}/` — agent credential mounts.
- `/jackin/host/` — read-only views of host paths exposed into the container.

This rule is non-negotiable across runtime code, Dockerfile templates, design proposals, roadmap items, and PR descriptions. Examples of what this rule blocks:

- New container paths under `/run/`, `/var/`, `/opt/`, `/srv/`, `/etc/`, or any other FHS root — even when they "feel natural" for the asset type (a Unix socket under `/run/` is the most common drift). The container is a single-purpose jackin runtime; the FHS layout is not what makes the in-container experience legible.
- Per-container scratch paths under `/tmp/jackin*` or `/var/run/jackin*`. If it's jackin-owned and ephemeral, it goes under `/jackin/run/`.
- Hard-coded paths in role-specific scripts that bypass the convention because "this is just for one role." Roles author their own files under `/home/agent/` or in the workspace; jackin-owned content stays under `/jackin/`.

**Host-side state is a separate convention.** The host root for jackin-owned paths is `~/.jackin/` (per the Never-mutate-host-silently rule above), with its own subdirectory layout (`~/.jackin/{data,cache,sockets,roles,run}/`). The container and host conventions are deliberately parallel but not identical — the host follows operator-home dotfile customs (`~/.<tool>/`), the container follows the single-root `/jackin/` convention. A bind-mount that maps `~/.jackin/sockets/<container>/` to `/jackin/run/` is the canonical shape.

When you find yourself wanting to introduce a new container-side path, place it under `/jackin/` first, then justify in the PR description if a real constraint forces an exception (e.g. a third-party tool that hard-codes `/run/<thing>` and cannot be relocated). PRs that introduce a top-level jackin-owned path outside `/jackin/` without an exception note must be rejected at review and the path moved.

The reason: a flat, single-root convention makes the in-container surface debuggable. An operator who wants to know "what does jackin do to my container?" can `ls /jackin/` and see the answer; without the rule the answer is "grep through every Dockerfile, every entrypoint, every roadmap doc." The rule also makes future cleanup straightforward — `rm -rf /jackin` removes every jackin-owned artifact, leaving the base image intact for whatever rebuild the operator wants next.

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

## Telemetry must be debuggable on demand without becoming noisy by default (hard rule)

**The standard log output (no debug flag) must be compact: lifecycle events, action breadcrumbs, and error paths only. The debug-flag log output must be a firehose detailed enough to reconstruct every operator keystroke, every protocol frame, every dispatch decision, and every render boundary. Both surfaces live in the same code, gated on the same flag — no `// TODO: remove debug logging` smell and no "rebuild with extra logging" round trip when an operator reports an issue.**

The shape is two-tier:

- **`clog!` (compact, always on).** Daemon start, session spawn/exit, child reap, PTY mutex poison, attach handshake outcomes, dialog dispatch arms that act (`Command`, `SpawnAgent`, `RenameTab`, `Dismiss`), pane/tab close, focus swap, error paths with the underlying errno. Quiet enough that a multi-hour session produces a log a human can scroll. Operators pasting these into bug reports get the timeline of *what happened*.
- **`cdebug!` (verbose, gated on `JACKIN_DEBUG=1`).** Every byte arriving from the client, every parser event with its dispatch state (dialog open / focused pane / prefix awaiting), every PTY write with the bytes and the destination session, every render frame size and reason, every dialog redraw, every per-tick state ticker. The macro skips the format + write entirely when the flag is off, so production runs pay nothing. With the flag on, the trace is detailed enough to localize "key X produced no visible effect" from the log alone — chunk line proves the byte reached the daemon, parser line proves it classified, dispatch line proves the routing decision, PTY-write line proves the byte hit the slave fd.

The flag is the same `JACKIN_DEBUG` the host's `--debug` flag sets — it flows into the container via `env_passthrough` in `daemon.rs` and is captured once at `logging::init()` time. New verbose telemetry sites should branch on `cdebug!`, not `clog!`. New compact telemetry sites should branch on `clog!`. Anything that fires more than ~10 times per minute under normal operation belongs on `cdebug!`.

When you find yourself adding "TEMPORARY logging to triage a regression", stop and convert it to `cdebug!` instead — the next bug report needs the same telemetry, and removing-and-readding-it on every regression cycle is exactly the loop this rule exists to break. The same applies to any other surface that grows a telemetry / tracing layer (the host CLI's `tui::tprintln`, the docs site's render warnings, the `runtime::launch` path): two tiers, debug-gated firehose, default compact.

When the current logs are insufficient to explain a complex or inconsistent behaviour, do not guess at the fix. First add durable `cdebug!` telemetry that captures the missing state, ask the operator to rerun the repro with `--debug`, and then make the fix from that new evidence. The only exception is when the missing state can be obtained safely from the live process or container without changing code; in that case inspect it directly and keep going.

The reason: operators can rarely reproduce on demand. When they hit something weird, they need to be able to paste a log that already has the answer — without rebuilding, without enabling extra instrumentation we forgot to ship, and without an extra round of "now please run it again with this added line". The host's `--debug` flag is the single switch that turns the firehose on; everything downstream honours it.

**Debug output never reaches a rich full-screen TUI (hard rule).** When a rich alternate-screen surface owns the terminal — the launch/loading cockpit, the workspace console, the in-container multiplexer — `--debug` must not print a single line over it. The firehose is written **only** to the diagnostics run file under `~/.jackin/data/diagnostics/runs/<run-id>.jsonl`, and external-command output is captured (never streamed to the screen) for the duration. The screen stays the clean rich experience; the evidence lands in the file. The mechanism is the `rich_surface_active` flag plus the active diagnostics run: `emit_debug_line` / `active_debug` route to the run file, and the command runner suppresses live streaming whenever `--debug` is on or a rich surface is active. Streaming child output straight to `stdout`/`stderr` while a rich surface is up is a bug; route it through the diagnostics run instead.

So the operator can retrieve that file, a `--debug` invocation surfaces the **run id on the plain CLI before the TUI starts** (`[jackin] debug mode — run id: jk-run-…`, plus the file path), and for TUI-bearing commands on an interactive terminal it gates entry behind an `Enter` press so the id can be copied before the screen switches to the alternate buffer. After exit the operator — or the agent they hand the file to — uses that id to find the artifact. Never trade the clean rich surface for inline debug spew: when you need more evidence, add `cdebug!` / diagnostics sites that write to the run file, not prints to the screen.

## Reuse before writing — DRY (hard rule)

**Before writing new code, check whether something close enough already exists. If yes, extend, parameterise, or wrap it instead of writing a parallel copy. If no, write the new thing in a shape future callers can reuse.**

This applies to *every* layer of the codebase: render helpers, state-derivation functions, parsing/validation, CLI argument structs, docker mount-list builders, TUI block layout, dialog dispatch, OS abstraction, hook scripts, build scripts. Whenever you are about to write a function whose behaviour is "mostly the same as `<other_function>` but with one branch flipped" — stop, refactor the existing one to accept the difference, and use it.

Concrete checks before adding new code:

1. **`grep` for the verb, the noun, and the surrounding nouns.** "I need to render global mounts" → `rg 'global_mount' src/`. "I need to derive cwd from a manifest" → `rg 'fn .*cwd|manifest.*cwd' src/`. Multi-noun phrases catch helpers named for adjacent concepts. If the search returns one match, read it before writing a new function; if it returns multiple, the duplication this rule prevents has already started — flag it in the PR even if your change is narrow.
2. **Walk the call sites of the closest match.** If the existing function has two or three callers that pass different arguments, the right move is usually to add a parameter (or a small enum) and route every caller through the same function. If existing call sites would have to grow ugly to share, *say so in a comment* on the new function and keep the duplication explicit so the next reader can decide.
3. **Look one directory up.** Helpers often live in `<feature>/mod.rs`, `console/manager/render/mod.rs`, `runtime/mod.rs`, `instance/mod.rs`. If `<feature>/sub.rs` is about to grow a private helper that doesn't depend on `sub.rs`-only state, the helper belongs in the parent `mod.rs` (or in a sibling `helpers.rs`) where the next feature in the same family can use it.
4. **Symmetric variants demand symmetric implementations.** When two functions handle "current dir" vs "saved workspace" — or "agent" vs "shell" — or "Linux" vs "macOS" — the per-variant deltas should be data, not control flow. If both paths run `f()` + `g()` + `h()` but in slightly different order or with one missing call, the missing call is almost always a bug waiting to surface (one of the variants got extended, the other didn't). Pull the shared sequence into a single function and pass the variant-specific bits as arguments.
5. **Constraints / extension points beat copies.** If a new caller needs *slightly* different behaviour, prefer (in order): (a) a new parameter on the existing function with a sensible default; (b) a small `enum` whose match lives inside the existing function; (c) a trait the existing function takes by reference. Forking the function into `do_foo_for_x` and `do_foo_for_y` is the last resort, and only when the divergence is structural enough that a shared body would be more confusing than two siblings.

Why this rule exists: every parallel implementation is a future bug. When the operator (or an agent) extends one of the two paths and forgets the other, the divergence shows up later as "feature works on workspace screen but not current-directory screen" — exactly the class of bug this project has hit before. The two functions look so close that the missing call site reads as obviously correct in isolation, and only a side-by-side diff or an end-to-end test catches it. Adding a parameter to one function makes both paths advance together; adding a second function makes them drift.

Examples of the kind of pattern this rule blocks (drawn from real findings):

- `sidebar_inputs_for_workspace` and `sidebar_inputs_for_current_dir` build the same `SidebarInputs` struct with overlapping body. Extending one to surface a new field while leaving the other untouched is the bug. The fix is to factor the divergent piece (picker-role resolution, role-binding presence) into helpers both functions call, not to add another sibling function for a third selection kind.
- `focused_block_still_scrollable` matching only `ManagerListRow::SavedWorkspace` for the global-mounts focus while the corresponding render path also accepts `ManagerListRow::CurrentDirectory`. The render and scrollability checks must read from the same selection-to-rows helper, otherwise the focus calculation lags behind the visible content.
- Adding a per-agent `LAUNCH=` block to `docker/runtime/entrypoint.sh` when an existing block already handles "agent X with optional credential mount" via a `case`. The new agent should extend the `case`, not duplicate the surrounding `seed_home_dir` / chmod / exec scaffolding.

When you do choose to duplicate (because the deltas are too structural for a shared body, or the shared body would defer the divergent decision to a runtime branch that hurts readability), leave a one-line comment on each copy naming the sibling and the *reason* divergence is preserved. That way the next reader sees the trade-off up front and does not "fix" the duplication by sweeping both copies into a confusing common path.

This rule applies equally to Rust source, Dockerfile snippets in `docker/`, shell scripts under `docker/runtime/`, `.zshrc` / `config.fish` / hook scripts under `docker/construct/`, `justfile` recipes, CI workflow steps under `.github/workflows/`, and TypeScript helpers under `docs/scripts/`. The cost of one good helper is much smaller than the cost of three slightly-different copies and the bugs that follow from extending only one of them.

## Changelog (agent-only)

**Do not add entries to `CHANGELOG.md` until the first tagged release.**

The changelog exists to communicate breaking changes and new features to *users of released software*. Before a first release there are no such users, and every change is implicitly "unreleased" — adding entries now creates noise that will need to be cleaned up before the release and may give a false impression that the project follows a stable release cadence.

When the first release is being cut, the operator will explicitly ask for the changelog to be populated. Until then, leave `CHANGELOG.md` unchanged.

## Roadmap freshness (agent-only)

Before marking any PR ready to land, and again whenever the operator asks to merge a PR, check whether the change ships, advances, defers, or invalidates anything under `docs/src/content/docs/reference/roadmap/`. If yes, update the roadmap item's `**Status**`, related files, and implementation notes in the same PR, then update `docs/src/content/docs/reference/roadmap.mdx` so the item appears only in the correct overview section.

Do this check even when the PR is mostly code, tests, CI, or rule changes. The roadmap is an operator-facing source of truth, not a retrospective cleanup task. A feature that lands without moving its roadmap item leaves stale planning docs behind and should be treated as an incomplete PR. If a merge request reveals stale roadmap state, stop before merging, update the roadmap and PR description, and only then continue with normal merge verification.

Run the sidebar and overview audits documented in `docs/AGENTS.md` after any roadmap status or file movement. If a roadmap item is partially shipped, keep it in **Partially implemented** with the remaining phases named; do not duplicate the same item under **Planned**.

Roadmap pages are for planned, researched, designed, deferred, or remaining work. Once behavior ships, move the operator details to normal docs (`guides/`, `commands/`, `reference/`) and replace roadmap detail with a short status plus canonical-doc links. Do not keep long copied implementation walkthroughs in roadmap items after the feature is documented elsewhere.

**Fully-resolved roadmap items must be retired in the same PR that ships the last piece, not left behind as `Status: Resolved` pages.** The full retirement procedure — confirming there is no remaining work, auditing inbound links, splitting page detail between user-facing and contributor-facing docs, replacing the page with a single Completed bullet, and re-running the sidebar / overview / docs verification audits — lives in [`PULL_REQUESTS.md`](PULL_REQUESTS.md) under "Retire fully-resolved roadmap items in the same PR." Read it before deciding to keep or delete a `Resolved` page.

## Documentation as the source of truth (agent-only)

**The published docs site is the spec.** Every feature jackin' ships must be described from two angles, and both must be kept current in the same PR that lands the change:

- **User-facing docs** (the *Operator* and *Role Authoring* sidebar groups: `getting-started/`, `guides/`, `commands/`, `developing/`) describe **what jackin' does from outside the binary**. They answer "if I run this command or set this config, what will happen?" without naming on-disk paths the operator never edits, internal Rust types, or implementation steps. A reader following only the user-facing docs must be able to use the feature successfully.
- **Contributor-facing docs** (the *Internals* sidebar group: `reference/architecture.mdx`, `reference/configuration.mdx`, `reference/codebase-map.mdx`, `reference/claude-token-orchestrator.mdx`, `reference/schema-versions.mdx`, `reference/tui-design-decisions.mdx`, plus active items under `reference/roadmap/`) describe **how jackin' is built**. On-disk layout, struct/enum/function names, design decisions, trade-offs, file paths under `src/`, and links into the source tree all live here. This surface is what an agent or contributor reads before changing code, and it is what they update when their change makes the description stale.

Both surfaces are load-bearing. If an operator-visible behaviour ships without an update to the user-facing docs, the feature is not actually shipped — operators have no way to learn it exists or how to invoke it. If an internal change ships without an update to the contributor-facing docs, the next agent reading the internals page is debugging against a stale spec.

**Before marking any PR ready to merge — and again whenever the operator asks to merge it — re-verify every change against the published docs and update both surfaces in the same PR.** Concretely:

1. Walk the diff and ask, for each change: does this change what an operator sees, types, or relies on? If yes, the matching `guides/`, `commands/`, `getting-started/`, or `developing/` page must be updated in this PR.
2. Walk the diff again and ask: does this change a struct, enum, function name, on-disk path, schema version, design decision, or any other detail an internals page describes? If yes, the matching `reference/` page must be updated in this PR.
3. Apply the **Roadmap freshness** rule above: status updates, sidebar/overview audits, and retire-when-fully-resolved.
4. Run `bun run build`, `bun run check:repo-links`, `bunx tsc --noEmit`, and `bun test` from `docs/`. A docs change that doesn't compile or breaks repo-file references is incomplete.

Do not split a feature PR from its docs PR by default. The docs land with the code that makes them true; landing them later means the docs are wrong for the gap, and the gap is exactly when other agents and operators will read them. The exception is the explicit "docs-only follow-up" pattern named in `PULL_REQUESTS.md`, which the operator authorizes per case.

**Audience-correct placement is not optional.** When you find yourself wanting to put a TOML schema fragment, on-disk path, or struct name on a user-facing page, the placement is wrong — that detail goes on the matching internals page, and the user-facing page links to it. When you find yourself wanting to write `jackin foo --bar` operator instructions on an internals page, that block belongs in the `commands/` page, and the internals page links out. The split is what lets each audience trust their surface; mixing them weakens both.

This rule does not retire when jackin' ships its first release; the audience split is permanent. The roadmap-retirement portion of this rule and the **Roadmap freshness** rule retire only when there are no roadmap items left to maintain.

## Push every commit immediately (hard rule)

**After creating any commit on a feature branch, push to the remote in the same turn.** Do not leave commits local-only. The operator checks progress via the GitHub PR, not via a local checkout they may not have. A commit that exists only on the agent's local clone is invisible to CI, invisible to the operator, and lost if the session ends.

The single exception is a chain of rapid fixup commits made in one turn — push once after the last commit in the chain, not after every intermediate one. But the push must still happen before the turn ends.

This rule applies even when the operator did not explicitly ask to push — finishing the work includes making it visible.

## Pull requests — two reading surfaces

All pull-request rules live in two places, split by audience:

- [`PULL_REQUESTS.md`](PULL_REQUESTS.md) — shared PR flow, body-shape spec, Verify-locally template, docs-only PR requirements, roadmap-retirement procedure. Both humans and agents read this. Start here.
- [`.github/AGENTS.md`](.github/AGENTS.md) — agent-only extras: per-PR merge authorization, base-branch requirement, force-push policy, body-construction shell-quoting rules, iteration vs merge-readiness behavior, CI-green-before-merge, title/description reconciliation, squash-merge format with PR-number + trailers, and the `jackin-capsule` smoke-test mandate. Also covers GitHub Actions workflow authoring (mise-only installs, job-level env scope, publish gating, smoke-testing push-only jobs).

Discovery flow Claude Code uses: `.github/CLAUDE.md` is `@AGENTS.md`, so the file auto-loads whenever the working directory is under `.github/` — including when reading the PR template.

[`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) is the canonical body shape with inline guidance. Copy it as the starting point and fill in the placeholders.

## Commit Attribution (agent-only)

Every commit created or edited by an AI agent in this repository must include one `Co-authored-by` trailer for each AI agent involved in that commit. A commit touched by one agent has one agent trailer; a commit created by one agent and later amended, reused, or repaired by another agent preserves the original trailer and adds the later agent's trailer. The trailer identifies the **agent tool**, not the underlying model — do not add `Co-authored-by: Claude` or `Co-authored-by: Codex` merely because another agent used one of those vendors' models under the hood.

Squash merge commits follow the same attribution model at PR scope: include one `Co-authored-by` trailer for each AI agent listed on the pull request's commits, as described in "PR squash merge messages".

Until the listed agents emit their trailers automatically, trailers must be added by hand when creating or amending the commit.

**Trailers by agent:**

- **Claude** (Claude Code CLI, or any Claude-API coding agent used directly):

  ```text
  Co-authored-by: Claude <noreply@anthropic.com>
  ```

- **Codex** (OpenAI Codex CLI):

  ```text
  Co-authored-by: Codex <codex@openai.com>
  ```

- **Amp** (Sourcegraph Amp, regardless of underlying model):

  ```text
  Co-authored-by: Amp <amp@ampcode.com>
  ```

- **OpenCode** (OpenCode CLI, regardless of underlying GLM model):

  ```text
  Co-authored-by: opencode-agent[bot] <opencode-agent[bot]@users.noreply.github.com>
  ```

  This matches the GitHub App identity used by OpenCode when it creates commits, as defined in the `anomalyco/opencode` repository. Do not alter the format — match what OpenCode emits.

Amp may additionally emit an `Amp-Thread-ID:` metadata trailer; that is acceptable alongside the single `Co-authored-by: Amp` trailer because the thread ID identifies the conversation, not a second agent.

If you are uncertain which agent is creating the commit, ask — the trailer is how the operator tracks which agent produced which change, and wrong attribution is worse than no attribution.

## Code review & automated scanning (agent-only) — see `PULL_REQUESTS.md`

All review-time rules — accepted-exception catalog, design-principles check, applying review fixes, iterating on operator feedback — live in [`PULL_REQUESTS.md`](PULL_REQUESTS.md). Read that file before reviewing or iterating on any PR.

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

This rule applies to inline `//` comments, multi-line `/// `/// `//!` doc comments, and to test-method docstrings. Operator-facing surfaces (`clap` `--help` text, `eprintln!` lines the operator sees, README prose) follow the docs split rules in `docs/AGENTS.md` instead — those are not "comments" in the sense above.

## Walking the operator through local validation (agent-only)

When walking the operator through manual validation of a jackin' feature (smoke testing a PR, reproducing a bug, executing a PR test plan), every `jackin <subcommand>` invocation in the recipe MUST include `--debug`. That includes `cargo run --bin jackin -- <subcommand> --debug` while iterating from a checkout.

The `--debug` flag captures every external command the CLI issues (`docker`, `git`, `id`, etc.) along with their output, plus the `[jackin debug ...]` instrumentation, into the diagnostics run file (`~/.jackin/data/diagnostics/runs/<run-id>.jsonl`) — never onto a rich TUI (see the telemetry hard rule above). At the start of a `--debug` run the CLI prints the run id and that file path before any TUI takes the screen. This makes the run triage-able by the agent: when something doesn't behave as expected, the operator shares the run id (or the file) and the agent reads the structured JSONL to localize the issue without guessing. Ask the operator for the run id printed at start, not for a pasted terminal scrollback.

Do not list `git diff --check` as PR verification. It is not a meaningful
acceptance check for jackin' PRs; prefer targeted commands that exercise the
changed behavior plus CI.

For user smoke tests, suggest `jackin console` first, and prefer the
`the-architect` role over `agent-smith` when a role choice is needed. From a
checkout, the usual operator-facing smoke command is:

```bash
cargo run --bin jackin -- console --debug
```

Use `jackin load` only when the PR specifically needs the load CLI path. In
that case, prefer:

```bash
cargo run --bin jackin -- load the-architect . --debug
```

Do not add `--no-intro` to debug smoke commands. Debug mode already suppresses
the intro by design, so `--debug --no-intro` is redundant noise.

If the operator reports unexpected behavior from a clean (non-debug) run, the FIRST follow-up should be to ask them to rerun with `--debug` and share the run id printed at start (the agent then reads the run's JSONL file) before proposing fixes.

This does not apply to:

- Inspection commands the operator runs (`pgrep`, `pmset`, `cat`, `ls`) — those aren't `jackin` invocations.
- Production recommendations or scripted automation (debug output is too noisy for those).

## Testing `jackin-capsule` changes locally (agent-only) — see `.github/AGENTS.md`

All rules for the `jackin-capsule` smoke-test mandate — the eval one-shot build invocation, the `ensure_available` resolution order, the required PR Verify-locally block — live in [`.github/AGENTS.md`](.github/AGENTS.md) under the `## jackin-capsule PRs (hard rule)` section. Read that file before opening or reviewing a PR that touches `crates/jackin-capsule/`.

## TUI design decisions (agent-only)

All TUI design rules — navigation conventions, W3C ARIA Tabs pattern, focusability, component reuse, color palette, modal sizing, scroll semantics, hint/footer rules, and more — live in [`docs/src/content/docs/reference/tui-design-decisions.mdx`](docs/src/content/docs/reference/tui-design-decisions.mdx).

**Read that document before implementing any TUI change.** When a new decision is made (operator explains what should change and why), add it there immediately, not here.

## Shared conventions

Rules in the files below apply to everyone working in the repo — human and agent:

- [PULL_REQUESTS.md](PULL_REQUESTS.md) — pull-request shape, body authoring rules, iteration / refresh / merge policy, docs-only PR requirements.
- [RULES.md](RULES.md) — documentation-location convention (no project rules in tool-specific files).
- [BRANCHING.md](BRANCHING.md) — branch naming, feature-branch policy, what never to commit to `main`.
- [COMMITS.md](COMMITS.md) — Conventional Commits format, DCO sign-off, pre-commit verification commands.
- [TESTING.md](TESTING.md) — test runner setup and commands.
- [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) — navigational map of the codebase, documentation site, Docker assets, and CI workflows.
- [DEPRECATED.md](DEPRECATED.md) — ledger of deprecated APIs, CLIs, config values, and usage patterns that are still supported but should eventually be removed.
- [TODO.md](TODO.md) — small follow-up items (especially upstream dependencies waiting on a fix), the per-PR stale-docs checklist, and the convention for code-level `TODO(<topic>)` markers that link back to this file.
- [CONTRIBUTING.md](CONTRIBUTING.md) — contribution flow, DCO v1.1 text, and license terms for external contributors.
- [.github/AGENTS.md](.github/AGENTS.md) — GitHub Actions workflow authoring rules: mise-only tool installation, env-var scope, publish gating, and smoke-testing push-only jobs.
