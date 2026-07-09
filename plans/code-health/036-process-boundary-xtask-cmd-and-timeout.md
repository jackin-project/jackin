# Plan 036: Process-execution boundary — one xtask command module, timeout in the runner port

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md` — unless a reviewer dispatched you and told
> you they maintain the index.
>
> **Drift check (run first)**: `git diff --stat 0971da66d..HEAD -- crates/jackin-xtask/src/ crates/jackin-core/src/runner.rs crates/jackin-docker/src/shell_runner.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (Step 4 touches the production `ShellRunner`; Steps 1-2 are xtask-internal and LOW)
- **Depends on**: none (**file-conflict note**: plans 011/022 also edit `crates/jackin-xtask/src/ci.rs` — this plan only *re-routes the bodies* of `run_step`/`output_step`; coordinate by landing whichever is first and rebasing mechanically)
- **Category**: tech-debt
- **Planned at**: commit `0971da66d`, 2026-07-09

## Why this matters

Roadmap Phase 2, "Shared contracts to extract", item 1 (`docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` line 135): consolidate command-output wrappers into one capture/timeout/retry/status model. A 2026-07-09 census found ~40 ad-hoc wrappers across 15 crates, three duplicated shapes, and **four independent timeout implementations** — while the production async engine (`ShellRunner`, the one the launch pipeline uses for `docker`/`git`) has **no timeout at all**: a hung `git fetch` hangs the launch forever. Full workspace unification is L-effort and blocked on design questions (sync PID-1 capsule vs async host); this plan takes the two bounded, highest-leverage cuts: (a) collapse jackin-xtask's six independently-rolled spawn-check-capture helper families into one module (pure dev-tooling, zero production risk), and (b) add `timeout` to the `CommandRunner` port's `RunOptions` and honor it in `ShellRunner` (closes the production gap). Everything else is recorded residue.

## Current state

- **The port** — `crates/jackin-core/src/runner.rs:56-79`:

```rust
/// Subprocess execution seam for `docker`, `git`, and other external commands.
pub trait CommandRunner {
    async fn run(&mut self, program: &str, args: &[&str], cwd: Option<&Path>, opts: &RunOptions) -> anyhow::Result<()>;
    async fn capture(&mut self, program: &str, args: &[&str], cwd: Option<&Path>) -> anyhow::Result<String>;
    async fn capture_secret(&mut self, program: &str, args: &[&str], cwd: Option<&Path>) -> anyhow::Result<String>;
}
```

  `RunOptions` (`runner.rs:14-53`) fields: `capture_stderr`, `capture_stdout`, `quiet`, `extra_env: Vec<…>`, `null_stdin`, `stream_captured_output`, `interactive`, `tee_to_build_log`, `build_log_sink` — **no timeout, no retry**. `Default` impl at `:40-53`. jackin-core rule (its `AGENTS.md`): types/traits/pure helpers only, no I/O — the field is vocabulary; enforcement lives in implementors.
- **Production impl** — `ShellRunner`, `crates/jackin-docker/src/shell_runner.rs:222-514` (tokio; streaming tee to `BuildLogSink`, secret mode, stderr summarization; `do_capture` at `:486-512`). **Zero timeout anywhere in it.** Its crate rule (`jackin-docker/CLAUDE.md`): "shell-command capture goes through `shell_runner` … never bare `Command::output`" — i.e. ShellRunner is already the intended single async engine.
- **Implementors of the trait** (all must keep compiling — the trait itself does NOT change in this plan): `ShellRunner`, capsule `GitRunner` (`exit_assess.rs:65`), `SharedCommandRunner` (`shared_runner.rs:38`), test fakes `MockGit`/`FakeRunner`×3/`ScriptedRunner`.
- **The xtask duplication** (six families, one crate, each hand-rolls spawn→status-check→capture):
  - `crates/jackin-xtask/src/pr.rs:188-204` `run_output(cmd) -> Result<Vec<u8>>` + `display_command` (`:207-219`) + `shell_quote` (`:221-231`):

