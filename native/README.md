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
| `../scripts/build-usage-menu-bar-app.sh` | Universal static `.app` (local/PR/release assembly) |
| `../scripts/verify-usage-menu-bar-app.sh` | Fail-closed verifier (ad-hoc or `RELEASE_MODE=1`) |
| `../scripts/sign-notarize-usage-menu-bar.sh` | Developer ID sign + notarize + staple + final ZIP |
| `../scripts/release-usage-menu-bar-state.sh` | Independent release/cask state for reconciliation |

## SDK requirement

Deployment target stays **macOS 14+**. **Release builds must use the macOS 26 SDK** so Tahoe Liquid Glass resolves in `GlassFallbacks.swift` (the only file allowed to contain `#available(macOS 26, *)`). On macOS 14/15 or with Reduce Transparency, chrome falls back to system materials.

## Universal static assembly (source of truth)

One path builds the local, PR, and release app:

1. **Pinned tools** via `mise.toml` (`cargo:uniffi` provides `uniffi-bindgen`; `mise install`).
2. **Static XCFramework** — `scripts/build-usage-xcframework.sh` builds arm64 + x86_64 Rust staticlibs and assembles `target/xcframework/JackinUsageFFI.xcframework` with Clang module `jackin_usage_ffiFFI`.
3. **SwiftPM** — `native/Package.swift` consumes that XCFramework as a `binaryTarget` (no host `target/release` dylib path).
4. **App** — `JACKIN_APP_VERSION=… JACKIN_APP_BUILD=… ./scripts/build-usage-menu-bar-app.sh` produces a **universal** `JackinUsageMenuBar.app` with no embedded dylib/framework/XCFramework, then ad-hoc signs.
5. **Verify** — `./scripts/verify-usage-menu-bar-app.sh native/dist/JackinUsageMenuBar.app` (optional ZIP arg for round-trip). `RELEASE_MODE=1` requires Developer ID + notarization/staple/Gatekeeper.

```bash
mise install
JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/build-usage-menu-bar-app.sh
JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/verify-usage-menu-bar-app.sh native/dist/JackinUsageMenuBar.app
open native/dist/JackinUsageMenuBar.app
```

Swift tests (full Xcode): after the XCFramework exists, `cd native && swift test -c release`.

## CI / release contracts (secret **names** only)

| Surface | Detail |
|---|---|
| PR gate | CI job `Native usage menu bar` — assembly, verify, Swift tests, soft launch |
| Validate release | `workflow_dispatch` **Release** with `mode=validate` — secret-free fixture `0.0.0`/`1`, ad-hoc must fail `RELEASE_MODE=1`, reconciliation read-only |
| Publish release | `mode=publish` or tag `vX.Y.Z` on main — environment **`release-macos`**, GitHub-hosted macOS only |
| Secrets (env `release-macos`) | `DEVELOPER_ID_APPLICATION_P12_BASE64`, `DEVELOPER_ID_APPLICATION_P12_PASSWORD`, `APP_STORE_CONNECT_API_KEY_P8`, `APP_STORE_CONNECT_KEY_ID`, `APP_STORE_CONNECT_ISSUER_ID` |
| Variables (repo) | `JACKIN_DEVELOPER_ID_TEAM_ID`, `JACKIN_DEVELOPER_ID_CERT_SHA256` |
| Artifact | `jackin-usage-menu-bar-<VERSION>-universal-apple-darwin.zip` + `.sha256` + `.bundle` + `.sbom.json` + GitHub attestation |
| Tap | Formula + `Casks/jackin-usage-menu-bar.rb` in one PR; **first cask never auto-merged** |

### Local notarization rehearsal

```bash
export DEVELOPER_ID_APPLICATION='Developer ID Application: Your Name (TEAMID)'
export NOTARY_PROFILE=jackin-notary   # or set APP_STORE_CONNECT_* path/key/issuer
export JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1
./scripts/build-usage-menu-bar-app.sh
./scripts/sign-notarize-usage-menu-bar.sh
# final ZIP: native/dist/jackin-usage-menu-bar-0.6.0-universal-apple-darwin.zip
```

Credential material must never be committed. CI deletes PKCS#12/API key material before cosign/syft/attestation.
