# Homebrew Preview Channel Design

This design adds an automatically updated Homebrew preview channel for `jackin`.
The goal is to make the latest successful commit from `main` easy to install and
test locally via the existing tap, while keeping the stable release flow
separate and unchanged.

## Goals

- Add a `jackin@preview` formula in the Homebrew tap.
- Update `jackin@preview` automatically after successful CI runs on `main`.
- Keep preview builds source-based so users can validate the latest code without
  introducing a separate binary packaging system.
- Use a Ghostty-style preview version string that contains both a semver core
  and a commit hash.
- Preserve the existing tagged stable release flow for `jackin`.

## Non-Goals

- Replacing the stable `jackin` formula.
- Publishing prebuilt preview binaries.
- Changing the meaning of `brew install --HEAD jackin`.
- Introducing a nightly schedule or a separate prerelease branch.

## Naming and Versioning

The rolling channel will be named `preview`, not `nightly` or `tip`.

Rationale:

- `nightly` implies a time-based build cadence, which does not match publishing
  after every successful `main` commit.
- `tip` is technically accurate for latest-commit builds, but `preview` is the
  chosen product-facing channel name.
- `preview` aligns with the intent of an unstable but easy-to-install prerelease
  channel.

The preview formula version format will be:

```text
0.5.0-preview+abc1234
```

Where:

- `0.5.0` comes from `Cargo.toml` after stripping the `-dev` suffix.
- `preview` is the release channel identifier.
- `abc1234` is the short SHA for the successful `main` commit.

This follows the Ghostty-style format requested by the user. In the Homebrew
tap, the version will be set explicitly so the formula does not rely on URL
inference.

## High-Level Flow

There will be two separate Homebrew update paths:

- Stable release path: existing tagged release workflow updates `jackin`.
- Preview path: new `main` workflow updates `jackin@preview`.

The preview publication flow is:

1. A commit lands on `main`.
2. The normal CI workflow succeeds.
3. A new preview workflow runs after that successful CI completion.
4. The workflow reads the base version from `Cargo.toml`.
5. It computes `preview_version = <base>-preview+<shortsha>`.
6. It downloads the source tarball for the exact commit SHA.
7. It computes the tarball `sha256`.
8. It updates `Formula/jackin@preview.rb` in `donbeave/homebrew-tap`.
9. It commits and pushes the tap change if the formula content changed.

## `jackin` Repository Changes

Add a new workflow at `.github/workflows/preview.yml`.

### Triggering

The workflow should run on:

- `workflow_run` for the existing `CI` workflow
- only when that workflow completed successfully
- only for the `main` branch
- optional `workflow_dispatch` for manual reruns

This ensures preview publication reflects the latest successful CI state, not
merely the latest pushed commit.

### Workflow Responsibilities

The workflow should:

1. Check out the exact commit that passed CI.
2. Read the package version from `Cargo.toml`.
3. Strip a trailing `-dev` suffix to get the semver base.
4. Compute:
   - `full_sha`
   - `short_sha`
   - `preview_version="${base_version}-preview+${short_sha}"`
5. Download the commit tarball from GitHub:
   - `https://github.com/donbeave/jackin/archive/${full_sha}.tar.gz`
6. Compute `sha256` for that tarball.
7. Clone `donbeave/homebrew-tap` using `HOMEBREW_TAP_TOKEN`.
8. Rewrite `Formula/jackin@preview.rb` with the new version, URL, and checksum.
9. Commit and push only when the resulting file differs from the current one.

### Secrets

The workflow will use the existing tap automation model:

- `HOMEBREW_TAP_TOKEN` with permission to push to `donbeave/homebrew-tap`

No new external service is required.

## Tap Changes

Add a new formula at `Formula/jackin@preview.rb`.

### Formula Shape

The preview formula should intentionally mirror the stable formula closely.

Expected structure:

```ruby
class JackinATPreview < Formula
  desc "Matrix-inspired CLI for orchestrating AI coding agents at scale"
  homepage "https://github.com/donbeave/jackin"
  url "https://github.com/donbeave/jackin/archive/<fullsha>.tar.gz"
  version "0.5.0-preview+abc1234"
  sha256 "..."
  license "Apache-2.0"

  depends_on "rust" => :build
  depends_on "docker" => :optional

  conflicts_with "jackin", because: "preview and stable install the same binary"

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "jackin", shell_output("#{bin}/jackin --version")
  end
end
```

### Why a Separate Formula

The separate formula provides:

- a clear install surface for preview users: `brew install donbeave/tap/jackin@preview`
- isolation from the stable update path
- a reproducible, pinned formula for each published preview state

### Why Not Reuse `head`

The stable formula already exposes `head`, but `head` is not a great fit for the
desired workflow because:

- it is less discoverable than a named preview channel
- it tracks a moving branch at install time rather than a pinned successful CI
  commit
- it does not provide a user-facing preview version string

`head` remains useful for direct developer installs, while `jackin@preview`
becomes the curated rolling preview channel.

## Update Semantics

Each published preview formula is tied to one exact commit tarball and checksum.
That means:

- installs are reproducible for the current formula revision
- the tap always points to the latest successful `main` commit
- users can run `brew update && brew upgrade jackin@preview` to move forward

The tap does not preserve a historical catalog of preview formula revisions.
It only advances the current `jackin@preview` formula.

## Failure Handling

The preview workflow should fail without modifying the tap when:

- `Cargo.toml` does not contain a parseable version
- the tarball download fails
- the checksum cannot be computed
- the tap cannot be cloned or pushed

The workflow should no-op successfully when the generated formula content is
identical to the current tap file.

## User Experience

Users get three installation modes:

- `brew install jackin` for stable tagged releases
- `brew install jackin@preview` for the latest successful `main` build
- `brew install --HEAD jackin` for a direct developer-style install from Git

This makes each channel distinct:

- stable = tagged release
- preview = curated rolling prerelease from green `main`
- head = raw branch install

## Verification Plan

When implementing this design, verify the following:

1. The new preview workflow runs only after successful CI on `main`.
2. The generated preview version string matches the expected
   `<base>-preview+<shortsha>` format.
3. `Formula/jackin@preview.rb` is updated correctly in the tap.
4. `brew audit --strict --formula jackin@preview` passes in the tap repository.
5. A local install of `donbeave/tap/jackin@preview` succeeds.
6. The existing stable release workflow still updates only `jackin`.

## Implementation Summary

Implementation will involve:

- adding `.github/workflows/preview.yml` in `jackin`
- adding `Formula/jackin@preview.rb` in `homebrew-tap`
- documenting preview installation in the tap README and relevant project docs

No changes are required to the stable release formula beyond leaving it in
place.
