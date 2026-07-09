# Plan 033: Characterization tests for launch-core teardown, capsule client displace, and PTY failure recovery

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md` — unless a reviewer dispatched you and told
> you they maintain the index.
>
> **Drift check (run first)**:
> `git diff --stat 0971da66d..HEAD -- crates/jackin-runtime/src/runtime/launch/ crates/jackin-capsule/src/session.rs crates/jackin-capsule/src/session/ crates/jackin-capsule/src/daemon.rs crates/jackin-capsule/src/daemon/ crates/jackin-capsule/src/attach_protocol.rs crates/jackin-capsule/src/client_writer.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M-L
- **Risk**: MED (test-only code, but suite A constructs a large context struct; risk is wasted effort, not breakage)
- **Depends on**: none (pairs with plans 008 and 032: plan 008 changes launch finalization behavior — if 008 has landed, its new teardown behavior is the behavior to characterize; plan 032's MISSING worklists consume whatever this plan cannot reach)
- **Category**: tests
- **Planned at**: commit `0971da66d`, 2026-07-09

## Why this matters

The codebase-health roadmap (Phase 2, `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` lines 122, 130) requires characterization tests around launch and capsule-daemon behavior **before** the planned decompositions of `run_launch_core` and `daemon.rs` — "Refactor with characterization first" is a stated principle (line 31). Today the three highest-risk behaviors have zero or near-zero direct coverage: `run_launch_core` (the ~1,200-line launch heart) has **no test that calls it**; the capsule's client-takeover path (`daemon.rs:1190-1269`) has **no direct test** (the single "reattach" test only exercises capability refresh on an in-memory struct); and the PTY writer/reader failure-recovery branches in `session.rs` are unreachable with the current inert test double. Until these exist, every launch or daemon refactor is unverifiable, and the Phase 2 restructures stay blocked.

## Current state

### Suite A target — `run_launch_core`

- `crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs:133` — entry point:

```rust
pub(super) async fn run_launch_core<D, R>(ctx: LaunchCore<'_, D, R>) -> anyhow::Result<String>
where
    D: DockerApi,
    R: CommandRunner,
```

  `LaunchCore<'a, D, R>` is defined at `launch_core.rs:42-92` and destructured immediately (`:139-173`; fields include `paths`, `config`, `selector`, `workspace`, `docker`, `runner`, `opts`, `backend`, `image_decision`, `restoring`, `container_name`, `git_pull_join`). Phase order inside the body: adopt sidecar + arm `LoadCleanup` (`:176-211`), grant/profile validation with `cleanup.run(docker).await` on each failure (`:227-278`), sidecar future (`:297-323`), image materialization (`:327-500`), manifest write (`:502-555`), credential preflight (`:577-646`), role-state prepare (`:700`), workspace materialize + sidecar join (`:852-880`), backend dispatch (`:981`), `launch_role_runtime` (`:1072`), success bookkeeping (`:1099`), post-session teardown classification (`:1204-1311`).
- **Visibility**: `pub(super)` — visible only inside the `launch_pipeline` module. Tests must live at `crates/jackin-runtime/src/runtime/launch/launch_pipeline/tests.rs` (create it and declare `#[cfg(test)] mod tests;` in `launch_pipeline.rs` if not already declared — check first).
- Production construction exemplar: `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs:1108` (`launch_core::run_launch_core(launch_core::LaunchCore { paths, config, selector, workspace, docker, runner, opts, git, ... }).await`).
- `LoadCleanup` — `crates/jackin-runtime/src/runtime/launch/load_cleanup.rs:29`; `run(&self, docker)` (`:80`) is best-effort, returns `()`, and issues in order: `remove_container(container)`, `remove_container(dind)`, `remove_volume(certs)`, `remove_network(network)`, optional socket-dir removal (`:90-114`). **Observable in tests via `FakeDockerClient::recorded` op strings** (`"docker rm -f <name>"`, `"docker volume rm <name>"`, `"docker network rm <name>"`).
- `FakeDockerClient` — `crates/jackin-runtime/src/runtime/test_support.rs:200` (module gated `#[cfg(any(test, feature = "test-support"))]`, re-exported at `:189`). Struct-literal configuration, no builder:

```rust
pub struct FakeDockerClient {
    pub recorded: std::cell::RefCell<Vec<String>>,
    pub inspect_queue: std::cell::RefCell<std::collections::VecDeque<ContainerState>>,
    pub inspect_state_by_name: std::cell::RefCell<HashMap<String, ContainerState>>,
    // ... per-method queues: list_containers_queue, list_networks_queue,
    // list_image_tags_queue, remove_image_queue, exec_capture_queue,
    // inspect_image_labels_queue, inspect_network_queue ...
    pub fail_with: Vec<(String, String)>,
    pub created_containers: std::cell::RefCell<Vec<(String, ContainerSpec)>>,
    pub created_networks: std::cell::RefCell<Vec<(String, HashMap<String, String>, bool)>>,
}
```

  `fail_with` substring-matches per-call op strings and bails with the given message. Empty-queue defaults are lenient (`pop_inspect` → `ContainerState::NotFound`), except `remove_image` panics if unscripted.
- `FakeRunner` (the `CommandRunner` fake) — same file, `:42-147`: `recorded`/`run_recorded`/`run_options`/`fail_on`/`fail_with`/`capture_queue`/`side_effects`.
- Existing arrange/act/assert exemplars:
  - `crates/jackin-runtime/src/runtime/launch/tests.rs:8447+` — `inspect_attach_outcome_*` family; helper `inspect_docker(state)` at `:8466`.
  - `crates/jackin-isolation/src/finalize/tests.rs:46+` — drives `finalize_foreground_session` with both fakes plus a `NoPrompt` double.
  - Standard fixtures: `JackinPaths::for_tests(dir.path())` + `tempfile::TempDir` (see `crates/jackin-runtime/src/runtime/attach/tests.rs:9-22`), `install_all_test_stubs`, `seed_valid_role_repo` (both in `test_support.rs`).

### Suite B target — client displace / active-client policy

- Invariant (documented at `crates/jackin-capsule/src/daemon.rs:7-8`): at most one attach client is active; a new `Hello` displaces the previous client.
- State lives on `Multiplexer`: `client: ClientWriter` (`daemon.rs:208`, "the only writer to the attach socket"), `attached_task: Option<JoinHandle<()>>` (`:214`), `attached_terminal` (`:277`), `attached_capabilities` (`:281`).
- The takeover branch — `daemon.rs:1190-1269` (inline in the `run_daemon` select loop, no direct test):

```rust
detach_attached_task(&mut mux, "takeover").await;
// Drain any stale frames the old client task pushed into cmd_tx ...
let mut drained = 0u32;
while cmd_rx.try_recv().is_ok() {
    drained = drained.saturating_add(1);
}
// ...
let (new_out_tx, new_out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
mux.client.attach(new_out_tx.clone());
```

- Displace helper — `crates/jackin-capsule/src/attach_protocol.rs:195-215`:

```rust
pub(crate) async fn detach_attached_task(mux: &mut Multiplexer, context: &str) {
    detach_attached_task_with_reason(mux, context, None).await;
}

async fn detach_attached_task_with_reason(mux: &mut Multiplexer, context: &str, reason: Option<&str>) {
    let had_sender = send_attached_shutdown(mux, context, reason);
    if had_sender {
        tokio::time::sleep(Duration::from_millis(ATTACH_SHUTDOWN_FLUSH_GRACE_MS)).await;
    }
    if let Some(handle) = mux.attached_task.take() {
        handle.abort();
    }
}
```

- `ClientWriter::attach`/`take` — `crates/jackin-capsule/src/client_writer.rs:39`/`:47` (attach clears the dead-send latch and drops the prior out-of-band bytes/sender).
- Input routing: per-client task `handle_attach_client` (`attach_protocol.rs:286`) forwards `ClientFrame`s over `cmd_tx`; the daemon processes them via `handle_client_frame` (`crates/jackin-capsule/src/daemon/control.rs:108`); `ClientFrame::Input(bytes)` parses and applies to the mux (`control.rs:120-144`).
- The one existing "reattach" test — `crates/jackin-capsule/src/daemon/tests.rs:6678` `reattach_updates_capabilities_without_resetting_model_palette` — mutates `attached_terminal`/`attached_capabilities` in place; it never exercises `detach_attached_task`, `ClientWriter::attach`, or frame routing.
- Test helpers available: `single_pane_tab_mux()` (`daemon/tests.rs:249`), `test_mux` (`:152`), `test_session(rows, cols) -> (Session, UnboundedReceiver<Vec<u8>>)` (`:806`).

### Suite C target — PTY failure recovery

- Writer task branches — `crates/jackin-capsule/src/session.rs:468-518`:

```rust
tokio::task::spawn_blocking(move || {
    let writer = match master_for_write.lock() {
        Err(_) => { crate::clog!("session {sid}: PTY master mutex poisoned; aborting writer task"); None }
        Ok(guard) => match guard.take_writer() {
            Ok(w) => Some(w),
            Err(e) => { crate::clog!("session {sid}: take_writer failed: {e}; aborting writer task"); None }
        },
    };
    let Some(mut writer) = writer else {
        if event_tx_writer_err.send(SessionEvent::Exited {
            session_id: sid,
            reason: Some("session PTY writer failed to initialize".to_owned()),
        }).is_err() { /* clog: channel closed */ }
        return;
    };
    // ... write_all error mid-stream → Exited { reason: "session PTY write failed: {e}" } ...
```

  Reader task (`:521-582`): lock-poison / `try_clone_reader()` failure → `Exited { reason: "session PTY reader failed to initialize" }`; `Ok(0)` EOF → break with **no** Exited (the child reaper at `:604-622` is the authoritative exit signal); `read` `Err(e)` → clog + break; output-channel closed → break.
- Current doubles — `NullMasterPty` (defined twice: `session/tests.rs:37-69` and `daemon/tests.rs:26-56`) is inert: `try_clone_reader` → `io::empty()` (clean EOF only), `take_writer` → `io::sink()` (never fails). **It cannot produce**: `take_writer` Err, `try_clone_reader` Err, mid-stream write Err, read Err. `RecordingMasterPty` (`session/tests.rs:75-108`) only records resize.
- Session test constructor: `Session::new_for_test(..., Arc::new(Mutex::new(Box::new(NullMasterPty))), Arc::new(Mutex::new(Box::new(NullChildKiller))))` (`session/tests.rs:110-121`). Exit statuses faked via `portable_pty::ExitStatus::with_exit_code(...)` (`:1110-1125`).

### Repo conventions that apply

- Tests live in a sibling `tests.rs` (`crates/AGENTS.md`: `foo.rs` declares `#[cfg(test)] mod tests;`, all tests inline in `foo/tests.rs`, no child modules).
- Async tests use `#[tokio::test]` (current-thread default; `start_paused = true` variant exists at `daemon/tests.rs:81` for timer-driven paths — use it for suite B's flush-grace sleep). Sync tests that only inspect recorded ops use plain `#[test]`.
- Assertions are direct `assert_eq!` on enums and fake-recorded fields — no insta snapshots in these areas; match that idiom.
- Comments: non-obvious WHY only.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Format | `cargo fmt` | exit 0 |
| Runtime crate tests | `cargo nextest run -p jackin-runtime` | all pass |
| Capsule crate tests | `cargo nextest run -p jackin-capsule` | all pass |
| Lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Merge-readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope** (the only files you should modify/create):
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs` (add `#[cfg(test)] mod tests;` if absent)
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline/tests.rs` (create)
- `crates/jackin-capsule/src/daemon/tests.rs` (add suite B tests)
- `crates/jackin-capsule/src/attach_protocol/tests.rs` (create if a helper-level test fits better there; check whether `attach_protocol.rs` already declares `mod tests`)
- `crates/jackin-capsule/src/session/tests.rs` (suite C tests + the new fault-injecting double)
- `crates/jackin-runtime/README.md`, `crates/jackin-capsule/README.md` (Tests-column rows for new tests.rs files only — required by `crates/AGENTS.md` on structural change)

**Out of scope** (do NOT touch, even though they look related):
- Any production code in `launch_core.rs`, `daemon.rs`, `session.rs`, `attach_protocol.rs`, `client_writer.rs` — this plan **characterizes** current behavior; if a test wants a seam that doesn't exist, record it and move on (STOP condition 4 covers the one allowed exception).
- `crates/jackin-runtime/src/runtime/launch/load_cleanup.rs` — behavior under test, not editable.
- Plan 008's finalization changes (separate plan; if already landed, characterize the landed behavior).
- The `test-layout-allowlist.toml` — new tests.rs files follow the standard layout; no allowlist entries.

## Git workflow

- Branch: current active branch if the operator designates one; otherwise propose `test/characterization-launch-displace-pty` and wait for confirmation (repo rule: never commit to `main`; one PR per session).
- Conventional Commits, signed, pushed immediately after each commit (repo hard rule):
  `git commit -s -m "test(runtime): characterize run_launch_core failure teardown"` then `git push`.
- jackin-capsule changes: the capsule smoke-test mandate applies to PRs touching `jackin-capsule` — note it in the PR body per `.github/` rules.

## Steps

### Step 1: Suite A scaffolding — minimal `LaunchCore` fixture

Check whether `launch_pipeline.rs` declares `#[cfg(test)] mod tests;`; add the declaration and create `launch_pipeline/tests.rs`. Build a helper `fn test_launch_ctx(...)` that assembles the cheapest possible `LaunchCore`: `JackinPaths::for_tests(tempdir)`, a minimal `AppConfig` (copy construction from an existing runtime test that builds one — search `AppConfig` literals in `crates/jackin-runtime/src/runtime/launch/tests.rs`), `FakeDockerClient::default()`, `FakeRunner::default()`, `install_all_test_stubs`/`seed_valid_role_repo` as needed, `backend: Backend::Docker`, `restoring: false`, a fixed `container_name`. Read `launch_core.rs:42-92` for the full field list; for fields whose construction is unclear, find how `launch_pipeline.rs:1000-1108` builds them.

**Verify**: `cargo nextest run -p jackin-runtime launch_pipeline` → compiles, 0 tests may run yet (fixture only).

### Step 2: Suite A — grant-failure teardown test (the cheap failure path)

Write `grant_validation_failure_runs_cleanup_in_order`: configure the ctx so grant/profile validation fails (`launch_core.rs:227-278` — read `bail_on_grant_errors`/`tagged_grant_errors` to find the cheapest failing input, e.g. an invalid grant in the role config seeded by the fixture). Assert: (a) `run_launch_core` returns `Err`; (b) `FakeDockerClient::recorded` contains the `LoadCleanup::run` op sequence in order — `docker rm -f <container>` before `docker rm -f <dind>` before `docker volume rm` before `docker network rm` (only the entries applicable to the fixture's `DockerResources`; read `load_cleanup.rs:80-114` and assert exactly what the fixture arms).

**Verify**: `cargo nextest run -p jackin-runtime grant_validation_failure` → 1 test passes.

### Step 3: Suite A — mid-pipeline failure test

Write `image_phase_failure_writes_failed_setup_and_cleans`: script `fail_with` on the first Docker op of the image-materialization phase (read `launch_core.rs:327-500` to pick the op string; `fail_with` substring-matches). Assert `Err` return, cleanup ops recorded, and — if the instance-status file is observable under the tempdir paths (`write_instance_status`, `launch_core.rs:502-555` and the failure arms) — assert the status file records `FailedSetup`. If status isn't file-observable with the fixture, assert on recorded ops only and note it in the test comment.

**Verify**: `cargo nextest run -p jackin-runtime image_phase_failure` → passes.

### Step 4: Suite B — displace helper + writer-latch characterization

In `daemon/tests.rs` (or `attach_protocol/tests.rs` if `attach_protocol.rs` already has a tests module — prefer wherever `Multiplexer` test helpers are importable), add:

1. `detach_attached_task_sends_shutdown_and_aborts_reader` (`#[tokio::test(start_paused = true)]`): build a mux via `single_pane_tab_mux()`, attach a channel via `mux.client.attach(tx)`, park a dummy `tokio::spawn` handle in `mux.attached_task`, call `detach_attached_task(&mut mux, "takeover")`. Assert the Shutdown frame arrives on the old rx (read `send_attached_shutdown` in `attach_protocol.rs` for the exact frame encoding — assert on the frame type, not raw bytes, if a decode helper exists), and the parked handle `.is_finished()` after the call.
2. `client_writer_attach_displaces_prior_sender`: attach channel A, then attach channel B; send output through `mux.client`; assert A's rx is closed/empty and B's rx receives.
3. `input_frames_apply_to_post_takeover_mux`: after simulating takeover (helper from test 1 + fresh attach), feed `ClientFrame::Input(b"x")` through `handle_client_frame(&mut mux, frame)` and assert the active session received the bytes (via the `test_session` receiver from `daemon/tests.rs:806`).

The select-loop drain (`daemon.rs:1204-1210`) is **not** unit-testable without driving `run_daemon`; do not attempt it. Add one sentence to the test module header comment: the drain + initial-burst ordering remains covered only by the daemon loop itself (plan 032's spec marks it MISSING).

**Verify**: `cargo nextest run -p jackin-capsule detach_attached_task client_writer_attach input_frames_apply` → 3 tests pass.

### Step 5: Suite C — fault-injecting PTY double

In `session/tests.rs`, add `FaultMasterPty` next to `NullMasterPty`: configurable `take_writer_err: Option<io::ErrorKind>`, `clone_reader_err: Option<io::ErrorKind>`, `writer_fails_after: Option<usize>` (writer that errors on the Nth `write_all`), `reader_yields: Vec<Result<Vec<u8>, io::ErrorKind>>` (scripted reads ending in EOF or error). Implement `portable_pty::MasterPty` by delegating everything else to the `NullMasterPty` behavior. Keep it local to `session/tests.rs` (the daemon copy of `NullMasterPty` stays untouched).

**Verify**: `cargo nextest run -p jackin-capsule -E 'binary(jackin-capsule)' session` → existing session tests still pass (double compiles, unused-yet is fine if a first test lands in the same commit).

### Step 6: Suite C — recovery-branch tests

Four tests using `Session::new_for_test` with `FaultMasterPty`, each asserting the `SessionEvent` stream (the receiver returned by the test constructor):

1. `writer_init_failure_emits_exited_with_reason` — `take_writer_err: Some(...)` → expect `Exited { reason: Some("session PTY writer failed to initialize") }` (`session.rs:484-497`).
2. `reader_init_failure_emits_exited_with_reason` — `clone_reader_err: Some(...)` → expect `Exited { reason: Some("session PTY reader failed to initialize") }`.
3. `mid_stream_write_failure_emits_exited` — `writer_fails_after: Some(0)`, send input into the session → expect `Exited { reason }` where reason starts with `"session PTY write failed:"` (`session.rs:498-516`).
4. `read_error_breaks_without_exited_event` — `reader_yields: vec![Err(...)]` → assert **no** `Exited` arrives from the reader (only the reaper emits exit; with `NullChildKiller`/faked child the channel stays quiet — assert via `try_recv` after a yield, or timeout-recv under `start_paused`).

These are `spawn_blocking` tasks — tests must `await` the event channel, not sleep-poll; use `event_rx.recv().await` with `tokio::time::timeout`.

**Verify**: `cargo nextest run -p jackin-capsule writer_init_failure reader_init_failure mid_stream_write read_error_breaks` → 4 tests pass.

### Step 7: README rows + full gates

Add Tests-column rows for the new `launch_pipeline/tests.rs` (jackin-runtime README structure table) and confirm capsule README rows for `session/tests.rs`/`daemon/tests.rs` already exist (they should; only add what's missing). Run the full gate set.

**Verify**: `cargo fmt && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings && cargo nextest run -p jackin-runtime -p jackin-capsule` → exit 0, all pass. Then `cargo xtask ci --fast` → exit 0.

## Test plan

This plan **is** tests. New coverage summary: 2 launch-core failure-path tests (grant-failure teardown ordering, mid-pipeline FailedSetup+cleanup), 3 displace-seam tests (shutdown+abort, writer latch, post-takeover input routing), 4 PTY-recovery tests (writer init, reader init, mid-stream write, read-error-vs-EOF). Pattern exemplars: `inspect_attach_outcome_exited_zero_returns_stopped` (`launch/tests.rs:8475`) and `still_running_with_zero_sessions_cleans` (`finalize/tests.rs:47`).

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo nextest run -p jackin-runtime -p jackin-capsule` exits 0 with ≥9 new tests present (grep test names above in the run output)
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` exits 0
- [ ] `cargo xtask ci --fast` exits 0
- [ ] No production (non-test) file modified: `git diff --name-only` shows only tests.rs files, the `mod tests;` declaration line in `launch_pipeline.rs`, and README table rows
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

1. The code at the cited locations doesn't match the excerpts (drift since `0971da66d`).
2. Suite A's fixture cannot reach the grant-validation failure with fakes alone (e.g. it requires a real git repo or real Docker socket before phase 2) after a reasonable attempt — deliver suites B+C and report exactly which `LaunchCore` field blocked construction; that report feeds the Phase 2 decomposition plan.
3. A test's verification fails twice after a reasonable fix attempt.
4. A branch under test is genuinely unreachable without adding a production seam (e.g. the poisoned-mutex arms — poisoning requires a panicked holder thread; if you cannot trigger it from the test side, SKIP those two arms and note them as MISSING in the test module comment rather than adding `#[cfg(test)]` hooks to `session.rs`).
5. `run_launch_core` is no longer `pub(super)` at `launch_pipeline/launch_core.rs:133` or has moved — the decomposition may have started; re-planning is needed, not adaptation.

## Maintenance notes

- These are **characterization** tests: they pin current behavior, including oddities (e.g. reader read-error emits no `Exited`; the reaper is authoritative). When the Phase 2 decompositions intentionally change behavior, update the assertion **and** cite the behavioral-spec section (plan 032) in the same PR.
- Suite B documents (in the test-module comment) that the select-loop drain and initial-frame-burst ordering are untested — the daemon decomposition should extract that branch into a testable `perform_takeover` unit; these seam tests then become its regression floor.
- Reviewer scrutiny points: suite A must assert cleanup op **order**, not just presence (ordering is the contract `load_cleanup.rs` encodes); suite C must not sleep-poll (`start_paused`/`timeout` only).
- Deferred (recorded, not this plan): displace-under-real-socket integration test (needs a drivable daemon loop), poisoned-mutex arms, PTY flood/backpressure soak (ledger item BUG-pty-unbounded-channel).
