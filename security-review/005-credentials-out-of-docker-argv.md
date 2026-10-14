# Plan 005: Route forwarded credentials through the docker child environment, not `docker run` argv

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `security-review/README.md`.
>
> **Drift check (run first)**: `git diff --stat a4761957d..HEAD -- crates/jackin-runtime/src/runtime/launch/launch_runtime.rs crates/jackin-core/src/env_model.rs`
> If either changed, compare the "Current state" excerpts against the live code
> before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none (independent of plan 002, though both concern credential handling)
- **Category**: security
- **Planned at**: commit `a4761957d`, 2026-07-09

## Why this matters

Every credential jackin❯ forwards into a capsule — GitHub tokens (`GH_TOKEN`,
`GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN`), agent API keys (`ANTHROPIC_API_KEY`,
`XAI_API_KEY`, `AMP_API_KEY`, `GROK_DEPLOYMENT_KEY`), and 1Password-resolved
secret values — is placed into the `docker run` **argv** as `-e KEY=VALUE`. A
process's argv is exposed at `/proc/<pid>/cmdline`, which is **world-readable**
(mode 0444). So for the lifetime of each `docker run`, any local process can read
every forwarded secret via `ps aux` / `/proc/<pid>/cmdline`. The diagnostics
redactor cannot cover this — it is OS-level process metadata, not a log sink.

The fix: pass secret-bearing env vars to docker through the **docker CLI
process's own environment** and reference them with a bare `-e KEY` (no value) in
argv. Docker forwards a bare `-e KEY` by reading `KEY` from its own environment.
`/proc/<pid>/environ` is mode 0400 (readable only by the owning uid), so the
secret leaves world-readable argv. Non-secret metadata (`JACKIN_*`) stays inline
as `-e KEY=value` — it is not sensitive and keeps existing argv-shape tests
stable.

## Current state

`crates/jackin-runtime/src/runtime/launch/launch_runtime.rs`:

- `run_args: Vec<&str>` is built at `:389`.
- Credential + metadata env vars are accumulated into `env_strings: Vec<String>`
  (`:565` onward) as `"KEY=value"`, including the resolved operator/role env
  (`:585-593`, which holds plaintext 1Password-resolved values and agent API
  keys), `GROK_DEPLOYMENT_KEY` (`:610`), and the GitHub tokens
  (`:633-638`, `:721-739`).
- They are pushed to argv verbatim at `:741-744`:

  ```rust
  for env_str in &env_strings {
      run_args.push("-e");
      run_args.push(env_str);
  }
  ```

- The run options are built at `:316-319` and used at `:956`:

  ```rust
  let docker_run_opts = RunOptions { quiet: !debug, ..RunOptions::default() };
  // ...
  let run_role = runner.run("docker", &run_args, None, &docker_run_opts);
  ```

- `RunOptions` (`crates/jackin-core/src/runner.rs:16-37`) derives `Clone` and has
  `pub extra_env: Vec<(String, String)>`. `ShellRunner::apply_run_opts`
  (`crates/jackin-docker/src/shell_runner.rs:60-62`) already applies `extra_env`
  to the spawned command's environment via `cmd.envs(...)`.

