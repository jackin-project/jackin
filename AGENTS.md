# AGENTS.md

This repository uses `main` as its primary branch. This file is the canonical home for rules and restrictions that apply only to AI agents. Rules that apply equally to human contributors and agents live in topic-specific files linked under **Shared conventions** below.

## Project staffing: solo maintainer (agent-only)

Jackin has exactly one human contributor — the operator. There is no second reviewer available, and GitHub does not let a PR author approve their own pull request. This shapes several rules and tooling choices that an agent might otherwise expect to default differently:

- Branch protection on `main` does **not** require an approving review (`required_approving_review_count = 0` in `jackin-github-terraform`). Do not propose raising it without a concrete plan for how a second human will review every PR.
- "Get a second pair of eyes" is not an available pre-merge step. Pre-merge confidence comes from CI, the path-aware aggregator status checks, the strict up-to-date branch policy, and the agent following the rules in `PULL_REQUESTS.md` — not from a human reviewer the operator does not have.
- Multi-agent review (running `code-reviewer` / `comment-analyzer` / `silent-failure-hunter` / etc. in parallel before requesting merge) is the substitute for the missing second human. Treat those review passes as load-bearing rather than optional polish.
- For irreversible or high-blast-radius changes, prefer asking the operator to confirm one more time over assuming the green CI run is sufficient. The cost of pausing 30 seconds is much lower than the cost of a bad merge that an absent second reviewer would have caught.

This rule retires when the project gains additional human reviewers.

## Project status: pre-release (agent-only)

Jackin has no released version — it is a proof-of-concept. **Breaking changes are expected and acceptable.** When schemas change (on-disk state layout, CLI flags, role/agent shapes outside the three versioned files listed below), do not write migration code, compatibility shims, fallback parsers for old field names, "tolerant ignore + warn" handlers, or deprecation warnings. Make the new shape the only shape; let stale data fail with the standard parser error.

`config.toml`, per-workspace files at `~/.config/jackin/workspaces/<name>.toml`, and `jackin.role.toml` are exceptions: all three are versioned schemas (`CURRENT_CONFIG_VERSION`, `CURRENT_WORKSPACE_VERSION`, `CURRENT_MANIFEST_VERSION` in `src/config/migrations.rs` and `src/manifest/migrations.rs`). Any PR that touches `AppConfig`, `WorkspaceConfig`, `RoleManifest`, `HooksConfig`, or any other type whose serde representation lives in one of those three files must ship with five artifacts:

1. Bump of the relevant `CURRENT_*_VERSION`.
2. A migration step in the corresponding registry (`CONFIG_MIGRATIONS`, `WORKSPACE_MIGRATIONS`, `MANIFEST_MIGRATIONS`).
3. A new fixture directory under `tests/fixtures/migrations/<file-kind>/from-<predecessor-version>/` containing `meta.toml`, `before.toml`, and `after.toml`. The fixture harness in `tests/migration_fixtures.rs` walks every supported `from_version` on every CI run and asserts byte-equal output, so a delayed operator landing on the new schema after several version bumps still upgrades cleanly.
4. Re-bake of every existing fixture's `after.toml` so it walks through the new step too. The fixture for the oldest supported `from_version` is the load-bearing test for users delayed by months — its diff is the proof the new chain is composable.
5. A new entry at the top of the **Timeline** section in `docs/src/content/docs/reference/schema-versions.mdx` with date, predecessor, fixture link, summary, and a before/after example.

A non-additive change (renamed field, removed field, type change, restructured table) without these five artifacts is incomplete; reviewers block merge until they appear or the change is reshaped to be additive (new optional field with a serde default). Operator config files migrate automatically on startup; per-workspace files migrate on first load; role manifests migrate via `jackin-validate --migrate <role-repo-path>`.

Do not memorialize old shapes in code comments ("formerly named X", "old location was Y") or in documentation files outside the changelog. The git history is the record of what changed; the code should describe only the current shape.

