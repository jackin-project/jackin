# Plan 021: Phase 1/2 — `missing_docs` on `jackin-protocol` plus a typed wire error

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat e80d5cc0a..HEAD -- crates/jackin-protocol/`
> If plan 019 landed, `lib.rs`/`attach.rs` will carry a `#![deny(…)]` block and
> checked-slicing rewrites — expected; coordinate around them. On any other
> mismatch with the "Current state" excerpts, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW-MED (doc sweep is additive; the error-typing change touches a wire enum variant — see the compatibility note)
- **Depends on**: none hard (sequence after plan 019 if both are in flight, to avoid attach.rs conflicts)
- **Category**: docs / tech-debt
- **Planned at**: commit `e80d5cc0a`, 2026-07-09

## Why this matters

Roadmap Phase 1's rustdoc gates (item 3) name the foundational crates for `missing_docs` adoption "crate-by-crate behind the dry run rather than workspace-wide, because the first adoption sweep is large" — and `jackin-protocol` is the right first crate: it is pure, it is the host↔capsule contract (documentation here is the API an agent copies when threading a new frame — one of the roadmap's own "golden agent tasks"), and its measured public surface is bounded (165 public items, 5 `pub mod`, 10 source files). Alongside, one genuine stringly-typed wire error survives from the Phase 2 audit: `ClientFrame::ClipboardImageError(String)` ships an unstructured message over the protocol, so the receiving side can only substring-match failures. (The audit's other two "String error" candidates were vetted down: `ParseMountIsolationError(String)` already derives thiserror and carries the offending input — idiomatic; `ParseProfileError` needs only a mechanical thiserror conversion, recorded in the index ledger as a small independent item.)

## Current state

Verified at the planning commit.

- No `missing_docs` anywhere in the workspace (audit-confirmed; root `Cargo.toml` `[workspace.lints.rust]` lines 118-147 has no entry, and no crate sets it locally).
- `jackin-protocol` surface: 165 items matching `^\s*pub (fn|struct|enum|trait|type|const|mod|use)`, 5 `pub mod`, 10 files. Many items already carry docs (e.g. `ExitAction` at `lib.rs:99` and `Provider` at `lib.rs:215` are fully documented, variant-by-variant); the sweep fills gaps, it does not start from zero.
- The wire enum variant, `crates/jackin-protocol/src/attach.rs:485-494`:

  ```rust
      Detach,
      FocusIn,
      FocusOut,
      ClipboardImage(ClipboardImage),
      ClipboardImageStart(ClipboardImageStart),
      ClipboardImageChunk(ClipboardImageChunk),
      ClipboardImageEnd(ClipboardImageEnd),
      ClipboardImageError(String),
      HostNotice(String),
  ```

  (`HostNotice(String)` is a display-text notice by design — human-readable payload is its contract; leave it. `ClipboardImageError` is a *failure signal* consumed programmatically — that is the one to type.)
- Wire-compatibility context: `ClientFrame` is hand-encoded/decoded in attach.rs (length-prefixed frames; see the `u16::try_from(...)` length sites around attach.rs:631-670). Changing the variant's payload changes the frame encoding. **Pre-release policy applies** (PRERELEASE.md: breaking changes OK, no migration shims; host and capsule ship in lockstep from one workspace), so an encoding change is acceptable — but keep it minimal and version-conscious: the payload stays a single string on the wire, gaining a machine-readable `kind` prefix is NOT the approach; instead type the Rust surface and encode kind + message as two fields only if the existing frame codec makes that trivial. Read the encoder/decoder for `ClipboardImageError` first and pick the cheaper of: (a) enum with `kind` byte + message string on the wire, or (b) keep wire = message string, add `ClipboardErrorKind` derived on the Rust side at construction sites. Bias to (a) only if the codec change is ≤ ~30 lines total.
- Existing doc conventions to match: `Provider`'s per-variant docs (`lib.rs:215+`) and `ExitAction`'s type-level doc (`lib.rs:95-99`) are the exemplars — short, contract-stating, no restating of names. `missing_errors_doc`/`missing_panics_doc` are workspace-allowed (Cargo.toml:201-202), so `# Errors`/`# Panics` sections are NOT required.
- Adoption mechanism (same as plan 019): a crate-level inner attribute in `crates/jackin-protocol/src/lib.rs` — `#![deny(missing_docs)]` — after the `//!` header (and after plan 019's clippy block if present), since the workspace table stays untouched until more crates adopt.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Dry run (worklist) | `cargo clippy -p jackin-protocol -- -W missing_docs 2>&1 \| grep -c 'missing documentation'` | a count |
| Crate gate | `cargo clippy -p jackin-protocol --all-targets -- -D warnings` | exit 0 |
| Doc build | `cargo doc -p jackin-protocol --no-deps --locked` | exit 0, no warnings |
| Crate tests | `cargo nextest run -p jackin-protocol` | all pass |
| Dependent crates | `cargo nextest run -p jackin-capsule -p jackin-console -p jackin-runtime -p jackin` | all pass |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-protocol/src/**` — doc comments + the `#![deny(missing_docs)]` attribute + the `ClipboardImageError` typing (attach.rs encoder/decoder + variant)
- Call sites of `ClipboardImageError` in consumer crates (find them: `rg -n 'ClipboardImageError' crates/ -g '*.rs'`) — construction and match sites updated to the typed form
- `crates/jackin-protocol/README.md` (public-API section refresh if item names change)
- Roadmap Phase 1 rustdoc item 3 status (first crate adopted)
- `plans/code-health/README.md` row

**Out of scope**:
- `missing_docs` on any other crate (next waves: core, config, manifest, env, term — in the index ledger)
- `#[non_exhaustive]` on protocol enums — OPEN DECISION recorded in the index (conflicts with the documented exhaustive-match drift-guard on `Provider`); do not add it
- `HostNotice(String)` — display text by design
- `ParseProfileError` thiserror conversion in jackin-core (separate S item in the ledger)
- Any frame OTHER than `ClipboardImageError`

## Git workflow

- Branch off `main`: `docs/protocol-missing-docs`.
- Two commits minimum: `feat(protocol): type the clipboard image error frame` then `docs(protocol): document the public surface and deny missing_docs`. `-s`, push each. PR to `main`; do not merge. Touches the capsule dependency closure → capsule smoke block in the PR body.

## Steps

### Step 1: Type the clipboard error

Read the `ClipboardImageError` encode/decode paths in attach.rs and every consumer (`rg -n 'ClipboardImageError'`). Introduce:

```rust
/// Why a clipboard image transfer failed. Machine-matchable; the display
/// text carried alongside is for operator surfaces only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardImageError {
    /// The assembled image exceeded the transfer size bound.
    TooLarge { limit_bytes: u64 },
    /// Chunks arrived out of order or a chunk went missing.
    ChunkSequence,
    /// The payload failed to decode as a supported image format.
    UnsupportedFormat,
    /// Anything else — carries the original human-readable message.
    Other(String),
}
```

— **derive the actual variant set from the real construction sites** (the list above is the shape, not gospel: enumerate what messages are constructed today and give each a variant; `Other(String)` remains the compatibility catch-all). Wire encoding per the Current-state decision rule (kind byte + message vs string-only). Update all construction/match sites; matches on the old `(String)` payload that substring-matched now match variants.

**Verify**: `cargo nextest run -p jackin-protocol -p jackin-capsule -p jackin-console` → all pass; `rg 'ClipboardImageError\(String\)' crates/` → no matches.

### Step 2: Doc sweep

Run the dry-run count. Then document every flagged item: one-to-three-line contract-stating docs matching the `Provider`/`ExitAction` style. For wire types, state the frame's direction (host→capsule or capsule→host) and when it is emitted — that is the information an agent threading a new frame needs. Do NOT pad: a `pub use` re-export needs no doc if the target is documented (re-check what the lint actually flags).

**Verify**: dry-run count → 0.

### Step 3: Deny + gates

Add `#![deny(missing_docs)]` to `crates/jackin-protocol/src/lib.rs` (after the `//!` header and any plan-019 block). Full crate + dependent verification.

**Verify**: `cargo clippy -p jackin-protocol --all-targets -- -D warnings` → exit 0; `cargo doc -p jackin-protocol --no-deps --locked` → clean; `cargo nextest run -p jackin-protocol -p jackin-capsule -p jackin-console -p jackin-runtime -p jackin` → all pass.

### Step 4: Docs + roadmap

- `crates/jackin-protocol/README.md`: public-API section reflects the typed error (structure unchanged otherwise).
- Roadmap Phase 1 rustdoc item 3: `jackin-protocol` adopted; remaining five crates listed as next; note the adoption mechanism (crate-inner `#![deny]` until workspace-wide).

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- Step 1 adds/updates tests in `attach.rs`'s sibling `tests.rs`: encode→decode round-trip for each new error variant (pattern: the existing clipboard frame round-trip tests — find them via `rg -n 'ClipboardImage' crates/jackin-protocol/src/attach/tests.rs`).
- No tests needed for docs; the deny attribute is the gate.

## Done criteria

- [ ] `ClipboardImageError` is a typed enum; round-trip tests per variant pass; no `(String)` payload remains
- [ ] `#![deny(missing_docs)]` in protocol lib.rs; dry-run count 0; `cargo doc` clean
- [ ] Consumer crates green (capsule, console, runtime, jackin)
- [ ] README + roadmap updated; `plans/code-health/README.md` row updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`

## STOP conditions

Stop and report back if:

- `ClipboardImageError` construction sites don't partition into a small variant set (>6 distinct failure shapes) — the enum design needs the operator's input.
- The wire codec change for option (a) exceeds ~30 lines or touches frames other than this one.
- The dry-run count exceeds 250 (surface much larger than the 165-item proxy — re-scope before writing docs for days).
- Any consumer crate matches on the error's *message text* in a way a variant cannot express (report the site — that is exactly the debt this fixes, but it may encode behavior needing a decision).

## Maintenance notes

- Next `missing_docs` crates in ledger order: jackin-core (312 items — biggest; consider splitting), jackin-config (196), jackin-term (154), jackin-env (99), jackin-manifest (41 — trivially next).
- When all six foundational crates carry the attribute, move `missing_docs` into the workspace rust-lints table scoped via per-crate `[lints]` or keep inner attributes — decide then.
- Reviewer should scrutinize: the variant set against real construction sites (no speculative variants), and doc quality on the frame enums (direction + trigger stated, not name restated).
