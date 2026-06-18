# Auth Modal Subsystem — Unification Plan

Working design doc for collapsing the duplicated **auth-form modal lifecycle** shared between the **global settings** and **workspace editor** surfaces. Scratch artifact for the refactor; not published docs. Delete once the work lands and the durable behavior is captured in `docs/content/docs/internal/specs/auth-source-folder-sync.mdx`.

> **Scope note.** This started as "unify the source-folder picker," but that picker is one instance of a larger class (see Root cause). The goal is one shared implementation of the *entire* auth-form modal lifecycle, reused verbatim by both surfaces. Source-folder is the first slice, not the whole job.

## Why / goal

Operator-facing goal: the auth experience in global settings and in the workspace editor is *the same thing* — same look, same keys, same behavior — and must therefore be one implementation, never two that drift.

## Root cause (why the architecture permits this whole class of bug)

The source-folder bugs were not isolated defects; they are what the current structure *guarantees*. The auth-form modal lifecycle is implemented **twice**, once per surface, across two parallel modal enums with two separate stash APIs. Every behavior — open, key dispatch, validate, apply, error, cancel/restore — exists as a left/right twin. Any change to one twin that is not mirrored to the other produces exactly the class of bug we hit:

- Validation landed in the settings commit path; the editor commit path is a different function and stayed unvalidated → `~/.cargo` accepted for Codex.
- The rejection rendered differently per surface (file-browser inline reject line vs nothing) until both were pointed at the standard dialog.
- Earlier still, the new-tab credential overwrite and the source-folder-not-read bugs lived in one provisioning path while the other was fine.

The enabling condition is structural: **two modal subsystems for one concept.** Patching each twin as it bites leaves the structure — and the next divergence — in place. The correct fix removes the condition: one shared auth-modal subsystem that both surfaces drive, so a behavior cannot exist on one surface and not the other.

The pieces that are *already* shared prove this is feasible rather than a rewrite: `AuthForm` (the form model), `AuthFormTarget<K>`, `FileBrowserState`, the key-plan state machine, all rendering, `AuthKind`/`AuthMode`, validation, and the error dialog constructor are single implementations today. The duplicated remainder is only the **modal-enum wrapper, the dispatch handler, the stash API, and the apply/open glue** — which is exactly what this plan unifies.

## Current state — shared vs forked

### Already shared (one implementation, both surfaces)

| Piece | Location |
|---|---|
| Form model `AuthForm` + mutators (`cycle_mode`, `set_source_folder` at `auth_panel.rs:275`, `shows_source_folder` at `auth_panel.rs:280`) | `crates/jackin-console/src/tui/components/auth_panel.rs` |
| Key-plan state machine (`auth_form_key_plan_with_source_folder`) | `crates/jackin-console/src/tui/components/auth_panel.rs` |
| Rendering (`render_form`, `auth_panel_title`, `required_height`) | `crates/jackin-console/src/tui/components/auth_panel.rs` |
| `AuthKind` / `AuthMode` / `auth_mode_supports_source_folder` (`auth.rs:127`) | `crates/jackin-console/src/tui/auth.rs` |
| Folder validation `validate_auth_source_folder` → `validate_sync_source_dir` | `crates/jackin/src/console/domain.rs`, `crates/jackin-runtime/src/instance/auth.rs` |
| Error dialog constructor `invalid_source_folder_error_popup_state` | `crates/jackin-console/src/tui/components/error_popup.rs` |

### Forked (parallel twins — same job, separate code)

| Concern | Settings | Editor |
|---|---|---|
| Modal variant | dedicated `SettingsAuthModal::SourceFolderPicker { state }` (`model.rs:541`) | generic `Modal::FileBrowser { target, state }` + `FileBrowserTarget::AuthFormSourceFolder` (`app.rs:63`, `editor/model.rs:425`) |
| Open fn | inline arm `global_mounts/auth.rs:203-214` | `open_auth_source_folder_browser_from_form` `auth.rs:401-447` |
| `shows_source_folder()` guard at open | **absent** | present (`auth.rs:412`) |
| Commit dispatch | inline arm `global_mounts/auth.rs:309-340` | `effects.rs:270-314` → `apply_file_browser_to_editor` (`editor.rs:1167`) |
| Validation call | `validate_picked_source_folder` (`auth.rs:317`) | inline (`effects.rs:291`) |
| Wrong-folder UX | `auth.error` → promoted to settings error popup, modal kept (`auth.rs:325-328`) | stacked `ErrorPopup` sub-modal + return-guard (`editor.rs:775-784`) |
| Apply twin | `apply_source_folder_to_settings_auth_form` (`global_mounts/auth.rs:491-515`) | `apply_source_folder_to_auth_form` (`auth.rs:570-594`) |
| Stash API | `push_auth_modal` / `restore_pending_auth_form` (`model.rs:605-616`) + direct `modal_parents` | `open_sub_modal` / `pop_modal_chain` (`editor/model.rs:264-281`) |

