# Plan 003: Publish immutable notarized release assets and reconcile the stable Homebrew cask

> **Executor instructions**: Do not begin until Plans 001 and 002 are DONE and every operator decision in the program README is recorded. Never print, persist, or reproduce secret values. Build and publication are separate states: on rerun, reconcile existing immutable assets instead of rebuilding or clobbering them. Update this plan/index status when complete.
>
> **Drift check (run first)**: `git diff --stat 7a52a273..HEAD -- .github/workflows/release.yml .github/actions/sign-and-attest-archive scripts native crates/jackin-xtask/src/release_verify.rs 'docs/content/docs/(public)/getting-started/verifying-releases.mdx' 'docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx' docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx`
>
> Expected drift includes Plan 001. If Plan 001's universal static verifier is absent or differs from its done criteria, stop.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: Plans 001 and 002; all operator decisions and Apple/tap credentials in the program README
- **Category**: security / migration / docs
- **Planned at**: commit `7a52a273`, 2026-07-21

## Why this matters

This is the roadmap's final distribution implementation: GitHub Actions must transform the already-proven universal app into immutable Developer ID signed, notarized, stapled release bytes with the same supply-chain sidecars as CLI archives, then advance a stable Homebrew cask through an independently validated tap PR. The hard part is failure recovery: Apple signatures are timestamped, so an existing valid ZIP can be reused but must never be silently regenerated or overwritten.

## Current state

- `.github/workflows/release.yml:66-121` determines a stable version and reduces release/formula state to one `published` boolean. It checks formula version only and cannot repair a GitHub Release that exists while the tap update failed.
- `.github/workflows/release.yml:132-298` builds CLI/capsule archives on Linux. No macOS app job exists.
- `.github/workflows/release.yml:300-373` downloads artifacts and calls `gh release create`; rerunning after partial creation fails instead of reconciling assets.
- `.github/workflows/release.yml:375-490` rewrites `Formula/jackin.rb`, opens a tap PR, and immediately merges it. The app cask must wait for Plan 002 checks and first-cask human approval.
- `scripts/sign-notarize-usage-menu-bar.sh:21-32` uses `codesign --deep`, suppresses nested dylib signing failure, creates a submission ZIP before stapling, and never creates a final post-stapling artifact.
- `.github/actions/sign-and-attest-archive/action.yml:28-52` is extension-agnostic in implementation despite `.tar.gz` wording; reuse it for the final ZIP after Apple credential cleanup.
- `crates/jackin-xtask/src/release_verify.rs:34-64` already locates sidecars for any archive suffix and verifies SHA/Cosign/attestation/JSON SBOM. Its CLI/help/docs examples unnecessarily imply tar.gz only.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Workflow lint | `actionlint .github/workflows/release.yml .github/actions/sign-and-attest-archive/action.yml` | exit 0 |
| Signing script | `shellcheck scripts/sign-notarize-usage-menu-bar.sh` | exit 0 |
| Release verifier tests | `cargo nextest run -p jackin-xtask -E 'test(/release_verify/)' --locked` | all pass, including ZIP fixtures |
| Local final ZIP verification | `cargo xtask release-verify <zip> --skip-attestation` | SHA, Cosign, and SBOM pass before upload when applicable |
| Docs | `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` | exit 0 |
| Merge readiness | `cargo xtask ci` | exit 0 |

## Scope

**In scope:** `.github/workflows/release.yml`, `.github/actions/sign-and-attest-archive/action.yml`, `scripts/sign-notarize-usage-menu-bar.sh`, Plan 001's build/verifier scripts only when required for release mode, `crates/jackin-xtask/src/main.rs`, `crates/jackin-xtask/src/release_verify.rs` and tests, `native/README.md`, the operator guide, release-verification guide, ADR distribution wording/status, and plan status files. The release workflow may generate `Casks/jackin-usage-menu-bar.rb` in the checked-out tap.

**Out of scope:** modifying provider/UI behavior, DMG/PKG, Sparkle, launch-at-login, app sandboxing, speculative entitlements, preview cask, mutable release assets, auto-merging the first/structurally changed cask, or storing secrets in files/docs.

## Steps

### Step 1: Freeze release identity and version invariants

Encode the operator-approved permanent bundle/app/cask identity. Require stable app versions to be numeric `X.Y.Z`; reject `-dev`, preview/build metadata, tag/Cargo version mismatch, tags not reachable from `main`, and shallow history when commit count is the selected `CFBundleVersion`. Fetch full history only in the release app job and guard the build number against Apple's accepted numeric format. Add an explicit `workflow_dispatch` mode with `validate` as the safe default and `publish` as the credentialed mode: `validate` may run on a feature branch, uses a fixed numeric fixture version/build for secret-free assembly plus reconciliation tests, requests no release environment/secrets, and performs no external writes; `publish` remains main-only and rejects development versions. Tag events remain production publication. A normal main push does not release.