```rust
fn run_output(cmd: &mut Command) -> Result<Vec<u8>> {
    let display = display_command(cmd);
    #[expect(clippy::disallowed_methods, reason = "xtask automation shells out to git, gh, cargo, and mise")]
    let output = cmd.output().with_context(|| format!("running {display}"))?;
    if output.status.success() { Ok(output.stdout) } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!("{display} failed with {}\n{}", output.status, stderr.trim()))
    }
}
```

  - `crates/jackin-xtask/src/ci.rs:261-273` `run_step(root, step) -> Result<()>` (status-only) and `:275-292` `output_step(root, step) -> Result<String>` (capture), both driven by `Step { name, program, args, env }` (`:24-29`).
  - `crates/jackin-xtask/src/construct.rs:516` `run_checked(cmd)` + `docker()` (`:502`), `builder_exists` (`:508`), `command_label` (`:526`), `git_sha` (`:552`).
  - `crates/jackin-xtask/src/release_verify.rs:175` a second, separate `run_checked(cmd, label)`.
  - `crates/jackin-xtask/src/profile_matrix.rs:206` `command_output(program, args) -> Result<String>`.
  - `crates/jackin-xtask/src/schema.rs:194/:208` `git_show_version` / `git(root, args) -> Result<std::process::Output>`.
  Plus `shell_quote` triplicated: `pr.rs:221`, `construct.rs:526` (`command_label`), and `jackin-dev/main.rs:966` (different crate — out of scope, recorded residue).
- **Timeout engines elsewhere** (context; NOT consolidated by this plan): capsule `wait_child_with_timeout` + `WaitOutcome{Exited,Reaped,TimedOut,Failed}` (`crates/jackin-capsule/src/util.rs:54-104`, the PID-1-aware sync engine), usage `run_cli_with_timeout_full` (`usage/format.rs:238`), env `OpCli::read` recv_timeout + ETXTBSY retry (`op_cli.rs:291`, plan-031 territory), snapshot deadline loop (`runtime/snapshot.rs:255`).
- Repo conventions: `clippy::disallowed_methods` denies `std::process::Command::output` — every xtask call site carries a scoped `#[expect]`; the new module centralizes that to ONE `#[expect]`. Module layout: self-named files, tests in `<mod>/tests.rs`. Comments: non-obvious WHY only.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| xtask tests | `cargo nextest run -p jackin-xtask` | all pass |
| docker-crate tests | `cargo nextest run -p jackin-docker` | all pass |
| Full lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| xtask self-check (exercises run_step live) | `cargo xtask lint --strict` | exit 0 |
| Merge-readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope** (the only files you should modify/create):
- `crates/jackin-xtask/src/cmd.rs` (create) + `crates/jackin-xtask/src/cmd/tests.rs` (create) + the `mod cmd;` declaration in `crates/jackin-xtask/src/main.rs` (or `lib.rs` — match where existing modules are declared)
- `crates/jackin-xtask/src/{pr,ci,construct,release_verify,profile_matrix,schema}.rs` (re-route to `cmd`, delete the local helpers)
- `crates/jackin-core/src/runner.rs` (add `timeout` to `RunOptions` + doc header naming the canonical engines)
- `crates/jackin-docker/src/shell_runner.rs` + its `shell_runner/tests.rs` (honor timeout)
- `crates/jackin-xtask/README.md`, `crates/jackin-core/README.md`, `crates/jackin-docker/README.md` (structure/API rows per `crates/AGENTS.md`)

**Out of scope** (do NOT touch, even though the census names them):
- Capsule/usage/env/runtime/pr-trailers/dev wrappers — the sync-vs-async unification is a design decision recorded as residue; `jackin-capsule/src/util.rs`'s `WaitOutcome` engine stays the capsule-side canonical engine.
- The `CommandRunner` **trait signature** — `capture`/`capture_secret` take no `RunOptions`; threading options through them is a breaking trait change touching 8 implementors. Not this plan.
- Retry semantics — only two bespoke retry sites exist (op ETXTBSY, image backoff); both are domain-specific. Recorded residue.
- `jackin-dev/main.rs:966` shell-quoter (separate crate, separate PR).

## Git workflow

- Branch: current active branch if the operator designates one; otherwise propose `refactor/process-boundary-xtask-cmd` and wait for confirmation.
- Conventional Commits, signed, push after each: suggested sequence `refactor(xtask): one cmd module for subprocess helpers` then `feat(core): RunOptions timeout honored by ShellRunner`.

## Steps

### Step 1: Create `jackin-xtask/src/cmd.rs`

One module owning the crate's subprocess vocabulary. Target surface (adjust names only if a collision exists):