### Core asymmetry

Two **distinct** modal enum types:

- `SettingsAuthModal` — `crates/jackin-console/src/tui/screens/settings/model.rs:523-550`. 5 variants. Has a bespoke `SourceFolderPicker`. No target concept.
- `ConsoleModal` (aliased `Modal`) — `crates/jackin-console/src/tui/app.rs:35-124`. ~20 variants. Source folder rides the generic `FileBrowser { target }`.

`SettingsAuthState<Env, Modal, PendingOpCommit>` is **generic**; its `Modal` param is instantiated as `SettingsAuthModal`, and its `modal_parents: Vec<Modal>` is therefore `Vec<SettingsAuthModal>`. Both surfaces share `FileBrowserState`, `AuthForm`, and `AuthFormTarget<K>` — but expose **different stash APIs** (`push_auth_modal`/`restore_pending_auth_form` vs `open_sub_modal`/`pop_modal_chain`) and **different modal shapes**. That is the gap to close.

### Full duplication inventory (the whole class, not just source folder)

Every row is a left/right twin doing the same job in two places. `S` = `crates/jackin/src/console/tui/input/global_mounts/auth.rs`; `E` = `crates/jackin/src/console/tui/input/auth.rs` (+ `effects.rs`, `editor.rs`).

| Lifecycle step | Settings twin | Editor twin |
|---|---|---|
| Open the auth form | `open_settings_auth_form` (S:89) | `open_auth_form_modal` (E:42) |
| Modal key dispatch | `handle_settings_auth_modal` (S:137) | `handle_auth_form_key` (E:253) |
| Open credential source picker | inline arm + `SourcePicker` | `open_auth_source_picker_from_form` (E:458) |
| Apply plain credential | `apply_plain_text_to_settings_auth_form` (S:457) | `apply_plain_text_to_auth_form` (E:549) |
| Apply 1Password source pick | (settings source picker arm) | `apply_plain_source_picker_to_auth_form` (E:516) |
| Open op picker | inline | `open_op_picker_from_auth_source` (E:601) |
| Apply op-picker commit | `apply_op_picker_to_settings_auth_form_committed` (S:594) | `apply_op_picker_to_auth_form_committed` (E:640) |
| Op-picker read (runner) | `apply_op_picker_to_settings_auth_form_with_runner` (S:527) | `apply_op_picker_to_auth_form_with_runner` (E:719) |
| Op-picker read (validator) | `apply_op_picker_to_settings_auth_form_with_validator` (S:540) | `apply_op_picker_to_auth_form_with_validator` (E:730) |
| Op-picker commit failed | `apply_op_picker_settings_commit_failed` (S:627) | `apply_op_picker_commit_failed` (E:679) |
| Open source-folder browser | inline `OpenSourceFolderBrowser` arm (S:203) | `open_auth_source_folder_browser_from_form` (E:401) |
| Apply source folder | `apply_source_folder_to_settings_auth_form` (S:491) | `apply_source_folder_to_auth_form` (E:570) |
| Validate source folder | `validate_picked_source_folder` (S:484) | inline (effects.rs:291) |
| Restore form after cancel | `restore_settings_auth_form` (S:448) | `restore_auth_form_after_op_picker_cancel` (E:693) |
| Missing-stash logging | ad-hoc `debug_log!` (several) | `log_missing_return_path` + `AUTH_MISSING_FOLDER_COMMIT` (E:502) |
| Persist on save | `persist_settings_auth_form` (S:634) | `persist_form` (E:807) |
| Clear kind | `clear_settings_auth_kind` (S:656) | `clear_layer` (E:840) |

That is ~17 paired functions plus two dispatch handlers plus two modal enums plus two stash APIs plus two render/footer/mouse arms — all for one concept.

## Target architecture

End state: **one shared auth-modal subsystem** that both surfaces drive. The whole lifecycle (open form, dispatch keys, open child pickers, apply each kind of pick, validate, error, cancel/restore, persist) lives once; each surface contributes only a thin trait impl describing how *its* state stashes/restores modals and opens the infra modals it already owns.

### Shared `AuthModal` enum + `AuthModalHost` trait

