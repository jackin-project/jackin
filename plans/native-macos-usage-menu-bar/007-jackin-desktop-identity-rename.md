# Plan 007: Rename the app to jackin❯ Desktop and ship the logomark status-item icon

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. If anything in "STOP conditions" occurs, stop and report — do not improvise. When done, update this plan's row in `plans/native-macos-usage-menu-bar/README.md`.
>
> **Drift check (run first)**: `git diff --stat be6fb79e..HEAD -- native scripts/build-usage-menu-bar-app.sh scripts/verify-usage-menu-bar-app.sh scripts/sign-notarize-usage-menu-bar.sh scripts/release-usage-menu-bar-state.sh scripts/test-release-usage-menu-bar-state.sh .github/workflows/ci.yml .github/workflows/release.yml crates/jackin-xtask/src/release_verify 'docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx' 'docs/content/docs/(public)/getting-started/verifying-releases.mdx' docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx`
> If any in-scope file changed since `be6fb79e`, compare the "Current state" excerpts against live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1 (must land before the first `mode=publish` release — Plan 003 activation)
- **Effort**: M
- **Risk**: MED (broad mechanical rename; CI/release surfaces)
- **Depends on**: none (Plans 001–003 engineering already landed; 003 activation is blocked on Apple secrets, which is exactly the window this rename must use)
- **Category**: direction / migration
- **Planned at**: commit `be6fb79e`, 2026-07-22

## Why this matters

Operator decision (2026-07-22, superseding program decision 1 of 2026-07-21): the app becomes **jackin❯ Desktop** — visible name `Jackin Desktop`, bundle `JackinDesktop.app`, executable `JackinDesktop`, bundle ID `com.jackin-project.desktop`, cask token `jackin-desktop`, release asset `jackin-desktop-<VERSION>-aarch64-apple-darwin.zip`. Plan 003's publish mode is still blocked on Apple secrets, so **no published ZIP, cask, or installed user carries the old identity** — renaming now is a pure pre-release change. Waiting until after activation would burn a version and ship a dead-name cask. The status-item icon simultaneously becomes the jackin❯ logomark (template image) instead of the placeholder SF Symbol. Authoritative spec: `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx` ("Identity" table, lines 30–41).

