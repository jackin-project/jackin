# jackin-test-support

Canonical fakes and role-repo seed helpers shared across jackin❯ workspace test suites — extracted so that `jackin-isolation` no longer had to dev-depend back up on `jackin-runtime` just to reach `FakeRunner`/`FakeDockerClient` (the workspace's only prod+dev dependency cycle).

## What this crate owns

- `FakeRunner` — an in-memory `jackin_core::CommandRunner` fake for subprocess injection.
- `FakeDockerClient` — an in-memory `jackin_core::DockerApi` fake.
- `seed_valid_role_repo` / `first_temp_role_repo` / `TEST_DOCKERFILE_FROM` — minimal valid role-repo fixtures for tests exercising `validate_role_repo` and role-registration temp-dir discovery.

## Architecture tier and allowed dependencies

Test-support tier: sits just above `jackin-core`. Allowed workspace dependencies: `jackin-core`, `jackin-manifest` (both leaf/domain crates that do not depend on this crate or on `jackin-runtime`). **Production crates must never depend on this crate — it is consumed via `[dev-dependencies]` only.** Anyone's dev-deps may depend on it; it may depend only downward itself.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`runner.rs`](src/runner.rs) | `FakeRunner` | — |
| [`docker.rs`](src/docker.rs) | `FakeDockerClient` | — |
| [`seed.rs`](src/seed.rs) | role-repo seed fixtures | — |

## Public API

`FakeRunner`, `FakeDockerClient`, `seed_valid_role_repo`, `first_temp_role_repo`, `TEST_DOCKERFILE_FROM` — all re-exported from the crate root. Moved byte-for-byte (behavior-preserving) from `jackin-runtime`'s former `runtime::test_support` module; `jackin-runtime`'s `install_all_test_stubs` stayed behind (it needs `jackin_image` and has no consumer outside `jackin-runtime`'s own tests).

## Dedupe candidates (follow-up, not done here)

Three fakes duplicate these types elsewhere in the workspace and were deliberately left in place by this extraction (different call sites, different plan):

- `crates/jackin/tests/common/mod.rs` — a `FakeRunner`.
- `crates/jackin-host/src/caffeinate/tests.rs` — its own `FakeDockerClient` + `FakeRunner`.
- Console `StubRunner`s (×4) — different trait, not a candidate.

Future Phase 3 test-support helpers (deterministic builders, snapshot normalization, fixed dims/theme, a `ManualClock` re-export) land here too, one PR each.

## How to verify

```sh
cargo nextest run -p jackin-test-support
cargo clippy -p jackin-test-support --all-targets -- -D warnings
cargo tree -p jackin-test-support -i --edges normal   # inverse tree: no production crate should list it
```
