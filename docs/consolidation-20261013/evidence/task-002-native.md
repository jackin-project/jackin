# TASK-002 native/product evidence

Read-only inspection on 2026-10-13. No commit, push, build, `mise`, test, sign, notarize, or publish. Neither the repository `/Users/donbeave/Projects/jackin-project/jackin` nor the worktree `/Users/donbeave/Projects/jackin-project/wt-baseline-P` was edited. The worktree `HEAD` is `fba69d60c2373156dc0dcecee8e085c83776966f`.

- M = `1c3dd1be4a3b1ed518133ec3899c084c792370ff`
- P = `fba69d60c2373156dc0dcecee8e085c83776966f`

None of the six tips is an ancestor of M or of P, and M is not an ancestor of any tip. "Unique" means reachable from the tip and not reachable from M. Every unique parent below is an ancestor of M, so the unique set is the tip commit except for B015.

## Unique commits

### B015 `7646c61bd4261a5908693bdaf0a6ecd9e9643448`

Merge of `6f025e5a5ea3e8c645a15770fcb8a839c9494308` into `17d3d69e58098c721159a4692fdb3a8eeae06973`. Merge-base with M and with P is that first parent, which is in M. Seven commits are not in M:

| SHA | Subject |
| --- | --- |
| `886859e58f76e7a1737cba4968fd05ee7e4df2d5` | migrate Apple CI to Velnor generic recipe (schema 2, pin `f3b94af3`); this is the substantive generated-workflow migration |
| `9974584f4dafcaae2f51acde87c1c19d38831ee6` | merge `origin/main` (`68edddabb1a0f3b191f488cdf4d7206f3df5a381`) into the migration branch |
| `e42933039cb5cad2c823931af9e8efbbd8cd6536` | repin generator to `b634efd2` and regenerate workflows |
| `f1d6c4d2b430cf13dfe48f818356bdbc5ae2eb28` | pin `b634efd2` runtime provenance in `.github-gen/velnor-workflow.toml` only |
| `39fc154eca4c00cecbd3234d928991f58b026eee` | merge `origin/main` (`17d3d69e58098c721159a4692fdb3a8eeae06973`) |
| `6f025e5a5ea3e8c645a15770fcb8a839c9494308` | repin to Velnor `690b3935d979bd5400bc82b1f7e68d139125acd3` and regenerate |
| `7646c61bd4261a5908693bdaf0a6ecd9e9643448` | tip merge into the first parent |

Net tree versus the first parent touches 21 files, all generator inputs or generated CI outputs, plus `crates/jackin-usage-ffi/boltffi.toml`, `.xcode-version`, and `mise.toml`. It does not change `native/Package.swift`, `native/project.yml`, `native/Scripts/run-ui-tests.sh`, or `crates/jackin-xtask/src/desktop.rs`.

### B025 `0437497d7f22fbdb7c8aad986be932704d707fe9`

One unique commit. Parent `c52e912bd3757c4ca736be288b0e03779144b561` is in M. Subject: recovery capture of staged worktree-index state, generated without source hooks. The commit changes only `.github-gen/velnor-workflow.toml`, `.github-gen/visibility.toml`, `.github-gen/static/renovate.yml`, `.github-gen/static/renovate-validate.yml`, `.xcode-version`, `crates/jackin-usage-ffi/boltffi.toml`, and `mise.toml`. Those five product-input blobs are byte-identical to B015. `.github/**` is unchanged from the parent, and the tip has zero `xcframework-jackinusage` product-transport hits. The staged inputs are the B015 contract; the tip's generated workflows are the parent's and are not B015's regenerated outputs.

### B029 `8fb49688a6408627e22118f92d24f47785564979`

One unique commit. Parent `ebf5d5cdcc0639611de138b5e1fb5b5d540f6804` is in M. Subject and body say inputs only, renderer pin stays `44243ed4917d53ad74a96c64d1c58dbe52c944c1`, `policy --check` is expected to fail, and the commit must not land before M1 regeneration. `.github/**` diff versus the parent is empty. The tip's `.github/ci/.github-actions-generator-state` is the inherited generated-ownership file, not updated by this commit. `.github-gen/velnor-workflow.toml` is `schema = 2` with `revision = "44243ed4917d53ad74a96c64d1c58dbe52c944c1"` and the comment "knowingly unregenerated" / "DO NOT LAND BEFORE THE M1 REGEN". There is no `[native.apple]` table. `boltffi.toml` does add `deployment_target = "26.0"`. The synthetic `swift-package-native` unit is removed from the input. Product-transport hits in the tip workflows are zero.

