# Plan 005: Contain payload bytes and redact secrets before anything leaves the process over OTLP

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-capsule/src/session.rs crates/jackin-capsule/src/client_writer.rs crates/jackin-docker/src/shell_runner.rs crates/jackin-usage/src/logging.rs crates/jackin-diagnostics/src/observability.rs`
> Plans 003/004 legitimately touched logging.rs/observability.rs. On excerpt
> mismatch in session.rs / client_writer.rs / shell_runner.rs, STOP.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/003-error-severity-truth.md (macro variants exist), plans/004-route-record-direct-through-tracing.md (crash evidence now exports)
- **Category**: security
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

When an operator runs `jackin --debug` with an OTLP endpoint configured (the standard triage setup — the launch path injects both into the container together), the telemetry stream ships raw terminal payload off-host:

- `session.rs` dumps **every PTY output chunk as unbounded hex** (`session feed_pty bytes={:02x?}`) — the agent's full screen output, file contents it prints, any secret it echoes.
- `client_writer.rs` dumps up to 1200 bytes of rendered frame content per frame (`send-bytes`), printable ASCII verbatim.
- `shell_runner.rs` bridges **every stdout/stderr line of every captured subprocess** (`cmd.stdout`/`cmd.stderr` debug categories) and the first stdout line of captures — a build step or tool that prints a token exports it.
- Live-backend evidence (research doc): Docker-inspect-shaped payloads with token-shaped env values were observed in exported log bodies.

The local JSONL file is mode-0600 precisely because "the firehose can carry tokens or credentials" (`run.rs:939-945`) — but the OTLP path has **no equivalent containment and no redaction anywhere** (grep for redact/scrub/mask in the telemetry crates: only `redact_env_args` for command *argv*). This plan draws the boundary: payload bytes never bridge to OTLP; text that does bridge passes a redactor.

## Current state

Capsule payload dumps:

- `crates/jackin-capsule/src/session.rs:1091` — `cdebug!("session feed_pty bytes={:02x?}", …)` per PTY read chunk, **no length cap** (companion metrics line at `:1147`).
- `crates/jackin-capsule/src/client_writer.rs:147` — `cdebug!("send-bytes: {escaped}")` per emitted frame, `escape_for_log` (`:155`) caps at 1200B, printable ASCII passes verbatim, no redaction.
- `crates/jackin-capsule/src/daemon/control.rs:136` — decoded `InputEvent` (keystrokes) logged via `cdebug!`.
- All `cdebug!` lines currently bridge to OTLP at DEBUG when `JACKIN_DEBUG` + endpoint are both set (`crates/jackin-usage/src/logging.rs:156-164` → `telemetry::bridge_log`).

Host subprocess output:

- `crates/jackin-docker/src/shell_runner.rs:400-411` — `log_captured_output` emits every captured stdout/stderr line via `jackin_diagnostics::emit_debug_line("cmd.stdout"/"cmd.stderr", line)` when `self.debug`.
- `:444-449` — Normal-capture mode emits first stdout line as `-> {first_line}`.
- `redact_env_args` (`shell_runner.rs:84-102`, excerpt verified) masks only `-e KEY=VALUE` / `--env KEY=VALUE`; `--build-arg K=V` and password-ish flags pass through. `crates/jackin-docker/src/docker_client.rs:608` `exec_capture` logs full exec argv with no redaction.
- The existing secret-capture discipline: `CommandRunner::capture_secret` (see `crates/jackin-core/src/runner.rs:64-78`) is doc-comment-enforced only.

Crash evidence: plan 004 capped `container_crash_log` at 4096 bytes; content is still raw log-tail text (can include the above byte dumps when `--debug`).

An existing redaction exemplar to reuse/extend: `crates/jackin-capsule/src/exec.rs:167,206` — `secrets_for_redaction`/`redact_pem` applied to exec command capture. Read it before writing the new redactor; match its naming/approach where sensible (DRY rule: extend, don't fork).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Format / lint | `cargo fmt --check` ; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Tests | `cargo nextest run --all-features` | all pass |
| Payload-bridge grep | `rg -n "feed_pty bytes|send-bytes" crates/jackin-capsule/src` | sites exist pre-change |

## Scope

**In scope**:

- `crates/jackin-usage/src/logging.rs` — add a file-only (non-bridging) debug macro variant
- `crates/jackin-capsule/src/session.rs`, `client_writer.rs`, `daemon/control.rs` — switch payload dumps to the file-only variant + cap the hex dump
- `crates/jackin-docker/src/shell_runner.rs` — output redaction + wider argv redaction
- `crates/jackin-docker/src/docker_client.rs` — `exec_capture` argv redaction
- `crates/jackin-diagnostics/src/` — new `redact.rs` module (self-named file, no mod.rs) + apply at `emit_jsonl_event_with_level` and the plan-004 crash-evidence path
- Tests in the respective `<module>/tests.rs` files
- `docs/content/docs/reference/runtime/diagnostics.mdx` — redaction/containment paragraph

**Out of scope**:

- The JSONL **file** content (stays raw by design — it is local, 0600, and the operator's own; document that). Only the OTLP-bridged representation is redacted.
- The roadmap "debug bundle" item (separate work; this plan's redactor is a building block it can reuse).
- `capture_secret` type-enforcement (Maintenance note).
- Changing what plan 003 classified; changing span shapes (007).

## Git workflow

- Propose branch `fix/telemetry-redaction-boundary`; wait for operator confirm. `git commit -s`, conventional subjects (`fix(capsule): …`, `fix(docker): …`, `feat(diagnostics): add export redactor`), push after each.

## Steps

### Step 1: File-only debug variant in the capsule tier

In `crates/jackin-usage/src/logging.rs`, add:

```rust
/// `cdebug!` variant whose line reaches ONLY stderr + multiplexer.log — never
/// the OTLP bridge. For payload content (PTY bytes, frame dumps, keystrokes):
/// triage needs it in the local file; it must not leave the machine.
#[macro_export]
macro_rules! cdebug_local {
    ($($arg:tt)*) => {{
        if $crate::logging::debug_enabled() {
            let line = format!("[jackin-capsule debug] {}", format_args!($($arg)*));
            $crate::logging::write_line(&line);
        }
    }};
}
```

Re-export in `crates/jackin-capsule/src/lib.rs` beside `clog`/`cdebug` (and the plan-003 `cwarn`/`cerror`).

**Verify**: `cargo check --all-targets --all-features` → exit 0.

### Step 2: Switch payload sites + cap the hex dump

- `session.rs:1091`: switch to `cdebug_local!` AND cap the dump to the first 256 bytes of the chunk (`&bytes[..bytes.len().min(256)]`, note truncation + total length in the line). The uncapped hex was unbounded even for the local file.
- `client_writer.rs:147` (`send-bytes`): switch to `cdebug_local!` (keep the existing 1200B escape cap).
- `daemon/control.rs:136` (decoded input events): switch to `cdebug_local!` — keystrokes are payload.
- Leave the *metrics* lines (`session.rs:1147` parse-time/geometry, `client_writer.rs:131` frame counters) on `cdebug!` — they are numbers, not payload.

**Verify**: `rg -n "cdebug!\(\"session feed_pty bytes|cdebug!\(\"send-bytes" crates/jackin-capsule/src` → no matches; `cargo nextest run -p jackin-capsule --all-features` → pass.

### Step 3: Redactor module in jackin-diagnostics

New `crates/jackin-diagnostics/src/redact.rs` (+ `pub mod redact;` in `lib.rs`), pure functions:

```rust
/// Mask token-shaped values in text bound for export. Covers:
/// KEY=VALUE where KEY matches (?i)(token|secret|key|password|passwd|credential|authorization|bearer)
/// long opaque values: base64/hex runs >= 32 chars following ":" or "=" 
/// well-known prefixes: ghp_/gho_/ghu_/github_pat_/sk-/xox[bpars]-/AKIA[A-Z0-9]{16}/eyJ (JWT)
/// PEM blocks: -----BEGIN ... PRIVATE KEY----- .. END
pub fn redact_text(input: &str) -> Cow<'_, str>;
/// redact_text + hard cap (char-boundary safe), noting truncation.
pub fn redact_and_cap(input: &str, max_bytes: usize) -> String;
```

Implement with the `regex` crate (already in the workspace tree — check `Cargo.lock`; if `jackin-diagnostics` lacks the dep, add `regex = "1"` to its `[dependencies]` — ENGINEERING.md prefers the maintained crate over hand-rolled scanning). Compile patterns once via `OnceLock`. Replacement token: `<redacted>`. Read `crates/jackin-capsule/src/exec.rs` `redact_pem`/`secrets_for_redaction` first; if its PEM logic is reusable verbatim, move/share it here and re-point exec.rs (DRY) — only if the move is mechanical; otherwise duplicate the PEM regex with a one-line comment naming the sibling.

**Verify**: unit tests in `crates/jackin-diagnostics/src/redact/tests.rs` (see Test plan) → pass.

### Step 4: Apply at the export choke points

1. `observability.rs::emit_jsonl_event_with_level`: run `message` and `detail` through `redact_text` **only for the emitted tracing event** — the JSONL file writer receives the original (the layer gets fields post-redaction… wait: the file is fed FROM this same event via the layer). Decision: redact BOTH uniformly. Rationale (accept and document): a 0600 local file with a redacted token still points the operator at which key was involved (`GH_TOKEN=<redacted>`), and one uniform path is structurally simpler than dual representations — no divergence class. Update `reference/runtime/diagnostics.mdx` ("No filtering is applied" sentence at `:134-136` becomes the redaction statement).
2. Plan-004 crash-evidence path: replace the bare cap with `redact_and_cap(evidence, 4096)`.
3. `shell_runner.rs::log_captured_output` (`:400-411`) and the `-> {first_line}` emit (`:444-449`): wrap each line with `jackin_diagnostics::redact::redact_text` before `emit_debug_line`.
4. Widen `redact_env_args` (`shell_runner.rs:84-102`): also mask the value after `--build-arg K=V` (and `-e K=V` inline form `-e=K=V` if present in call sites — grep `rg -n '"-e="' crates/` first; skip if unused), and any arg matching `--password(=|$)`/`-p<value>` for registry logins if such call sites exist (grep `rg -n '"--password"' crates/` — if none, note it and skip). Apply `redact_env_args` in `docker_client.rs::exec_capture` (`:608`) before the `debug_log!`.

**Verify**: `cargo nextest run --all-features` → pass, including plan-001-seam assertions updated where bodies now show `<redacted>`.

### Step 5: Export-boundary characterization tests + docs

Docs paragraph in `reference/runtime/diagnostics.mdx`: payload bytes (PTY/frame/keystroke dumps) are local-file-only; exported text is masked by pattern; local files remain 0600 and are the full-fidelity record.

**Verify**: full gate — fmt, clippy, `cargo nextest run --all-features` → exit 0.

## Test plan

`redact/tests.rs` (pure): masks `GH_TOKEN=abc123`, `Authorization: Bearer eyJ…`, `ghp_<36 chars>`, `sk-<48 chars>`, PEM block, 64-char hex after `=`; does NOT mask ordinary prose, short values (`PORT=8080`), git SHAs in context `commit 5d3661cff` (7-12 hex must survive — assert!), URLs without credentials; cap truncates at char boundary with marker. Seam tests (plan 001 infra): `subprocess_stdout_is_redacted_in_export` — emit a captured line containing a fabricated `FAKE_TOKEN=zzzz…` via `emit_debug_line("cmd.stdout", …)` under an active run + test subscriber → exported body contains `<redacted>`, not the value (use an obviously fake value; never a real-looking live credential in the test source). `pty_bytes_never_export` — call the capsule macro path? (capsule macros write files — instead assert by grep in Done criteria + the `cdebug_local!` unit: it must not call `bridge_log`; test via a `#[cfg(test)]` bridge-call counter in `telemetry.rs`). Pattern: plan 001 tests.