Brand rule (repo `RULES.md`): prose/docs/UI say **jackin❯ Desktop** (with chevron); code identifiers, bundle/executable names, cask tokens, paths use the plain forms above (`JackinDesktop`, `jackin-desktop`). `CFBundleName` is `Jackin Desktop` (Finder can't render the chevron reliably; this is the operator-approved plaintext fallback surface).

## Current state

Identity is hard-coded in a small number of places (no static Info.plist exists — the build script generates it):

- `scripts/build-usage-menu-bar-app.sh:9` — `DIST="$ROOT/native/dist/JackinUsageMenuBar.app"`; lines 78–101 heredoc-generate `Info.plist`:

  ```text
  scripts/build-usage-menu-bar-app.sh:83-88
    <key>CFBundleExecutable</key>  <string>JackinUsageMenuBar</string>
    <key>CFBundleIdentifier</key>  <string>com.jackin-project.usage-menu-bar</string>
    <key>CFBundleName</key>        <string>jackin usage</string>
  ```

  Lines 43–47 build `--product JackinUsageMenuBar`; lines 66–67, 110 copy/inspect `Contents/MacOS/JackinUsageMenuBar`.

- `scripts/verify-usage-menu-bar-app.sh:29-32` — asserts exact plist values:

  ```bash
  [[ "$bid" == "com.jackin-project.usage-menu-bar" ]] || fail "bundle id $bid"
  [[ "$exe" == "JackinUsageMenuBar" ]] || fail "executable name $exe"
  ```

  Also `:12`, `:20`, `:85-87` reference the app/executable path.

- `scripts/sign-notarize-usage-menu-bar.sh:24` — default app path `native/dist/JackinUsageMenuBar.app`; `:126` final-ZIP stem.
- `scripts/release-usage-menu-bar-state.sh:32` asset name, `:71` cask path `Casks/jackin-usage-menu-bar.rb`, `:18` `TAP_REPO` default. `scripts/test-release-usage-menu-bar-state.sh:41-44, 60, 64, 86` fixture strings.
- `native/Package.swift:7,13,26,28` — package `JackinUsageMenuBar`, executable product `JackinUsageMenuBar`, target path `Sources/JackinUsageMenuBar`. Products also include `.library("JackinUsageBridge")` and binary target `jackin_usage_ffiFFI` — those names **stay** (FFI-layer identifiers, see Out of scope).
- `native/Sources/JackinUsageMenuBar/JackinUsageMenuBarApp.swift:11,17` — `struct JackinUsageMenuBarApp`, keepalive window id `"JackinUsageMenuBarKeepalive"`.
- `native/Sources/JackinUsageMenuBar/StatusItemLabel.swift:13-15` — current icon:

  ```swift
  Image(systemName: "gauge.with.needle")
      .symbolRenderingMode(.monochrome)
  ```

  `:29,31` accessibility strings `"jackin usage …"`. `PopoverRoot.swift:44-46` — header `Text("jackin❯ usage")`, `.accessibilityLabel("jackin usage")`.
- `native/Sources/JackinUsageBridge/PresentationStore.swift:43-47` — UserDefaults key `"jackin.usageMenuBar.showPercent"`.
- `.github/workflows/ci.yml:1124-1197` — job `native-usage-menu-bar` builds/verifies `native/dist/JackinUsageMenuBar.app`, ZIP round-trip at `:1178-1180`, launch smoke `:1194-1197`.
- `.github/workflows/release.yml` — job `build-usage-menu-bar` (`:375`); asset stem `jackin-usage-menu-bar-${VERSION}-aarch64-apple-darwin.zip` at `:524,527,537,549,565,572`; SBOM identity grep `:554` (`grep -q 'JackinUsageMenuBar\|jackin-usage-menu-bar'`); cask heredoc `:803-817`:

  ```text
  cat > homebrew-tap/Casks/jackin-usage-menu-bar.rb <<EOF
  cask "jackin-usage-menu-bar" do
    ...
    url ".../jackin-usage-menu-bar-${VERSION}-aarch64-apple-darwin.zip"
    name "jackin usage"
    app "JackinUsageMenuBar.app"
  ```

  Tap staging references at `:830,844,862`.
- `crates/jackin-xtask/src/release_verify/tests.rs:64-74` — fixture asset names `jackin-usage-menu-bar-…zip`.
- Docs: operator guide `docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx:21,24,28,33-35,48-49`; `docs/content/docs/(public)/getting-started/verifying-releases.mdx:47-48`; ADR-011 distribution section `docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx:75`; roadmap identity table + checklist `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx:30-41, 401`.
- Logomark source asset (seed for the template icon): `docs/public/brand/jackin-monogram.svg` — fully outlined `j❯` mark (white fill), with `docs/public/brand/jackin-monogram.png` raster sibling. There is **no** `.xcassets`, `.icns`, or image asset anywhere under `native/` today.
- Homebrew tap repo `jackin-project/homebrew-tap` has Plan 002's `cask-validation.yml`; the cask itself has **never been published** (Plan 003 BLOCKED), so the token rename has no installed-base impact.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Swift build | `cd native && swift build -c release` | exit 0 |
| Swift tests | `cd native && swift test -c release` | all pass |
| App assembly | `JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/build-usage-menu-bar-app.sh` | exit 0; app at `native/dist/JackinDesktop.app` |
| App verify | `JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/verify-usage-menu-bar-app.sh native/dist/JackinDesktop.app` | exit 0 |
| Shell lint | `shellcheck scripts/build-usage-menu-bar-app.sh scripts/verify-usage-menu-bar-app.sh scripts/sign-notarize-usage-menu-bar.sh scripts/release-usage-menu-bar-state.sh scripts/test-release-usage-menu-bar-state.sh` | exit 0 |
| Workflow lint | `actionlint .github/workflows/ci.yml .github/workflows/release.yml` | exit 0 |
| Release-state fixtures | `./scripts/test-release-usage-menu-bar-state.sh` | ALL FIXTURES PASS |
| xtask tests | `cargo nextest run -p jackin-xtask -E 'test(/release_verify/)' --locked` | all pass |
| Docs audits | `cd docs && bun install --frozen-lockfile && bun run build && cd .. && cargo xtask docs repo-links && cargo xtask roadmap audit` | exit 0 |
| Merge readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope** (the only files you should modify):

- `native/Package.swift`, everything under `native/Sources/JackinUsageMenuBar/` (directory renames to `native/Sources/JackinDesktop/`), `native/Sources/JackinUsageBridge/PresentationStore.swift` (UserDefaults key namespace only), `native/Tests/JackinUsageBridgeTests/ArchitectureTests.swift` (path constants only), `native/README.md`
- New icon asset file(s) under `native/Sources/JackinDesktop/Resources/`
- `scripts/build-usage-menu-bar-app.sh`, `scripts/verify-usage-menu-bar-app.sh`, `scripts/sign-notarize-usage-menu-bar.sh`, `scripts/release-usage-menu-bar-state.sh`, `scripts/test-release-usage-menu-bar-state.sh` (identity strings **inside**; filenames stay — see Out of scope)
- `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- `crates/jackin-xtask/src/release_verify/tests.rs` (fixture asset names)
- `docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx`, `docs/content/docs/(public)/getting-started/verifying-releases.mdx`, `docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx`, `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx` (tick checklist item; update the identity table's "Was/Becomes" framing to completed), `docs/content/docs/roadmap/index.mdx` if wording references the old name
- `plans/native-macos-usage-menu-bar/README.md` (status row)

**Out of scope** (do NOT touch, even though they look related):

- `JackinUsageBridge` library/target/directory, `jackin_usage_ffiFFI` binary target, `JackinUsageFFI.xcframework`, `scripts/build-usage-xcframework.sh`, `scripts/generate-usage-swift-bindings.sh`, the generated `jackin_usage_ffi.swift` — FFI-layer identifiers, not app identity. Renaming them regenerates nothing user-visible and churns the UniFFI toolchain.
- `HOST_USAGE_STATE_REL = "usage-menu-bar"` (`crates/jackin-usage/src/host.rs:21`) and the `~/.jackin/data/usage-menu-bar/` durable-store paths — on-disk data layout recorded in ADR-011, not identity. A path rename is a separate data-migration decision.
- Docs page slugs/URLs (`/guides/macos-usage-menu-bar/`, `adr-011-native-macos-usage-menu-bar`, roadmap `native-macos-usage-menu-bar`) — stable published URLs; page **content** updates, slugs stay.
- Script **filenames** (`*-usage-menu-bar-*.sh`) and workflow **job ids** (`native-usage-menu-bar`, `build-usage-menu-bar`) — internal engineering names referenced by CI path filters (`ci.yml:195-205`, `hygiene.yml:128`) and required-check config; renaming them risks silently detaching required checks. Record as deferred follow-up if the operator wants it.
- The CLI command `jackin usage …` and all its docs — different product surface entirely.
- `Formula/jackin.rb` in the tap; formula is the CLI, untouched.

## Git workflow

- Stay on the active feature branch. If starting from `main`, propose `feature/jackin-desktop-identity` and wait for operator confirmation.
- Conventional Commits, sign every commit (`git commit -s`), push immediately after each commit. Suggested subject: `feat(desktop)!: rename app to jackin❯ Desktop (JackinDesktop.app, com.jackin-project.desktop)`.
- Do NOT open a PR unless the operator asked.

## Steps

### Step 1: Rename the SwiftPM product and sources

In `native/Package.swift`: package name `JackinDesktop`; executable product and target `JackinDesktop` with path `Sources/JackinDesktop`. Leave `JackinUsageBridge`, `jackin_usage_ffiFFI`, and the test target untouched. `git mv native/Sources/JackinUsageMenuBar native/Sources/JackinDesktop`; rename `JackinUsageMenuBarApp.swift` → `JackinDesktopApp.swift` and the struct to `JackinDesktopApp`; keepalive window id → `"JackinDesktopKeepalive"`. Update visible/accessibility strings: popover header `Text("jackin❯ Desktop")` (`PopoverRoot.swift:44`), accessibility labels `"jackin Desktop"` (`PopoverRoot.swift:46`, `StatusItemLabel.swift:29,31` — a11y strings are spoken text, chevron omitted deliberately). In `PresentationStore.swift:43-47` change the UserDefaults key to `"jackin.desktop.showPercent"` (pre-release: losing a local dev preference is acceptable; no migration shim — repo `PRERELEASE.md`). In `ArchitectureTests.swift`, update any hard-coded `Sources/JackinUsageMenuBar` scan paths to `Sources/JackinDesktop`.

**Verify**: `cd native && swift build -c release && swift test -c release` → exit 0, all tests pass; `rg -n "JackinUsageMenuBar" native/Sources native/Tests native/Package.swift` → no matches.

### Step 2: Add the jackin❯ logomark template icon

Copy `docs/public/brand/jackin-monogram.svg` into the app target as a template image. SwiftPM cannot bundle SVG for `NSImage`, so export a PDF (preferred, resolution-independent): `rsvg-convert -f pdf -o native/Sources/JackinDesktop/Resources/JackinMark.pdf docs/public/brand/jackin-monogram.svg` (or `qlmanage`/Inkscape equivalent; any tool producing a vector PDF with the outlined mark is fine — the mark is already pure white fill + alpha, which is exactly the template-image contract: shape from alpha, color ignored). Declare it in `Package.swift` on the executable target: `resources: [.copy("Resources/JackinMark.pdf")]`. In `StatusItemLabel.swift` replace the SF Symbol:

```swift
if let mark = Bundle.module.image(forResource: "JackinMark") {
    Image(nsImage: { mark.isTemplate = true; return mark }())
        .renderingMode(.template)
        ...
```

(shape to taste, but: `isTemplate = true` mandatory, target glyph ~16×16 pt inside the 22 pt status-bar working area, dim-to-0.45 behavior preserved from the current code, SF Symbol fallback allowed only if the resource genuinely fails to load). Keep `.monochrome`/template rendering — never colorize the status item.

**Verify**: `cd native && swift build -c release` → exit 0; run the assembled app (Step 3) and confirm the status item shows the `j❯` mark, correctly tinted in both light and dark menu bars (visual check; record result).

### Step 3: Update assembly, verify, and sign scripts

`scripts/build-usage-menu-bar-app.sh`: `DIST` → `native/dist/JackinDesktop.app`; `--product JackinDesktop`; binary copy/inspect paths → `Contents/MacOS/JackinDesktop`; plist heredoc → `CFBundleExecutable=JackinDesktop`, `CFBundleIdentifier=com.jackin-project.desktop`, `CFBundleName=Jackin Desktop`. **New**: SwiftPM now emits a resource bundle for the executable target (named like `JackinDesktop_JackinDesktop.bundle` in the build products dir) — copy it into `"$DIST/Contents/Resources/"` after the binary copy, and fail if it is missing (`Bundle.module` aborts at runtime without it). `scripts/verify-usage-menu-bar-app.sh`: expected bundle id `com.jackin-project.desktop`, executable `JackinDesktop`, and add an assertion that `Contents/Resources/JackinDesktop_JackinDesktop.bundle` exists. `scripts/sign-notarize-usage-menu-bar.sh`: default app path and final-ZIP stem → `JackinDesktop.app` / `jackin-desktop-…`. `scripts/release-usage-menu-bar-state.sh` + `scripts/test-release-usage-menu-bar-state.sh`: asset stem `jackin-desktop-<VERSION>-aarch64-apple-darwin.zip`, cask path `Casks/jackin-desktop.rb`.

**Verify**: `JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/build-usage-menu-bar-app.sh` → exit 0; same env `./scripts/verify-usage-menu-bar-app.sh native/dist/JackinDesktop.app` → exit 0; `./scripts/test-release-usage-menu-bar-state.sh` → ALL FIXTURES PASS; `shellcheck` on all five scripts → exit 0; `open native/dist/JackinDesktop.app` → status item appears with logomark, `Cmd-,` Settings opens.

### Step 4: Update CI and release workflows

`.github/workflows/ci.yml` job `native-usage-menu-bar` (`:1124-1197`): app path, ZIP name, launch-smoke binary path → `JackinDesktop` forms. `.github/workflows/release.yml` job `build-usage-menu-bar`: asset stem at `:524,527,537,549,565,572` → `jackin-desktop-…`; SBOM grep `:554` → `'JackinDesktop\|jackin-desktop'`; cask heredoc `:803-817` → file `Casks/jackin-desktop.rb`, token `jackin-desktop`, `name "Jackin Desktop"`, `desc "Native macOS status-bar app for jackin agent usage quotas"`, `app "JackinDesktop.app"`; tap staging `git add Casks/jackin-desktop.rb` at `:830,844`; PR body cask mention `:862`. Job ids stay (Out of scope). Update `crates/jackin-xtask/src/release_verify/tests.rs:64-74` fixture names to the new asset stem.

**Verify**: `actionlint .github/workflows/ci.yml .github/workflows/release.yml` → exit 0; `cargo nextest run -p jackin-xtask -E 'test(/release_verify/)' --locked` → all pass; then dispatch the release workflow in safe mode against the branch: `gh workflow run release.yml --ref <branch> -f mode=validate` → run succeeds end-to-end with the new asset name (this is the repo's required pre-merge proof for workflow changes).

### Step 5: Update docs, roadmap, and program records

Operator guide: install command `brew install --cask jackin-project/tap/jackin-desktop`, app name `JackinDesktop.app`, asset name, title/prose "jackin❯ Desktop" (keep the page slug). `verifying-releases.mdx:47-48`: new asset stem. ADR-011 `:75` distribution paragraph: new artifact/cask/app identity (note the supersession date). Roadmap page: convert the identity table from "Was/Becomes" to the completed identity, tick the checklist item `jackin❯ Desktop identity rename … + jackin❯ logomark status-item asset`, and update the "Implementation plans" pointer line to include plans 007–012. `native/README.md:15,34-41,55-66`: new names in build/verify/release walkthroughs and Paths A–C. Update this plan's row + the program README status table.

**Verify**: `cd docs && bun run build` → exit 0; `cargo xtask docs repo-links && cargo xtask roadmap audit && cargo xtask research check` → exit 0; `rg -n "jackin-usage-menu-bar|JackinUsageMenuBar|com\.jackin-project\.usage-menu-bar" --glob '!plans/**' --glob '!docs/content/docs/reference/research/**' .` → remaining hits only in: script filenames, workflow job ids, CI path filters, `HOST_USAGE_STATE_REL` + its tests/ADR state-dir lines, and historical plan documents (001–006). Anything else is a missed rename.

### Step 6: Full gate

**Verify**: `cargo xtask ci --fast` → exit 0. `cd native && swift test -c release` → all pass.

## Test plan

- No new test files. Existing enforcement adapts: `ArchitectureTests.swift` path updates (Step 1), `verify-usage-menu-bar-app.sh` new expected values + resource-bundle assertion (Step 3), `release_verify` fixture renames (Step 4), release-state fixtures (Step 3).
- Manual matrix (record in the PR): status item logomark light/dark + dimmed, Settings opens, launch-at-login round-trip still registers under the new bundle ID (old registration under `com.jackin-project.usage-menu-bar` may linger in System Settings on dev machines — note it; no migration code).
- `gh workflow run release.yml --ref <branch> -f mode=validate` green (Step 4).

## Done criteria

- [ ] `native/dist/JackinDesktop.app` assembles, verifies, launches; status item shows the jackin❯ logomark as a template image.
- [ ] Plist: `CFBundleIdentifier=com.jackin-project.desktop`, `CFBundleName=Jackin Desktop`, `CFBundleExecutable=JackinDesktop`, `LSUIElement=true`.
- [ ] Release validate run green with asset `jackin-desktop-<VERSION>-aarch64-apple-darwin.zip`; cask heredoc emits `Casks/jackin-desktop.rb` with `app "JackinDesktop.app"`.
- [ ] `rg` sweep from Step 5 shows no stray old-identity strings outside the allowed set.
- [ ] Docs (guide, verifying-releases, ADR-011, roadmap incl. checklist tick + plans pointer) updated; all docs audits pass.
- [ ] `cargo xtask ci --fast` exit 0; Swift tests pass; plan/README status rows updated.

## STOP conditions

- Plan 003 has already published a notarized ZIP or merged a cask under the old identity (check: `gh release list` shows a non-dev release with a `jackin-usage-menu-bar` asset, or the tap has `Casks/jackin-usage-menu-bar.rb` on `main`). The "pure pre-release rename" premise is dead — report; the operator must decide on a deprecation path.
- `Bundle.module` resource loading fails in the assembled app (icon missing at runtime) after Step 3's bundle-copy fix — the SwiftPM resource-bundle name/layout differs from expectation. Report the actual build-products layout; do not fall back to embedding the icon as base64 in source.
- The validate-mode release dispatch fails for a reason unrelated to your diff.
- Renaming appears to require touching an out-of-scope file (e.g. a required-check name would change).

## Maintenance notes

- Plans 008–012 refer to the app by the new names; execute this plan first (or in the same PR series) so their scope paths (`native/Sources/JackinDesktop/**`) resolve.
- Plan 003 activation (`mode=publish`) and Plan 004's clean-install proof must use the new cask token `jackin-desktop`; their plan texts still carry the old strings — the program README supersession note (2026-07-22) governs. When 003/004 execute, read their identity values through this plan.
- Deferred follow-ups (recorded, not licensed): script/job-id renames to `*-desktop-*`; `HOST_USAGE_STATE_REL` path rename (data migration); repo secrets/variables names are identity-neutral and stay.
- Reviewer focus: the release.yml hunks (asset/cask strings appear ~15 times — a missed one publishes a mixed-identity release), and the resource-bundle copy in the assembly script.