The inspected change is that unmatched input/output state. Ancestors are not that state: the parent is already in M, and this commit does not modify `native/Package.swift` (`d5209e673f15c5c0bb52fb73d296743625f51343`) or `native/project.yml` (`e688246b512210de72bdf93235772728853db183`).

This tip is not generated output that P regenerates. The unique change is source input. P has no `.github-gen/velnor-workflow.toml`, no `.github/ci/project.toml`, and no `.github/workflows/ci-unit-swift.yml`. P's native cadence is handwritten `native/mise.toml` plus `crates/tools/jackin-xtask/src/desktop.rs`. P does not render those Velnor workflows from current inputs.

### B031 `432e0108ce90b64b428c69c27d4a2ac23e559379`

One unique commit. Parent `fd14ac6a7e0642842e66eeeb31d4b589d07148e1` is in M. Subject: converge native workflow generation. It scans Apple manifests with Velnor `816046893ef55359ee2f41903d1f1f893d59303e`, writes `[native.apple]` `cargo_profile = "desktop-release"` and `deployment_floor = "26.0"`, adds `.xcode-version`, and regenerates `.github/ci/project.toml`, `.github/ci/.github-actions-generator-state`, and the workflow YAMLs including `ci-unit-swift.yml` and `desktop-merge.yml`. The tip has the product-transport marker (`xcframework-jackinusage` hits across `project.toml` and `ci-unit-swift.yml`; `project.toml` also contains `desktop-release`). `crates/jackin-usage-ffi/boltffi.toml` at this tip has no `deployment_target` key. `native/Package.swift` and `native/project.yml` are the same blobs as M and as the other tips.

### B033 `d16cd795d4a539b84ba5f44290663b2363d49350`

One unique commit, same parent as B031, and that parent is in M. The patch adds `[native.apple]` with `cargo_profile = "desktop-release"` and `deployment_floor = "26.0"` to `.github-gen/velnor-workflow.toml` (generator revision still `4dec6b9ec28b0d51cb370fd8f5d5401c6186adf0`, and the scan still excludes `native/Package.swift`) and changes one input digest line in `.github/ci/.github-actions-generator-state` (`config` `656591d1813eca30` to `ef6570b1b49fd56b`). No workflow YAML changes. Product-transport hits at the tip are zero. The declaration is not adopted by generated producer/consumer commands in this commit.

### B034 `33d88b1cf764f614d3935c11c48decd53386083e`

One unique commit. Parent `68edddabb1a0f3b191f488cdf4d7206f3df5a381` is in M. Subject: fall back to an in-process broker when the sidecar is missing, and ship the sidecar with jackin. Files:

- `crates/jackin-runtime/src/usage_broker.rs` (new shared `run_service_process`)
- `crates/jackin/src/bin/usage_broker.rs` (new `[[bin]]` `jackin-usage-broker`)
- `crates/jackin/Cargo.toml`
- `crates/jackin-runtime/src/lib.rs`
- `crates/jackin-runtime/src/bin/usage-broker/main.rs` (thinned)
- `crates/jackin-runtime/src/bin/usage-broker/tests.rs` (deleted)
- `crates/jackin-usage/src/host/broker.rs`
- `crates/jackin-usage/src/host/broker/tests.rs`
- `crates/jackin/src/cli/usage.rs`
- `crates/jackin/src/console/adapter/run.rs`
- `crates/jackin-usage-ffi/src/bridge/tests.rs`

## What is present in P

These behaviors are in P's tree. They are also in M, with the path moves noted. They are not created by the archive tips' unique commits; those tips leave the handwritten native product files unchanged. P's `native/project.yml` blob equals M and every inspected tip (`e688246b512210de72bdf93235772728853db183`). P's `native/Scripts/run-ui-tests.sh` blob equals M (`460ba77f5ca807caa51ad8855eeb76d58faedb4d`).

### Profile and deployment floor

Present.

