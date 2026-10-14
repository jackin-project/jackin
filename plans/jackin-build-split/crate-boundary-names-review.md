# Usage crate boundary and naming review

Status: evidence-only design; no extraction or build claim is approved.

This review uses committed source objects and static tree inspection only. It ran no Cargo command, compiler, test, cache, or account access.

## Source identity and method

| Source | Commit | Tree | Parent |
|---|---|---|---|
| Initial-main comparison | `0aa821a088e1bacf3d4d85a4c9faaa67faa85132` | `44a423e48497a7c2f62a8638707ce326560fca46` | `6c389d38eadab93d6d6a4005e01dbdd8c4160221` |
| Assigned task source | `5db2c19a915bf17d2795f812ab8e52b5489ce43f` | `11765a23348fdd2c990e284ff46e5a41c91394f4` | `c7aede8f9eb3296027ab652135501486d2786e00` |

The task-source `jackin-usage` tree and its four consumer manifests match initial-main byte-for-byte. The task worktree’s uncommitted OMP changes were excluded.

The inspected checkout has no repository `RTK.md`; `/root/.codex/RTK.md` and the applicable repository `AGENTS.md` were read. The assigned note path was absent in the clean signed-record worktree.

Static commands used:

- `git show -s --format='%H %T %P' <commit>` recorded source identity.
- `git ls-tree -rl <commit> crates` supplied committed Rust blob sizes and paths.
- A read-only Python aggregation grouped `.rs` blobs by crate and module path.
- `git grep -n` traced manifest edges, symbols, Turso usage, and protocol DTO ownership.
- `git diff --quiet 0aa821a 5db2c19 -- crates/jackin-usage <four consumer manifests>` exited `0`.
- `git grep` found no Turso, config, host, coordinator, or store-backend reference in the provider subtree.
- No Cargo, metadata, build, test, cache, or runtime command ran.

## Source-size context

Rust-source byte counts include tests and benches. They measure committed `.rs` blobs, not compiled code or build cost.

| Crate | Initial-main bytes / files | Task-source bytes / files |
|---|---:|---:|
| `jackin-console` | 3,053,579 / 242 | 3,065,649 / 242 |
| `jackin-capsule` | 2,214,298 / 150 | 2,214,229 / 150 |
| `jackin-runtime` | 2,126,288 / 110 | 2,299,549 / 113 |
| `jackin-usage` | 1,979,967 / 84 | 1,979,967 / 84 |
| `jackin` | 1,422,094 / 138 | 1,428,271 / 138 |
| `jackin-xtask` | 1,083,915 / 107 | 1,083,915 / 107 |
| `jackin-config` | 942,244 / 57 | 964,083 / 57 |

The usage package’s main source responsibilities are balanced in size, but their dependencies and consumers differ.

| Current module | Rust bytes / files | Production bytes | Test-path bytes |
|---|---:|---:|---:|
| `usage.rs` plus `usage/` | 837,463 / 35 | 528,134 | 309,329 |
| `host.rs` plus `host/` | 833,598 / 15 | 440,993 | 392,605 |
| `coordinator.rs` plus `coordinator/` | 146,048 / 6 | 84,830 | 61,218 |
| `usage_snapshot_store` | 65,769 / 2 | 44,440 | 21,329 |
| `token_monitor.rs` plus `token_monitor/` | 55,555 / 14 | 36,695 | 18,860 |
| `store_backend.rs` plus tests | 7,950 / 2 | 2,394 | 5,556 |

The provider subtree contains fifteen provider adapters, shared normalization, formatting, and view helpers. `host/discovery.rs` calls provider-specific parsing and snapshot helpers in production.

## Current responsibility and dependency map

