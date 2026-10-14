# TASK-006 comparison: tooling tip T versus the integration worktree

Read-only comparison. No git repo was edited, no commit was created, and cargo, mbx, and mise were not run.

This report does not approve TASK-006.

## Bounds

| Tree | Path | SHA |
|---|---|---|
| T parent / merge-base with older main | `/Users/donbeave/Projects/jackin-project/wt-baseline-T` | `9b6e1d92608fed65579b0ab5bb4b04d2a8dd602b` |
| T exclusive commits, oldest first | same | `f8fd6f0f1803470b2d5efeac93482f11a2debee8` `refactor: simplify mise task surface`; `9ef617d014df15c6d967501bcbbdcb3689ed453c` `chore: modernize Rust and mise tooling` |
| T tip | same | `9ef617d014df15c6d967501bcbbdcb3689ed453c` |
| Integration worktree | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` | `274665c0f5f06061920326a76a6ec0007deb1f36` on `integration/branch-consolidation-20261013` |
| Policy tip, reference only | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` |

Range `9b6e1d92..9ef617d0` changes 74 files. Integration already uses the layered crate roots (`crates/apps`, `crates/core`, `crates/services`, `crates/adapters`, `crates/tools`, `crates/ui`). Comparisons below use those destinations, not T's flat `crates/jackin-*` paths.

Verdicts:

- **PORT** — a behavior from these two commits is still missing and a later implementer should add it under the integration worktree.
- **ALREADY_PRESENT** — the integration tree already has the same behavior, or a newer equivalent.
- **REJECT** — do not copy this T change onto the integration tree.

## PORT

### Generated CI still installs the pre-bump toolchain

**Files:** `.github/workflows/ci.yml` (T); `.velnor/config.toml` was not changed by either commit.

T's only workflow edit is a generated pin refresh: Rust `1.98.1` to `1.99.0`, MBX `mr-boxington@1.21.0` to `1.22.0`, and Mise cache/version `2026.9.18` to `2026.10.3`. The file header says it is generated from `.velnor/config.toml`. The Velnor config does not hardcode those versions; both T and integration set `[stacks.rust] compile_driver = "mbx"`.

Integration source pins are already newer than T:

- `rust-toolchain.toml` channel `1.99.0`
- `mise.toml` `min_version = "2026.10.7"`, `rust = { version = "1.99.0", mr_boxington = true }`, `mr-boxington = "1.23.0"`
- `mise.lock` MBX `1.23.0`

The checked-in workflow was not regenerated. `.github/workflows/ci.yml` still has `RUSTUP_TOOLCHAIN: 1.98.1`, `rust@1.98.1` (272 install/exec sites), `mr-boxington@1.21.0` (188 sites), and Mise `version: 2026.9.18`. It contains no `1.99.0`.

**Reason:** the exclusive T behavior is "the generated workflow installs the reviewed Rust, MBX, and Mise pins." That alignment is missing. Copying T's `ci.yml` would install MBX 1.22.0 and Mise 2026.10.3 and would replace layered package paths with the flat layout.

### MBX-only compiler-cache policy text

**Files:**

- `docs/content/research/engineering/build-performance/rust-build-cache-hygiene.mdx`
- `docs/content/research/engineering/build-performance/shared-ci-compiler-cache.mdx`
- `docs/content/roadmap/rust-build-cache-hygiene.mdx`
- `TESTING.md` (one paragraph)

T added a 2026-10-06 policy banner: MBX is the only compiler cache; the sccache/kache comparison is historical and its setup steps must not be applied. The roadmap outcome says MBX is the sole compiler cache, and remaining-work item 2 says not to add a second backend. `TESTING.md` says "MBX is the only compiler cache" and drops `sccache` from the list of Cargo CI tools that fan-out jobs must not reinstall.

Integration does not contain "MBX is the only compiler cache" or "sole compiler cache". The two research pages still read as current backend studies, including "For CI, sccache is retained because it is already wired". The roadmap still says "Select and wire the compiler-cache backend". `TESTING.md` still lists `sccache` next to nextest, deny, shear, audit, fuzz, and hack.

**Reason:** this is the documented rejection of a second compiler cache. It is not a request to restore sccache. The integration code already omits sccache (see REJECT / ALREADY_PRESENT below); these pages still tell a reader to keep it.

