# jackin❯ Desktop (native macOS usage menu bar)

Display-only Swift shell over `jackin-usage-ffi` (UniFFI). Product identity:
**jackin❯ Desktop** (`JackinDesktop.app`, bundle ID `com.jackin-project.desktop`).
Rust owns probes, cache, severity, and `status_bar_label`. CodexBar is a visual
reference only (clean-room).

## Layout

| Path | Role |
|---|---|
| `../crates/jackin-usage` | Host probes + `HostUsageRuntime` |
| `../crates/jackin-usage-ffi` | Synchronous UniFFI facade |
| `Generated/` | UniFFI C header + module map (regenerate) |
| `Sources/JackinUsageBridge` | Generated Swift + `PresentationStore` + pure display helpers |
| `Sources/JackinDesktop/` | Split UI: `StatusItemLabel`, `PopoverRoot`, Settings, Usage window, glass, logomark |
| `cargo xtask desktop …` / `mise run desktop-*` | Canonical build, verify, XCFramework, bindings, sign/notarize, release-state, secrets bootstrap |

## SDK requirement

Deployment target stays **macOS 14+**. **Release builds must use the macOS 26 SDK** so Tahoe Liquid Glass resolves in `GlassFallbacks.swift` (the only file allowed to contain `#available(macOS 26, *)`).

Liquid Glass is applied only to the **navigation / control layer** (status chips, glance panel chrome, agent tile island, sidebar, footer, unified toolbar) per Apple HIG. **Content** (provider cards, overview rows, metric bodies) uses standard materials so hierarchy stays clear. On macOS 14/15 or with Reduce Transparency, chrome falls back to system materials.

## Apple Silicon (arm64) static assembly (source of truth)

One path builds the local, PR, and release app:

1. **Pinned tools** via `mise.toml` (`cargo:uniffi` provides `uniffi-bindgen`; `mise install`).
2. **Static XCFramework** — `cargo xtask desktop xcframework` (or as part of build) produces `target/xcframework/JackinUsageFFI.xcframework` with Clang module `jackin_usage_ffiFFI`.
3. **SwiftPM** — `native/Package.swift` consumes that XCFramework as a `binaryTarget` (no host `target/release` dylib path).
4. **App** — `mise run desktop-build -- <version> <build>` produces a **arm64 (Apple Silicon)** `JackinDesktop.app` with no embedded dylib/framework/XCFramework, then ad-hoc signs.
5. **Verify** — `mise run desktop-verify` (optional ZIP via `cargo xtask desktop verify <app> <zip>`). `--release` requires Developer ID + notarization/staple/Gatekeeper.

```bash
mise install

# One-shot local smoke (build + verify + launch menu-bar app)
mise run desktop

# Or step by step:
mise run desktop-build -- 0.6.0 1   # prints absolute path + DESKTOP_APP=…
mise run desktop-verify             # fail-closed bundle checks
mise run desktop-run                # launch (LSUIElement — no Dock icon; look at menu bar)

# equivalent cargo:
#   cargo xtask desktop build --version 0.6.0 --build 1
#   cargo xtask desktop verify
#   cargo xtask desktop run
```

Build/verify/run each print a clear banner with the **absolute** app path (`DESKTOP_APP=…` for grepping). The default bundle is `native/dist/JackinDesktop.app`.

Swift tests (full Xcode): after the XCFramework exists, `cd native && swift test -c release`.

Status-item chip harness (no XCTest — remaining% / dual-bucket / multi-provider parity):

```bash
cd native && swift run -c release StatusItemChipHarness
```

Default status-item display is **all enabled providers** (icon + **remaining %**, OpenUsage-style; strip cap default 8). Empty data shows `—`. Settings → Percent style can flip compact + chip lines to **% used**.

| Operator entry | Rust implementation |
|---|---|
| `mise run desktop` | build + verify + run (local smoke) |
| `mise run desktop-build -- <ver> <build>` | `cargo xtask desktop build` |
| `mise run desktop-verify` | `cargo xtask desktop verify` |
| `mise run desktop-run` | `cargo xtask desktop run` |
| `mise run desktop-run -- --verify` | `cargo xtask desktop run --verify` |
| `mise run desktop-xcframework` | `cargo xtask desktop xcframework` |
| `mise run desktop-bindings` | `cargo xtask desktop bindings` |
| `mise run desktop-sign-notarize` | `cargo xtask desktop sign-notarize` |
| `mise run desktop-release-state -- <ver>` | `cargo xtask desktop release-state` |
| `mise run desktop-bootstrap-secrets -- …` | `cargo xtask desktop bootstrap-secrets` |

