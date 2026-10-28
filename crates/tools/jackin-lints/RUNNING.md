# Running custom lints

```sh
# Native MBX dispatch lets Dylint's driver resolve its pinned Rustup compiler.
mise run check:custom-lints
```

Install the exact compiler prerequisites once, using Rustup:

```sh
rustup toolchain install nightly-2026-10-18 --profile minimal \
  --component clippy,llvm-tools-preview,rustc-dev,rustfmt
```

The task verifies the full compiler commit and installed components, builds from
this crate's directory for its Cargo configuration and UI paths, and runs
the complete Nextest UI suite. The task resolves Mise's selected MBX, `dylint-link`,
and Cargo plugins before constructing its private tool path. Library builds and
Nextest run through that MBX under the explicit nightly. A private native `cargo`
symlink to MBX routes Dylint's recursive Cargo commands through the same provider;
MBX excludes its alias before selecting Cargo through Rustup's shims. The alias
remains alive for the entire pipeline, with the selected Nextest directory ahead
of Cargo home. The task selects its exact linker with encoded Cargo flags
(including paths with spaces). This intentionally replaces the
configuration's PATH-based linker selection. The supported linker produces
the ABI-labelled library; the task never relabels a library built by another compiler. It copies
only that library into a fresh search directory and invokes root Dylint with
`--no-metadata --all --fail-on-no-libraries -- --workspace --locked`. Workspace metadata
autobuild is removed completely. A stale library elsewhere cannot enter that
search directory, and a missing library fails instead of silently checking none.
The root check clears the library-build linker flags; the nightly driver remains
selected by the library ABI name.

The qualified compiler is `nightly-2026-10-18` (`rustc 1.101.0-nightly`,
commit `8006091eb`, 2026-10-17). The date names the published nightly manifest;
the compiler commit date is one day earlier. `rustup run` makes the lint's
nightly take precedence over a parent workspace's Mise-selected stable Cargo.
The native MBX alias followed by Rustup shims also lets Dylint's isolated driver
build select its own written toolchain file after Dylint removes inherited
compiler environment variables. Putting a stable compiler's Cargo ahead of this
dispatch chain would bypass that selection. Inherited MBX Cargo-shim dispatch
variables are cleared; its cache, session, and scheduler settings are preserved.

The UI test runs Dylint 6.2.0's supported compiletest harness inside Nextest and
checks the positive, clean, and closure-boundary fixtures.

The current generated CI does not run this lint on a schedule or as a required
job. The pinned tools and UI corpus support local qualification; they do not
provide hosted enforcement. Wire an explicit generated task and required lane
before describing Dylint as a CI gate.

## Current qualification

The standalone nightly library build and Nextest UI corpus were previously
qualified on macOS ARM with the pinned compiler. The one-line vendored
`branches` fix resolves the prior root-workspace blocker. In an isolated
`--no-default-features --lib` check, the original package passes on stable Rust
1.99.1 but fails on nightly-2026-10-18; the patched package passes on both.
Stable `jackin-image` and `jackin-xtask` checks also pass. A pinned
`nightly-2026-10-18` check of `jackin-usage-store-backend` also passes with
vendored `branches` and its `std` feature disabled. The full nightly
workspace and Dylint qualification have not been rerun after the fix. Those
earlier results do not qualify the complete workspace after this dependency
correction.

The canonical Mise task runs the 11 selected Rust regression tests through pinned
Nextest before it starts the complete check. It installs no toolchain and makes
no directory-level Rustup override. The task remains a local pilot, not a
required native preview or CI gate.
