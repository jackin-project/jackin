# Plan 006: Validate role-repo git URLs (scheme allowlist + argument-injection guard) before clone

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving on. If
> anything in "STOP conditions" occurs, stop and report — do not improvise. When
> done, update the status row in `security-review/README.md`.
>
> **Drift check (run first)**: `git diff --stat a4761957d..HEAD -- crates/jackin-runtime/src/runtime/repo_cache.rs`
> If it changed, compare the "Current state" excerpts against the live code
> before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: S-M
- **Risk**: LOW
- **Depends on**: none
- **Category**: security
- **Planned at**: commit `a4761957d`, 2026-07-09

## Why this matters

jackin❯ clones operator-configured role repositories with `git clone <url>`. The
URL comes from `[roles.<name>].git` config and reaches `git clone` with **no
scheme allowlist, no `--` end-of-options separator, and no leading-dash
rejection**. Git's `ext::` transport runs an arbitrary shell command during
clone (`git clone 'ext::sh -c <cmd>'` = command execution); `file://` / local
paths read arbitrary host repos; a URL beginning with `-` is parsed by `git` as
an option (argument injection). Today the operator-configured, trust-gated origin
makes this defense-in-depth rather than a live hole — but the guard's absence is
certain, and a URL reaching this path from any less-trusted layer (shared
workspace/role config, a future import flow) would be exploitable. Cheap,
high-value hardening.

## Current state

`crates/jackin-runtime/src/runtime/repo_cache.rs`:

- First-install clone (`:274-282`) — note it does not even set
  `GIT_TERMINAL_PROMPT=0`:

  ```rust
  runner.run("git", &["clone", git_url, &temp_repo_path], None, &git_run_opts).await
  ```

- `clone_args` builder (`:320-325`) — no `--` separator:

  ```rust
  fn clone_args<'a>(git_url: &'a str, dest: &'a str, branch: Option<&'a str>) -> Vec<&'a str> {
      branch.map_or_else(
          || vec!["clone", git_url, dest],
          |b| vec!["clone", "-b", b, git_url, dest],
      )
  }
  ```

- `normalize_github_url` (`:307-315`) only rewrites GitHub SSH→HTTPS; every other
  URL shape passes through unvalidated.

Repo conventions: errors in this file use a `RepoError` enum
(`map_err(RepoError::CloneFailed)`, `RepoError::InvalidRoleRepo`) — add a new
variant in that same enum for the validation failure. `RunOptions.extra_env:
Vec<(String,String)>` is applied to the git process environment by `ShellRunner`
(so `GIT_ALLOW_PROTOCOL` set there reaches git).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Targeted tests | `cargo nextest run -p jackin-runtime -E 'test(git_url)'` | pass |
| Crate tests | `cargo nextest run -p jackin-runtime` | all pass |
| Clippy | `cargo clippy -p jackin-runtime --all-targets --locked -- -D warnings` | exit 0 |

## Scope

**In scope**: `crates/jackin-runtime/src/runtime/repo_cache.rs` and its sibling
test module (`repo_cache/tests.rs` — create + declare `#[cfg(test)] mod tests;`
if absent; check first).

**Out of scope**: the trust-confirmation flow (`confirm_role_trust`) elsewhere —
this plan adds a syntactic guard, not a trust-policy change. Do not touch other
crates.

## Git workflow

- Branch: operator's active branch, or `fix/git-clone-url-allowlist`.
- One commit, conventional, signed. Example:
  `fix(runtime): validate role-repo git URL scheme and reject arg injection before clone`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Add a `validate_git_url` guard

Add a pure validator in `repo_cache.rs`. Reject anything that is not an
`https`/`ssh`/`git`(+`git+ssh`) URL, reject a leading `-`, and reject the
dangerous transports explicitly:

```rust
/// Reject role-repo URLs that could execute code or inject git arguments before
/// they reach `git clone`: `ext::`/`fd::`/`file://`/local paths (arbitrary
/// command/host-file access) and leading-dash URLs (argument injection). Allows
/// the standard remote transports only.
fn validate_git_url(url: &str) -> Result<(), RepoError> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err(RepoError::UnsafeGitUrl(url.to_owned()));
    }
    // scp-style: git@host:path — allowed (host present, no scheme).
    let is_scp_style = !trimmed.contains("://") && trimmed.contains('@') && trimmed.contains(':');
    let scheme_ok = trimmed.starts_with("https://")
        || trimmed.starts_with("ssh://")
        || trimmed.starts_with("git://")
        || trimmed.starts_with("git+ssh://");
    if scheme_ok || is_scp_style {
        // Explicitly reject transports smuggled inside an otherwise-ok string.
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("ext::") || lower.starts_with("fd::") || lower.starts_with("file://") {
            return Err(RepoError::UnsafeGitUrl(url.to_owned()));
        }
        Ok(())
    } else {
        Err(RepoError::UnsafeGitUrl(url.to_owned()))
    }
}
```

Add the `UnsafeGitUrl(String)` variant to the `RepoError` enum in this file (find
the enum — it already holds `CloneFailed`/`InvalidRoleRepo`; match their
`#[error(...)]` attribute style). Message e.g.
`#[error("refusing to clone role repo from unsafe git URL: {0}")]`.