| Responsibility | Current owner | Boundary evidence and consumers |
|---|---|---|
| Provider parsing, credential formats, API polling, normalization | `jackin-usage::usage` | Fifteen adapters return protocol-owned usage views; host discovery calls them. Capsule and usage-FFI call presentation helpers. |
| Credential discovery, host account inventory, broker, projection | `jackin-usage::host` | CLI, usage-broker binary, capsule relay, runtime relay, and usage-FFI call host APIs. |
| Per-account scheduling and persisted coordinator state | `jackin-usage::coordinator` | Runtime relay, broker implementation, and runtime/FFI integration tests use its executor and capability APIs. |
| Snapshot history and database chokepoint | `usage_snapshot_store`, `store_backend` | Host account loading and tests use snapshot storage. Jackin CLI cache and OpenCode token monitoring also use the Turso chokepoint. |
| Agent token-session monitoring | `jackin-usage::token_monitor` | Capsule re-exports it; OpenCode polling reads its SQLite message table through `store_backend`. |
| CLI account cache | `jackin/src/cli/usage/store.rs` | Jackin owns SQL today; path is `data_dir/daemon/accounts.db`, schema version `1`, keyed with core account hashing. |

Only four manifests directly depend on `jackin-usage`: `jackin`, `jackin-capsule`, `jackin-runtime`, and `jackin-usage-ffi`. The package has no usage-specific feature gates; its default feature set is empty.

`jackin-usage` directly depends on core, config, protocol, diagnostics, telemetry, and `turso`. Provider modules use core, protocol, telemetry, and network libraries, but do not import host, config, coordinator, Turso, or the store backend.

The current dependency direction can therefore be made one-way: host runtime depends on provider adapters. The provider package must not depend on host runtime or operator configuration.

Existing ownership should remain explicit:

- `jackin-core::account_key_hash` owns the stable hash algorithm used by usage persistence.
- `jackin-config` owns `AiProvider`, `AccountConfig`, and operator account configuration.
- `jackin-protocol` owns `FocusedUsageView`, `AccountUsageSnapshotView`, broker capabilities, and `UsageProjectionV1` wire contracts.
- `jackin-instance::auth` owns selected auth-source snapshots; `jackin-core::LaunchSelection` owns launch selection.
- `jackin-process` remains generic subprocess transport. Usage-specific credential policy does not belong there.

Do not create a parallel usage DTO crate. Provider-local request types may cross the host/provider Rust API, but they must not duplicate serialized protocol types or canonical account identity.

## Stable names and rejected alternatives

| Name | Disposition | Reason |
|---|---|---|
| `jackin-usage-providers` | Preferred | Names provider adapters and normalization directly, independent of execution location. |
| `jackin-usage-host` | Preferred | Names host discovery, broker, coordinator, projection, storage, and token monitoring. |
| `jackin-session-usage` | Reject | “Session” describes token monitoring, not the fifteen-provider usage capability or account broker. |
| `jackin-host-usage` | Reject | Less consistent with `jackin-usage-ffi`; “host usage” can mean usage measured on a host. |
| `jackin-usage-model` / `jackin-usage-dto` | Reject | Shared wire DTOs already belong to `jackin-protocol`; a second owner invites type drift. |
| `jackin-usage-store` | Defer | The storage surface is small and has three current Turso call paths; a standalone crate would be premature. |

Keep Turso within `jackin-usage-host` for the first boundary. That package can own snapshot history, the CLI account cache, and OpenCode’s SQLite reader without an extra tiny crate.

The existing xtask enforces `jackin-usage` as the sole Turso owner. Update that gate and its tests to enforce the new host package after extraction.

## Bounded first extraction

Split the mixed package into `jackin-usage-providers` and `jackin-usage-host`. Move provider sources, `process_telemetry.rs`, and tests into providers. Move host, broker, coordinator, storage, and token-monitor sources into host.

The public dependency is one-way: `jackin-usage-host` depends on `jackin-usage-providers`. Expose an explicit Rust adapter API for validated provider inputs; keep provider-specific parsing types local where possible.

At broker and wire boundaries, return existing `jackin-protocol` views. The provider crate has no dependency on host, config, or Turso.

Keep account selection and credential admission in the host crate. Keep account-key hashing in core and the wire projection types in protocol. Keep snapshot storage and `daemon/accounts.db` ownership in host.

