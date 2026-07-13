# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

<!-- next-header -->

## [Unreleased]

### Added

- Added `cargo xtask release-verify <archive>.tar.gz` to verify signed release archives against their SHA256 sidecar, cosign bundle, GitHub artifact attestation, and SBOM JSON.

### Changed

- **Breaking:** Docker launches now default to the `standard` security profile instead of `compat`. `standard` keeps sudo off, disables DinD unless explicitly granted, applies resource limits, and enables `no-new-privileges` while sudo is off. Use `--docker-profile compat` or `[docker] profile = "compat"` to opt back into privileged DinD, passwordless sudo, and legacy resource-unlimited behavior.
