# Distribution (headless)

## Purpose

Ship jackin❯ Desktop to operators: notarized public ZIP, Homebrew cask,
production install proof. Headless — acceptance is artifact checks, not a
screen. Folds the prior program's open plans 003/004 (D14).
Anchors: F9 · Evidence: research/jackin-desktop-verification-tooling/01-commands.md;
plans/native-macos-usage-menu-bar/003-notarized-release-and-cask.md,
004-production-proof-and-roadmap-retirement.md; native/README.md (release
contracts)

## Requirements

### Requirement: Notarized public release
A release SHALL produce `jackin-desktop-<VERSION>-aarch64-apple-darwin.zip`
(+ `.sha256`, `.bundle`, `.sbom.json`, attestation) whose app is Developer
ID-signed, notarized, stapled, and Gatekeeper-accepted; `cargo xtask desktop
verify --release` (or the release workflow's equivalent) SHALL pass; ad-hoc
signatures MUST fail the release verify.
Covers: F9, B6 · Evidence: native/README.md release tables; .github/workflows/release.yml:523-572; verification-tooling ch. 01

#### Scenario: Validate mode without secrets
- **GIVEN** the Release workflow in `mode=validate`
- **WHEN** it runs on the secret-free fixture
- **THEN** assembly passes and the ad-hoc app fails `--release` verify (guardrail proven)

#### Scenario: Publish
- **GIVEN** Apple secrets present in the `release-macos` environment
- **WHEN** `mode=publish` (or tag) runs
- **THEN** the notarized artifact set publishes and `spctl`-level acceptance holds

### Requirement: Homebrew cask install proof
The release SHALL land a `Casks/jackin-desktop.rb` cask (first cask never
auto-merged) and a production install SHALL be proven on a clean host:
`brew install --cask jackin-desktop` yields a launchable app whose status
bar renders real provider data.
Covers: F9, B6 · Evidence: native/README.md (tap row); prior plan 004 intent

#### Scenario: Clean-host install
- **GIVEN** a Mac without the repo
- **WHEN** the cask installs and the app launches
- **THEN** Gatekeeper accepts it and enabled providers appear in the menu bar

## Notes

- Blocked-on-secrets reality: Apple Developer ID material is org-provisioned
  (`release-macos` env secrets — names only in native/README.md). Plans
  carry a STOP when secrets are absent; bootstrap path A documented in
  native/README.md.
