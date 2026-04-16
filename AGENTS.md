# AGENTS.md

## Rules

See [RULES.md](RULES.md) for project-wide conventions that apply to all AI agents.
Follow them strictly.

## Branching

All new features and bug fixes must be developed on a dedicated feature branch.
Never commit directly to `main`.

- Create a branch from `main` before starting work: `git checkout -b feature/<short-description>`
- Use prefixes that match the type of change: `feature/`, `fix/`, `refactor/`, `chore/`
- Keep branch names short, lowercase, and hyphen-separated
- Merge back to `main` via pull request after review

## Codex Commit Attribution

Until Codex supports automatic commit trailers, every commit created by the
Codex agent in this repository must include this exact trailer:

```text
Co-authored-by: Codex <codex@openai.com>
```

Add it manually when creating or amending Codex-generated commits.

## Project Structure

See [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) for a navigational map of the codebase, documentation site, Docker assets, and CI workflows.
Use it to quickly locate files and understand which docs to update alongside code changes.

## Pre-commit Verification

Before committing **any** change, run all three checks and ensure zero warnings and zero failures:

```sh
cargo fmt -- --check && cargo clippy && cargo nextest run
```

If formatting fails, run `cargo fmt` to fix it, then re-run the checks.

See [TESTING.md](TESTING.md) for test runner setup, commands, and additional details.

## Security Exceptions

See [REVIEW_STATUS.md](REVIEW_STATUS.md) for active review findings and
accepted security exceptions.
Do **not** flag items listed in its "Accepted Exceptions" section as issues
during code review or automated scanning.
