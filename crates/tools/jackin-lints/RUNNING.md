# Running custom lints

```sh
# Dylint's nested driver build must resolve Cargo through rustup's shims.
mise run check:custom-lints
```

Install the exact compiler prerequisites once, using Rustup:

```sh
rustup toolchain install nightly-2026-10-18 --profile minimal \
  --component clippy,llvm-tools-preview,rustc-dev,rustfmt
```

The task verifies the full compiler commit and installed components, builds from
this crate's directory for its Cargo configuration and UI paths, and runs
the complete Nextest UI suite. The task resolves Mise's selected `dylint-link`
and Cargo plugins before putting Rustup shims first. It invokes those exact
plugins and selects that exact linker with encoded Cargo flags (including paths
with spaces), so Cargo-home binaries cannot shadow Mise's selected tools. This intentionally replaces the
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
The shim-first PATH also lets Dylint's isolated driver build select its own
written toolchain file after Dylint removes inherited compiler environment
variables. Direct compiler-toolchain binaries or a Mise Cargo wrapper ahead of
the shims would bypass that selection.

The UI test runs Dylint 6.2.0's supported compiletest harness inside Nextest and
checks the positive, clean, and closure-boundary fixtures.

The current generated CI does not run this lint on a schedule or as a required
job. The pinned tools and UI corpus support local qualification; they do not
provide hosted enforcement. Wire an explicit generated task and required lane
before describing Dylint as a CI gate.

## Current qualification

The explicit nightly library build and the Nextest UI corpus pass on macOS ARM
with the pinned compiler. The root workspace check currently fails closed:
Turso 0.8.3 brings `branches` 0.4.6 without its `std` feature, whose nightly path
calls the removed `core::intrinsics::abort`. Both Cargo lockfiles remain
unchanged. This pre-existing upstream dependency incompatibility requires a
separate dependency correction; it is not suppressed or treated as a passing
workspace lint run. The historical pilot result below is not a current result.

The canonical Mise task runs its regression controls and complete check in one
script with a ten-minute execution timeout. It installs no toolchain and makes
no directory-level Rustup override. The task remains a local pilot, not a
required native preview or CI gate.