| Fact | M | P |
| --- | --- | --- |
| Cargo profile | `Cargo.toml` `[profile.desktop-release]` (inherits release, `strip = false`, `debug = "line-tables-only"`) | same table in `Cargo.toml` |
| Profile constant and floor | `crates/jackin-xtask/src/desktop.rs`: `DESKTOP_PROFILE = "desktop-release"`, `MIN_OS = "26.0"`, exported as `MACOSX_DEPLOYMENT_TARGET` for bindings, staticlib, broker, and xcodebuild | `crates/tools/jackin-xtask/src/desktop.rs`, same constants |
| XcodeGen floor | `native/project.yml` `deploymentTarget.macOS: "26.0"`, `MACOSX_DEPLOYMENT_TARGET: "26.0"`, `LSMinimumSystemVersion: "26.0"` | same file, same blob |
| SwiftPM floor | `native/Package.swift` `.macOS(.v26)`, tools `6.2` | `native/Package.swift` `.macOS(.v26)`, tools `6.4` |

P does not have the archive `[native.apple]` table or the `boltffi.toml` `deployment_target = "26.0"` key. The floor is enforced by xtask, `project.yml`, and `Package.swift`.

### Harness counts and SwiftPM nonzero proof

Present as an enumerated set of five, plus a nonzero xUnit gate.

`run_desktop_tests` in both desktop.rs files builds the XCFramework when `target/xcframework/JackinUsage.xcframework` is missing, then runs:

- `StatusItemChipHarness`
- `DesktopArchitectureLint`
- `DesktopParityMatrixHarness`
- `DesktopSoTParityHarness`
- `ProviderMarksHarness`

Products are declared in `native/Package.swift`. The function fails if a harness command fails. It does not parse a numeric harness total. `run_swift_unit_tests` requires both `native/.build/swift-unit-tests.xml` (XCTest) and `native/.build/swift-unit-tests-swift-testing.xml` (Swift Testing), and `validate_test_totals` refuses `tests == 0` and any failure or error. P's message strings say `mise exec -- mbx +1.99.0 xtask`. M's say `cargo xtask` / `mise run`.

### UI checks

Present. `native/Scripts/run-ui-tests.sh` counts indented `func test` declarations under `native/UITests`, requires that count to be greater than zero, runs each name with `-only-testing:JackinDesktopUITests/JackinDesktopUITests/<name>`, requires that invocation's `totalTestCount` to be 1 and `failedTests` to be 0, and requires passed count to equal the source count. Both trees have 19 `func test` matches in `native/UITests/JackinDesktopUITests.swift`. The tests were not executed.

P invokes the script from `native/mise.toml` `[tasks.merge]` after `[tasks.ci]`. M invokes it from `mise.toml` `[tasks.desktop-test-ui]`, called by `[tasks.desktop-merge]`.

### Release stamping and dSYM validation

Bundle stamping and dSYM UUID agreement are present in both `desktop.rs` `build_app` functions.

Order inside `build_app`: `build_xcframework`, then the `jackin-usage-broker` binary with `--profile desktop-release` and `MACOSX_DEPLOYMENT_TARGET=26.0`, then `xcodegen generate --spec project.yml`, then `xcodebuild` with `MARKETING_VERSION={version}` and `CURRENT_PROJECT_VERSION={build}`, then copy the app to `native/dist/JackinDesktop.app`, copy the broker beside the app executable, then copy `JackinDesktop.app.dSYM` to `native/dist/JackinDesktop.app.dSYM` and require the app Mach-O UUID to equal the dSYM DWARF UUID (`dwarfdump --uuid`). `verify_app` checks `LSMinimumSystemVersion` against `MIN_OS`.

P's `native/mise.toml` `[tasks.ci]` exports `JACKIN_APP_VERSION` default `0.6.0` and `JACKIN_APP_BUILD` default `1` before `xtask desktop build`. M's `mise.toml` `[tasks.desktop-build]` passes the same defaults into `cargo xtask desktop build`.

M also has `mise.toml` `[tasks.desktop-release-env]`, which derives `JACKIN_APP_VERSION` / `JACKIN_APP_BUILD` from a workflow-dispatch fixture, a tag, or `Cargo.toml`, using `git rev-list --count HEAD` for `CFBundleVersion`, and appends them to `GITHUB_ENV`. That task name is absent from P's `mise.toml` and `native/mise.toml`. Both trees still have the release/cask printer: M `crates/jackin-xtask/src/desktop/release_state.rs`, P `crates/tools/jackin-xtask/src/desktop/release_state.rs`. M wraps it as `[tasks.desktop-release-state]`. P has no matching mise task in the files searched.

### Bundled sidecar packaging

Present, fail-closed. This is not B034's in-process fallback.

