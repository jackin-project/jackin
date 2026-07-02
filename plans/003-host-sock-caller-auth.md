# Plan 003: Authenticate callers of the `host.sock` credential resolver

> **Executor instructions**: This is a security-hardening plan. Follow step by step, run every
> verification command, and honor STOP conditions. Update `plans/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 46511939d..HEAD -- crates/jackin-runtime/src/exec_host.rs crates/jackin-runtime/src/runtime/launch`
> On any change, compare "Current state" against live code; mismatch → STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none (but is item 1 of the hardening cluster, plan 043)
- **Category**: security
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`/jackin/run/host.sock` is the host-side credential-resolution boundary. The operator credential
**picker** is the intended per-use approval gate for on-demand secrets (`on_demand = true`) — the ones
deliberately withheld from launch-time env. The picker is enforced daemon-side *before the daemon
connects* to this socket, but the socket itself performs **no caller authentication**: it is
bind-mounted into the container and reachable by any in-container process running as the host UID. So a
compromised/prompt-injected agent can connect directly and resolve the **entire** configured on-demand
binding set without the picker ever prompting — collapsing on-demand protection down to the same
exposure as launch-time injection, defeating the reason the mode exists. This is acknowledged in the
module doc as remaining hardening tracked on the `jackin-exec` roadmap item; this plan closes it (or
narrows it) before the larger daemon migration lands.

## Current state

- `crates/jackin-runtime/src/exec_host.rs:27-35` — the module doc states the gap explicitly:
  > This allow-list bounds *what* can be resolved to the operator-configured set; it does NOT enforce
  > per-use operator *approval*. … `/jackin/run/host.sock` is reachable by any in-container process, so
  > a compromised agent could connect directly and resolve the whole configured set without a picker.
  > Binding it to the daemon (e.g. `SO_PEERCRED` or a one-time per-confirm token) is remaining hardening.
- `crates/jackin-runtime/src/exec_host.rs:92-108` — `run_listener` binds the `UnixListener` and locks
  the **parent dir** to `0o700` (restricts other *host* UIDs; does not stop the in-container agent,
  which runs as the host UID).
- `crates/jackin-runtime/src/exec_host.rs:~133-191` — `handle_connection` validates each request against
  `allowed_bindings` but performs **no peer/caller identity check** before resolving.
- The legitimate caller is the in-container **capsule daemon** on behalf of an operator-approved picker
  action; a direct agent connection is the threat.

Conventions: `unsafe_code = "forbid"` workspace-wide, so raw `getsockopt(SO_PEERCRED)` via `libc` is not
allowed inline — use a safe wrapper. `tokio::net::UnixStream` exposes peer credentials safely; see
below.

## Design options (pick one in Step 1, record the choice)

1. **`SO_PEERCRED` peer-UID/PID check** — `tokio::net::UnixStream` on Linux supports
   `stream.peer_cred()` (returns `UCred` with uid/gid/pid) with **no `unsafe`**. Reject connections whose
   peer PID is not the capsule daemon's PID (the launch path knows the daemon PID / can pass it in). This
   is the cheapest robust gate on Linux. **Caveat:** on Docker Desktop (macOS) the socket is bridged via
   a relay, so `peer_cred` may report the relay, not the agent — verify behavior there and fall back to
   option 2 if peercred is unusable on that platform.
2. **One-time per-confirmation token** — the daemon mints a short-lived token when the operator approves
   a picker action; the resolver only honors a request carrying a currently-valid token. Works regardless
   of socket bridging but needs a token-mint/verify channel between picker approval and the resolver.

Recommendation: implement **option 1 (`peer_cred`)** where the socket is a same-kernel `UnixStream`
(Linux), and treat Docker Desktop as a documented residual (the daemon-migration on the roadmap subsumes
it). If `peer_cred` is unavailable/unreliable on the target platform, STOP and report before building
option 2 — it is a larger change the operator should scope.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Build | `cargo check -p jackin-runtime --all-targets` | exit 0 |
| Test | `cargo nextest run -p jackin-runtime -E 'test(/exec_host/)'` | all pass |
| Clippy | `cargo clippy -p jackin-runtime -- -D warnings` | exit 0 |

## Scope

**In scope:** `crates/jackin-runtime/src/exec_host.rs`, its `tests.rs`, and the launch call site that
spawns the listener (`start_for_container`, `exec_host.rs:80-90`) if it must thread the daemon PID/token.

**Out of scope:**
- The capsule-daemon side of the picker flow beyond what's needed to supply the daemon PID/token.
- The `allowed_bindings` allow-list logic (keep it; it is the *what*-bound, this adds the *who*-bound).
- Any change to the `op read` invocation (that's plan 001).

## Steps

### Step 1: Add a caller-identity gate in `handle_connection`

Before resolving, obtain the peer credential (`stream.peer_cred()` on the accepted `UnixStream`) and
reject (close, log via `jackin_diagnostics::debug_log!`) any connection whose peer does not match the
expected daemon identity. Thread the expected identity (daemon PID, or an accepted-UID policy) from the
launch path through `start`/`start_for_container` into the listener. Record in a code comment which
option (1 or 2) was chosen and why.

### Step 2: Preserve the legitimate path

Ensure the capsule daemon's own connection still authenticates (it is the same-kernel peer on Linux). Add
a `clog!`/`debug_log!` line on a rejected connection so an operator sees "resolver rejected an
unauthenticated caller" in diagnostics.

**Verify**: `cargo clippy -p jackin-runtime -- -D warnings` → exit 0 (confirms no `unsafe` slipped in).

### Step 3: Tests

- Positive: a connection from the expected peer identity resolves an allow-listed binding.
- Negative: a connection from a non-matching peer identity is rejected without resolving.
  Use the existing `exec_host/tests.rs` harness (it already exercises the allow-list); model the peer-cred
  case after however that test constructs the `UnixStream` pair.

**Verify**: `cargo nextest run -p jackin-runtime -E 'test(/exec_host/)'` → all pass incl. new tests.

## Done criteria

- [ ] `handle_connection` rejects a connection whose peer identity does not match the daemon (test proves it)
- [ ] Legitimate daemon path still resolves (positive test passes)
- [ ] `cargo clippy -p jackin-runtime -- -D warnings` exits 0 and **no `unsafe`** was added
      (`grep -rn "unsafe" crates/jackin-runtime/src/exec_host.rs` → no new matches)
- [ ] Chosen approach (peercred vs token) documented in a code comment and in this plan's row note
- [ ] `plans/README.md` row updated
- [ ] The module doc at `exec_host.rs:27-35` is updated to reflect the new gate (and any residual, e.g.
      Docker Desktop bridging)

## STOP conditions

- `peer_cred()` reports the relay rather than the agent on the target platform (Docker Desktop): stop and
  report — option 2 (token) is a larger scoped change for the operator to approve.
- Threading the daemon identity into the listener requires touching the capsule protocol beyond a single
  parameter: report the surface before expanding scope.
- Any step tempts you toward `unsafe` / raw `libc` — do not; find the safe `tokio`/`std` API or STOP.

## Maintenance notes

- This gate becomes redundant once the reactive-daemon program (plan 042) moves credential resolution
  in-daemon; note that in the code comment so a future maintainer removes it deliberately, not by accident.
- Reviewer should confirm the negative test actually exercises a *different* peer, not just an unbound ref.
- Update the `jackin-exec` roadmap item's Status/Related Files (per repo's docs gate) to reflect that
  caller auth landed (or landed for Linux with a documented Docker Desktop residual).