### Research citations of removed root desktop tasks and a missing release workflow

**Files:**

- `research/agent-usage-platform/09-shipped-surface-review.md`
- `research/agent-usage-platform/08-surfaces-release-tooling.md`

T stopped citing deleted root tasks. `09-shipped-surface-review.md` replaced `mise run desktop-build` with a desktop build command and `mise run desktop-lint` with `mise -C native run lint`. Integration still has both removed names. Root `mise.toml` has only `renovate-upstream-sources`. Native tasks include `lint`, not `desktop-lint` or `desktop-build`.

T's release section in `08-surfaces-release-tooling.md` says there is no checked-in release workflow and publication is operator-driven. Integration's command table is already the newer MBX form, but the publication paragraph cites `.github/workflows/release.yml` lines 400–432, 470–508, and 510–639 at HIGH confidence. The workflows directory contains only `ci.yml`. `native/README.md` and `PROJECT_STRUCTURE.md` already say publication is not automated and the checked-in workflow is `ci.yml`.

**Reason:** these two files still describe entry points T removed and a workflow file the integration tree does not have. Update the citations to the current MBX and `native/mise.toml` commands. Do not paste T's bare `cargo xtask` table over the MBX table.

## ALREADY_PRESENT

### Rust 1.99, workspace dependency pins, and layered manifests

**Files:** `Cargo.toml`, `rust-toolchain.toml`, `crates/jackin/Cargo.toml`, `crates/jackin-lints/Cargo.toml`, `crates/jackin-manifest/fuzz/Cargo.toml`, `crates/jackin-usage-ffi/Cargo.toml`, `crates/jackin-xtask/Cargo.toml`, `crates/jackin-telemetry/registry/rust.toml`.

All 78 third-party `[workspace.dependencies]` strings match. `rust-version` is `1.99`. Integration `rust-toolchain.toml` channel is `1.99.0` with the same components and targets. Integration keeps the longer toolchain comment; that comment is not a behavior.

Already matched pins and code include `default-run = "jackin"` (`crates/apps/jackin/Cargo.toml`), dylint `6.1.0`, boltffi `=0.31.0`, fuzz `toml = "1.1.6"`, and semantic-conventions registry `=0.33.0` with checksum `d288b1e1bc3590052fbb8f0f635ef248dd9022cf1fbaeb6582b7ad6081a17b87`.

`Cargo.lock` is not identical. Integration resolves newer patch releases for the same contract: `opentelemetry` / `opentelemetry-otlp` `0.33.1` versus T `0.33.0`, and `syn` `3.0.7` versus T `3.0.6`. Exact pins that T bumped (`fast-down` 6.0.1, `zip` 8.6.0, `turso` 0.8.2, `tokio` 1.53.2, `quick-xml` 0.42.0, `boltffi` 0.31.0, `insta` 1.49.0, `rustix` 1.1.5, `toml` 1.1.6) match.

Fuzz and lint lockfiles (`crates/jackin-config/fuzz/Cargo.lock`, `crates/jackin-env/fuzz/Cargo.lock`, `crates/jackin-lints/Cargo.lock`, `crates/jackin-manifest/fuzz/Cargo.lock`, `crates/jackin-protocol/fuzz/Cargo.lock`) follow those pins. Integration fuzz manifests live under `fuzz/`.

### API adaptations required by those pins

**Files and integration destinations:**

- `crates/jackin-diagnostics/src/observability/otlp/retry.rs` → `crates/services/jackin-diagnostics/src/observability/otlp/retry.rs` (`RetryPolicy::default()` with the same retry bounds)
- `crates/jackin-docker/src/error.rs`, `crates/jackin-docker/src/net.rs` → `crates/adapters/jackin-docker/` (`DownloadFlushFailed`, fast-down 6 client/pusher/event loop)
- `crates/jackin-telemetry/src/schema/attrs.rs` → layered telemetry crate (comments say `=0.33.0`)
- `crates/jackin-xtask/src/ci_junit.rs` and the xUnit parser in `desktop.rs` → `crates/tools/jackin-xtask/` (`normalized_value`, string attribute names)
- `crates/jackin-xtask/src/telemetry_registry.rs` (Weaver URL prefix taken from the locked version)

### Root task removal, with MBX as the newer caller