1. **Extract a shared `AuthModal` enum** holding the auth-specific modal states: `Form`, `CredentialSourcePicker`, `SourceFolderBrowser` (the `FileBrowserState` + `AuthFormSourceFolder` target), and references to the infra pickers (op-picker, text-input) it opens. Both surfaces embed it:
   - Editor: the existing `Modal::AuthForm` / `Modal::AuthSourcePicker` / `Modal::FileBrowser{AuthFormSourceFolder}` collapse into `Modal::Auth(AuthModal)`.
   - Settings: `SettingsAuthModal` *becomes* (or wraps) `AuthModal`; the bespoke `SourceFolderPicker` is retired in favor of the shared `SourceFolderBrowser`.
2. **`AuthModalHost` trait** implemented by `EditorState` and `SettingsAuthState`, abstracting only what genuinely differs:
   - stash/restore: `stash_auth_modal`, `restore_auth_modal` (unify `push_auth_modal`/`open_sub_modal` and `restore_pending_auth_form`/`pop_modal_chain`),
   - open infra modals the surface owns and shares with non-auth uses: `open_text_input`, `open_op_picker`,
   - surface the standard error dialog: `show_auth_error(reason)` (editor → `Modal::ErrorPopup`; settings → `auth.error`/`settings.error_popup`) — both using `invalid_source_folder_error_popup_state` and the same title.
3. **One dispatch handler** `handle_auth_modal<H: AuthModalHost>(host, key, …)` replacing `handle_settings_auth_modal` and `handle_auth_form_key`.
4. **One of each apply/open helper**, generic over `H: AuthModalHost`, replacing all ~17 twins: `open_auth_form`, `open_credential_source_picker`, `open_source_folder_browser`, `apply_plain_credential`, `apply_op_picker_committed` (+ `_with_validator`), `apply_op_picker_failed`, `apply_source_folder` (validate + apply or error), `restore_auth_form`.
5. **One missing-stash logger** (`log_missing_return_path` + the existing constants) for both.

Infra modals that the editor shares with non-auth flows — `TextInput` (mount dst, env keys) and `OpPicker` (env values) — stay as the surface's own modal variants and are reached through the `AuthModalHost` openers; they are **not** folded into `AuthModal`. The shared code asks the host to open them and receives the committed value back through the existing return-path stash.

### Source-folder slice (first increment, mostly already done)

1. **Shared decision fn** (already have the validator; formalize the decision):
   - `enum AuthSourceFolderOutcome { Apply(PathBuf), Reject(String) }`
   - `fn decide_auth_source_folder(kind: Option<AuthKind>, committed: PathBuf) -> AuthSourceFolderOutcome` in `crates/jackin/src/console/domain.rs`, wrapping `validate_auth_source_folder`.
2. **Shared apply via a small trait** to collapse the apply twins. The two differ only in modal enum + which `modal_parents` they pop:
   - `trait AuthFormModalHost { fn pop_stashed_auth_form(&mut self) -> Option<StashedAuthForm>; fn remount_auth_form_with_source(&mut self, form: StashedAuthForm, source: PathBuf); fn show_source_folder_error(&mut self, reason: String); }`
   - `StashedAuthForm` carries the shared `AuthForm` state + `target` + `literal_buffer` (all already identical between surfaces).
   - Implement for `SettingsAuthState` and `EditorState`.
   - One free fn `commit_auth_source_folder(host: &mut impl AuthFormModalHost, kind, committed)` that calls `decide_*` then either `remount_auth_form_with_source` or `show_source_folder_error`. Both surfaces call this.
3. Delete `apply_source_folder_to_settings_auth_form`, `apply_source_folder_to_auth_form`, `validate_picked_source_folder` — replaced by the trait impls + shared fn.
4. Standardize the missing-stash logging on the shared `log_missing_return_path` + `AUTH_MISSING_FOLDER_COMMIT` (drop the ad-hoc `debug_log!`).

5. **Option B (full collapse).** Give `SettingsAuthModal` the generic `FileBrowser { target, state }` shape mirroring the editor, retire `SourceFolderPicker`, unify the open constructor. Touches render (`settings.rs:233`), footer (`footer/modal.rs:130`), mouse (`mouse.rs:473`), tests.
6. Unify the open step: one `open_source_folder_browser(host, &form)` gating on `shows_source_folder()`, stashing the form, mounting the browser — replacing the inline settings arm (`global_mounts/auth.rs:203-214`) and `open_auth_source_folder_browser_from_form` (`auth.rs:401-447`). This also closes a current divergence: settings does **not** gate on `shows_source_folder()` today.
7. Unify the wrong-folder UX: one standard dialog, one title (`invalid_source_folder_error_popup_state`), same dismiss-returns-to-picker behavior on both. Editor stacks `ErrorPopup` + pops back; settings currently promotes `auth.error` with the generic settings title — converge on the dedicated title and identical dismiss target.