| Role | M | P |
| --- | --- | --- |
| Sibling binary target | `crates/jackin/Cargo.toml` `[[bin]]` name `jackin-usage-broker`, path `crates/jackin/src/bin/usage-broker/main.rs` | `crates/apps/jackin/Cargo.toml` `[[bin]]` name `jackin-usage-broker`, path `crates/apps/jackin/src/bin/usage-broker/main.rs` |
| Default executable | `crates/jackin-usage/src/host/broker.rs` `default_service_executable`: `JACKIN_USAGE_BROKER_BIN` or `current_exe` parent joined with `jackin-usage-broker` | `crates/services/jackin-usage-broker/src/config.rs` `default_service_executable`, same rule |
| Missing or unusable executable | process activation returns `Unavailable` | `crates/services/jackin-usage-broker/src/ensure.rs` `ensure_usage_broker_process` and `ensure_usage_monitor_process` return `Unavailable` with the executable path; no in-process branch |
| App bundle copy | `crates/jackin-xtask/src/desktop.rs` `build_app` copies the broker into the app | `crates/tools/jackin-xtask/src/desktop.rs` `build_app`, same copy and `verify_broker` |

`ensure_usage_broker_with_executor` still exists in P at `crates/services/jackin-usage-broker/src/service.rs` and is used by tests, including `crates/adapters/jackin-usage-ffi/src/bridge/tests.rs`. It is not the missing-sidecar product path in `ensure.rs`.

### FFI before SwiftPM and XcodeGen, local xtask only

Present inside P's build and harness functions, with two gaps.

`build_app` calls `build_xcframework` before `xcodegen` and before `xcodebuild`. `run_desktop_tests` calls `build_xcframework` when the framework directory is missing, before the five `swift run` harnesses. `build_xcframework` uses `desktop-release` and `MACOSX_DEPLOYMENT_TARGET=26.0`, then `boltffi pack apple` with working directory `crates/adapters/jackin-usage-ffi`, then requires repository-root `target/xcframework/JackinUsage.xcframework`.

Consumers:

- `native/Package.swift` binary target `JackinUsageFFI` path `../target/xcframework/JackinUsage.xcframework` (repository-root `target/xcframework` when resolved from `native/`)
- `native/project.yml` linked framework `../target/xcframework/JackinUsage.xcframework` on `JackinUsageBindings`, `JackinDesktop`, and `JackinDesktopTests`

P's `crates/adapters/jackin-usage-ffi/boltffi.toml` still says `output = "../../target/xcframework"`. From `crates/adapters/jackin-usage-ffi` that relative path is `crates/target/xcframework`, not repository-root `target/xcframework`. M's manifest is `crates/jackin-usage-ffi/boltffi.toml` with the same relative string, which resolves to repository-root `target/xcframework` and matches `Package.swift`, `project.yml`, and the xtask check. xtask does not pass an output override. This inspection did not run `boltffi`, so this is path arithmetic, not a build log.

`run_swift_unit_tests` starts at `swift build` and does not pack the framework itself. P's `[tasks.ci]` calls it only after `xtask desktop build`.

P's `[tasks.ci]` also runs `xcodegen generate --spec native/project.yml` immediately after `xtask desktop bindings-check`. `bindings-check` runs `boltffi generate swift`, not `pack apple`. That first XcodeGen invocation is not gated on the XCFramework. M's `mise.toml` `[tasks.desktop-ci]` has the same shape: `desktop-bindings-check`, then `desktop-generate`, then later `desktop-test` and `desktop-build`. M additionally has `[tasks.swift-package-native-ci]`, which runs `desktop-xcframework` and then `xtask desktop test-swift`. B015 deletes that mise task. P does not have it.

## What exists only on these archive tips

Absent from M and from P: `.github-gen/velnor-workflow.toml`, `.github-gen/visibility.toml`, `.github/ci/project.toml`, `.github/ci/.github-actions-generator-state`, `.xcode-version`, and the generated `.github/workflows/ci-*.yml` product graph. P's root `mise.toml` says native workflows live in `native/mise.toml`.

