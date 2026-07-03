# Plan 015: Settings ↔ workspace-editor parity by construction — one row model per tab family

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-console/src/tui/screens/ crates/jackin-console/src/tui/components/editor_rows.rs crates/jackin-console/src/tui/components/auth_panel.rs crates/jackin-console/src/tui/mount_display.rs`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P1 (the user-visible parity debt; the docs call divergence here "a bug")
- **Effort**: L
- **Risk**: MED (snapshot-guarded; indentation rules are load-bearing)
- **Depends on**: none hard (independent of 012; touches different layers)
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03
- **Execution status**: BLOCKED — drift check found existing changes in `auth_panel.rs`, `editor_rows.rs`, and the workspace view before plan work.

## Why this matters

The design canon: "Settings screens mirror workspace screens … must reuse the workspace widgets and flow helpers wherever behavior is the same; keep separate code only for the different persistence target or config scope. Visual drift between the two is a bug" (`components.mdx` rule 6), reinforced by `dialogs.mdx` §"Settings ↔ Workspace Editor Parity" ("every fix or addition that lands in one panel must immediately be audited against the other"). Reality: each tab family's read-only rows are built twice — two auth-row enums with parallel renderers, two env/secrets line builders, two mount line builders (with a **live header drift**: the editor uses shared `render_mount_header`, settings hand-builds its own header with a different column set), two general-tab row styles — and the `▸ `-cursor/label/value idiom is copy-pasted ~10 times because no labeled-row primitive exists. Every row fix must be made twice; the mount header shows what happens when it isn't. This plan lifts one row model + renderer per family and a shared labeled-field line helper, so parity holds by construction.

## Current state

All paths under `crates/jackin-console/src/tui/` (each verified at `a2ec1b237`):

**Auth rows — duplicated enums + renderers:**
```rust
// screens/editor/view/auth_tab.rs:22
pub(crate) enum EditorAuthLineRow {
    AuthKind { label: String },
    WorkspaceMode { mode_label: String, inherited: bool },
    WorkspaceSource { display: AuthSourceDisplay },
    WorkspaceSourceFolder { display: AuthSourceFolderDisplay },
    RoleHeader { role: String, expanded: bool },
    RoleMode { mode_label: String },
    RoleSource { display: AuthSourceDisplay },
    RoleSourceFolder { display: AuthSourceFolderDisplay }, ... }
// screens/settings/view.rs:56
pub enum SettingsAuthLineRow {
    Kind { label: String },
    Mode { mode_label: String },
    Source { display: AuthSourceDisplay },
    SourceFolder { display: AuthSourceFolderDisplay },
    Spacer, }
```
Both feed parallel `render_auth_line`/`render_auth_source_line`(/`render_(auth_)source_folder_line`) fns (editor `auth_tab.rs:266,:399`; settings `view.rs:1045,:1105`) over the same `AuthSourceDisplay`/`AuthSourceFolderDisplay` types. The editor enum = settings enum ∪ role-scoped variants — i.e. one enum parameterized by scope covers both.

**Env/Secrets — two line builders with near-identical closures:**
```rust
// screens/editor/view/secrets_tab.rs:18  (secret_lines over SecretsRow)
pub(crate) fn secret_lines(rows, cursor, show_cursor, area_width,
    value_for: impl Fn(&SecretsScopeTag, &str) -> Option<SecretValueDisplay>,
    is_unmasked: ..., role_in_registry: ..., role_var_count: ...) -> Vec<Line>
// screens/settings/view.rs:882  (env_lines over SettingsEnvRow)
pub fn env_lines(rows, selected_row, show_cursor, area_width,
    value_for: impl Fn(&SettingsEnvScope, &str) -> Option<SecretValueDisplay>,
    is_unmasked: ..., role_var_count: ...) -> Vec<Line>