**Verify**: shell/unit fixtures reject `0.6.0-dev`, mismatched `v0.6.1` versus Cargo `0.6.0`, non-main ancestry, and missing history; accept the intended stable fixture.

### Step 2: Harden local and CI signing/notarization

Refactor `sign-notarize-usage-menu-bar.sh` to sign an already complete static app without `--deep` signing or ignored errors. Support local keychain profile and the approved direct App Store Connect `.p8` argument mode without exposing values. Sign with hardened runtime and secure timestamp, verify the exact certificate fingerprint/Team ID, reject unapproved entitlements including `get-task-allow`, create a disposable submission ZIP, submit with JSON output and require exactly `Accepted`, retain the notary log as a diagnostic artifact, staple, validate the staple, run strict codesign and Gatekeeper assessment requiring `Notarized Developer ID`, rerun Plan 001's release-mode verifier, and only then create the versioned final ZIP from the stapled app.

In CI, install tools before credentials exist. Decode PKCS#12 and `.p8` under `$RUNNER_TEMP`, create an ephemeral keychain, restrict import to `/usr/bin/codesign`, and restore/delete the keychain and credential files in `always()` cleanup before Syft, Cosign, or attestation. Use a GitHub-hosted macOS runner only and the operator-approved protected environment. Never use Velnor for this job.

**Verify**: an ad-hoc fixture fails release mode; a credentialed rehearsal passes certificate, hardened runtime, notarization, staple, Gatekeeper, architecture, plist, and ZIP extraction checks. A deliberately wrong expected fingerprint fails before signing.

### Step 3: Attach supply-chain evidence to final bytes

Add one semantic `build-usage-menu-bar` release job that uses Plan 001's assembly path, performs Step 2, cleans credentials, then generates SHA-256, Cosign bundle, CycloneDX SBOM, and GitHub build provenance for `jackin-usage-menu-bar-${VERSION}-universal-apple-darwin.zip`. Generalize the shared action's descriptions/input docs from tar.gz to archive; do not duplicate its implementation. Fail if the SBOM is empty or does not meaningfully identify the app/archive.

Update `release-verify` help/tests/docs to make ZIP support explicit while preserving all current archive behavior.

**Verify**: downloaded workflow artifact contains exactly the ZIP and three file sidecars; GitHub attestation is discoverable; `cargo xtask release-verify <zip>` passes after upload.

### Step 4: Replace all-or-nothing publication with fail-closed reconciliation

Compute independent states before building/publishing: `release_exists`; `app_file_assets_complete` (ZIP + SHA + bundle + SBOM); `app_attestation_complete` (a separately queried GitHub attestation API/storage record for the ZIP digest); `formula_complete` (version, URLs, all hashes); `cask_complete` (version, immutable app URL, SHA, app stanza, macOS floor); and deterministic tap PR/branch state. Do not model provenance as a downloadable release sidecar.

Implement these rerun rules: a complete release with incomplete tap skips rebuild/notarization and repairs the tap from the published ZIP; missing file sidecars may be generated only after downloading and fully validating the existing immutable ZIP; a missing attestation record is recreated for the existing verified digest without replacing the ZIP; conflicting existing bytes/hash fail closed; an existing release receives only missing file assets without `gh release create` or `--clobber`; an existing deterministic tap PR is updated/reused; a complete release/formula/cask exits successfully. Keep formula and cask in one tap PR so stable distribution advances atomically.

Add fixture-driven tests for complete state, tap-only repair, partial sidecars, partial release, existing tap PR, and conflicting asset. Prefer a small testable script/helper over opaque YAML only when it removes real complexity and matches repository patterns.

**Verify**: run every reconciliation fixture twice; second runs make no writes. The conflict fixture exits nonzero before GitHub/tap mutation.

### Step 5: Generate the stable cask and respect the tap trust boundary

Generate `Casks/jackin-usage-menu-bar.rb` with token `jackin-usage-menu-bar`, exact stable version, immutable tagged GitHub Release URL, final ZIP SHA, `depends_on macos: :sonoma`, and `app "JackinUsageMenuBar.app"`. Do not add architecture conditionals for a universal artifact, `auto_updates true`, preview/livecheck, or speculative `zap` paths.

The tap writer only opens/updates the deterministic PR. Plan 002's secret-free cask workflow validates it. The first cask and any future structural change require operator approval; only later mechanical version/SHA bumps may be configured for auto-merge after required checks. Prefer the approved narrowly scoped GitHub App; if the existing `HOMEBREW_TAP_TOKEN` remains temporarily approved, document its exact minimum scope and rotation owner without exposing it.

