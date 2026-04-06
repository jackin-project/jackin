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

## Project Structure

See [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) for a navigational map of the codebase, documentation site, Docker assets, and CI workflows.
Use it to quickly locate files and understand which docs to update alongside code changes.

## Testing

See [TESTING.md](TESTING.md) for test runner setup, commands, and pre-commit verification requirements.

## Security Exceptions

See [SECURITY_EXCEPTIONS.md](SECURITY_EXCEPTIONS.md) for reviewed and accepted security findings.
Do **not** flag items listed there as issues during code review or automated scanning.
