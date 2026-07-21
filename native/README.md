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

## Build (developer)

```bash
cargo build -p jackin-usage-ffi --release
./scripts/generate-usage-swift-bindings.sh
cd native
swift build -c release
# Full Xcode required for XCTest:
DYLD_LIBRARY_PATH=../target/release swift test
./scripts/build-usage-menu-bar-app.sh
open native/dist/JackinUsageMenuBar.app
```

## Notarization residual

Requires Apple Developer ID + notarytool profile. When secrets unavailable in CI:

```bash
export DEVELOPER_ID_APPLICATION='Developer ID Application: …'
export NOTARY_PROFILE=jackin-notary
./scripts/sign-notarize-usage-menu-bar.sh
```

Document residual in the roadmap until stapled builds ship in CI.