This rule retires when jackin ships its first tagged release.

## Never mutate the host machine silently (hard rule)

**The operator's host machine is their property. Jackin must never write to host-side state — files, git config, repo `.git/config`, `.git/refs`, `~/.gitconfig`, `~/.config/gh/`, `~/.claude/`, `~/.codex/`, the host's git remotes, or any user repository — without an explicit, opt-in, surfaced-in-the-launch-summary action. All "smoothing" jackin does to make a container work belongs *inside the container*.**

This is non-negotiable across schemas, design proposals, roadmap items, runtime behavior, and PR descriptions. Examples of what this rule blocks:

- Rewriting a host repository's `origin` remote from SSH to HTTPS because "the container can't push via SSH." The fix belongs in the container's `--global` git config and credential helper, not the host repo's `.git/config`.
- Running `gh auth setup-git` on the host as part of a `jackin` command. The container can run it; the host stays untouched.
- Editing `~/.gitconfig`, `~/.ssh/config`, or any user dotfile during a launch, refresh, or "fix it for me" path. Suggest the change in the launch summary; do not apply it.
- Force-pushing, fetching, pulling, or pruning on the host's git repo as a side effect of provisioning. The only host-side git commands jackin runs today are the ones the operator explicitly opted into (`git_pull_on_entry`, `worktree add` under `isolation = "worktree"`), and those stay scoped to the workspace's mounted repos.
- Writing the host's `~/.config/gh/hosts.yml` from the container's in-session `gh auth login`. In-container token rotation must not flow back to the host without an explicit operator-controlled bidirectional-sync opt-in (tracked under the [GitHub CLI auth strategy](docs/src/content/docs/reference/roadmap/github-cli-auth-strategy.mdx) follow-ups).

**Read paths against the host are fine.** `gh auth token --hostname github.com`, parsing `~/.config/gh/hosts.yml`, reading `~/.claude.json`, looking up the host's git user.email — all read-only. The forbidden direction is host-side *writes* triggered by jackin without explicit operator opt-in.

When a design proposal or roadmap item mentions doing anything to the host, the proposal must call it out under a "Host-side effects" section, the implementing PR must surface the action in the launch summary, and the change must be opt-in (config flag, CLI flag, or operator confirmation prompt). PRs that touch the host silently must be rejected at review.

The reason: the host machine is where the operator works. Surprise mutations break their flow, surface as inexplicable bugs in their non-jackin terminals, and erode trust in the orchestrator. The whole point of jackin is to absorb the messiness inside containers so the host stays clean.

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

## Changelog (agent-only)

**Do not add entries to `CHANGELOG.md` until the first tagged release.**

The changelog exists to communicate breaking changes and new features to *users of released software*. Before a first release there are no such users, and every change is implicitly "unreleased" — adding entries now creates noise that will need to be cleaned up before the release and may give a false impression that the project follows a stable release cadence.

When the first release is being cut, the operator will explicitly ask for the changelog to be populated. Until then, leave `CHANGELOG.md` unchanged.

## Roadmap freshness (agent-only)

Before marking any PR ready to land, and again whenever the operator asks to merge a PR, check whether the change ships, advances, defers, or invalidates anything under `docs/src/content/docs/reference/roadmap/`. If yes, update the roadmap item's `**Status**`, related files, and implementation notes in the same PR, then update `docs/src/content/docs/reference/roadmap.mdx` so the item appears only in the correct overview section.

Do this check even when the PR is mostly code, tests, CI, or rule changes. The roadmap is an operator-facing source of truth, not a retrospective cleanup task. A feature that lands without moving its roadmap item leaves stale planning docs behind and should be treated as an incomplete PR. If a merge request reveals stale roadmap state, stop before merging, update the roadmap and PR description, and only then continue with normal merge verification.

