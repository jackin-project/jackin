# AGENTS.md

Primary branch: `main`.

> **CLAUDE.md = symlink to AGENTS.md beside it** — recreate: `ln -s AGENTS.md CLAUDE.md`. See [RULES.md](RULES.md).

## Hard rules (always-on)

- **Stay on active branch.** Never commit `main`; propose branch, get operator confirm. One PR per session = one branch. → [CONTRIBUTING.md](CONTRIBUTING.md)
- **No silent host writes.** No dotfiles, `.git`, `~/.config/gh`, `~/.gitconfig`, host remotes — without explicit opt-in surfaced in launch summary. Reads OK. → [HOST_AND_CONTAINER.md](HOST_AND_CONTAINER.md)
- **Container paths under `/jackin/` only.** No FHS roots (`/run`, `/var`, `/opt`, `/etc`, `/tmp/jackin*`). → [HOST_AND_CONTAINER.md](HOST_AND_CONTAINER.md)
- **Brand: `jackin'`** in prose (lowercase, trailing apostrophe). Code/paths/commands/env vars: no apostrophe. → [RULES.md](RULES.md)
- **Every commit: sign `-s`, push immediately.** → [CONTRIBUTING.md](CONTRIBUTING.md)
- **Pre-release: breaking changes OK, no migration shims.** Exception: `config.toml`, per-workspace files, `jackin.role.toml` versioned; schema changes ship 5 artifacts under one version bump per PR. → [PRERELEASE.md](PRERELEASE.md)

## Commits & Branching

@CONTRIBUTING.md

### Stay on active branch

**Never create new branch when existing feature branch or open PR in scope.**

- Session start: `git branch --show-current` + `gh pr list --head <branch>`. Open PR → all work that branch.
- On `main`: propose `<prefix/name>`, ask: "This is on `main`. I suggest `<branch>`. Should I create it?" Wait for confirm.
- Work feels like different branch: ask first. Default: stay on active branch.
- Never push to remote branch other than what local tracks. Local `pr-435` vs remote `fix/foo` → `git push origin HEAD:<remote-branch>`. No extra remote branches.

### Force pushes

Never `git push --force` / `--force-with-lease` without explicit operator approval for that branch/PR in current conversation.

Normal pushes (new commits): no approval needed. History rewrites (amend DCO, rebase, squash): ask first, name branch + reason. Prefer follow-up commit unless operator requests rewrite.

`git fetch -f` OK — updates local remote-tracking refs only, not remote branch.

### Sync active branch with main

Default to a normal merge commit when bringing `main` into the active PR branch. Do not rebase or rewrite history unless the operator explicitly asks for that branch.

```sh
git fetch origin main
git merge --no-ff origin/main -m "chore(merge): sync main into <branch>"
git push
```

Merge-sync commit subjects must still follow Conventional Commits. Use `chore(merge): sync main into <branch>` unless a more specific non-release maintenance type is clearly better.

### Push after every commit

Push immediately after every `git commit`. No local-only commits.

```sh
git commit -s -m "feat(scope): description"
git push
```

Exception: explicit operator instruction to hold.

## Engineering

[ENGINEERING.md](ENGINEERING.md):

- Prefer maintained crates over hand-rolled parsers / serializers / format handlers / crypto.
- Reuse before writing (DRY). Extend or parameterise; symmetric variants share one body.
- Two-tier telemetry: `clog!` compact always-on; `cdebug!` firehose gated on `JACKIN_DEBUG=1`.
- Comments: non-obvious WHY only — never narrate WHAT.

Rust workspace specifics → `AGENTS.md` under `crates/`.

## PRs, review, docs gates

Read [PULL_REQUESTS.md](PULL_REQUESTS.md) before opening/iterating/merging. Pre-merge gates on **every** PR:

- **Roadmap freshness** — update roadmap item status when change ships/advances/defers.
- **Docs as source of truth** — update user-facing + contributor-facing docs same PR.

Agent PR extras (base branch, force-push, CI-green, squash format) → `.github/AGENTS.md`.

## Testing

- Runner, render-conformance fixtures, `--debug` validation → [TESTING.md](TESTING.md).
- `jackin-capsule` smoke-test mandate → `.github/AGENTS.md`.

## TUI

Read [TUI Design](docs/content/docs/reference/tui/index.mdx) before any TUI change. Label/keybinding/modal rules → [RULES.md](RULES.md).

| Surface | Directory |
|---|---|
| Shared components | `crates/jackin-tui/src/` |
| Capsule | `crates/jackin-capsule/src/tui/` |
| Host console | `src/console/tui/` |
| Lookbook | `crates/jackin-tui-lookbook/src/` |

Cross-cutting TUI behaviour (focusability, navigation, color, modal sizing, hints) → matching page under `docs/content/docs/reference/tui/` same PR.

## Topic file index

- [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) — codebase, docs site, Docker assets, CI map.
- [CONTRIBUTING.md](CONTRIBUTING.md) — branching, Conventional Commits, DCO, push-after-commit, merge-readiness, license.
- [PULL_REQUESTS.md](PULL_REQUESTS.md) — PR flow, body shape, review, roadmap & docs gates, solo-maintainer model.
- [TESTING.md](TESTING.md) — test runner, fixtures, `--debug` validation.
- [ENGINEERING.md](ENGINEERING.md) — libraries, DRY, telemetry, comments.
- [HOST_AND_CONTAINER.md](HOST_AND_CONTAINER.md) — host-write ban, `/jackin/` container layout.
- [PRERELEASE.md](PRERELEASE.md) — breaking-change policy, schema versioning, changelog hold.
- [RULES.md](RULES.md) — doc-location convention, symlink rule, brand spelling, TUI labels/keybindings/modals.
- [DEPRECATED.md](DEPRECATED.md) — deprecated APIs, CLIs, config values.
- [TODO.md](TODO.md) — follow-ups, per-PR stale-docs checklist, `TODO(<topic>)` convention.
