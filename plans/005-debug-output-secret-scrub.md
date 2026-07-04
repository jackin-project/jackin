# Plan 005: Scrub secret-shaped tokens from captured `--debug` command output (investigate + guard)

> **Executor instructions**: This is an **investigate-then-guard** plan (LOW confidence: a latent
> asymmetry, no confirmed leak). Do the investigation first; only add the scrubber if Step 1 confirms
> the mechanism. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-docker/src/shell_runner.rs crates/jackin-diagnostics/src/run.rs`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: security
- **Planned at**: commit `46511939d`, 2026-07-03

## Investigation finding

- `ShellRunner::log_captured_output` was reachable under `--debug` and wrote captured stdout/stderr lines without output scrubbing.
- `RunDiagnostics::write_command_output` stripped ANSI sequences but persisted sidecar stdout/stderr without token-shape scrubbing.
- Known credential-producing launch paths use `capture_secret`, so no confirmed current command was found leaking a credential into these sinks.
- The risk is latent defense-in-depth: a future debug command or build step that echoes a token could persist it to owner-only diagnostics files.
- The guard now scrubs captured output at both sinks before JSONL/debug or sidecar persistence.

## Why this matters

The launch path correctly redacts injected secrets at the **command-string** layer (`redact_env_args`
masks `-e KEY=VALUE`/`--env` argument values). But captured command **output** (stdout/stderr) is
persisted **verbatim** under `--debug` into `~/.jackin/data/diagnostics/runs/*.jsonl` (files are
`0o600`, so blast radius is the local user). No current command on this path was found to print a
credential, so this is a residual asymmetry, not a confirmed leak — but the two sinks (argv vs output)
should offer the same redaction guarantee so a *future* diagnostic subcommand or role build step that
echoes a token doesn't silently write it to disk.

## Current state

- `crates/jackin-docker/src/shell_runner.rs:84-102` — `redact_env_args` masks only argv env values.
- `crates/jackin-docker/src/shell_runner.rs:400-411` — `log_captured_output` writes captured
  stdout/stderr lines verbatim under `--debug`.
- `crates/jackin-diagnostics/src/run.rs:266-277` — sidecar `write_command_output` writes captured
  stdout/stderr with only ANSI-stripping (`strip_bytes`), no secret redaction; files created `0o600`
  (`run.rs:942-945`).
- An existing token-shape redaction test exists to reuse the patterns from:
  `crates/jackin-capsule/src/exec/tests.rs` (exercises PRIVATE-KEY / `ghp_` / `sk-` shapes).

## Scope

**In scope:** `crates/jackin-docker/src/shell_runner.rs`, `crates/jackin-diagnostics/src/run.rs`, and a
shared scrubber helper (place it where both can use it — likely `jackin-diagnostics` since it owns the
sink; confirm neither crate would create a dependency cycle with `grep` on their `Cargo.toml`).

**Out of scope:** the argv redaction (already correct); changing what commands are captured; the `0o600`
mode (already correct).

## Steps

### Step 1 (investigate): confirm the mechanism and decide scope

Read the three cited sites and confirm captured output is written without redaction. Grep the launch and
diagnostics paths for any command whose output plausibly contains a credential
(`grep -rn "capture\|exec_capture\|log_captured" crates/jackin-runtime/src crates/jackin-docker/src`).
Write a 5-line finding into this plan's row note: is there a *reachable* leak today, or is this purely
defense-in-depth? If purely latent, still proceed (cheap guard), but say so.

### Step 2 (guard): add a token-shape scrubber over captured output

Add a `scrub_secrets(&str) -> Cow<str>` helper matching well-known token shapes (PEM `-----BEGIN ... KEY-----`
blocks, `ghp_`/`gho_`/`ghs_`, `sk-…`, `AKIA…`, `op://` values, generic high-entropy `KEY=…` where KEY
matches a secret-ish name). Reuse the shapes already tested in
`crates/jackin-capsule/src/exec/tests.rs`. Route `log_captured_output` (shell_runner) and
`write_command_output` (diagnostics run) through it before writing to JSONL/sidecars. Scope patterns
tightly to avoid over-redacting normal diagnostic text.

**Verify**: `cargo check -p jackin-docker -p jackin-diagnostics --all-targets` → exit 0.

### Step 3: Tests

- Feed lines containing each token shape through `scrub_secrets`; assert the secret is masked and normal
  text is untouched.
- A test that `write_command_output`/`log_captured_output` masks a planted `ghp_`-shaped token.

**Verify**: `cargo nextest run -p jackin-docker -p jackin-diagnostics -E 'test(/scrub|redact/)'` → pass.

## Done criteria

- [ ] Step 1 finding recorded (reachable-today vs latent)
- [ ] Captured stdout/stderr passes through `scrub_secrets` before persistence
- [ ] Token-shape tests pass; normal text un-redacted
- [ ] `cargo clippy -p jackin-docker -p jackin-diagnostics -- -D warnings` exits 0
- [ ] `plans/README.md` row updated

## STOP conditions

- The scrubber would live in a crate that creates a dependency cycle — report and pick the other home.
- Over-redaction breaks existing diagnostics tests that assert exact captured text — narrow the patterns;
  if narrowing can't satisfy both, STOP and report the conflict.

## Maintenance notes

- Never reproduce a secret value in a test fixture — use synthetic shaped strings (`ghp_` + filler), as
  the existing exec test does.
- If a real leaking command is found in Step 1, escalate priority and note the specific command in the row.