## Done criteria

- [ ] `rg -n "feed_pty bytes|send-bytes" crates/jackin-capsule/src` shows only `cdebug_local!` call sites
- [ ] `rg -n "bridge_log" crates/jackin-usage/src/logging.rs` → hits only inside `clog!`/`cdebug!`/`cwarn!`/`cerror!` (not `cdebug_local!`)
- [ ] Redactor unit tests pass incl. the git-SHA-survives case
- [ ] Seam test proves a token-shaped value in subprocess stdout exports as `<redacted>`
- [ ] `cargo nextest run --all-features` / clippy / fmt exit 0
- [ ] diagnostics.mdx updated ("No filtering" sentence replaced)
- [ ] `plans/README.md` row updated

## STOP conditions

- Redacting the JSONL file path breaks the pty-fixture extraction flow (`cargo xtask pty-fixture` reads PTY *stream* events — check what it reads: `crates/jackin-xtask/src/pty_fixture.rs`; if it depends on the raw `feed_pty bytes` hex lines in the run JSONL, the Step-2 cap/localization changes its input. STOP and report the dependency — the fixture flow may need the in-container `multiplexer.log` path instead, which still has full fidelity).
- A redaction pattern mass-matches normal output in existing tests (e.g. hex object ids) — tighten patterns; if precision below "git SHAs survive" is unreachable, STOP.
- `regex` cannot be added to `jackin-diagnostics` (dependency-graph objection in review) — report; do not hand-roll a scanner (ENGINEERING.md).

## Maintenance notes

- Reviewer scrutiny: the pattern list is a denylist of shapes — it will not catch every secret. The structural guarantee is the *payload containment* (Step 2): content channels never bridge. Text redaction is defense-in-depth.
- Follow-up (deferred): newtype for secret-bearing args so `capture` vs `capture_secret` is compiler-enforced; the roadmap debug-bundle item should reuse `redact.rs`.
- If a credential is ever confirmed to have reached a backend before this plan, treat it as burned: rotate it — deletion from the store is not sufficient.
- New payload-ish debug sites must use `cdebug_local!`; PR review should challenge any new `cdebug!` whose format args include raw buffers.
