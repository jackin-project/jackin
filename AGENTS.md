# AGENTS.md

Canonical entry point for AI agents in this repo. Primary branch `main`. Slim index: each rule stated in one or two lines, linked to topic file holding full rule, examples, rationale. **Read linked file before acting in that area** — one-liner tells you rule exists, not that you can skip detail.

Rules applying equally to humans live in topic files. Agent-only rules marked `(agent)`; largest agent-only clusters in [`BRANCHING.md`](BRANCHING.md), [`PULL_REQUESTS.md`](PULL_REQUESTS.md), and `AGENTS.md` under `.github/` (auto-loaded when working there).

> **`AGENTS.md` and `CLAUDE.md` auto-load — never link to them.** Each `CLAUDE.md` is a symlink to the `AGENTS.md` beside it (never a copy, never an `@import`); recreate one with `ln -s AGENTS.md CLAUDE.md`. Harness loads root `AGENTS.md` always and a subdirectory's `AGENTS.md` whenever you work in that subtree, so no file ever links to an `AGENTS.md` or `CLAUDE.md` — reference rule by topic or name governing directory in plain text. See [`RULES.md`](RULES.md).

## Always-on hard rules

These bind every session. Full rule and examples in linked file.

- **Stay on the active branch** (agent). Never commit to `main`; propose a branch, get operator confirmation first. One open PR per session means one branch. → [`BRANCHING.md`](BRANCHING.md)
- **Never mutate the host machine silently.** No host-side writes — dotfiles, repo `.git`, `~/.config/gh`, `~/.gitconfig`, host remotes, user repos — without explicit opt-in surfaced in launch summary. Reads fine. → [`HOST_AND_CONTAINER.md`](HOST_AND_CONTAINER.md)
- **Everything jackin' owns in a container lives under `/jackin/`.** No FHS roots (`/run`, `/var`, `/opt`, `/etc`, `/tmp/jackin*`). → [`HOST_AND_CONTAINER.md`](HOST_AND_CONTAINER.md)
- **Brand is `jackin'`** in prose — lowercase, trailing apostrophe. No-apostrophe spelling only for code identifiers, paths, commands, env vars. → [`RULES.md`](RULES.md)
- **Push every commit immediately** after creating it; never leave commits local-only. → [`COMMITS.md`](COMMITS.md)
- **Pre-release: breaking changes OK — no migration shims.** Exception: `config.toml`, per-workspace files, and `jackin.role.toml` versioned; schema changes ship five artifacts under one version bump per PR. → [`PRERELEASE.md`](PRERELEASE.md)

## Engineering

Cross-cutting code-craft rules, all in [`ENGINEERING.md`](ENGINEERING.md):

- **Prefer maintained crates** over hand-rolled parsers / serializers / format handlers / crypto.
- **Reuse before writing (DRY).** Extend or parameterise existing code; symmetric variants share one body.
- **Two-tier telemetry.** `clog!` compact and always-on; `cdebug!` firehose gated on `JACKIN_DEBUG=1`.
- **Comments explain non-obvious WHY,** never narrate WHAT.

Rust workspace specifics (module layout, lints, supply-chain hygiene) load from `AGENTS.md` under `crates/` when you work there.

## Pull requests, review, and docs gates

Read [`PULL_REQUESTS.md`](PULL_REQUESTS.md) before opening, iterating on, or merging a PR. Home for PR body shape, Verify-locally policy, solo-maintainer review model, and two pre-merge gates applying to **every** PR (even code-only ones):

- **Roadmap freshness** — move a roadmap item's status when a change ships, advances, or defers it.
- **Documentation as the source of truth** — update both user-facing and contributor-facing docs surfaces in same PR. Audience split is permanent, detailed by `AGENTS.md` under `docs/`.

Agent-only PR extras (merge authorization, base branch, force-push, CI-green-before-merge, squash format) live in `AGENTS.md` under `.github/`, auto-loading when you work there.

## Testing and validation

- Test runner, capsule render-conformance fixtures, operator `--debug` validation rule → [`TESTING.md`](TESTING.md).
- `jackin-capsule` smoke-test mandate → `AGENTS.md` under `.github/` (auto-loaded there).

## TUI

Read [TUI Design](docs/content/docs/reference/tui/index.mdx) section before any TUI change. Label, keybinding, list-modal rules in [`RULES.md`](RULES.md). Terminal-rendering code must live in a designated TUI directory:

| Surface | Directory |
|---|---|
| Shared components | `crates/jackin-tui/src/` |
| Capsule | `crates/jackin-capsule/src/tui/` |
| Host console | `src/console/tui/` |
| Lookbook | `crates/jackin-tui-lookbook/src/` |

Any cross-cutting TUI behaviour (focusability, navigation, color, modal sizing, hints) must be written into matching page under `docs/content/docs/reference/tui/` in same PR that adds it.

## Topic file index

Shared (humans and agents):

- [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md) — navigational map of codebase, docs site, Docker assets, CI.
- [`BRANCHING.md`](BRANCHING.md) — branch naming, feature-branch policy, rebase rule, force-push policy.
- [`COMMITS.md`](COMMITS.md) — Conventional Commits, DCO sign-off, push-after-commit, merge-readiness checks.
- [`PULL_REQUESTS.md`](PULL_REQUESTS.md) — PR flow, body shape, review, roadmap & docs gates, solo-maintainer model.
- [`TESTING.md`](TESTING.md) — test runner, fixtures, operator `--debug` validation.
- [`ENGINEERING.md`](ENGINEERING.md) — libraries, DRY, telemetry, comments.
- [`HOST_AND_CONTAINER.md`](HOST_AND_CONTAINER.md) — host-write ban, `/jackin/` container layout.
- [`PRERELEASE.md`](PRERELEASE.md) — breaking-change policy, schema versioning, changelog hold.
- [`RULES.md`](RULES.md) — doc-location convention, symlink rule, brand spelling, deprecations, TUI labels/keybindings/modals.
- [`DEPRECATED.md`](DEPRECATED.md) — ledger of deprecated APIs, CLIs, config values.
- [`TODO.md`](TODO.md) — small follow-ups, per-PR stale-docs checklist, code `TODO(<topic>)` convention.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contribution flow, DCO v1.1 text, license terms.

Several subdirectories carry their own `AGENTS.md` with rules scoped to that subtree — `.github/` (agent-only PR extras, GitHub Actions authoring), `docs/` (docs-site stack, TypeScript rule, three-audience split, roadmap audits), `crates/` (Rust module layout, lints, supply-chain hygiene), `crates/jackin-tui-lookbook/` (lookbook public-API-only rule), `docker/construct/` (prefer official package-manager installs). Not linked here on purpose: harness loads each one automatically when you work in its directory, in addition to this file.
