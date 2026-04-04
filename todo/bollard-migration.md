# Migrate Docker CLI to Bollard API Client

**Status**: Deferred — incremental migration is the pragmatic path

## Problem

All Docker operations use `ShellRunner` which shells out to the `docker` CLI. Error handling relies on string-matching stderr text (e.g., `"No such container"`, `"No such network"` in `is_missing_cleanup_error()`), which is brittle across Docker versions and locales.

## Why It Matters

- String-matched error detection can break silently on Docker updates or non-English locales
- No structured error codes from CLI — only exit code 1 for most failures
- The `bollard` crate provides a typed Rust Docker API client over Unix socket/TCP with proper HTTP status codes (e.g., 404 for "not found" vs 500 for real errors)

## Open Security Finding

Finding #5 from `SECURITY_REVIEW_FINDINGS.md` — `is_missing_cleanup_error()` still string-matches Docker error messages. This migration would resolve it.

## Options

1. **Full migration to `bollard`**: Replace all `ShellRunner` Docker calls with `bollard` API calls. Significant refactor.

2. **Incremental migration**: Start with cleanup/lifecycle operations (where string matching is most problematic), keep CLI for `docker build` and `docker run -it` (where interactive TTY is needed).

## Related Files

- `src/docker.rs` — `ShellRunner`, command execution
- `src/runtime.rs` — `is_missing_cleanup_error()`, all Docker lifecycle calls