```
Both delegate leaf rendering to the already-shared `render_secret_key_line` (`components/editor_rows.rs:121`) — the *loop + row enum + sentinel/indent logic* is what's forked.

**Mounts — live drift:** editor `mounts_tab.rs:65` `mount_lines` starts `vec![render_mount_header(path_w)]` (shared header, `editor_rows`-adjacent; also used by `mount_display.rs:187`); settings `view.rs:1152` `global_mount_lines` pushes its own hand-built header `Line::from(Span::styled(...))` with a different column set.

**General tabs:** editor `general_tab.rs:10` uses a shared `render_editor_row` helper; settings `view.rs:782` `general_lines` hand-inlines spans with its own `label_width` (26 vs env's 22).

**The copy-pasted cursor idiom** (~10 sites): `let cursor_col = if selected { "\u{25b8} " } else { "  " };` at `secrets_tab.rs:33`, `auth_tab.rs:275,:285,:319`, settings `view.rs` (`:805,:857,:895,:1053,:1087,:1112,:1171` region), `auth_panel.rs:577` (`cursor_span`).

**Indentation rules that MUST survive** (`dialogs.mdx` §"Env / Secrets Sentinel Indentation"): workspace-level `+ Add environment variable` sentinel aligns at the 2-char `cursor_col` indent only; role-level sentinels and role section headers keep 5-space `"     "` indentation (visible in `secrets_tab.rs:50,:57,:89`).

**Existing shared home for this layer:** `components/editor_rows.rs` (has `render_secret_key_line`, `action_row_style`, `disclosure_style`). Extend this module — do not create a new junk drawer.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-console` then full | pass |

## Scope

