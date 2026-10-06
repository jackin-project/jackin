# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

<!-- next-header -->

## [Unreleased]

### Added

- Added a non-TTY programmatic launch surface. `LoadOptions` now carries the decisions the interactive path prompts for — agent, account source folder, model, reasoning effort, launch env, pre-approved on-demand bindings, extra mounts, and force — and `LoadOptions::programmatic` validates them up front: a missing trust grant, an unresolved agent, or a `--role-branch` is a validation error rather than a dialog no daemon can answer. A programmatic launch starts the container without attaching and reports the instance identity it claimed.
- Added `--on-demand` to `jackin config env set` and `jackin workspace env set`, and an `On demand` column to both `env list` tables. An on-demand value never enters the launch environment; only its name reaches the container, and the host resolves it per use through `jackin-exec`.
- Added `image_decision` and `published_image` to the `jackin load --dry-run --format json` plan, resolved from the role manifest so the dry run reports the image the launch would actually use.
- Added `cargo xtask release-verify <archive>.tar.gz` to verify signed release archives against their SHA256 sidecar, cosign bundle, GitHub artifact attestation, and SBOM JSON.

### Changed

- **Breaking:** Docker launches now default to the `standard` security profile instead of `compat`. `standard` keeps sudo off, disables DinD unless explicitly granted, applies resource limits, and enables `no-new-privileges` while sudo is off. Use `--docker-profile compat` or `[docker] profile = "compat"` to opt back into privileged DinD, passwordless sudo, and legacy resource-unlimited behavior.
