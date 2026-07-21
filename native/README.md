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
| `Sources/JackinUsageBridge` | Generated Swift + `PresentationStore` |
| `Sources/JackinUsageMenuBar` | `LSUIElement` `MenuBarExtra` app |
| `../scripts/generate-usage-swift-bindings.sh` | Bindings |
| `../scripts/build-usage-xcframework.sh` | XCFramework |
| `../scripts/build-usage-menu-bar-app.sh` | Local `.app` |
| `../scripts/sign-notarize-usage-menu-bar.sh` | Operator Developer ID path |

## Build (developer)

```bash
cargo build -p jackin-usage-ffi --release
./scripts/generate-usage-swift-bindings.sh
cd native
DYLD_LIBRARY_PATH=../target/release swift test -c release
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