```rust
pub(crate) fn run(cmd: &mut Command) -> Result<()>            // status-only, error names the command
pub(crate) fn output(cmd: &mut Command) -> Result<Vec<u8>>    // capture stdout, error includes trimmed stderr
pub(crate) fn output_string(cmd: &mut Command) -> Result<String> // lossy-UTF8 + into_owned convenience
pub(crate) fn display_command(cmd: &Command) -> String
pub(crate) fn shell_quote(value: &OsStr) -> String
```

Move the bodies from `pr.rs:188-231` verbatim (they are the best-written copies), with the single `#[expect(clippy::disallowed_methods, reason = "xtask automation shells out to git, gh, cargo, and mise; centralized here")]` on the one `.output()` call and one on the `.status()` call in `run`. `run` takes a pre-built `Command` so callers keep setting `current_dir`/`envs` themselves (that's what `Step` does today).

**Verify**: `cargo check -p jackin-xtask` → exit 0 (module compiles; not yet referenced).

### Step 2: Re-route the six families

- `pr.rs`: delete `run_output`/`display_command`/`shell_quote`; call `cmd::output`/`cmd::display_command`.
- `ci.rs`: `run_step` keeps its `emit(…)` line and `Step`-to-`Command` assembly, then delegates the spawn/status/error to `cmd::run`; `output_step` likewise to `cmd::output_string`. Do NOT change `Step`, `build_steps`, or any step registration (plans 011/022 own those).
- `construct.rs`: `run_checked` → `cmd::run`; `command_label` → `cmd::display_command` (verify output-format parity — `command_label` may format slightly differently; the error-message text may change, that's acceptable, but keep the label content equivalent); `git_sha`/`builder_exists` → `cmd::output_string`/`cmd::run` as fits.
- `release_verify.rs`: its `run_checked(cmd, label)` → `cmd::run` (fold the label into the command display).
- `profile_matrix.rs` `command_output` and `schema.rs` `git`/`git_show_version` → `cmd::output_string` / `cmd::output` (schema's `git` returns full `Output` — if callers use stderr/status fields, add `pub(crate) fn output_raw(cmd) -> Result<std::process::Output>` to `cmd.rs` rather than forcing a fit).
- Delete every now-unused local helper and its `#[expect(clippy::disallowed_methods)]`.

**Verify**: `cargo clippy -p jackin-xtask --all-targets -- -D warnings` → exit 0 (dead_code deny catches leftovers). `grep -rn "fn run_output\|fn run_checked\|fn command_output\|fn shell_quote\|fn display_command\|fn command_label" crates/jackin-xtask/src/ --include="*.rs" | grep -v "src/cmd"` → empty. Then `cargo xtask lint --strict` → exit 0 (live exercise of `run_step` via the real gates).

### Step 3: `cmd` tests

`crates/jackin-xtask/src/cmd/tests.rs` (declare `#[cfg(test)] mod tests;` in `cmd.rs`): quote-passthrough for plain args, quoting for spaces/quotes (`shell_quote("a b")` → `'a b'`, embedded `'` → `'"'"'` escape), `output` success captures stdout (`echo`), `output` failure error contains program name + trimmed stderr (`sh -lc 'echo err >&2; exit 3'`), `run` non-zero status errors. Model formatting on the moved bodies — these pin the behavior the six families now share.

**Verify**: `cargo nextest run -p jackin-xtask cmd` → new tests pass.

### Step 4: `RunOptions.timeout` + ShellRunner enforcement

- `runner.rs`: add `pub timeout: Option<std::time::Duration>` to `RunOptions` with doc: "Deadline for the child process. `None` = no deadline. Enforced by implementors that own real processes (`ShellRunner`); fakes may ignore it." Add `timeout: None` to the `Default` impl (`:40-53`). Extend the module `//!` header (currently stale — it claims the concrete runner lives in `src/docker/mod.rs`) to name the canonical engines: async host = `jackin-docker::shell_runner::ShellRunner`; sync capsule = `jackin-capsule`'s `wait_child_with_timeout` engine; and that new wrappers must route through one of them.
- `shell_runner.rs`: in the paths that spawn and wait (read `:222-514`; both the streaming `run` path and `do_capture`), wrap the wait in `tokio::time::timeout(dur, …)` when `opts.timeout` is `Some`; on expiry `child.kill().await` (best-effort), then return an error whose text contains the program, the configured seconds, and the word `"timed out"` (so callers/logs can classify). `capture`/`capture_secret` take no opts (trait unchanged) — thread the default (no timeout) internally; only `run` honors it this wave.

**Verify**: `cargo clippy -p jackin-core -p jackin-docker --all-targets -- -D warnings` → exit 0. Struct-literal constructors of `RunOptions` that don't use `..Default::default()` will fail to compile — fix each by adding the field (grep `RunOptions {` workspace-wide).

### Step 5: ShellRunner timeout test

In `shell_runner/tests.rs` (check whether it exists; create + declare if not): `#[tokio::test]` spawning `sleep 5` via `ShellRunner::run` with `timeout: Some(Duration::from_millis(200))` → completes well under 1s, returns `Err` containing `"timed out"`; and a control test `sleep 0` with the same timeout → `Ok`. Follow the crate's existing test idioms.

**Verify**: `cargo nextest run -p jackin-docker shell_runner` → both pass; the timeout test's wall time < 2s in nextest's timing output.

### Step 6: READMEs + full gates

Update the three crate READMEs (new `cmd.rs` row + tests link in jackin-xtask; `RunOptions.timeout` note in jackin-core's Public API section; timeout behavior line in jackin-docker). Full gates.

**Verify**: `cargo fmt && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings && cargo nextest run -p jackin-xtask -p jackin-docker -p jackin-core && cargo xtask ci --fast` → all exit 0.

## Test plan

New tests: 5+ in `cmd/tests.rs` (quoting ×2, output success/failure, run failure), 2 in `shell_runner/tests.rs` (timeout kill, no-timeout control). Existing regression net: `cargo xtask lint --strict` and `cargo xtask ci --fast` exercise `run_step`/`output_step` against real cargo/git processes; `cargo nextest run -p jackin-runtime` exercises `RunOptions` construction via `FakeRunner` call sites.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `grep -rn "expect(clippy::disallowed_methods" crates/jackin-xtask/src/ | wc -l` ≤ 2 (both inside `cmd.rs`)
- [ ] `grep -rn "\.output()" crates/jackin-xtask/src/ --include="*.rs" | grep -v "src/cmd"` → empty
- [ ] `grep -n "pub timeout" crates/jackin-core/src/runner.rs` → 1 hit; `grep -n "timed out" crates/jackin-docker/src/shell_runner.rs` → ≥1 hit
- [ ] `cargo nextest run -p jackin-xtask -p jackin-docker` exits 0 with the ≥7 new tests present
- [ ] `cargo xtask ci --fast` exits 0
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

1. The cited xtask helper bodies don't match the excerpts (plans 011/022 may have restructured `ci.rs` — re-locate `run_step`/`output_step` and proceed only if their shape is recognizably the same; otherwise report).
2. `schema.rs`/`construct.rs` callers turn out to depend on helper-specific behavior that `cmd.rs` can't reproduce without growing a 4th/5th variant — two extra variants (`output_raw`) are fine; more means the consolidation premise is wrong for xtask; report.
3. ShellRunner's streaming/tee structure makes a clean `tokio::time::timeout` wrap impossible without restructuring its reader tasks (i.e. the change stops being ~30 lines) — ship Steps 1-3 (xtask half) alone and report the ShellRunner finding; the timeout then becomes its own plan.
4. Any behavioral test elsewhere fails because an error-message string changed — check whether the assertion is on a message this plan rewrote; update ONLY assertions that assert xtask/shell-runner error text, and report any other failure.

## Maintenance notes

- Recorded residue (ledger items, NOT this plan): unify the four timeout engines behind the capsule `WaitOutcome` model or an async equivalent; thread `RunOptions` through `capture`/`capture_secret` (breaking trait change, 8 implementors); domain retry policies; `jackin-dev`'s shell-quoter; the ~30 remaining ad-hoc wrappers in capsule/usage/host/jackin.
- Plan 031 (op-probe typed error) owns `op_cli.rs` — its ETXTBSY retry stays bespoke.
- Reviewer scrutiny: Step 2's `construct.rs` label-format parity (release tooling reads those logs), and that Step 4's kill path can't leak a zombie (`kill().await` then one `wait` — tokio reaps on kill-await).
- Future launch-pipeline call sites should start setting `timeout` on long-haul git/docker `run` calls — candidates: `git fetch`/`git clone` in materialization (see plan 020's chokepoint work for where those live). That adoption is deliberate follow-up, one call site at a time.