Run the sidebar and overview audits documented in `docs/AGENTS.md` after any roadmap status or file movement. If a roadmap item is partially shipped, keep it in **Partially implemented** with the remaining phases named; do not duplicate the same item under **Planned**.

Roadmap pages are for planned, researched, designed, deferred, or remaining work. Once behavior ships, move the operator details to normal docs (`guides/`, `commands/`, `reference/`) and replace roadmap detail with a short status plus canonical-doc links. Do not keep long copied implementation walkthroughs in roadmap items after the feature is documented elsewhere.

## Pull requests (agent-only) — see `PULL_REQUESTS.md`

All rules for opening, iterating on, refreshing, reviewing, and merging pull requests live in [`PULL_REQUESTS.md`](PULL_REQUESTS.md). **Read that file before opening any PR.** It covers:

- Per-PR merge authorization (agents never merge without explicit "merge it" confirmation).
- Force-push authorization (agents never rewrite an existing remote branch without explicit operator approval).
- Required PR body shape (Summary / hard-rule callout / What's deferred / Verify locally / Migration notes).
- "Verify locally" templates, including the `export TIRITH=0` line that lets multi-line pastes survive the `tirith` paste scanner.
- PR-body authoring rules — no hard-wrap, no verbosity / duplication, no deployed-docs links, no mechanical CI-shaped checks.
- PR-body refresh policy: refresh **on operator request** or at merge-readiness, not after every iteration commit.
- Documentation-only PR requirements (run the docs site locally + bold-URL-per-page format).
- CI-must-be-green-before-merge, title/description reconciliation, squash-merge messages with PR-number + trailers.
- Workflow / CI changes — third-party-CLI env vars must be scoped to the consuming job (workflow-level `env:` leaks into every job and breaks tools that read those vars as default-selection); changes to push-only / main-only / `workflow_run`-gated jobs must be smoke-tested via `gh workflow run --ref <feature-branch>` before merge because PR-time CI never exercises them; registry / production publish steps must hard-gate on `main` so PRs and feature-branch dispatches verify-build but never publish.

[`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) is the canonical body shape with inline guidance. Copy it as the starting point and fill in the placeholders.

## Commit Attribution (agent-only)

Every commit created by an AI agent in this repository must include **exactly one** `Co-authored-by` trailer identifying the agent that made the commit. The trailer identifies the **agent tool**, not the underlying model — **never stack multiple agent trailers on one commit** (for example, an Amp-generated commit must not also carry `Co-authored-by: Claude` or `Co-authored-by: Codex` just because Amp used one of those vendors' models under the hood).

Exception: a squash merge commit may include multiple `Co-authored-by` trailers when multiple AI agents materially contributed to the PR. In that case, include one trailer per contributing agent as described in "PR squash merge messages".

Until the listed agents emit their trailers automatically, the trailer must be added by hand when creating or amending the commit.

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

When walking the operator through manual validation of a jackin feature (smoke testing a PR, reproducing a bug, executing a PR test plan), every `jackin <subcommand>` invocation in the recipe MUST include `--debug`. That includes `cargo run --bin jackin -- <subcommand> --debug` while iterating from a checkout.

The `--debug` flag prints every external command jackin issues (`docker`, `git`, `id`, etc.) along with their captured output, plus jackin's own `[jackin debug ...]` instrumentation. This makes the operator's terminal output triage-able by the agent: when something doesn't behave as expected, the operator can paste the full debug log and the agent can localize the issue without guessing.

Do not list `git diff --check` as PR verification. It is not a meaningful
acceptance check for jackin PRs; prefer targeted commands that exercise the
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

If the operator reports unexpected behavior from a clean (non-debug) run, the FIRST follow-up should be to ask them to rerun with `--debug` and paste the full output before proposing fixes.

This does not apply to:

- Inspection commands the operator runs (`pgrep`, `pmset`, `cat`, `ls`) — those aren't jackin invocations.
- Production recommendations or scripted automation (debug output is too noisy for those).

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