**Files:** `mise.toml`, `native/mise.toml`, `hk.pkl`, `CONTRIBUTING.md`, `COMMITS.md`, `PROJECT_STRUCTURE.md`, `TESTING.md` (command table), `docker/construct/README.md`, docs command pages, `crates/jackin-dev/src/main.rs`, `crates/jackin-xtask/src/main.rs`, `clippy_affected.rs`, `preview.rs`, `desktop.rs`, desktop tests, `scripts/phase0-apple-container.sh`, `scripts/hooks-install` references, `crates/jackin-usage-ffi/README.md`, `boltffi.toml`.

T deleted the root task list (construct, ci, fmt, clippy, desktop-*, release-preview, hooks) and left nine native tasks. Integration root `mise.toml` keeps one task T dropped, `renovate-upstream-sources`, and its native file has T's nine tasks plus `lint`.

Native `ci` in integration is the newer graph: `mbx +1.99.0 xtask desktop bindings-check`, then `mbx +1.99.0 xtask desktop xcframework`, then `xcodegen`, then format and SwiftLint, then test/build/verify. T's native `ci` calls bare `cargo xtask` and does not pack the XCFramework before `xcodegen`.

Integration tool pins that supersede T's root `mise.toml`:

| Pin | T tip | Integration |
|---|---|---|
| Mise `min_version` | `2026.10.3` | `2026.10.7` |
| Rust backend | `version = "latest"`, MBX on | `version = "1.99.0"`, MBX on |
| `mr-boxington` | `1.22.0` | `1.23.0` |
| boltffi | `cargo:boltffi_cli` 0.31.0, macOS only | `github:boltffi/boltffi` 0.31.0 with the platform archive regex |
| hyperfine | absent | `1.20.0` |
| zig | `0.17.0` | `0.17.0` |

`hk.pkl` amends hk 2.5.0 in both trees. Hook steps are direct `cargo fmt`, `cargo xtask clippy-affected`, `actionlint`, `cd native && swiftlint lint --strict`, and the native swift-format find. Install text is `scripts/hooks-install`. Integration comments point at `crates/tools/jackin-xtask`.

Contributor and construct docs in integration call `mise exec -- mbx +1.99.0 xtask ...` rather than T's bare `cargo xtask` or the deleted `mise run` names. `jackin-dev` builds the local construct image through `mise_exec_command("cargo", ["xtask", "construct", "build-local"])` and no longer treats `mise.toml` as a construct-image reason. `phase0-apple-container.sh` tells the operator to `brew install container`.

`PROJECT_STRUCTURE.md` lists only `ci.yml`. `native/README.md` already says publication is not automated by a checked-in workflow, and its build commands use MBX.

Broker missing-executable text lives in `crates/services/jackin-usage-broker/src/ensure.rs` and names `mise exec -- mbx +1.99.0 build --bins -p jackin`, which is the newer form of T's `cargo build --bins -p jackin` hint in `crates/jackin-usage/src/host/broker.rs`.

### Native packaging, preview script, and Swift 6.4

**Files:** `scripts/release-preview-package` (byte-identical to T), `native/Package.swift`, `native/Design/Prototypes/UnifiedAgentUsage/Package.swift` (both already `swift-tools-version: 6.4`), design audit/signoff/readiness docs (identical to T), `native/Scripts/VisualQA/capture-final-matrix.sh`, `native/Tests/JackinUsageBridgeTests/ArchitectureTests.swift`, `crates/jackin-xtask/tests/homebrew_preview_008.rs`, `crates/jackin-usage-ffi/boltffi.toml`.

The capture script and architecture test assert `mise -C "$repo" exec -- mbx +1.99.0 xtask desktop build|verify`. The Homebrew preview test reads `scripts/release-preview-package` and checks `native/mise.toml` `sign-notarize` for credential cleanup and `mbx +1.99.0 xtask desktop sign-notarize`. Boltffi output paths use the extra `../` required by `crates/adapters/jackin-usage-ffi/`.

Integration `crates/tools/jackin-xtask/tests/case_02.rs` checks `mr-boxington = "1.23.0"`, `rust = { version = "1.99.0", mr_boxington = true }`, channel `1.99.0`, and the native `ci` graph including XCFramework before `xcodegen`. That replaces T's assertion of `rust = { version = "latest" }` and MBX `1.22.0`, and the deleted `swift-package-native-ci` root task.

### CI evidence no longer depends on a mise task body