## CI / release contracts (secret **names** only)

| Surface | Detail |
|---|---|
| PR gate | CI job `Native usage menu bar` — assembly, verify, Swift tests, soft launch |
| Validate release | `workflow_dispatch` **Release** with `mode=validate` — secret-free fixture `0.0.0`/`1`, ad-hoc must fail `--release`, reconciliation read-only |
| Publish release | `mode=publish` or tag `vX.Y.Z` on main — environment **`release-macos`**, GitHub-hosted macOS only |
| Secrets (env `release-macos`) | `DEVELOPER_ID_APPLICATION_P12_BASE64`, `DEVELOPER_ID_APPLICATION_P12_PASSWORD`, `APP_STORE_CONNECT_API_KEY_P8`, `APP_STORE_CONNECT_KEY_ID`, `APP_STORE_CONNECT_ISSUER_ID` |
| Variables (repo) | `JACKIN_DEVELOPER_ID_TEAM_ID`, `JACKIN_DEVELOPER_ID_CERT_SHA256` |
| Artifact | `jackin-desktop-<VERSION>-aarch64-apple-darwin.zip` + `.sha256` + `.bundle` + `.sbom.json` + GitHub attestation |
| Tap | Formula + `Casks/jackin-desktop.rb` in one PR; **first cask never auto-merged** |

### Local notarization rehearsal

```bash
export DEVELOPER_ID_APPLICATION='Developer ID Application: Your Name (TEAMID)'
export NOTARY_PROFILE=jackin-notary   # or set APP_STORE_CONNECT_* path/key/issuer
export JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1
mise run desktop-build -- 0.6.0 1
mise run desktop-sign-notarize
# final ZIP: native/dist/jackin-desktop-0.6.0-aarch64-apple-darwin.zip
```

Credential material must never be committed. CI deletes PKCS#12/API key material before cosign/syft/attestation.

## Activating the first notarized release (creative paths)

Apple Developer ID material is **org-provisioned**, not inventable in CI. Three ways to finish distribution:

### Path A — Bootstrap secrets (preferred)

1. Enroll / use an **Apple Developer Program** team that can create a **Developer ID Application** certificate.
2. Export the cert as PKCS#12 + create an App Store Connect **Team** API key (`.p8` + key id + issuer).
3. Load them into GitHub without printing values:

```bash
# From local files:
cargo xtask desktop bootstrap-secrets \
  --p12 ./DeveloperID.p12 --p12-password-env P12_PASS \
  --p8 ./AuthKey_XXXXXX.p8 --key-id XXXXXX --issuer <issuer-uuid> \
  --team-id <TEAMID> --cert-sha256 <sha256-hex>

# Or from unlocked 1Password:
cargo xtask desktop bootstrap-secrets \
  --op-p12 'op://Vault/Item/p12file' \
  --op-p12-password 'op://Vault/Item/password' \
  --op-p8 'op://Vault/Item/notesPlain' \
  --key-id XXXXXX --issuer <issuer-uuid> \
  --team-id <TEAMID>
```

4. Land this PR, cut a **non-dev** version on `main` (not `*-dev`), then:

```bash
gh workflow run release.yml --ref main -f mode=publish -f lanes=github
```

5. Approve/merge the tap PR after `cask-validation` (first cask is never auto-merged).
6. Plan 004: `cargo xtask release-verify` on the public ZIP + `brew install --cask` on Apple Silicon (arm64).

### Path B — First stable jackin❯ release rides the same tag

Menu-bar artifacts are part of the existing Release workflow. The first non-dev tag after secrets exist publishes CLI + capsule **and** the notarized menu-bar ZIP + formula/cask PR atomically. No separate product release track.

### Path C — Validate forever until Path A

`mode=validate` (secret-free) already proves assembly, release-mode negative check, and reconciliation. That is the merge gate. Production bytes wait on Path A/B only.

### Offline reconciliation fixtures

```bash
cargo nextest run -p jackin-xtask --locked desktop::release_state
```
