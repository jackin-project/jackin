# Crate and Build Plan

- Status: IN PROGRESS
- Evidence: Static repository inspection at base `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`.

## Workspace shape

- The [workspace manifest](../../Cargo.toml) lists 31 member crates.
- The workspace excludes `crates/jackin-lints`.
- The `jackin` package builds four binaries: `jackin`, `jackin-role`, `jackin-usage-broker`, and `build-jackin-capsule`.
- The `jackin` package directly depends on workspace crates for configuration, runtime, launch, image, Docker, console, protocol, usage, and telemetry.
- The [xtask crate guide](../../crates/jackin-xtask/README.md) defines build and CI tooling outside runtime crates.

## Build metadata

Six package build scripts call `jackin_build_meta::derive_workspace_crate_version`: `jackin`, `jackin-image`, `jackin-docker`, `jackin-launch`, `jackin-runtime`, and `jackin-capsule`.

The `jackin-diagnostics` build script records the Rust compiler version. These remain static source findings. The first MBX attempt failed before compilation; a later attempt succeeded once. See [build results](build-results.md#reviewed-linker-v2-source-proof).

## Candidate scope

The crate-design worker identifies `jackin` glue as the candidate split target.

It identifies no dependent workspace crate for that target. It has not ranked side binaries or console host extraction. Those decisions need build evidence.

Do not treat this candidate as approved. Record measured dependencies and build effects after the baseline and extraction runs.

## Usage crate naming proposals

Two independent Luna proposals use `jackin-session-usage` and `jackin-host-usage`. Another names the boundary by data, such as token or provider usage. Protocol owns shared DTOs.

These names are alternatives, not selected crate boundaries. No proposal has completed the consumer map, dependency graph, extraction targets, or expected build effect. Keep those items IN PROGRESS until source ownership and measurements support a design.

| Design item | Status | Evidence needed |
|---|---|---|
| Crate names and public boundaries | IN PROGRESS | Compare session/host responsibilities with token/provider responsibilities. |
| Direct and indirect consumers | IN PROGRESS | Trace workspace dependencies and binary entry points. |
| Protocol DTO ownership | IN PROGRESS | Define stable protocol types and their consumers. |
| Extraction targets | IN PROGRESS | Select targets after the dependency map. |
| Expected build effect | IN PROGRESS | Measure repeated baseline and split scenarios. |

Do not infer ownership from a proposed crate name. Do not claim a performance benefit before measurements.

## Interface requirements

The extraction review must map each interface to its current consumer, contract, and verification command.

| Interface | Status | Required record |
|---|---|---|
| CLI | IN PROGRESS | Commands, arguments, output, and binary consumers. |
| Account discovery | IN PROGRESS | Host inputs, account selection, and consumers. |
| Authentication | IN PROGRESS | Credential boundaries, inputs, and consumers. |
| Launch | IN PROGRESS | Role inputs, launch outputs, and runtime consumers. |
| Console and TUI | IN PROGRESS | Entry points, host services, and consumers. |
| Storage | IN PROGRESS | Schemas, migrations, and read/write consumers. |
| FFI and native targets | IN PROGRESS | Native targets, toolchains, and linking consumers. |
| Release | IN PROGRESS | Release artifacts, signing boundary, and downstream consumers. |

The workspace manifest identifies four binary targets in the `jackin` package: `jackin`, `jackin-role`, `jackin-usage-broker`, and `build-jackin-capsule`. Verify each target after extraction. No dependent workspace crate was identified for the candidate target in the static review. Confirm that result during the complete consumer inventory.

## Architect and account route

- The Architect role repository is `jackin-project/jackin-the-architect`, at reported main SHA `2cf461e2fed1b95d9fd1e7ba74c10d4d8b1c685d`.
- Its v1alpha5 manifest contains provider tables that Jackin's current v1alpha7 schema rejects.
- Jackin validates the role schema before image planning.
- Manifest validation: `crates/jackin-manifest/src/manifest.rs` and `crates/jackin-manifest/src/repo.rs`.
- Runtime image planning: `crates/jackin-runtime/src/runtime/launch/image_plan.rs`.
- Runtime repository cache: `crates/jackin-runtime/src/runtime/repo_cache.rs`.
- Strict manifest types: `crates/jackin-core/src/manifest.rs`.
- Migration code and tests: `crates/jackin-manifest/src/migrations.rs` and `crates/jackin-manifest/src/manifest/tests.rs`.
- Account configuration discovery: `crates/jackin-config/src/accounts/discovery.rs`.
- Host usage discovery and Codex usage: `crates/jackin-usage/src/host/discovery.rs` and `crates/jackin-usage/src/usage/codex.rs`.
- Authentication and launch configuration: `crates/jackin-instance/src/auth.rs` and `crates/jackin-runtime/src/runtime/launch/account_config.rs`.

Manifest and account route findings remain static. See [Debian results](debian-results.md).

## Owners

`crate_design` owns crate sizing and extraction boundaries. `architect_contract` owns role compatibility. `debian_codex_route` owns the host account route. Their work remains IN PROGRESS for implementation decisions.

See [build results](build-results.md), [CI coverage](ci-coverage.md), and [reviews](reviews.md).
