# Testing

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

## Merge-readiness Verification

Do not run formatting, clippy, and the full test suite before every commit by
default. Run the full verification suite when a pull request is ready to be
merged, or earlier only when the operator explicitly asks for it:

```sh
cargo fmt -- --check && cargo clippy -- -D warnings && cargo nextest run
```

All three must pass with zero warnings and zero failures.
If formatting fails, run `cargo fmt` to fix it.
