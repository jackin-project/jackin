# Native macOS agent-usage menu bar

Display-only Swift shell over `jackin-usage-ffi` (UniFFI). Rust owns probes,
cache, severity, and `status_bar_label`. CodexBar is a visual reference only
(clean-room).

## Layout

| Path | Role |
|---|---|
| `../crates/jackin-usage` | Host probes + `HostUsageRuntime` |
| `../crates/jackin-usage-ffi` | Synchronous UniFFI facade |
| `Generated/` | UniFFI C header + module map (regenerate) |
| `Sources/JackinUsageBridge` | Generated Swift + `PresentationStore` + pure display helpers |
| `Sources/JackinUsageMenuBar/` | Split UI: `StatusItemLabel`, `PopoverRoot`, `SurfaceCard`, `SettingsView`, `GlassFallbacks` |
| `../scripts/generate-usage-swift-bindings.sh` | Bindings |
| `../scripts/build-usage-xcframework.sh` | XCFramework |
| `../scripts/build-usage-menu-bar-app.sh` | Local `.app` |
| `../scripts/sign-notarize-usage-menu-bar.sh` | Operator Developer ID path |

## SDK requirement

Deployment target stays **macOS 14+**. **Release builds must use the macOS 26 SDK** so Tahoe Liquid Glass resolves in `GlassFallbacks.swift` (the only file allowed to contain `#available(macOS 26, *)`). On macOS 14/15 or with Reduce Transparency, chrome falls back to system materials.

## Universal static assembly (source of truth)

One path builds the local, PR, and future release app:

1. **Pinned tools** via `mise.toml` (`cargo:uniffi` provides `uniffi-bindgen`; `mise install`).
2. **Static XCFramework** — `scripts/build-usage-xcframework.sh` builds arm64 + x86_64 Rust staticlibs and assembles `target/xcframework/JackinUsageFFI.xcframework` with Clang module `jackin_usage_ffiFFI`.
3. **SwiftPM** — `native/Package.swift` consumes that XCFramework as a `binaryTarget` (no host `target/release` dylib path).
4. **App** — `JACKIN_APP_VERSION=… JACKIN_APP_BUILD=… ./scripts/build-usage-menu-bar-app.sh` produces a **universal** `JackinUsageMenuBar.app` with no embedded dylib/framework/XCFramework, then ad-hoc signs.
5. **Verify** — `./scripts/verify-usage-menu-bar-app.sh native/dist/JackinUsageMenuBar.app` (optional ZIP arg for round-trip).

Signing and notarization remain Plan 003 work (`scripts/sign-notarize-usage-menu-bar.sh`).

```bash
mise install
JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/build-usage-menu-bar-app.sh
JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/verify-usage-menu-bar-app.sh native/dist/JackinUsageMenuBar.app
open native/dist/JackinUsageMenuBar.app
```

Swift tests (full Xcode): after the XCFramework exists, `cd native && swift test -c release`.

## Notarization residual

Requires Apple Developer ID + notarytool profile. When secrets unavailable in CI:

```bash
export DEVELOPER_ID_APPLICATION='Developer ID Application: …'
export NOTARY_PROFILE=jackin-notary
./scripts/sign-notarize-usage-menu-bar.sh
```

Document residual in the roadmap until stapled builds ship in CI.
