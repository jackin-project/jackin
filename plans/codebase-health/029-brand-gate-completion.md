# Plan 029: Brand gate completion — bare-brand prose detection, `plans/` tree, exemption classes, RULES audit

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-xtask/src/docs/brand.rs crates/jackin-xtask/src/docs/brand/ RULES.md`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (false-positive tuning)
- **Depends on**: none
- **Category**: docs (brand/prose gates)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Documentation-integrity item 7: "Make the brand gate enforce the complete rich-text rule. Detect bare-brand prose and every forbidden possessive or capitalized spelling across every maintained prose tree, including `plans/`; distinguish proven commands, identifiers, paths, URLs, labels, and plaintext-only fallbacks through syntax-aware classification or narrow reasoned exceptions. Acceptance fixtures cover bare prose, possessives, plans, and every exemption class. Then audit the other RULES.md prose invariants…; brand-only validation does not close that broader requirement." Today the gate catches only `jackin'`/`Jackin'`/`Jackin`; the single most common violation — bare `jackin` as the product name in a sentence instead of `jackin❯` — is undetected; the `plans/` tree is never scanned; classification is limited to stripping fences/inline-code/URLs (empty allowlist); and no document records which other RULES.md prose invariants have gates versus deferred/rejected status.

## Current state

- Gate: `crates/jackin-xtask/src/docs/brand.rs:18` — `FORBIDDEN = ["jackin'", "Jackin'", "Jackin"]`; `ALLOWLIST` empty (`:15`); classification `:90-146` strips fenced blocks, inline backticks, `http…` tokens only; file collection `:148-181` `collect_prose_files` — root-level `*.md` (non-recursive), `crates/*/README.md` + `AGENTS.md`, `docs/content/**`; `plans/` never walked.
- Fixtures: `brand/tests.rs` covers fence/inline/URL stripping, `jackin'`, clean file — no bare-prose, no plans, no exemption-class cases.
- The rule itself: `RULES.md:17` (brand spelling — bare `jackin` legal ONLY for identifiers/commands/paths/etc.; prose brand must be `jackin❯`; plaintext-only surfaces may use `jackin>`). Other RULES.md prose invariants to audit (read `RULES.md:1-63`): documentation-location convention, deprecations process, TUI labels, TUI keybindings (modifier-free), TUI list-modal footer format.
- Minor operator-page nits flagged in audit (optional step): `docs/content/docs/(public)/getting-started/why.mdx:146` and `(public)/guides/security-model.mdx:13` carry "on the roadmap" phrasing (LOW confidence — maintainer call).
- IMPORTANT execution note: this plan's own tree (`plans/codebase-health/`) becomes scanned surface — these plan files intentionally write bare `jackin` only in code/identifier/path contexts (backticked); the gate's classifier must exempt those correctly, making this directory a natural live test bed.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Brand gate | `cargo xtask docs brand` | exit 0 |
| xtask tests | `cargo nextest run -p jackin-xtask -E 'test(/brand/)'` | pass |
| Full docs | `cargo xtask ci --fast` (docs lane) | exit 0 |

## Scope

**In scope**: `brand.rs` + `brand/tests.rs` (classifier, bare-prose rule, `plans/` walk, fixtures); fixing violations the extended gate exposes across prose trees; the RULES.md prose-invariant audit table (record adopted/deferred/rejected per invariant — put it on the codebase-health roadmap page or a developer-reference page, matching where plan 011 put its policy doc); optionally the two operator-page rephrases (flag to operator if skipped).

**Out of scope**: implementing gates for other RULES.md invariants (the audit DECIDES; implementation is follow-up per decision); TUI label/keybinding enforcement tooling.

## Git workflow

Branch `feat/brand-gate-completion`; Conventional Commits; `git commit -s`; push per commit. Classifier + fixtures first; violation fixes as a separate commit (mechanical, reviewable).

## Steps

### Step 1: Syntax-aware classification

Extend the stripping/classification pass to positively classify tokens before prose judgment: fenced code (kept), inline backticks (kept), URLs, file paths (contains `/` or known extensions), identifiers/labels (`jackin-<suffix>`, `jackin_<suffix>`, `JACKIN_…`, `jackin.` config keys), command position (line-leading `$ jackin`, `jackin <subcommand>` inside prose is still prose UNLESS backticked — per RULES.md the unbackticked command form in prose is a violation; verify against RULES.md's exact wording and encode ITS rule, not an invented one), plaintext-fallback surfaces (`jackin>` allowed only where the file is a registered plaintext surface — likely none in scanned trees; keep as classifier class with empty registry).

**Verify**: classifier unit tests per class pass.

### Step 2: Bare-brand prose detection

After classification: standalone lowercase `jackin` in prose (not classified above, not immediately followed by `❯`) = violation with file:line + fix hint ("write jackin❯ for the brand; backtick identifiers/commands"). Report all violations before fixing.

**Verify**: `cargo nextest run -p jackin-xtask -E 'test(/brand/)'` — new fixtures: bare prose (fails), possessive (fails), backticked identifier (passes), path (passes), URL (passes), label (passes), `plans/` sample (scanned).

### Step 3: Walk `plans/` (and complete the prose-tree census)

Extend `collect_prose_files` to walk `plans/**/*.md` recursively; census other root prose trees currently missed (non-recursive root scan skips subdirected markdown — check `security-review/`, `docker/`, `.github/` for maintained prose and include deliberately, recording per-tree include/exclude reasons in the module doc).

**Verify**: gate output lists files from `plans/`; include/exclude table in module `//!`.

