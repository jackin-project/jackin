# Rust Review Findings

Review date: 2026-04-13

This file captures the senior Rust code review findings for `jackin` so the implementation branch has a durable record of what needs to be addressed and why.

## Overall Assessment

- Overall Rust quality: solid intermediate Rust with several senior-level strengths in validation, test coverage, and security posture.
- Approval status: the reviewed issues are addressed on `fix/review-findings`; pending normal human review and merge.
- Verification status on this branch: `cargo fmt -- --check`, `cargo clippy`, and `cargo nextest run` are green after the fixes below.

## Branch Status

- Findings 1 through 6 are implemented on `fix/review-findings`.
- The original `ShellRunner::capture()` deadlock class is fixed by draining stdout and stderr concurrently while the child process runs.
- High-output runtime commands that do not need output (`git clone`, `git pull`, `docker network create`, detached `docker run`) now use `run()` and still retain timeout protection.
- Agent and DinD filtering is role-aware for new resources and remains compatible with legacy `jackin.managed=true` containers created before the role labels existed.

## Top 5 To Fix First

All five priority items below are addressed on this branch.

1. Fix `ShellRunner::capture()` so piped child processes cannot deadlock on full stdout or stderr buffers.
2. Make `jackin-validate` delegate to the runtime repo validator so validation rules cannot drift.
3. Separate agent containers from DinD sidecars with explicit role labels and agent-only filtering.
4. Persist `last_agent` only after a successful load.
5. Preserve real prompt I/O failures instead of collapsing them into "Skipped".

## Findings

### 1. `ShellRunner::capture()` can deadlock on large output

- Severity: high
- Area: correctness, performance
- Location: `src/docker.rs`, especially `ShellRunner::capture()` and its callers in `src/runtime.rs`

#### What is wrong

`capture()` starts a child with piped stdout and stderr, waits for the child to exit by polling `try_wait()`, and only after that calls `wait_with_output()` to drain the pipes.

That is unsafe for commands that can emit enough output to fill the OS pipe buffer before exit. If that happens, the child blocks trying to write more data, while the parent keeps waiting for process exit.

Current callers include commands like:

- `git clone`
- `git pull --ff-only`
- detached Docker commands where the output is not actually needed

#### Why it is a Rust problem specifically

This is a known `std::process::Command` footgun: once stdout and stderr are piped, Rust gives you explicit responsibility for draining them. If you wait for exit before reading, you can deadlock a healthy process.

#### How to improve it

- Use `run()` instead of `capture()` for commands where the output is not consumed.
- Rework `capture()` so it drains stdout and stderr while the child is still running.
- Keep `capture()` for low-volume query commands only if you want a smaller immediate change.

#### Suggested refactor

- Switch `git clone`, `git pull`, `docker network create`, and detached `docker run` calls from `capture()` to `run()`.
- Later, if needed, reimplement `capture()` with concurrent readers and timeout handling.

### 2. `jackin-validate` does not use the same validation path as runtime loading

- Severity: high
- Area: correctness, API design, maintainability
- Location: `src/bin/validate.rs` versus `src/repo.rs`

#### What is wrong

The standalone validator reimplements repo validation instead of delegating to `validate_agent_repo()`.

As a result it:

- hard-codes a root `Dockerfile`
- validates the wrong Dockerfile path when `jackin.agent.toml` points elsewhere
- skips pre-launch hook validation
- skips symlink and repo-boundary validation
- can disagree with what the runtime actually accepts or rejects

#### Why it is a Rust problem specifically

Rust codebases benefit from routing behavior through a single typed API boundary. Duplicated rule systems in different binaries drift quickly because the compiler cannot enforce semantic consistency.

#### How to improve it

- Make `jackin-validate` call `validate_agent_repo(&repo_dir)`.
- Treat any extra CLI-only checks as warnings layered on top of the runtime validator, not as a replacement for it.

#### Suggested refactor

- Replace the local file-by-file validation logic in `src/bin/validate.rs` with a thin wrapper around `jackin::repo::validate_agent_repo`.

### 3. Agent and DinD resources are not modeled distinctly enough

- Severity: medium
- Area: correctness, API design, maintainability
- Location: `src/runtime.rs` and `src/lib.rs`

#### What is wrong

Both agent containers and DinD sidecars are tagged with `jackin.managed=true`, but several functions treat that label as if it meant "agent container only".