**Verify**: `cargo check -p jackin-runtime` exits 0.

### Step 2: Call the validator and normalize, before every clone

At each clone site, validate the **post-normalization** URL immediately before
the `runner.run("git", …clone…)` call. For the first-install clone at `:274`,
insert before it:

```rust
let git_url = normalize_github_url(git_url);
validate_git_url(&git_url)?;
```

(Adjust to the local binding — if `git_url` is a `&str` param, shadow it with the
normalized `String` and pass `&git_url`.) Do the same at any other `runner.run(…
"clone" …)` site and inside/around `clone_args` callers. Grep for every
`"clone"` usage in the file and guard each.

### Step 3: Add `--` and `GIT_ALLOW_PROTOCOL` defense-in-depth

In `clone_args` (`:320-325`), insert `--` before the positional URL so a URL
that slipped a leading dash can never be read as an option:

```rust
fn clone_args<'a>(git_url: &'a str, dest: &'a str, branch: Option<&'a str>) -> Vec<&'a str> {
    branch.map_or_else(
        || vec!["clone", "--", git_url, dest],
        |b| vec!["clone", "-b", b, "--", git_url, dest],
    )
}
```

Apply the same `--`-before-positionals shape to the inline first-install clone at
`:277` (`&["clone", "--", git_url.as_str(), &temp_repo_path]`).

Set `GIT_ALLOW_PROTOCOL=https:ssh:git:git+ssh` on the git run options via
`extra_env` so git itself refuses any other transport as a final backstop. Build
`git_run_opts` (`:270-273`) with:

```rust
let git_run_opts = RunOptions {
    quiet: !debug,
    extra_env: vec![
        ("GIT_ALLOW_PROTOCOL".to_owned(), "https:ssh:git:git+ssh".to_owned()),
        ("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()),
    ],
    ..RunOptions::default()
};
```

(If other clone call sites build their own `RunOptions`, apply the same
`extra_env` there. `GIT_TERMINAL_PROMPT=0` also closes the missing-prompt gap the
audit noted on the first-install path.)

**Verify**: `cargo check -p jackin-runtime` exits 0.

### Step 4: Tests

In the sibling test module add:

```rust
#[test]
fn rejects_dangerous_git_urls() {
    for url in [
        "ext::sh -c id",
        "fd::17/foo",
        "file:///etc/passwd",
        "-oProxyCommand=id",
        "--upload-pack=id",
        "",
    ] {
        assert!(super::validate_git_url(url).is_err(), "{url} must be rejected");
    }
}

#[test]
fn accepts_standard_git_urls() {
    for url in [
        "https://github.com/org/role.git",
        "ssh://git@example.com/org/role.git",
        "git@github.com:org/role.git",
        "git://example.com/org/role.git",
    ] {
        assert!(super::validate_git_url(url).is_ok(), "{url} must be accepted");
    }
}

#[test]
fn clone_args_place_double_dash_before_url() {
    let args = super::clone_args("https://github.com/o/r.git", "/dest", None);
    let dash = args.iter().position(|a| *a == "--").expect("has --");
    let url = args.iter().position(|a| *a == "https://github.com/o/r.git").unwrap();
    assert!(dash < url, "-- must precede the URL");
}
```

**Verify**: `cargo nextest run -p jackin-runtime -E 'test(git_url)'` and `test(clone_args)` pass.

### Step 5: Full check

**Verify**: `cargo nextest run -p jackin-runtime` all pass; clippy clean.

## Test plan

- `rejects_dangerous_git_urls`, `accepts_standard_git_urls`,
  `clone_args_place_double_dash_before_url`. The first is the load-bearing
  security regression test.
- Existing repo_cache tests must still pass.

## Done criteria

- [ ] `grep -n 'fn validate_git_url' crates/jackin-runtime/src/runtime/repo_cache.rs` matches
- [ ] `grep -n '"clone", "--"' crates/jackin-runtime/src/runtime/repo_cache.rs` matches (or the `-b … "--"` variant)
- [ ] `grep -n 'GIT_ALLOW_PROTOCOL' crates/jackin-runtime/src/runtime/repo_cache.rs` matches
- [ ] Every `runner.run("git", …"clone"…)` site is preceded by a `validate_git_url` call
- [ ] `cargo nextest run -p jackin-runtime` exits 0; new tests pass
- [ ] clippy clean; only `repo_cache.rs` (+ its tests.rs) modified
- [ ] `security-review/README.md` status row updated

## STOP conditions

Stop and report if:

- A legitimate operator URL form in existing tests/fixtures (e.g. a bare local
  path used intentionally for a local role repo) would be rejected by
  `validate_git_url` — report it; the allowlist may need a deliberate exception
  rather than silently breaking a supported flow.
- `RepoError` is not an enum in this file / is defined elsewhere — report where.
- There are clone call sites outside `repo_cache.rs` (grep the crate for
  `"clone"`); if so, list them rather than editing out of scope.

## Maintenance notes

- If a future flow accepts role URLs from a less-trusted source, this guard
  becomes load-bearing rather than defense-in-depth; keep the allowlist tight.
- Reviewer should confirm every clone path is both validated and `--`-guarded,
  and that `GIT_ALLOW_PROTOCOL` reaches the git process (via `extra_env`).
