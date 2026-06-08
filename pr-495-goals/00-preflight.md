# Goal — Phase 0: Preflight & spec gaps

Parent index: [`../PR-495-FIXES.md`](../PR-495-FIXES.md). HEAD baseline: `f920b29a`.

Orient, confirm the audit's headline work is already landed, and settle the spec contradictions that later phases depend on. This phase changes almost no code — it protects the later phases from acting on stale assumptions.

## Tasks

| ID | Status | Files / evidence | Verify | Acceptance |
|---|---|---|---|---|
| `ARCH-0` | done | `crates/jackin/src/runtime/` absent; `isolation/` = `tests.rs` only; `crates/jackin-diagnostics/Cargo.toml` core-only | `cargo check -p jackin -p jackin-diagnostics` | No orphaned `runtime/`/`isolation/` trees reappear; diagnostics has no `jackin-tui` dep. Guard, do not re-delete. |
| `PRE-1` | pending | `docs/content/docs/reference/tui/dialogs.mdx`, `chrome.mdx` | docs build | Debug-info backdrop wording is identical across both pages and matches shipped behavior: modal body/background hidden by default-bg backdrop; reserved bottom chrome/status stays visible. |
| `PRE-2` | pending | `docs/content/docs/reference/tui/chrome.mdx` | docs build | Build-log overlay doc says `Esc`/`q` close; an inside body click is a no-op unless it hits the scrollbar. (Implementation tracked separately; confirm code matches before editing — see note.) |
| `PRE-3` | pending | settings vs workspace-editor Auth render paths | read-only audit | A written finding: are Settings Auth and workspace-editor Auth one render path or two? Result feeds `DLG-3`. A fix on one that leaves the other drifting violates settings/editor parity. |

## Detail

### `ARCH-0` — confirm landed, guard against regression
The audit's #1 item (delete orphaned trees) and the diagnostics-dependency item are already done on this branch (see the index "Already landed" table). Action here is a no-op verification: run the `cargo check` and confirm the trees are still gone. If a later rebase reintroduces a shadowed `runtime/`/`isolation/` child file, delete it — keep only the re-export shim `.rs`.

### `PRE-1` — settle the Debug-info backdrop wording
`dialogs.mdx` and `chrome.mdx` historically described the backdrop differently (status-preserving vs "must not erase persistent chrome"). The shipped behavior (verified: launch clears the area with `Clear` then preserves bottom chrome) is the stricter rule. Make both pages say exactly that. No code change expected — this is doc reconciliation that `DBG-*` acceptance leans on.

### `PRE-2` — settle build-log close semantics in docs
The intended rule is keyboard-only close. Before editing the doc, confirm the current launch code: the audit flagged `crates/jackin-launch/src/tui/subscriptions.rs` mapping ordinary overlay clicks to `BuildLogClosed`. Re-check that path at HEAD. If the code still closes on inside click, that is a **separate implementation task** — record it as a new `DLG-*`-style row in `50-dialogs-rows.md` rather than only fixing the doc. Do not let the doc claim a behavior the code does not have.

### `PRE-3` — Auth parity audit
Read both Auth renderers. `crates/jackin-console/src/tui/screens/settings/view.rs` owns Settings Auth (`render_auth_source_line`, `render_auth_source_folder_line`). Find the workspace-editor Auth renderer and determine whether it shares those functions or forks them. Write the answer into `DLG-3` before implementing the gutter fix.

## Done definition
- `ARCH-0` verified green.
- `dialogs.mdx` and `chrome.mdx` agree on the backdrop rule and the build-log close rule.
- `DLG-3` carries a one-line parity finding from `PRE-3`.
- Any code-level build-log task discovered in `PRE-2` is filed in `50-dialogs-rows.md`.