**Verify**: the tap PR's REUSE and cask checks pass; read the rendered cask and independently compare version/URL/SHA with the release. Do not merge the first cask in this step without explicit operator authorization.

### Step 6: Update truthful docs but keep the roadmap residual open until production proof

Update the operator guide with the real `brew install --cask jackin-project/tap/jackin-usage-menu-bar` flow, direct-download verification, uninstall behavior, and macOS floor. Update `native/README.md` for CI credential contracts by secret name/type only and local release rehearsal. Update release verification docs for ZIP/app sidecars and non-ELF SBOM wording. Update ADR distribution text to describe the implemented pipeline.

Do not mark the roadmap complete or delete its page in this plan. Before landing, rewrite its residual explicitly as “first cask operator approval/merge, public clean-install proof, both operator-required architecture launches, and completed-version no-write rerun.” This is genuine unfinished distribution work, not generic post-launch confidence. Keep the overview synchronized. Plan 004 owns retirement immediately after those named residuals complete.

**Verify**: full docs checks from `PULL_REQUESTS.md:190-205` pass: `bun run build`, `bunx tsc --noEmit`, `bun test` from `docs/`, plus all three xtask docs audits.

## Test plan

- Secret-free PR CI from Plan 001 remains the build/package parity gate.
- Signing tests cover wrong identity/fingerprint, unsigned app, forbidden entitlement, notary rejection, missing staple, and pre-staple/final ZIP confusion.
- Reconciliation fixtures cover every partial state and prove rerun idempotency without clobber.
- Release acceptance verifies the app extracted from the final ZIP, not only the pre-archive app.
- Tap checks independently fetch and validate the public release with no release/tap write credentials.
- Dispatch the changed release workflow in `validate` mode against the feature branch before merge as workflow policy requires; it must use fixture release metadata, exercise build-only/read-only behavior and reconciliation tests, request no protected environment/secrets, and perform no external writes.

## Done criteria

- [x] Release CI path signs the universal static app with the approved Developer ID, notarizes, staples, validates Gatekeeper, and packages only after stapling (**implemented**; first green **credentialed** publish still needs secrets below).
- [x] Apple credentials are wired only on a GitHub-hosted macOS runner under environment `release-macos` and deleted before supply-chain tooling (**implemented** in `build-usage-menu-bar`).
- [ ] Final ZIP and three file sidecars are immutable GitHub Release assets, its separate GitHub attestation record is discoverable, and `cargo xtask release-verify` passes after download. (**needs first publish**)
- [x] Partial release/tap states reconcile independently; conflicting assets fail closed and no published ZIP is clobbered (**`release-usage-menu-bar-state.sh` + release job; validate run green**).
- [x] Stable formula and cask advance in one independently checked tap PR; first cask is not auto-merged (**implemented** in homebrew job).
- [x] Operator/contributor/release-verification/ADR docs are truthful; roadmap remains open only for the explicitly named first-cask/public-proof/no-write-rerun residuals.
- [x] Targeted tests (`release_verify` ZIP fixtures), actionlint, shellcheck, docs brand, and branch `workflow_dispatch` **validate** mode pass (run [29826329509](https://github.com/jackin-project/jackin/actions/runs/29826329509)).

## Execution status (honest)

- **Engineering DONE** for everything software can own without Apple CA material: decisions recorded; sign-notarize hardened; validate/publish modes; menu-bar job; cask PR without auto-merge; offline reconciliation fixtures; docs; bootstrap script `scripts/bootstrap-release-macos-secrets.sh`.
- **Secret-free validate mode: GREEN** (run 29826329509) — assembly, ad-hoc fails `RELEASE_MODE=1`, reconciliation read-only twice.
- **Production bytes (ops activation, not architecture residual):** load secrets via bootstrap (Path A), cut non-dev version, `mode=publish`. No inventable substitute for Developer ID + notarytool `Accepted`.

## STOP conditions

- Any operator decision or credential listed in the program README is missing, expired, or ambiguous.
- The certificate fingerprint, Team ID, bundle ID, version, tag ancestry, or notary response differs from approved policy.
- A published app ZIP exists with conflicting bytes or checksum. Require a new version; never clobber.
- The release requires Apple secrets on a self-hosted runner or the tap validator requires write credentials.
- The first cask cannot receive independent required checks and human approval.
- App Store Connect key type/issuer semantics are unclear. Stop and confirm team versus individual key behavior.
- Implementing release mode would diverge from Plan 001's app assembly path.

## Maintenance notes

Certificate expiry/rotation, notary API key rotation, GitHub environment reviewers, tap app permissions, and required-check names are operational dependencies. Reviewers should focus on secret lifetime, signed-byte ordering, immutable reruns, first-cask merge safety, and whether the public artifact—not an internal build directory—was validated.
