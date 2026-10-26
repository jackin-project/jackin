# jackin-core

Shared jackin❯ vocabulary and pure cross-surface projections; no workspace dependencies, async runtime, filesystem, subprocess or effects.

## What this crate owns

- Domain nouns every other crate speaks in: agent identity, instance, isolation, manifest fragments, env model, status, launch progress, operator notices.
- Ports, constants, paths, and selector/URL/path helpers reused by higher crates.
- Renderer-neutral product identity tokens live in [`jackin-brand`](../jackin-brand/); terminal protocol encoding and Ratatui adapters live in their presentation owners.

Cross-surface **product presentation** (Debug-info paint, product brand/domain Ratatui tokens, shared product chrome composition) lives in [`jackin-tui`](../../ui/jackin-tui/), not here.

## Architecture tier and allowed dependencies

**L0 domain + pure projections.** No Ratatui frame paint, layout adapters or widget bodies. `jackin-tui`/surfaces (`jackin-console`, `jackin-capsule`, `jackin-launch`) own presentation layout, hover builders, terminal-protocol scroll decode, Debug-info rendering and product tokens. Resolve neutral TermRock Theme/Role at the surface call site.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`agent.rs`](src/agent.rs) · [`agent/`](src/agent) | agent identity | — |
| [`instance.rs`](src/instance.rs) | instance type | — |
| [`manifest.rs`](src/manifest.rs) | manifest fragment | — |
| [`status.rs`](src/status.rs) | status type | — |
| [`operator_notice.rs`](src/operator_notice.rs) | operator notice | — |
| [`auth.rs`](src/auth.rs) | auth model | — |
| [`account_key.rs`](src/account_key.rs) | account key | — |
| [`claude_keychain.rs`](src/claude_keychain.rs) | Claude Code macOS Keychain service derivation (`claude_keychain_scope`) shared by instance provisioning and the usage probe | — |
| [`env_model.rs`](src/env_model.rs) | env model | — |
| [`env_value.rs`](src/env_value.rs) | env value | — |
| [`paths.rs`](src/paths.rs) | host paths + `PathsError` | — |
| [`isolation.rs`](src/isolation.rs) | isolation type | — |
| [`isolation_record.rs`](src/isolation_record.rs) | isolation record | — |
| [`worktree_dirty.rs`](src/worktree_dirty.rs) | worktree-dirty check | — |
| [`runner.rs`](src/runner.rs) | `CommandRunner` port | — |
| [`clock.rs`](src/clock.rs) | `Clock` port + `ManualClock` | — |
| [`launch_progress.rs`](src/launch_progress.rs) | launch progress | — |
| [`prompt_result.rs`](src/prompt_result.rs) | prompt result | — |
| [`selector.rs`](src/selector.rs) | selector | — |
| [`docker.rs`](src/docker.rs) | docker types | — |
| [`docker_security.rs`](src/docker_security.rs) | docker security | — |
| [`build_log_sink.rs`](src/build_log_sink.rs) | build-log sink stub | — |
| [`standalone_dialog.rs`](src/standalone_dialog.rs) | standalone dialog **port** (trait + sink; render impl in `jackin-launch`) | — |
| [`url_text.rs`](src/url_text.rs) | url text | — |
| [`path_text.rs`](src/path_text.rs) | path text | — |
| [`op_cache.rs`](src/op_cache.rs) | op cache | — |
| [`op_probe_error.rs`](src/op_probe_error.rs) | typed `op` probe errors | — |
| [`op_reference.rs`](src/op_reference.rs) | op reference | — |
| [V2](src/profile_material_v2.rs) | Profile credential source/material proofs | [guide](docs/profile-credential-proofs.md) |
| [`op_types.rs`](src/op_types.rs) | op types | — |
| [`constants.rs`](src/constants.rs) | shared constants | — |
| [`container_paths.rs`](src/container_paths.rs) | container-side `/jackin/` path chokepoint | — |

## Public API

Plan 019 surface: implementation modules are **private**; the intentional API is the **root re-export list** in [`lib.rs`](src/lib.rs) (`use jackin_core::{Agent, JackinPaths, …}`).

Remaining root `pub mod`s (individually justified):

| Module | Why public |
|---|---|
| `container_paths` | Namespace for container-side `/jackin/` path constants (`use jackin_core::container_paths`) across runtime/capsule/usage |

Higher crates implement the port traits defined here (e.g. `CommandRunner`) and pass the domain types through. Debug-info product presentation is `jackin_tui::operator_info`.

Typed construction/parse errors (thiserror): `ParseProfileError`, `ParseMountIsolationError`, `ParseAgentError`, `SelectorError`, `EnvCycleError`, `PathsError`, `OpProbeError`.

## Profile credential proofs

V2 is the only active profile-proof API. The unused normalized V1 helper is retired. No in-repository consumer or persisted-record owner was found, so no population conversion was performed. Untagged and `.v1` records reject; no live flow is active. Account/broker fingerprints remain separate. Fingerprints show change/equality, not authentication, authorization, credential validity or byte custody.

## How to verify

```sh
cargo nextest run -p jackin-core
cargo clippy -p jackin-core --all-targets -- -D warnings
```