**Files:** `crates/jackin-xtask/src/ci_evidence.rs`, `ci_evidence/validation.rs`, `ci_evidence/tests.rs`, both workflow fixtures.

Integration validation uses `job_id` and compares the step `run` to the contract command. It does not parse `mise.toml` for `tasks.ci-evidence`. The fixture copy asserts `mise.toml` is absent. Commands are:

- `MISE_AUTO_INSTALL=false mise exec -- mbx +1.99.0 xtask ci-evidence run`
- `MISE_AUTO_INSTALL=false mise exec -- mbx +1.99.0 xtask ci-evidence record-push`

T's extra negative test (rewrite the run line to `cargo xtask ci-evidence record-push` and expect "unexpected evidence execution pipeline") is the same bail that integration already has when `steps[2].run` differs from `contract.command`. Do not add T's bare `cargo xtask` string.

### sccache is already absent from the selected toolchain

**Files:** `crates/jackin-xtask/src/ci_stage.rs`, `release_archive.rs`, `preview.rs`, `preview/tests.rs`, deletion of `scripts/test-mbx-cargo-wrapper.py`.

Integration `TOOLS` does not list `sccache`. `release_archive` has no `sccache_stats` flag. Preview release-tool matching does not treat `sccache` as a release pin. `mise.toml` has no `[wrappers.cargo]`, no `RUSTC_WRAPPER`, and no `SCCACHE_GHA_ENABLED`. `scripts/test-mbx-cargo-wrapper.py` is absent. `crates/tools/jackin-xtask/src/ci/tests.rs` asserts generated workflow and installer text do not set `RUSTC_WRAPPER` to sccache.

### Docs toolchain locks already match T

**Files:** `docs/mise.toml`, `docs/mise.lock`, `docs/package.json`, `docs/bun.lock`.

`docs/mise.toml` is bun `1.4.2`, node `26.10.0`, lychee `0.24.2`. `docs/mise.lock`, `docs/package.json`, and `docs/bun.lock` match T tip byte for byte.

### Zig 0.17.0 provenance is already stricter than T

**Files:** `mise.toml`, `mise.lock`.

Both trees pin Zig `0.17.0` and the same archive URLs and checksums. T records `provenance = "minisign"` only on `macos-arm64`. Integration records it on `linux-x64` and `macos-arm64`. That is the selected provenance set.

### Other docs and native files that already match or are newer

**Files:** `native/Design/Prototypes/UnifiedAgentUsage/DESIGN_AUDIT.md`, `SIGNOFF.md`, `native/Design/UnifiedAgentUsage/SwiftProjectReadiness.md`, `SwiftProjectSetupAudit.md`, `docs/content/reference/runtime/configuration.mdx`, `docs/content/(public)/(role-authoring)/developing/construct-image.mdx`, `docs/content/reference/getting-oriented/xtasks.mdx`, `docs/content/research/engineering/desktop-verification/commands.mdx` (task names already say `mise -C native run ci`; commands already use MBX).

`desktop-verification/commands.mdx` still says `mise.toml` pins `cargo:boltffi_cli` at `0.30.1`. T's patch left that sentence in place, so it is not a behavior these commits added. The live pin is `github:boltffi/boltffi` `0.31.0`.

## REJECT

### Flat `Cargo.toml` / `Cargo.lock` over the layered workspace

T's manifest is the pre-split member list and flat `crates/jackin-*` paths. Integration's manifest is the layered graph, including the split runtime and usage crates. Copying T's manifest or lock would drop that layout and would downgrade `opentelemetry` 0.33.1 to 0.33.0 and `syn` 3.0.7 to 3.0.6.

### T's older MBX, Mise, and `rust = "latest"` pins

Do not write `mr-boxington = "1.22.0"`, Mise `2026.10.3`, or `rust = { version = "latest", mr_boxington = true }` into `mise.toml`, and do not copy the generated workflow that embeds those values. Integration's reviewed pins are MBX `1.23.0`, Mise `2026.10.7`, and Rust `1.99.0`.

### `cargo:boltffi_cli` in place of `github:boltffi/boltffi`

T installs boltffi as a Cargo tool. Integration installs `github:boltffi/boltffi` 0.31.0 with an explicit archive regex, macOS-only, aligned with the Cargo pin.

### Global sccache, `RUSTC_WRAPPER`, and `SCCACHE_GHA_ENABLED`

