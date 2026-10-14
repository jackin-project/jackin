# Parity and release

Covers: F18-F20; B1-B8; N1-N14.

## Requirements

### Requirement: one fixture truth

Shared Rust fixtures SHALL prove identical account identity, ordered windows, labels,
values, lifecycle, freshness, and failures across CLI, Console, Capsule, FFI, and
desktop adapters. Layout differences SHALL not change semantic truth.

#### Scenario: stale partial generation

- GIVEN the same fixture envelope
- THEN every surface identifies the same stale account, age, retained windows, and
  provider-local failure
- AND no consumer performs provider I/O.

### Requirement: architecture proof

Static bypass inventory and adversarial cross-process tests SHALL prove one generation
under concurrent consumers, owner-exit survival, cancellation isolation, catalog
revision replacement, crash recovery, and absence of direct provider calls.

### Requirement: surface conformance

Console and Capsule SHALL have repository render-conformance matrices for major states,
80×24/narrow/wide, focus, keyboard, scroll, removal, and errors. CLI SHALL have golden
human/JSON, TTY/non-TTY, Unicode/plaintext, wrapping, exit and stdout/stderr tests.
Desktop SHALL pass build, lint, tests, real-host UI, accessibility, display, contrast,
transparency, and blessed-reference evidence.

### Requirement: immutable release chain

One exact desktop artifact digest SHALL pass Developer ID signing, notarization,
stapling, quarantine-aware Gatekeeper launch, publication, Homebrew cask install,
launch, and uninstall. Credential values SHALL remain external inputs and MUST NOT be
recorded in plans, logs, fixtures, or repository files.

### Requirement: single delivery branch

All jackin❯ implementation, documentation, proof, commits, and PR updates SHALL remain
on `chore/roadmap-unified-agent-usage` and PR #898. Stable tag publication occurs only
after that PR merges. A release workflow's required external Homebrew-tap PR is a
distribution operation, not another jackin❯ implementation PR, and requires explicit
operator approval and recorded evidence.