There are already three inconsistent "is this key a secret?" predicates in the
tree, and **none** of them catches `GROK_DEPLOYMENT_KEY**:
`shell_runner::is_sensitive_arg_key` (`:130-149`, matches `token/secret/…/apikey/…`
— `grokdeploymentkey` matches none), `secret_scrub::is_secret_key`, and the
`redact.rs` regex. Do **not** reuse `is_sensitive_arg_key` — it would leak
`GROK_DEPLOYMENT_KEY`. Introduce one correct predicate.

Existing test harness to copy: `crates/jackin-runtime/src/runtime/launch/tests.rs`
inspects the built docker command string (`run_cmd`) — e.g. line 2596
`assert!(!run_cmd.contains("-e OPENAI_API_KEY="))` and line 2096
`assert!(!run_cmd.contains("-e JACKIN_ROLE="))`. That harness is how you assert
argv shape.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core tests | `cargo nextest run -p jackin-core -E 'test(credential)'` | pass |
| Predicate + partition tests | `cargo nextest run -p jackin-runtime -E 'test(secret_env)'` | pass |
| Runtime tests | `cargo nextest run -p jackin-runtime` | all pass |
| Clippy | `cargo clippy -p jackin-core -p jackin-runtime --all-targets --locked -- -D warnings` | exit 0 |

## Scope

**In scope**:
- `crates/jackin-core/src/env_model.rs` (+ its `tests.rs`) — new `is_credential_env_key`
- `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs` (+ its sibling
  `tests.rs`) — partition helper, replace the `:741-744` loop, clone run opts

**Out of scope** (do NOT touch):
- `crates/jackin-docker/src/shell_runner.rs` — leave `is_sensitive_arg_key` (its
  job is log redaction, not argv routing). Do not merge predicates here.
- The `-e` pushes at `launch_runtime.rs:498-562` (JACKIN_* metadata) and the
  OTLP propagation block (`:752-762`) — those are non-secret; leave inline.
- Wire/protocol, the capsule side (docker forwards `-e KEY` transparently; the
  container sees the same env either way).

## Git workflow

- Branch: operator's active branch, or `fix/creds-out-of-argv`.
- One commit, conventional, signed. Example:
  `fix(runtime): pass forwarded credentials via docker env, not world-readable argv`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Add the canonical credential-key predicate in jackin-core

In `crates/jackin-core/src/env_model.rs`, add a pure predicate. It must catch the
substring families AND the explicit known-credential names the substring rule
misses:

```rust
/// True when an env var name designates a credential/secret whose value must
/// not appear in world-readable process argv. Superset of the substring
/// families plus explicit known-credential names (e.g. `GROK_DEPLOYMENT_KEY`,
/// which contains none of the substrings below).
#[must_use]
pub fn is_credential_env_key(key: &str) -> bool {
    const EXPLICIT: &[&str] = &["GROK_DEPLOYMENT_KEY"];
    let upper = key.to_ascii_uppercase();
    if EXPLICIT.contains(&upper.as_str()) {
        return true;
    }
    let squashed = upper.replace(['-', '_'], "");
    ["TOKEN", "SECRET", "PASSWORD", "PASSWD", "CREDENTIAL", "APIKEY", "ACCESSKEY", "PRIVATEKEY", "AUTH"]
        .iter()
        .any(|needle| squashed.contains(needle))
}
```

Add tests in `crates/jackin-core/src/env_model/tests.rs` (create the `#[cfg(test)]
mod tests;` declaration in `env_model.rs` if not present — check first; if the
crate already tests env_model elsewhere, add there):

```rust
#[test]
fn credential_keys_are_detected() {
    for key in [
        "GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "ANTHROPIC_API_KEY",
        "XAI_API_KEY", "AMP_API_KEY", "GROK_DEPLOYMENT_KEY", "CLAUDE_CODE_OAUTH_TOKEN",
    ] {
        assert!(super::is_credential_env_key(key), "{key} should be credential");
    }
}

#[test]
fn non_credential_keys_are_not_flagged() {
    for key in ["JACKIN_ROLE", "NO_PROXY", "PATH", "HOME", "TERM", "JACKIN_NETWORK_MODE"] {
        assert!(!super::is_credential_env_key(key), "{key} should not be credential");
    }
}
```

**Verify**: `cargo nextest run -p jackin-core -E 'test(credential)'` passes.

### Step 2: Add a pure partition helper in `launch_runtime.rs`

Add a helper that splits `env_strings` into (argv `-e` entries) and (secret
`extra_env` pairs). Keep it pure so it is unit-testable without the launch
pipeline:

```rust
/// Partition `KEY=value` env strings for `docker run`. Non-credential vars
/// stay inline as `-e KEY=value`. Credential vars are routed through the docker
/// process environment: a bare `-e KEY` in argv plus the `(KEY, value)` pair in
/// `extra_env`, keeping secret values out of world-readable `/proc/<pid>/cmdline`.
///
/// Returns `(argv_flags, secret_env)` where `argv_flags` is the flat sequence of
/// `-e` args to append and `secret_env` are the pairs to add to `RunOptions::extra_env`.
fn split_secret_env(env_strings: &[String]) -> (Vec<String>, Vec<(String, String)>) {
    let mut argv_flags = Vec::with_capacity(env_strings.len() * 2);
    let mut secret_env = Vec::new();
    for env_str in env_strings {
        match env_str.split_once('=') {
            Some((key, value)) if jackin_core::env_model::is_credential_env_key(key) => {
                argv_flags.push("-e".to_owned());
                argv_flags.push(key.to_owned());
                secret_env.push((key.to_owned(), value.to_owned()));
            }
            _ => {
                argv_flags.push("-e".to_owned());
                argv_flags.push(env_str.clone());
            }
        }
    }
    (argv_flags, secret_env)
}
```

Add tests in the sibling test module for `launch_runtime.rs` (declare
`#[cfg(test)] mod tests;` in `launch_runtime.rs` and create
`launch_runtime/tests.rs` if there is no sibling test module already — check
first):

```rust
#[test]
fn secret_env_routes_credentials_off_argv() {
    let env = vec![
        "JACKIN_ROLE=the-architect".to_owned(),
        "GH_TOKEN=ghp_fake000000000000000000".to_owned(),
        "GROK_DEPLOYMENT_KEY=fake-deploy-key".to_owned(),
    ];
    let (argv, secret) = super::split_secret_env(&env);
    // Non-credential stays inline:
    assert!(argv.windows(2).any(|w| w == ["-e", "JACKIN_ROLE=the-architect"]));
    // Credentials appear as a bare `-e KEY` (no value) in argv:
    assert!(argv.windows(2).any(|w| w == ["-e", "GH_TOKEN"]));
    assert!(argv.windows(2).any(|w| w == ["-e", "GROK_DEPLOYMENT_KEY"]));
    // …and their values are NOT anywhere in argv:
    assert!(!argv.iter().any(|a| a.contains("ghp_fake")));
    assert!(!argv.iter().any(|a| a.contains("fake-deploy-key")));
    // …but ARE in the child env:
    assert!(secret.iter().any(|(k, v)| k == "GH_TOKEN" && v == "ghp_fake000000000000000000"));
    assert!(secret.iter().any(|(k, v)| k == "GROK_DEPLOYMENT_KEY" && v == "fake-deploy-key"));
}
```

