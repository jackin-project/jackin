# jackin-core

Shared jackin❯ vocabulary and pure cross-surface projections. This L0 leaf has no jackin❯ dependencies, async runtime, filesystem, subprocess, or side effects.

## What this crate owns

- Domain nouns every other crate speaks in: agent identity, instance, isolation, manifest fragments, env model, status, launch progress, operator notices.
- Ports, constants, paths, and selector/URL/path helpers reused by higher crates.
- Operator-information facts projected into TermRock's neutral table contract.
- Small self-contained ANSI and host-color tokens re-exported by presentation crates.

## Architecture tier and allowed dependencies

**L0 leaf/domain + pure projections.** No workspace dependencies or effects. TermRock and Ratatui types are limited to pure product projections (Debug-info facts, brand/domain color tokens, Role accessors). Presentation layout, hover builders, terminal-protocol scroll decode, and surface widgets live in `jackin-console` / `jackin-capsule` / `jackin-launch` — not here.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`agent.rs`](src/agent.rs) · [`agent/`](src/agent) | agent identity | [`tests.rs`](src/agent/tests.rs) |
| [`instance.rs`](src/instance.rs) | instance type | — |
| [`manifest.rs`](src/manifest.rs) | manifest fragment | — |
| [`status.rs`](src/status.rs) · [`status/`](src/status) | status type | [`tests.rs`](src/status/tests.rs) |
| [`operator_notice.rs`](src/operator_notice.rs) | operator notice | — |
| [`auth.rs`](src/auth.rs) | auth model | — |
| [`account_key.rs`](src/account_key.rs) · [`account_key/`](src/account_key) | account key | [`tests.rs`](src/account_key/tests.rs) |
| [`env_model.rs`](src/env_model.rs) · [`env_model/`](src/env_model) | env model | [`tests.rs`](src/env_model/tests.rs) |
| [`env_value.rs`](src/env_value.rs) · [`env_value/`](src/env_value) | env value | [`tests.rs`](src/env_value/tests.rs) |
| [`paths.rs`](src/paths.rs) · [`paths/`](src/paths) | host paths + `PathsError` | [`tests.rs`](src/paths/tests.rs) |
| [`isolation.rs`](src/isolation.rs) | isolation type | — |
| [`isolation_record.rs`](src/isolation_record.rs) | isolation record | — |
| [`worktree_dirty.rs`](src/worktree_dirty.rs) · [`worktree_dirty/`](src/worktree_dirty) | worktree-dirty check | [`tests.rs`](src/worktree_dirty/tests.rs) |
| [`runner.rs`](src/runner.rs) | `CommandRunner` port | — |
| [`clock.rs`](src/clock.rs) · [`clock/`](src/clock) | `Clock` port + `ManualClock` | [`tests.rs`](src/clock/tests.rs) |
| [`launch_progress.rs`](src/launch_progress.rs) | launch progress | — |
| [`prompt_result.rs`](src/prompt_result.rs) | prompt result | — |
| [`selector.rs`](src/selector.rs) | selector | — |
| [`docker.rs`](src/docker.rs) | docker types | — |
| [`docker_security.rs`](src/docker_security.rs) · [`docker_security/`](src/docker_security) | docker security | [`tests.rs`](src/docker_security/tests.rs) |
| [`debug_log.rs`](src/debug_log.rs) | `debug_log` stub | — |
| [`build_log_sink.rs`](src/build_log_sink.rs) | build-log sink stub | — |
| [`host_colors.rs`](src/host_colors.rs) | product brand/domain RGB tokens | — |
| [`tui_theme.rs`](src/tui_theme.rs) | pure Theme/Role access helpers for product paint (no widgets/layout) | product-token tests |
| [`ansi_tokens.rs`](src/ansi_tokens.rs) | ansi tokens | — |
| [`operator_info.rs`](src/operator_info.rs) | jackin❯ Debug-info facts, row policy, and pure projection into TermRock `DetailTable` | cross-surface suites |
| [`standalone_dialog.rs`](src/standalone_dialog.rs) · [`standalone_dialog/`](src/standalone_dialog) | standalone dialog | [`tests.rs`](src/standalone_dialog/tests.rs) |
| [`url_text.rs`](src/url_text.rs) | url text | — |
| [`path_text.rs`](src/path_text.rs) · [`path_text/`](src/path_text) | path text | [`tests.rs`](src/path_text/tests.rs) |
| [`op_cache.rs`](src/op_cache.rs) · [`op_cache/`](src/op_cache) | op cache | [`tests.rs`](src/op_cache/tests.rs) |
| [`op_probe_error.rs`](src/op_probe_error.rs) · [`op_probe_error/`](src/op_probe_error) | typed `op` probe errors | [`tests.rs`](src/op_probe_error/tests.rs) |
| [`op_reference.rs`](src/op_reference.rs) · [`op_reference/`](src/op_reference) | op reference | [`tests.rs`](src/op_reference/tests.rs) |
| [`op_types.rs`](src/op_types.rs) | op types | — |
| [`constants.rs`](src/constants.rs) | shared constants | — |
| [`container_paths.rs`](src/container_paths.rs) · [`container_paths/`](src/container_paths) | container-side `/jackin/` path chokepoint | [`tests.rs`](src/container_paths/tests.rs) |

## Public API

Plan 019 surface: implementation modules are **private**; the intentional API is the **root re-export list** in [`lib.rs`](src/lib.rs) (`use jackin_core::{Agent, JackinPaths, …}`).

Remaining root `pub mod`s (individually justified):

| Module | Why public |
|---|---|
| `container_paths` | Namespace for container-side `/jackin/` path constants (`use jackin_core::container_paths`) across runtime/capsule/usage |
| `debug_log` | Hosts `debug_log!` macro + sink; `#[macro_export]` shares the `jackin_core::debug_log` path with the module name |
| `operator_info` | Product-owned cross-surface Debug-info facts and TermRock projection; effects and terminal ownership remain in consumers |

Higher crates implement the port traits defined here (e.g. `CommandRunner`) and pass the domain types through.

Typed construction/parse errors (thiserror): `ParseProfileError`, `ParseMountIsolationError`, `ParseAgentError`, `SelectorError`, `EnvCycleError`, `PathsError`, `OpProbeError`.

## How to verify

```sh
cargo nextest run -p jackin-core
cargo clippy -p jackin-core --all-targets -- -D warnings
```