This affects behavior such as:

- listing managed agents
- rendering running-agent display names
- exiling all agents

That can leak `*-dind` containers into agent-oriented flows.

#### Why it is a Rust problem specifically

This is a type-modeling weakness. Distinct runtime roles are encoded as loose strings instead of expressed as a stronger internal model, so the compiler cannot help prevent mixing them up.

#### How to improve it

- Add an explicit agent role label such as `jackin.role=agent`.
- Keep `jackin.role=dind` for sidecars.
- Use agent-only filters anywhere the API concept is "agent" rather than "any managed resource".

#### Suggested refactor

- Introduce `LABEL_ROLE_AGENT` and `FILTER_ROLE_AGENT`.
- Apply the label to the launched agent container.
- Update agent-listing, outro, and exile code to use the agent-specific filter.

### 4. `last_agent` is persisted even when load fails

- Severity: medium
- Area: correctness
- Location: `src/lib.rs`

#### What is wrong

After calling `runtime::load_agent(...)`, the code updates and saves `last_agent` for saved workspaces before checking whether the load succeeded.

That means failed launches can still change future workspace auto-selection behavior.

#### Why it is a Rust problem specifically

This is a transactional `Result` boundary bug. In idiomatic Rust, persistent state changes should usually follow successful completion of the fallible operation they describe.

#### How to improve it

- Save `last_agent` only when `load_agent(...)` returns `Ok(())`.
- Keep the current warning behavior if saving the workspace metadata itself fails.

#### Suggested refactor

- Move the `last_agent` mutation and `config.save(...)` call inside an `if result.is_ok()` block.

### 5. Prompt errors are flattened into skipped input

- Severity: medium
- Area: error handling, API design
- Location: `src/terminal_prompter.rs` and `src/env_resolver.rs`

#### What is wrong

`TerminalPrompter` maps any `dialoguer` failure to `PromptResult::Skipped`.

That erases the difference between:

- deliberate skip
- user cancellation
- broken terminal I/O

Required prompts then fail with a misleading message like "required prompt cannot be skipped" even when the real cause was a terminal error.

#### Why it is a Rust problem specifically

Rust’s error model is strongest when distinct failure modes remain explicit in the type system. Collapsing all errors into a nominally valid state throws away context and makes debugging much harder.

#### How to improve it

- Make the prompter return a `Result`.
- Distinguish real skip/cancel outcomes from I/O failures.

#### Suggested refactor

- Replace `PromptResult`-only returns with something like `anyhow::Result<PromptOutcome>` where `PromptOutcome` can still represent `Value`, `Skipped`, or `Cancelled`.

### 6. Workspace auto-detection relies on fragile string equality

- Severity: low
- Area: correctness, idiomatic Rust
- Location: `src/lib.rs`, `src/launch.rs`, and `src/workspace.rs`

#### What is wrong

Workspace auto-selection compares current-directory strings directly against stored workspace workdir strings.

That is brittle for:

- symlinked paths
- canonicalization differences
- workspaces whose container workdir differs from the host path used to identify the workspace

The current documentation already warns users about these cases, which suggests the implementation model is weaker than the user model.

#### Why it is a Rust problem specifically

The code converts paths to `String` too early and loses the semantics of `Path` and `PathBuf`, which are exactly the tools Rust provides to avoid textual path bugs.

#### How to improve it

- Match on canonicalized host mount sources instead of raw workdir strings.
- Prefer the deepest containing mount when multiple saved workspaces could match the current directory.

#### Suggested refactor

- Introduce a workspace-identification helper based on canonical host paths and reuse it for both `jackin load` context resolution and launcher preselection.

## What Is Already Strong

- `unsafe_code` is forbidden in `Cargo.toml`, and the codebase currently keeps that promise.
- Manifest validation is thoughtful and defensive, especially around env references and reserved runtime variables.
- Repo validation has strong security instincts around path traversal, symlink rejection, and repo-boundary enforcement.
- The crate layout is understandable and responsibilities are mostly well separated.
- Test coverage is broad and catches many real behavioral boundaries.

## Current Branch Goal

This implementation branch exists to:

1. preserve these findings in-repo
2. address as many of them as is reasonable in one branch without overreaching
3. keep verification green with `cargo fmt -- --check`, `cargo clippy`, and `cargo nextest run`
