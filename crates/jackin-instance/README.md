# jackin-instance

Role instance lifecycle: the instance index, the per-role state directory, auth provisioning, and container naming. An "instance" is the on-disk + in-Docker state for a single running (or restorable) role session.

## What this crate owns

- The instance index and lifecycle (`lib`, `tests`), role-state directory management, and container naming (`naming`).
- Auth provisioning for an instance (`auth`) and the instance's view of its manifest (`manifest`).

## Architecture tier and allowed dependencies

**L1 application.** Allowed workspace dependencies: `jackin-core`, `jackin-config`, `jackin-manifest`, `jackin-diagnostics`. No presentation or runtime dependencies — instance lifecycle stays a domain/app concern above the leaf and below orchestration.

## Structure

- `src/auth.rs` / `src/auth/` — auth provisioning
- `src/manifest.rs` / `src/manifest/` — instance manifest view
- `src/naming.rs` / `src/naming/` — container/instance naming
- `src/lib.rs`, `src/tests.rs` — index + lifecycle + tests

## Public API

Instance identity, the role-state directory contract, and naming used by `jackin-runtime`, `jackin-isolation`, and the host CLI. Naming is shared with the capsule side via `jackin-protocol`.

## How to verify

```sh
cargo nextest run -p jackin-instance
cargo clippy -p jackin-instance --all-targets -- -D warnings
```