Update the four direct consumer manifests and imports. Jackin and runtime primarily need host APIs; capsule and usage-FFI need host APIs plus provider presentation helpers. Remove capsule’s broad usage wildcard re-export and migrate its callers directly.

The host API must replace Jackin CLI’s direct `store_backend` access. Preserve the `daemon/accounts.db` path, schema version `1`, uniqueness tuple, and `account_key_hash` behavior.

Move the protocol contract JSON fixture under `jackin-protocol` and update its `include_str!` users. Protocol tests currently read a fixture under `jackin-usage`, despite having no package dependency on usage.

Update `jackin-xtask` architecture inventory and desktop build checks for the new package names. Preserve the usage-FFI static-library target and native macOS verification.

## Migration coverage and dependency gates

Move provider parser, normalization, response, and credential-format tests with the provider code. Keep them offline and assert redaction on error paths.

Retain host discovery, credential-denial, account-identity, broker lease, coordinator, and projection tests in `jackin-usage-host`. Keep CLI account-cache and snapshot-schema tests beside host storage.

Retain runtime relay, usage-broker lifecycle, Capsule broker-client, and FFI bridge tests in their existing consumer packages. Keep protocol serialization fixtures and compatibility tests in protocol.

Use Mise and MBX for every compile or test command. Focused commands should target each new package; run the full four consumer-package suite and xtask gates after migration.

The provider manifest must not declare Turso or host/config dependencies. The host crate remains the only Turso owner, and xtask must enforce that rule.

## Build evidence and provisional measurement plan

The available build evidence is from initial-main source `0aa821a`, not the post-accepted-main baseline. No split scenario has run.

Initial-main cold `-p jackin` observations were `90.373`, `91.055`, and `98.997` seconds. Their approximately `9.5%` span is contended and had zero MBX hits or misses.

Initial-main warm-store fresh-target outer observations were `24.591`, `18.082`, and `17.518` seconds. They restored about `4.223` GB each and reported `865` hits and `181` misses; the range is about `39%` of the median.

The recorded critical path includes `turso_core` for `27.85` seconds before `jackin-usage` metadata and its runtime/CLI consumers. The split keeps Turso in the host dependency closure, so it does not predict a full-CLI cold-path reduction.

The same full build recorded a separate `aws-lc-sys` build-script interval of `44.48` seconds. The source report does not attribute that unit solely to usage, so this split does not claim to remove it.

The expected benefit is a package-scoped provider edit loop that omits host and Turso compilation. This is a structural expectation, not a measured gain.

Before implementation, record a second accepted-main baseline with at least three paired cold, warm, and provider-focused edit-loop repetitions. Keep profile, features, target, Mise/Rust/MBX versions, MBX store state, and resource budgets identical.

For cold comparisons, use a provisional materiality floor above the observed `9.5%` initial-main span. Recalculate that floor from the accepted-main repetitions before extraction.

Use `mise exec --deny-net -- mbx test --locked --offline -p jackin-usage --lib usage::` for the pre-split provider test target. Use the same filter with `jackin-usage-providers` after extraction.

For edit-loop pairs, apply the same neutral test-source edit within the provider subtree. Reuse a warm target and cache, then record compile and test wall times separately.

For warm comparisons, the existing full-build range is too wide to set a provider-specific threshold. Measure a scoped warm provider loop first; require improvement beyond its measured baseline variation.

Record medians, minimums, maximums, peak memory, MBX hits, misses, bypasses, and source/tool provenance. Treat an overlapping range or a result within baseline variation as inconclusive.

Measure full `-p jackin` cold and warm scenarios separately. Do not count their unchanged Turso work as an extraction benefit, and report any regression explicitly.

## Open gates

- A post-accepted-main baseline and scoped provider-only measurements remain pending.
- Provider input types and visibility need source-level interface design before moves begin.
- The complete consumer tests, protocol fixture relocation, xtask name changes, and macOS FFI path remain unverified.
- No extraction, compilation, test, performance, or full-CLI regression result is claimed here.
