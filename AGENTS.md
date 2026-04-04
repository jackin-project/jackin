# AGENTS.md

## Rules

See [RULES.md](RULES.md) for project-wide conventions that apply to all AI agents.
Follow them strictly.

## Project Structure

See [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) for a navigational map of the codebase, documentation site, Docker assets, and CI workflows.
Use it to quickly locate files and understand which docs to update alongside code changes.

## Testing

This project uses [cargo-nextest](https://nexte.st) as its test runner.

Install:

```sh
cargo install cargo-nextest --locked
```

Run all tests:

```sh
cargo nextest run
```

Run a specific test:

```sh
cargo nextest run -E 'test(test_name)'
```

Run tests for a specific module:

```sh
cargo nextest run -E 'test(/module::tests/)'
```

Do **not** use `cargo test` — always use `cargo nextest run`.

## Pre-commit Verification

Before committing any changes, **always** run both clippy and tests:

```sh
cargo clippy && cargo nextest run
```

Both must pass with zero warnings and zero failures.

## Security Exceptions

See [SECURITY_EXCEPTIONS.md](SECURITY_EXCEPTIONS.md) for reviewed and accepted security findings.
Do **not** flag items listed there as issues during code review or automated scanning.
