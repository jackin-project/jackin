# 01 — Proven verification commands (Desktop stack)

Questions: exact build/test/lint/verify commands executor plans may cite for
crates (`jackin-usage`, `jackin-usage-ffi`), the UniFFI bindings drift gate,
the Swift app, and release verification.
Informs: jackin-desktop
Method: codebase read (commands proven by CI green usage, not guessed)
Vetted: 2026-07-24

## Findings

### Crates tests
- `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` — exact CI
  step in job "Native usage menu bar" — `.github/workflows/ci.yml:1105` (job
  name), step at ci.yml ~line 1147 (`run: cargo nextest run -p jackin-usage -p
  jackin-usage-ffi --locked`) (confidence: HIGH)
- General runner and filters: `cargo nextest run`, `-E 'test(name)'`,
  `--all-features` — `TESTING.md:19-37` (confidence: HIGH)

### Bindings drift gate
- `cargo xtask desktop bindings` then
  `git diff --exit-code -- native/Generated native/Sources/JackinUsageBridge/jackin_usage_ffi.swift`
  — CI step, `.github/workflows/ci.yml` (~1150-1152); requires
  `cargo install uniffi --version 0.32.0 --features cli --locked` (CI installs
  it; mise `cargo:uniffi` provides `uniffi-bindgen` per `native/README.md`)
  (confidence: HIGH)

### App build / verify / run
- `cargo xtask desktop build --version "$JACKIN_APP_VERSION" --build "$JACKIN_APP_BUILD"`
  → `native/dist/JackinDesktop.app`; `cargo xtask desktop verify
  native/dist/JackinDesktop.app [zip]` — CI steps (~ci.yml:1154-1160);
  operator wrappers `mise run desktop-build|desktop-verify|desktop-run|desktop`
  — `mise.toml:84-129` (confidence: HIGH)
- CI also soft-launches the app binary and checks the log
  (`"$APP_BIN" > /tmp/jackin-desktop-launch.log`, ci.yml:1177-1197)
  (confidence: HIGH)

### Swift tests
- `cargo xtask desktop test` (mise `desktop-test`, `mise.toml:115-117`);
  macOS-only guard `require_macos("desktop test")` —
  `crates/jackin-xtask/src/desktop.rs:157`; full-Xcode alternative printed by
  xtask: `cd native && swift test -c release` —
  `crates/jackin-xtask/src/desktop.rs:197` (confidence: HIGH)
- **Snapshot runner stability:** GitHub's current official runner-image
  catalog exposes macOS 26 GA under explicit `macos-26` labels, while the
  moving `macos-latest` alias was scheduled to transition in June 2026.
  Native system-font/SF-Symbol golden tests must use explicit `macos-26`,
  freeze Dynamic Type/accessibility/animation inputs, and keep tolerant
  pixel comparison; otherwise the runner major can change without a source
  diff. —
  https://github.com/actions/runner-images#available-images
  (confidence: HIGH)

### Workspace lint/fmt gates
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
  and `cargo fmt` — workspace baseline, `crates/CLAUDE.md` (lint section);
  merge-readiness: `cargo xtask ci` / `cargo xtask ci --fast` —
  `CONTRIBUTING.md` (confidence: HIGH)

### Release verification
- `cargo xtask desktop sign-notarize` (mise `desktop-sign-notarize`),
  `--release` verify requires Developer ID + notarization/staple/Gatekeeper —
  `native/README.md` (build/verify + CI/release contracts tables); release
  artifact naming `jackin-desktop-<VERSION>-aarch64-apple-darwin.zip` —
  `.github/workflows/release.yml:523-549` (confidence: HIGH)

## Dead ends and contradictions
- None; commands converge across mise.toml, xtask source, CI, and docs.

## Open unknowns
- None load-bearing; notarization steps need org Apple secrets
  (`release-macos` environment — names only, `native/README.md`).