**In scope**:
- `components/editor_rows.rs` — grows: `labeled_field_line(...)` helper; shared `AuthLineRow` enum + renderers; shared env/secrets line-builder; mount header/lines unification with `mount_display.rs`
- `screens/editor/view/{auth_tab,secrets_tab,mounts_tab,general_tab}.rs` — become thin: build rows with scope data, call shared builders
- `screens/settings/view.rs` — same; delete `SettingsAuthLineRow`, `env_lines`' body, `global_mount_lines`' hand-built header, `general_lines`' inline spans
- `components/auth_panel.rs:577` `cursor_span` — reuse the helper
- Width/geometry twins (`editor_auth_line_width` and settings equivalents) — single-source alongside the renderers (the parity doc's "content width agreement" rule)

**Out of scope**:
- Input/dispatch handlers and modal flows (rows are read-only display).
- `save_preview.rs` (plan 016).
- Trust tab content (settings-only; it only adopts `labeled_field_line`).
- Any intentional content differences between panels (different tabs/rows are fine; different *rendering* of the same row kind is not).

## Git workflow

Branch (operator confirm): `refactor/settings-editor-row-unification`. `git commit -s` + push; commit per tab family. Update `dialogs.mdx` parity section same PR (state that rows are now shared by construction).

## Steps

### Step 1: `labeled_field_line` helper

In `editor_rows.rs`, add:

```rust
/// One cursor-gutter row: "▸ " (or "  ") + padded label + styled value.
/// `indent` prefixes AFTER the gutter (role-level rows pass "     ").
pub fn labeled_field_line(selected: bool, indent: &str, label: &str,
                          label_width: usize, value: Line<'static> /* or spans */,
                          emphasis: FieldEmphasis) -> Line<'static>
```

Derive the exact span construction from the most complete existing site (read `auth_tab.rs:266-330` and settings `view.rs:1045-1120` side by side; where they differ visually today, the EDITOR's rendering wins — it is the workspace surface the settings panel is documented to mirror). `FieldEmphasis` covers the bold-when-focused/inherited-dim variants both sides use.

**Verify**: `cargo nextest run -p jackin-console` — no behavior change yet (helper unused).

### Step 2: Auth family

Add shared `AuthLineRow` in `editor_rows.rs` = the editor enum with scope carried as data (workspace vs role variants stay — they render differently by design; settings constructs only the workspace-shaped subset + `Spacer`). Move/merge the editor's `render_auth_line`/`render_auth_source_line`/`render_source_folder_line` onto it (built on `labeled_field_line`), delete the settings twins (`view.rs:1045,:1105`) and `SettingsAuthLineRow` (`view.rs:56`), point both tabs at the shared builder. Single-source the line-width fn.

**Verify**: `cargo nextest run -p jackin-console` — editor snapshots unchanged; settings snapshots may change ONLY where they drift from the editor today (each such diff is a parity fix — list them in the PR body).

### Step 3: Env/Secrets family

Unify `secret_lines` (`secrets_tab.rs:18`) and `env_lines` (`view.rs:882`) into one builder in `editor_rows.rs`, generic over the scope tag (both closures take `(&Scope, &str)` — introduce a small trait or make the builder generic over `S` with the closures as-is). Preserve the sentinel indentation rules exactly (2-char workspace, 5-space role — the doc calls violations bugs). Delete both old loops; both tabs call the shared one. Keep `label_width` one value (22 — the env/secrets value today; general-tab width handled in Step 5).

**Verify**: `cargo nextest run -p jackin-console` — same snapshot policy as Step 2; sentinel-indent tests (if present: `rg -n 'sentinel' crates/jackin-console/src/tui --glob '*test*'`) must pass unchanged.

### Step 4: Mounts family

Make settings `global_mount_lines` (`view.rs:1152`) use `render_mount_header` with the same column set as the editor (`mounts_tab.rs:72`), reconciling with `mount_display.rs:181-187`'s existing usage. If the settings table legitimately lacks a column (e.g. Isolation applies only to workspace mounts — check the two headers' column lists first), parameterize `render_mount_header(columns)` rather than forking; the docs' Isolation-column reference is in `commands/workspace.mdx` — read the two current headers and record which columns each panel shows in the PR body.

**Verify**: `cargo nextest run -p jackin-console`; the settings mount header snapshot changes to the shared header — intended fix.

### Step 5: General tabs

Route settings `general_lines` (`view.rs:782`) and editor `general_tab.rs` rows through `labeled_field_line` / `render_editor_row` (merge those two — `render_editor_row` likely becomes `labeled_field_line`'s thin wrapper or vice versa; keep one). Pick ONE label width policy (per-tab width passed in; no hardcoded 26-vs-22 divergence unless content demands it — if it does, it's a parameter, not a fork).

**Verify**: `cargo nextest run -p jackin-console` → pass; `rg -n 'cursor_col = if' crates/jackin-console/src/tui` → 0 (all through the helper).

## Test plan

- Existing editor/settings view snapshot tests are the net; policy: editor snapshots must NOT change; settings snapshot diffs are each a named parity fix.
- New: a **parity test** — for a row kind present in both panels (auth Kind/Mode/Source, env key row, mount row), build both panels' lines from equivalent data and assert the rendered spans are identical modulo scope-specific content. This is the regression test that makes future drift a test failure instead of an operator bug report. Place in `crates/jackin-console/src/tui/view/tests.rs` (model on existing view tests there).

## Done criteria

- [ ] fmt / clippy / `cargo nextest run` exit 0
- [ ] `rg 'SettingsAuthLineRow' crates/` → 0
- [ ] `rg -n 'cursor_col = if' crates/jackin-console/src/tui` → 0
- [ ] Settings mount header rendered by `render_mount_header` (grep call site)
- [ ] One env/secrets line builder (`rg -n 'fn (secret|env)_lines' crates/jackin-console/src` → 1 shared fn or thin per-panel wrappers with no span logic)
- [ ] Parity test exists and passes
- [ ] `dialogs.mdx` parity section updated; `plans/README.md` updated

## STOP conditions

- Editor snapshots change at any step (the editor is the reference surface; a change means the shared builder didn't reproduce it).
- Sentinel indentation tests fail or the 2-char/5-space rule can't be expressed through the shared builder.
- The settings/editor mount column sets differ for a documented reason (found in docs or code comments) — parameterize, and if even that fights back, STOP with the column table.
- The scope-generic env builder needs more than ~2 trait methods — the abstraction is wrong; report the closure-signature table.

## Maintenance notes

- The parity test is the enforcement mechanism the docs' manual audit rule lacked — extend it when adding row kinds.
- Reviewer: check `editor_rows.rs` didn't become a junk drawer — everything added is row-rendering for the two panels, nothing else.
- Deferred: hover styling unification for rows (editor mounts pass `hovered_row`, settings don't — behavior difference to reconcile as its own decision).
