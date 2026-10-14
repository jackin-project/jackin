# TASK-015 gap audit

**Approval:** not approved. **Overall:** BLOCKED.

Read-only. No product files were edited. `cargo`, `xcodebuild`, and `mise` native tasks were not run. No commit.

## Pins

| Tree | Path | HEAD | Required |
| --- | --- | --- | --- |
| Integration | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` | `ef5ca1df379ac31a52a61f5b1284deae17f5eebd` | at or after `ef5ca1df` |
| Policy P | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` | `fba69d60` |

`ef5ca1df` is HEAD. `fba69d60` is not an ancestor of that HEAD. Their merge-base is `1c3dd1be4a3b1ed518133ec3899c084c792370ff`. Integration adopted the policy layout at `9e208685`, then corrected FFI output depth at `a1f0916c` and inserted the XCFramework pack at `77e7b546`. Those commits are in `ef5ca1df`. The audited files are clean in both worktrees.

Host: macOS 27.0.1 (`26A434`). Selected developer dir `/Applications/Xcode.app` is Xcode 27.0 (`CFBundleShortVersionString` 27.0, `ProductBuildVersion` `27A266a`). `/Applications/Xcode_26.6.app` is absent. A run on this Xcode 27 is not the pinned gate.

## Verdicts

| Item | Verdict |
| --- | --- |
| Native FFI is packed before SwiftPM and XcodeGen | ALREADY_PRESENT |
| `boltffi.toml` outputs `../../../target/xcframework` and `../../../native/Sources/JackinUsageBindings` | ALREADY_PRESENT |
| `native/mise.toml` `ci` runs bindings-check, then xcframework, then xcodegen | ALREADY_PRESENT |
| `ci` / `merge` / `scheduled` do not sign or notarize | ALREADY_PRESENT |
| Bundled `jackin-usage-broker` sibling (CHG-010) | ALREADY_PRESENT |
| In-process broker fallback (CHG-011) | REJECT |
| Intentionally unregenerated pre-M1 state (CHG-024) | REJECT |
| Execute `mise -C native run ci`, `merge`, and `scheduled` on Xcode 26.6 | BLOCKED |

## Order, with file:line

Integration `native/mise.toml` `[tasks.ci]` packs before either consumer:

- `native/mise.toml:28` `mbx +1.99.0 xtask desktop bindings-check`
- `native/mise.toml:29` `mbx +1.99.0 xtask desktop xcframework`
- `native/mise.toml:30` `xcodegen generate --spec native/project.yml`
- Swift format/lint follow at `native/mise.toml:31-32`
- SwiftPM-consuming `desktop test` is `native/mise.toml:33`
- `desktop build` is `native/mise.toml:36`
- counted SwiftPM `desktop test-swift` is `native/mise.toml:37`

Policy P `native/mise.toml:28-29` goes from `bindings-check` straight to `xcodegen generate` and does not run `xcframework`. Integration already has the missing line (`77e7b546`).

`merge` calls `ci` then `native/Scripts/run-ui-tests.sh` (`native/mise.toml:45-47`). `scheduled` calls `merge` then `deadcode` (`native/mise.toml:54-56`). Neither inserts a producer after XcodeGen.

`xtask desktop build` packs before XcodeGen even when invoked alone:

- `crates/tools/jackin-xtask/src/desktop.rs:1145-1146` calls `build_xcframework` and requires `target/xcframework/JackinUsage.xcframework`
- `crates/tools/jackin-xtask/src/desktop.rs:1038-1039` runs `bindings_check` before the pack
- `crates/tools/jackin-xtask/src/desktop.rs:1184-1190` runs `xcodegen generate --spec project.yml` only after that pack
- `crates/tools/jackin-xtask/src/desktop.rs:200-205` builds the XCFramework before SwiftPM harnesses when `desktop test` finds it missing

Consumers read that packed artifact, not a second producer:

- SwiftPM binary target `native/Package.swift:21-23` path `../target/xcframework/JackinUsage.xcframework`; bindings target `native/Package.swift:27-30`
- XcodeGen `native/project.yml:49` and `native/project.yml:125` depend on `../target/xcframework/JackinUsage.xcframework`
- Producer config `crates/adapters/jackin-usage-ffi/boltffi.toml:27` `output = "../../../target/xcframework"`
- Producer config `crates/adapters/jackin-usage-ffi/boltffi.toml:36` `output = "../../../native/Sources/JackinUsageBindings"`
- Crate cwd is `crates/tools/jackin-xtask/src/desktop.rs:44` `crates/adapters/jackin-usage-ffi`

From that crate, `../../../` is the repo root. Policy P `boltffi.toml:27` and `boltffi.toml:36` still use `../../`, which would land under `crates/`. Integration already uses the three-level paths (`a1f0916c`).

Shipping lane text is Xcode 26.6 / macOS 26.5 SDK, with Xcode 27 recorded as a nonblocking forward lane: `native/project.yml:5-6`, `native/README.md:30-31`. `format-check` pins `DEVELOPER_DIR=/Applications/Xcode_26.6.app/Contents/Developer` at `native/mise.toml:12`. The `ci` body does not export `DEVELOPER_DIR`. On this host, `xcrun` would follow Xcode 27. That is not a substitute gate and is not a reason to retarget the script.

## Sign / notarize

`[tasks.sign-notarize]` exists at `native/mise.toml:59-112` and calls `xtask desktop sign-notarize`. `ci`, `merge`, and `scheduled` do not call it. `desktop verify` enables Gatekeeper and stapler checks only when `--release` or `RELEASE_MODE` is set (`crates/tools/jackin-xtask/src/desktop.rs:152`, `desktop.rs:1361-1372`). The `ci` verify line `native/mise.toml:37` passes neither. XcodeGen sets `CODE_SIGNING_REQUIRED: NO` and `CODE_SIGN_IDENTITY: "-"` at `native/project.yml:113-114`. `desktop build` ad-hoc signs with identity `-` (`desktop.rs:1272-1282`). That is the existing local/PR shape, not Developer ID notarization, and this audit did not run it.

## Broker package and deferred fallback

CHG-010 is already in the tree. The sibling binary is `crates/apps/jackin/Cargo.toml:27-29` (`jackin-usage-broker` → `src/bin/usage-broker/main.rs`). `desktop build` copies it beside the app executable (`desktop.rs:1151-1176`, `desktop.rs:1228-1231`, `desktop.rs:1452-1453`).

CHG-011 stays deferred. `ensure_usage_broker_process` spawns the located executable or returns `Unavailable` (`crates/services/jackin-usage-broker/src/ensure.rs:16-63`). There is no in-process broker fallback. Do not port one.

CHG-024 stays rejected. Do not integrate the unmatched pre-M1 input/output snapshot `8fb49688`.

## Blocked execution

The only missing step is running the pinned native lanes on Xcode 26.6. This host does not have that Xcode. Do not record an Xcode 27 run as the gate.

Not run, and not PASS:

- `mise -C native run ci`
- `mise -C native run merge`
- `mise -C native run scheduled`

The macOS 26 OrbStack usage lane in the task (`xtask ci --e2e`, `nextest` `docker-e2e`) was also not run. This host is macOS 27.0.1. That is the same class of missing execution, not a product edit.

## Edit list

(empty)