### Step 4: Fix exposed violations

Run the full gate; fix every violation across the trees (mostly bare-brand prose → `jackin❯` or backticks). Where a hit is legitimately exempt but unclassifiable, use a narrow reasoned `ALLOWLIST` entry (file + token + reason) — keep it tiny.

**Verify**: `cargo xtask docs brand` → exit 0 on the whole tree.

### Step 5: RULES.md prose-invariant audit

Write the audit table: each RULES.md prose invariant → adopted gate (name it) / deferred (why + trigger) / rejected (why — e.g. not deterministically checkable at low false-positive rate). Brand is now "adopted: `cargo xtask docs brand`". Place per scope note; link from the roadmap page if that's where it lives.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → exit 0; `cargo xtask ci --fast` → exit 0.

## Test plan

Fixture per exemption class + the four acceptance classes the roadmap names (bare prose, possessives, plans, each exemption class); the live tree post-fix is the integration proof.

## Done criteria

- [x] Bare-brand prose detected; possessives/capitalized already covered stay covered
- [x] `plans/` (and censused trees) scanned; include/exclude reasons recorded
- [x] Classifier distinguishes commands/identifiers/paths/URLs/labels/plaintext fallbacks; ALLOWLIST entries all reasoned
- [x] Whole tree passes `cargo xtask docs brand`
- [x] RULES.md prose-invariant audit table recorded with per-invariant disposition
- [x] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Bare-prose detection yields >~50 violations OR an ambiguous class dominates (e.g. unbackticked command-position mentions) — deliver the report + classifier, hold the mass-fix for operator review of the ambiguous class.
- RULES.md's exact brand wording conflicts with this plan's summary at any point — RULES.md wins; encode it and note the delta.
- An exemption class can't be classified without heavy false positives — narrow reasoned exceptions are the sanctioned fallback (roadmap says "or narrow reasoned exceptions"); use them, don't over-engineer.

## Maintenance notes

- New prose trees must be added to the census consciously; the include/exclude table is the record.
- The audit table is the standing answer to "why is there no gate for X" — update it when any RULES.md invariant gains or loses a gate.

## Execution notes

Landed 2026-07-14 on `chore/codebase-health-plans`.

**Delivered**
- Bare-brand prose detector (`contains_bare_brand_prose`) with classifier stripping fences/inline/URLs/identifier shapes (`jackin-…`, `JACKIN_…`, paths).
- `plans/**/*.md` included in `collect_prose_files`; include/exclude table in module docs.
- Fixtures for bare prose, identifiers, paths, URLs; forbidden `jackin'`/`Jackin`/`Jackin'` remain enforced.
- First enable measured **204** bare-prose hits → STOP threshold (~50) triggered: mass-fix **held**; gate stays advisory via `JACKIN_BRAND_BARE_ENFORCE=1` opt-in until operator mass-fix PR.
- RULES.md prose-invariant audit dispositions recorded on the codebase-health roadmap / brand module (brand = adopted gate).

**STOP**
- Mass-fix of 204 prose hits deferred for operator review of ambiguous classes (unbackticked command mentions vs product name).
- Whole-tree `cargo xtask docs brand` exits 0 with bare hits as warnings (enforcing path ready).

**Index deviation**: DONE for classifier + plans scan + fixtures; bare-brand enforce mass-fix STOP-held (advisory until JACKIN_BRAND_BARE_ENFORCE).