**Verify**: `cargo nextest run -p jackin-runtime -E 'test(secret_env)'` passes.

### Step 3: Wire the partition into the run-role invocation

Replace the loop at `launch_runtime.rs:741-744`:

```rust
for env_str in &env_strings {
    run_args.push("-e");
    run_args.push(env_str);
}
```

with:

```rust
let (secret_argv, secret_env) = split_secret_env(&env_strings);
for flag in &secret_argv {
    run_args.push(flag.as_str());
}
```

`secret_argv` is a local `Vec<String>` that must outlive `run_args` (which
borrows from it) up to the `runner.run(...)` call at `:956` — declare it in the
same scope as `env_strings` (it already is, since this replaces an in-scope loop).

Then, at the run-role call site (`:955-956`), use a **cloned** `RunOptions`
carrying the secret env, so only this invocation gets the credentials in its
environment (do not mutate the shared `docker_run_opts`, which is reused at
`:1000`):

```rust
let mut run_role_opts = docker_run_opts.clone();
run_role_opts.extra_env.extend(secret_env);
jackin_diagnostics::active_timing_started("capsule", "docker_run_role", Some(container_name));
let run_role = runner.run("docker", &run_args, None, &run_role_opts);
```

**Verify**: `cargo check -p jackin-runtime` exits 0.

### Step 4: Add an argv-shape assertion using the existing launch harness

Find the launch test that builds `run_cmd` and asserts `-e` shapes (near
`tests.rs:2596`). Add an assertion in the closest existing test that forwards a
credential (or add a sibling test modeled on it) proving a forwarded credential
appears as bare `-e GH_TOKEN` and its value is absent from `run_cmd`. If the
harness makes this impractical, the `split_secret_env` unit test in Step 2 is the
required regression guard and this step is best-effort — note that in your status.

**Verify**: `cargo nextest run -p jackin-runtime` — all pass.

### Step 5: Full check

**Verify**: `cargo nextest run -p jackin-runtime -p jackin-core` all pass; `cargo
clippy -p jackin-core -p jackin-runtime --all-targets --locked -- -D warnings`
exits 0.

## Test plan

- `jackin-core`: `credential_keys_are_detected` + `non_credential_keys_are_not_flagged`.
- `jackin-runtime`: `secret_env_routes_credentials_off_argv` (the pure-helper
  regression test — this is the load-bearing one) and, if feasible, an
  integration assertion via the `run_cmd` harness.
- Verification: `cargo nextest run -p jackin-runtime -p jackin-core` → all pass.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `grep -n 'fn is_credential_env_key' crates/jackin-core/src/env_model.rs` matches
- [ ] `grep -n 'fn split_secret_env' crates/jackin-runtime/src/runtime/launch/launch_runtime.rs` matches
- [ ] `grep -n 'run_args.push(env_str)' crates/jackin-runtime/src/runtime/launch/launch_runtime.rs` returns nothing (old loop gone)
- [ ] `cargo nextest run -p jackin-core -p jackin-runtime` exits 0; the new tests pass
- [ ] `cargo clippy -p jackin-core -p jackin-runtime --all-targets --locked -- -D warnings` exits 0
- [ ] No files outside the in-scope list modified (`git status`)
- [ ] `security-review/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- Any existing launch test asserts a **positive** `-e <CREDENTIAL_KEY>=value`
  argv shape (e.g. `assert!(run_cmd.contains("-e GH_TOKEN="))`) — that test
  encodes the very behavior being fixed; report it so the assertion can be
  updated deliberately rather than silently.
- `env_strings` is dropped or moved before line 956 (the borrow in `run_args`
  would not compile) — report the actual scope.
- `docker_run_opts` turns out NOT to be reused at `:1000` for a different
  command (then mutating it directly is fine and the clone is unnecessary) — or
  IS reused for something that must not receive these env vars (then the clone is
  mandatory; confirm which).
- The resolved env at `:585-593` turns out to already strip secrets elsewhere
  (belt-and-suspenders is still fine, but note it).

## Maintenance notes

- `is_credential_env_key` is now the canonical secret-key predicate. A strong
  follow-up (recorded in `security-review/README.md`) is to unify the three
  existing predicates (`redact`, `secret_scrub`, `is_sensitive_arg_key`) onto
  this one so they can never disagree — but that is a separate plan; this one
  only introduces the predicate and uses it for argv routing.
- Reviewer must confirm: (a) no secret value can reach argv for any key
  `is_credential_env_key` returns true for; (b) the container still receives the
  same env (docker forwards bare `-e KEY` from its process env); (c) the shared
  `docker_run_opts` used elsewhere did not inherit the secrets unintentionally.
- If a new credential env name is added later that is neither a substring match
  nor in `EXPLICIT`, add it to `EXPLICIT` — the predicate is the one place to
  update.