| Obligation piece | Where it actually is | In P? |
| --- | --- | --- |
| Generated product transport: producer packs `target/xcframework/JackinUsage.xcframework` with `MACOSX_DEPLOYMENT_TARGET=26.0` and `--profile desktop-release`, stages `xcframework-jackinusage`, and the SwiftPM/XcodeGen consumer verifies that artifact before `swift build` / `xcodebuild` | B015 tip `.github/ci/project.toml` and `.github/workflows/ci-unit-swift.yml` / `ci-unit-rust.yml`. B031 tip has the same marker in the regenerated `project.toml` and `ci-unit-swift.yml` | No. P has the local `build_app` / harness order described above, not this generated graph |
| `[native.apple]` `cargo_profile = "desktop-release"`, `deployment_floor = "26.0"` | B015, B025 (same input blob as B015), B031, B033 in `.github-gen/velnor-workflow.toml`. Not in B029 | No table. The same two values are hardcoded in P xtask, `project.yml`, and `Package.swift` |
| `boltffi.toml` `deployment_target = "26.0"` | B015, B025, and B029 (`crates/jackin-usage-ffi/boltffi.toml` blob `569182d9244e44da0e214899f3cc4a1c05efe8fe`). Absent at B031 and B033 | No key in `crates/adapters/jackin-usage-ffi/boltffi.toml`. Floor is the xtask env and the Swift/Xcode deployment settings |
| B025 tip generated workflows matching those staged schema-2 inputs | Inputs match B015; `.github/**` is still parent `c52e912bd3757c4ca736be288b0e03779144b561`, with zero product-transport hits | No. Do not treat the B025 tip tree as a regenerated graph |
| B029 unmatched pre-M1 tree | Inputs at pin `44243ed4917d53ad74a96c64d1c58dbe52c944c1`; inherited `.github/**` not regenerated; commit text says `policy --check` must fail | No. Reject this inspected change. Do not reject the parent, which is already in M. P does not regenerate this output |
| B033 input digest without regenerated workflows | `.github-gen/velnor-workflow.toml` plus one `config` digest line; workflows unchanged and without product transport | The profile/floor values survive by other files. This generated adoption does not |
| `mise run desktop-release-env` as invoked by archive `release.yml` | Task body is in M `mise.toml`, so the derivation is not archive-only. The generated `release.yml` step is archive-only | The mise task is absent in P. Bundle stamping inside xtask is present |
| In-process missing-sidecar fallback | Only B034, `crates/jackin-usage/src/host/broker.rs` `ensure_usage_broker` else-branch and `ensure_usage_broker_falls_back_to_in_process_when_sidecar_missing` | No. P `ensure.rs` fails closed |

B015 and B031 are the only inspected tips whose generated commands implement FFI-before-SwiftPM/XcodeGen. B025 and B029 change inputs that describe that edge and leave the generated commands unregenerated. B033 declares profile/floor and updates the input digest only.

## In-process fallback: lease, CAS, and capability

B034 does not prove lease, CAS, or capability safety for the fallback. The fallback is not approved. Availability of a live handle is not that proof.

What the commit contains:

- If `sidecar_service_usable` is false (`None`, missing path, non-file, or mode without an execute bit), `ensure_usage_broker` calls `ensure_usage_broker_with_executor` on a background thread in the activating process. The comment says this uses the same leader election, transport, and CAS reconcile path as the sidecar.
- The shared hook `ensure_usage_broker_with_hooks` still runs discover, then read-lease, then the existing `reconcile_catalog_if_projection` closure. The commit reuses that hook. It does not add a CAS conflict, lost-owner, or concurrent-activation test.
- `setsid` detachment is only in the sidecar entry `run_service_process` (`crates/jackin-runtime/src/usage_broker.rs`). The in-process thread does not detach. The new comments describe client-death survival for the sidecar process, not for the fallback thread.
- New test `ensure_usage_broker_falls_back_to_in_process_when_sidecar_missing` activates with `service_executable` unset and with a missing path, then asserts `handle.catalog_lease == projection.projection_id` and `handle.capabilities == usage_broker_capabilities(&discovery)`. That is a happy-path equality check. It does not kill the owner, interleave two activators, or show a capability that cannot be bypassed.
- Other new tests classify usable binaries, name a missing executable, report spawn failure, and reject capsule scope. Negative activation that must still fail uses an uncreatable data directory, because a missing sidecar no longer fails closed.
- Stderr context is `usage broker unavailable: ...` from `unavailable_with_detail` and from `run_service_process`. That is diagnostic text, not a lease or capability proof.

M and P already ship the sibling `jackin-usage-broker` binary and resolve it beside `current_exe`. P's product activation stays on that executable and returns unavailable when it cannot be located or spawned.