### Remaining increments (the rest of the subsystem)

Each collapses one more twin family into the shared `AuthModalHost` path:

8. **Plain credential** — collapse `apply_plain_text_to_*_auth_form` (S:457 / E:549) and the 1Password source pick (`apply_plain_source_picker_to_auth_form`, E:516) into one shared apply.
9. **Op-picker family** — collapse the whole set: `apply_op_picker_*_committed`, `_with_runner`, `_with_validator`, `_commit_failed`, plus `open_op_picker_from_auth_source` / settings inline. The `_with_validator` bodies are already identical bar the host.
10. **Dispatch handler** — replace `handle_settings_auth_modal` (S:137) and `handle_auth_form_key` (E:253) with one `handle_auth_modal<H>`.
11. **Open form + restore + persist + clear** — collapse `open_*_auth_form`, `restore_*`, `persist_*`, `clear_*_kind` pairs.
12. **Retire the second modal enum** — once all auth variants live in shared `AuthModal`, fold the editor's auth variants into `Modal::Auth(AuthModal)` and make `SettingsAuthModal` an alias/newtype of `AuthModal`, leaving only infra modals surface-owned.

## Change-site checklist (every file that moves)

Track 1:
- `crates/jackin/src/console/domain.rs` — add `decide_auth_source_folder` + `AuthSourceFolderOutcome` + trait.
- `crates/jackin/src/console/tui/input/global_mounts/auth.rs` — replace arm `309-340`, delete apply twin `491-515`, delete `validate_picked_source_folder`; impl trait for `SettingsAuthState`.
- `crates/jackin/src/console/effects.rs` — replace commit logic `270-314` to call shared fn.
- `crates/jackin/src/console/tui/input/auth.rs` — delete apply twin `570-594`; impl trait for `EditorState`.
- `crates/jackin/src/console/tui/input/editor.rs` — `apply_file_browser_to_editor` arm `1188-1190` calls shared fn; keep ErrorPopup return-guard `775-784`.

Track 2 (Option B adds these):
- `crates/jackin-console/src/tui/screens/settings/model.rs` — modal variant `523-550`; stash API `605-616`.
- `crates/jackin/src/console/tui/components/settings.rs:233` — render.
- `crates/jackin/src/console/tui/components/footer/modal.rs:130` — footer hint.
- `crates/jackin/src/console/tui/input/mouse.rs:473` — mouse scroll.
- `crates/jackin/src/console/tui/view/frame.rs:369` — test-injection helper.

Tests to update:
- `crates/jackin/src/console/tui/input/global_mounts/tests.rs` (~`781`, `820`)
- `crates/jackin/src/console/tui/input/mouse/mouse_drag_tests.rs:1615,1628`
- `crates/jackin/src/console/tui/input/auth/tests.rs:377`
- new: a shared-path test proving editor + settings produce identical accept/reject for the same folder + agent.

## Feasibility (what is proven, what is constrained)

Per the correctness-first rule, the only valid stopping reason is a *proven* impossibility. None found. Evidence:

- **The hard parts are already shared.** `AuthForm`, `AuthFormTarget<K>`, `FileBrowserState`, the key-plan, rendering, validation, and the error dialog are single implementations. The remaining duplication is wrapper/dispatch/stash glue — mechanically unifiable.
- **The twins are line-for-line equivalent** except for the modal enum type and which `modal_parents` they pop. The code comments even say so (the editor op-picker apply "mirrors" the settings one). A trait over `{stash, restore, open_text_input, open_op_picker, show_error}` captures the entire difference.
- **`SettingsAuthState` is already generic** over its modal type, so embedding a shared `AuthModal` requires no new generic machinery there.

Real constraints (capability tradeoffs, not cost):

- **Infra modals are shared with non-auth flows.** Editor `TextInput` serves mount-dst and env-key entry; `OpPicker` serves env values. They cannot be folded into `AuthModal`; they stay surface-owned and are reached via host openers. This is a genuine design boundary, not a shortcut.
- **Two render/footer/mouse dispatchers exist** because the two surfaces render their modal stacks separately. Unifying the *modal data* (`AuthModal`) is independent of unifying *rendering*; rendering can route the shared `AuthModal` through one render fn, but that is its own increment and must be proven not to regress modal sizing/footer hints.
- **No impossibility identified.** If one surfaces during execution (e.g. a borrow/lifetime conflict in the host trait), record it here with the exact error before choosing any symptom-layer fallback.

