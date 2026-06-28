# jackin-build-meta

Build-script helper crate for jackin❯ binaries.

Workspace binaries need the same embedded runtime version string: the Cargo package version plus the current short git SHA when the checkout is available. Without this crate, each binary would need a parallel `build.rs` implementation and those rules could drift.

This crate intentionally stays tiny. It exposes `derive_workspace_crate_version`, emits the `cargo:rerun-if-*` lines that make Cargo rebuild when `.git/HEAD`, refs, packed refs, or `JACKIN_VERSION_OVERRIDE` change, and returns the version string for the caller's own `cargo:rustc-env` name.