T removes sccache from CI tool lists, release stats, and preview pins. Integration already has that removal, plus tests that the installer and generated workflow do not export a sccache wrapper. Do not restore a global wrapper or GHA backend, and do not reintroduce `scripts/test-mbx-cargo-wrapper.py`. The transparent `[wrappers.cargo]` shim with `MBX_CARGO_SHIM_MODE` is also gone; routing is the native Mise Rust backend plus explicit `mise exec -- mbx +1.99.0`.

### Zig 0.16 URLs and T's narrower minisign set

Parent `mise.lock` used `https://ziglang.org/download/0.16.0/`. T tip and integration both use 0.17.0 URLs. Do not restore 0.16. Do not replace integration's lock with T's: that would drop `provenance = "minisign"` from `linux-x64`.

### Hyperfine mismatches, and dropping hyperfine

T tip has no hyperfine tool and no hyperfine rows in `mise.lock`. Integration pins `hyperfine = "1.20.0"` and the lock maps each platform at its own archive: linux musl rows use `x86_64-unknown-linux-musl`, and `macos-arm64` uses `aarch64-apple-darwin`. Do not delete that pin. Do not add a musl key that points at a GNU archive, and do not point `macos-arm64` at an `x86_64-apple-darwin` archive. Those bad rows are not in T tip; they must not be introduced while reconciling locks.

### Bare `cargo xtask` as the contributor entry point

T replaced `mise run <task>` with `cargo xtask ...`. Integration's newer equivalent is `mise exec -- mbx +1.99.0 xtask ...` (and `mise -C native run ...` for the native cadences). Copying T's command strings into docs, hooks, fixtures, or tests would bypass the pinned MBX driver.

### Pre-commit fixture channel literal `1.99.0`

**File:** `scripts/test-pre-commit-snapshot`.

T writes `channel = "1.99.0"` into the throwaway fixture and stubs `rustup which --toolchain 1.99.0`. Integration writes and stubs `1.97.1`. The hook reads the toolchain file in the directory under test; the test supplies that file. The `1.97.1` fixture is internally consistent and is not the compiler `rust-toolchain.toml` selects. Changing the literal does not change hook behavior.

### Rewriting historical research commands to T's exact `cargo xtask` strings

Where a current command must be named, use the integration MBX form. Do not replace `research/agent-usage-platform/08-surfaces-release-tooling.md`'s MBX command table with T's `rtk cargo xtask` table.

## Concrete edits for a later implementer

Work only in `/Users/donbeave/Projects/jackin-project/wt-integration-20261013`. Do not copy files from T.

1. Regenerate `.github/workflows/ci.yml` from the current `.velnor/config.toml` so the generated file installs Rust `1.99.0`, MBX `1.23.0`, and Mise `2026.10.7`, and still builds the layered manifests. Do not hand-edit version strings through T's `ci.yml`, and do not check in T's flat-layout workflow. The workflow header says the generator input is `.velnor/config.toml`; this comparison did not run the generator.
2. Add T's MBX-only compiler-cache policy to the two build-performance research pages and to `docs/content/roadmap/rust-build-cache-hygiene.mdx` (outcome, and remaining-work item 2). In `TESTING.md`, state that MBX is the only compiler cache and remove `sccache` from the fan-out Cargo tool list. Leave the historical comparison text in place under that banner. Do not add sccache, `RUSTC_WRAPPER`, or `SCCACHE_GHA_ENABLED`.
3. In `research/agent-usage-platform/09-shipped-surface-review.md`, replace `mise run desktop-build` with `mise exec -- mbx +1.99.0 xtask desktop build --version <version> --build <build>`, and replace `mise run desktop-lint` with `mise -C native run lint`.
4. In `research/agent-usage-platform/08-surfaces-release-tooling.md`, remove the `.github/workflows/release.yml` line citations. State that credentialed publication is not performed by a checked-in workflow, matching `native/README.md`. Keep the existing MBX command table.

Nothing else from `9b6e1d92..9ef617d0` should be ported. In particular, do not copy the flat `Cargo.toml` or `Cargo.lock`, do not downgrade MBX or Mise, do not set Rust to `latest`, do not restore Zig 0.16 or a global compiler wrapper, and do not rewrite the hyperfine lock.

This report does not approve TASK-006.