## Parity checklist (must be identical on both surfaces, verified by test)

- Open gates on `shows_source_folder()` / `shows_credential_block()`.
- File browser opens with hidden files visible (`from_home_with_hidden`).
- Footer hints text identical.
- Esc/cancel returns to the same parent (auth form).
- Validation accept/reject identical for the same (agent, folder).
- Error dialog: same widget, same title, same dismiss target (returns to picker).
- Apply sets `set_source_folder` + refocuses Save + preserves `literal_buffer`.
- Missing-stash path logs via the shared logger (no silent drop).

## Decisions (locked)

- **Track 2: Option B** — full collapse of the settings modal variant.
- **Cadence: per-step commits** — commit + push after each sequencing step, suite-green.

## Open decisions (raise before the relevant step)

- Where the shared code lives: a new `crates/jackin/src/console/auth_modal.rs` module vs extending `domain.rs` (domain is "pure product rules"; modal lifecycle may not belong there).
- Whether `AuthModal` lives in `jackin-console` (alongside the shared `AuthForm`) or in the `jackin` app crate. Leaning `jackin-console` so both screens reference one type.
- Whether to retire `SettingsAuthModal` entirely (alias to `AuthModal`) or keep it as a newtype for screen-local additions.

## Sequencing (incremental, test-green + clippy-clean + per-step commit)

Source-folder slice (delivers the bug fix + proves the shared path):
1. Shared `decide_auth_source_folder` + `AuthModalHost` (source-folder subset) + `commit_auth_source_folder`; wire **editor**. Full `jackin` suite.
2. Wire **settings** to the shared fn; delete settings source-folder apply twin + `validate_picked_source_folder`. Suite.
3. Settings open parity: gate on `shows_source_folder()`. Suite.
4. Option B: collapse settings modal variant + open constructor; render/footer/mouse/tests. Suite.
5. Unify error UX (one dialog, one title, same dismiss). Cross-surface equivalence test. Update `auth-source-folder-sync.mdx` INV-7.

Rest of the subsystem (each its own commit):
6. Plain credential + 1Password source pick.
7. Op-picker family (committed/runner/validator/failed/open).
8. One dispatch handler `handle_auth_modal<H>`.
9. Open form + restore + persist + clear.
10. Retire the second auth modal enum; final cross-surface equivalence tests for every lifecycle step.

## Risks

- Two distinct modal enums + three `modal_parents` stash APIs — the trait must not leak one surface's modal type into the other. Keep `StashedAuthForm` enum-agnostic (only the shared `AuthForm` + `target` + `literal_buffer`).
- Editor `ErrorPopup` dismiss is shared with other flows; the return-to-picker guard is already scoped to the `AuthFormSourceFolder` parent — preserve that exact guard.
- Settings `auth.error` promotion runs in `after_settings_event` (`global_mounts.rs:1290`); keep that path intact when unifying the error UX.
- macOS symlink canonicalization: the committed path is symlink-resolved; tests must compare against the canonical form (already fixed in the kimi test).

## Definition of done

Source-folder slice (steps 1–5):
- One `commit_auth_source_folder` reused by both surfaces; no `apply_source_folder_*` twins; no per-surface validation call.
- Identical accept/reject + identical error dialog (same title, same dismiss target) on both surfaces, proven by a shared test.

Whole subsystem (steps 6–10):
- Every twin in the inventory table is gone; one generic `AuthModalHost`-driven implementation remains. `handle_settings_auth_modal` and `handle_auth_form_key` collapsed into `handle_auth_modal<H>`.
- A single shared `AuthModal` type; the editor's auth variants and `SettingsAuthModal` both reference it; only infra modals (`TextInput`, `OpPicker`) remain surface-owned.
- Cross-surface equivalence tests for every lifecycle step (open, each apply, validate, error, cancel, persist).

Both:
- Full `jackin` + `jackin-console` + `jackin-runtime` suites green; clippy clean per step.
- `auth-source-folder-sync.mdx` updated; this scratch file deleted when the subsystem is unified.

## What this does NOT change (guardrails)

- No behavior change visible to the operator beyond making the two surfaces identical and the error a proper dialog. The auth *semantics* (modes, provisioning, validation rules) are untouched.
- Infra modals keep their non-auth uses intact.
- The runtime `validate_sync_source_dir` (the actual correctness gate) is already shared and is not modified by this refactor.
