# 1Password Reference Storage — UUIDs as Identity, Names as Display

**Status:** Proposed
**Date:** 2026-04-27
**Scope:** `jackin` crate only
**Related:** Builds on `2026-04-23-workspace-env-resolver-design.md` (shipped) and `2026-04-23-toml-edit-migration-design.md` (shipped)

## Problem

Today, jackin stores 1Password references as human-readable URI strings:

```toml
[env]
CLAUDE_CODE_OAUTH_TOKEN = "op://Private/Claude/security/auth token"
```

When the user has multiple items sharing a name within a vault, this fails at launch:

```text
1Password reference "op://Private/Claude/security/auth token" failed:
[ERROR] could not get item Private/Claude: More than one item matches "Claude".
Try again and specify the item by its ID:
  * for the item "Claude" in vault Private: o6nady73qs3mju64jcq4ztjbdy
  * for the item "Claude" in vault Private: hyoveft67ghsr2wgou6ettilim
  * for the item "Claude" in vault Private: xlhmzzy7ctcououatbzwchbppm
```

The bug surfaces at the worst time — `jackin --debug` is mid-flight, the agent identity has resolved, the user is past the Context7 prompt, and now resolution fails with three opaque UUIDs and no path to recovery in the current command. The 1Password CLI itself produces the error; jackin only propagates it (`operator_env.rs:328`).

The asymmetry is that jackin's picker (`op_picker/mod.rs`) already drills down by **UUID** at every level (account, vault, item, field). Identity is unambiguous *at selection time* — but on commit, the picker discards the IDs and writes the human-readable URI verbatim from `op item get` JSON to the workspace TOML. The disambiguation is lost in serialization.

A second, smaller concern: the breadcrumb in the workspace editor (`render/editor.rs:711-729`) renders cramped because `format!("{key:label_width$}")` pads the key to exactly the longest-key width, leaving zero trailing space when there's only one row.

## Goals

1. **Make resolution unambiguous by construction.** Store the canonical UUID-form `op://` URI as the source of truth. Item-name collisions in 1Password no longer cause launch failures.
2. **Preserve human-readable display.** Render a snapshot breadcrumb (`Vault / Item / Section → Field`) in the editor so the user can verify which secret is referenced without seeing UUIDs.
3. **Disambiguate ambiguous picks visually.** When multiple items share a name in the same vault, embed the picked item's subtitle (typically a username) inline in the breadcrumb so the user sees *which* item was selected.
4. **Accept all official 1Password URI forms as input.** Names, IDs, mixed, special aliases (`password`, `username`, `notes`, `notesPlain`), section-bearing, query-bearing (`?attribute=otp`, `?attr=type`, `?ssh-format=openssh`).
5. **Keep the CLI ergonomic.** `jackin workspace env set X "op://Private/GitHub/password"` resolves automatically when `op` is available; ambiguity surfaces with actionable suggestions at set-time, not container-launch-time.
6. **Fix the editor cramping.** Always at least two-space gap between the key column and the value column.

## Non-Goals

- **Account-level scoping at resolve time** (`op read --account`). UUID URIs are unambiguous in practice across accounts; if a multi-account user reports cross-account confusion, add a single optional `account` field to `OpRef`. Non-breaking.
- **Background refresh of stale `path` snapshots.** Snapshot semantics — the breadcrumb captures what was true at pick time. If the user renames a 1Password item later, the stored path drifts; re-picking is the explicit refresh action. No periodic `op item get` polling.
- **Migration command for old workspaces.** Lossy upgrade only — bare `op://Vault/Item/Field` strings deserialize as plain literals, lose their `[op]` marker in the TUI, and the user re-picks. No `jackin workspace migrate` subcommand needed.
- **`${VAR}` substitution inside `op://` URIs.** 1Password's URI grammar allows `op://${APP_ENV}/...`; jackin treats anything containing `${` in an `op://` URI as a plain literal (never resolves). Pinning to UUIDs is incompatible with config-time variable expansion.
- **Headless `jackin workspace env pick <VAR>` command.** Out of scope; the picker stays TUI-only. CLI auto-resolution covers most cases without a dedicated subcommand.
- **TUI text-entry auto-resolution.** Typing `op://...` into a value cell is always a literal string — explicit picker invocation is the only path to creating an `OpRef` from the TUI.
- **Custom error wrapper for resolve-time `op read` failures from `OpRef` values.** UUID URIs cannot trigger the "more than one item matches" error, so no friendly wrap is needed at runtime. The existing variable-name error wrap continues to apply for unrelated failures (signed out, item deleted, field deleted, network).

## Background

Two prior specs in this directory are foundations and have shipped:

- **`2026-04-23-workspace-env-resolver-design.md`** introduced the operator-env layered model (global / per-agent / per-workspace / per-workspace×agent), defined the `op://` / `$VAR` / literal dispatch, and added `OpRunner` with `op read <reference>` subprocess invocation. All env values are stored as `BTreeMap<String, String>` today; dispatch sniffs the string for the `op://` prefix.
- **`2026-04-23-toml-edit-migration-design.md`** introduced `ConfigEditor` (`src/config/editor.rs`) — a `toml_edit::DocumentMut`-backed writer that preserves comments, blank lines, and key ordering on round-trip. Pre-built env mutators (`set_env_var`, `set_env_comment`, `remove_env_var`) are available for surgical edits.

The picker (`src/console/widgets/op_picker/`) already collects UUIDs from `op vault list --format json`, `op item list --format json`, and `op item get --format json`. Subtitles (used to disambiguate items sharing a title in the picker pane) come from each item's JSON `additional_information` field — populated for login items (typically the username/email), empty for secure notes. See `OpItem` at `operator_env.rs:393-397`.

The breadcrumb renderer at `render/editor.rs:711-729` already produces 3- and 4-segment breadcrumbs (`vault / item / section → field`) by parsing the stored `op://` string with `parse_op_reference`. The cramping bug lives at line 705's `format!("{key:label_width$}")`.

## Design

### Schema

Replace the env value type from `String` to a small untagged enum. Backward-compatible with existing TOML strings; serde picks the variant by structural shape (inline table vs scalar string).

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum EnvValue {
    /// Pinned 1Password reference: UUIDs in `op`, snapshot names in `path`.
    OpRef(OpRef),
    /// Literal value, $VAR / ${VAR}, or any string (incl. legacy bare `op://...`
    /// that downgrades to a literal — see Migration).
    Plain(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OpRef {
    /// Canonical `op://` URI. UUID-form preferred; mixed/named forms tolerated
    /// when hand-edited but not produced by the picker or CLI resolver.
    /// Format: `op://<vault>/<item>/[<section>/]<field>[?attribute=<name>]`
    pub op: String,

    /// Snapshot breadcrumb captured at pick / resolve time:
    ///   `<Vault>/<Item>[<subtitle>?]/[<Section>/]<Field>[?attribute=<name>]`
    /// The `[subtitle]` segment appears only when the item shares its name
    /// with another item in the same vault at write time.
    pub path: String,
}
```

The four `BTreeMap<String, String>` fields in `AppConfig.env`, `AgentSource.env`, `WorkspaceConfig.env`, and `WorkspaceAgentOverride.env` become `BTreeMap<String, EnvValue>`. The type swap is the largest single change; serde and the existing call sites mostly carry through without redesign.

### On-disk examples

```toml
[env]
# Unique item in vault — clean breadcrumb
STRIPE_KEY = { op = "op://abc.../def.../fld...", path = "Private/Stripe/api key" }

# Three "Claude" items — subtitle embedded to disambiguate which one was picked
CLAUDE_CODE_OAUTH_TOKEN = { op = "op://abc.../def.../fld...", path = "Private/Claude[alexey@zhokhov.com]/security/auth token" }

# OTP attribute preserved through canonicalization
GITHUB_OTP = { op = "op://abc.../def.../fld...?attribute=otp", path = "Private/GitHub/one-time password?attribute=otp" }

# Plain literal — unchanged
DB_URL = "postgres://..."

# Bare op:// — deserializes as Plain literal, no resolution attempted
LEGACY_TOKEN = "op://Work/db/password"
```

### 1Password URI grammar (per [official 1Password docs](https://developer.1password.com/docs/cli/secret-reference-syntax))

```
op://<vault>/<item>[/<section>]/<field>[?<attribute_query>]

vault    := <vault_name> | <vault_uuid>
item     := <item_name>  | <item_uuid> | <item_name>'['<subtitle>']'
section  := <section_name> | <section_id>
field    := <field_label> | <field_id> | <special_alias>
special_alias := "username" | "password" | "notes" | "notesPlain"
attribute_query := ("attribute"|"attr") "=" <attr_name>
                 | "ssh-format=openssh"
attr_name := "type" | "value" | "id" | "purpose" | "otp"
           | <file_attr>   (* "type"|"content"|"size"|"id"|"name" *)
```

Names support alphanumeric, `-`, `_`, `.`, and whitespace (case-insensitive). Names with whitespace must be quoted at the shell level; names with other special characters (e.g. `/`) require ID-form. The `<item_name>'['<subtitle>']'` form is jackin's display extension — accepted as input for disambiguation, written by the picker and CLI when needed.

### Picker behavior (`src/console/widgets/op_picker/mod.rs`)

The commit logic at `op_picker/mod.rs:703-722` changes shape. Today it writes the verbatim `field.reference` from `op item get`. After this change:

```rust
fn build_op_ref_on_commit(state: &OpPickerState) -> OpRef {
    let vault = state.selected_vault();   // OpVault { id, name }
    let item  = state.selected_item();    // OpItem  { id, name, subtitle }
    let field = state.selected_field();   // OpField { id, label, reference, ... }

    // Section info (id + name) is parsed out of `field.reference` when present —
    // 1Password's `op item get` puts the human-readable reference there with
    // section already correctly attributed; we just rewrite vault/item/field
    // segments to UUID form and preserve the section segment.
    // canonical_uuid_uri produces: op://<vault.id>/<item.id>/[<section.id>/]<field.id>[?query]
    let op = canonical_uuid_uri(vault, item, field);

    // Ambiguity check: is the picked item's name shared with any other item
    // currently listed in the vault? The full item list is in memory.
    let item_name_collides = state.items_in_vault()
        .iter()
        .filter(|i| i.id != item.id && i.name == item.name)
        .next()
        .is_some();

    // Defensive: if the item name itself contains brackets, suppress the
    // subtitle embed to keep `path` parsable. UUID still resolves correctly.
    let safe_to_embed = !item.name.contains('[') && !item.name.contains(']');
    let item_segment = if item_name_collides && safe_to_embed && !item.subtitle.is_empty() {
        format!("{}[{}]", item.name, item.subtitle)
    } else {
        item.name.clone()
    };

    // canonical_display_path produces: <vault.name>/<item_segment>/[<section.name>/]<field.label>[?query]
    // Section name comes from the same parse of `field.reference` as section.id above.
    let path = canonical_display_path(vault, item_segment, field);

    OpRef { op, path }
}
```

No additional `op` calls are required at commit — the picker has full vault/item/field metadata in memory from its prior list calls.

### CLI behavior (`src/cli/config.rs`, `src/cli/workspace.rs`)

`jackin workspace env set <VAR> <VALUE>` and `jackin config env set <VAR> <VALUE>`:

1. **If `<VALUE>` does not start with `op://`:** store as `EnvValue::Plain(value)`. Done. (No special-casing for `$VAR` syntax — that's still handled at resolve time, not at set time.)
2. **If `<VALUE>` starts with `op://`:**
   - Probe `op` CLI availability. If unavailable, error: *"`op` CLI not available; cannot resolve `op://...` reference. Install 1Password CLI, or hand-edit the TOML if you have UUIDs."* No silent fallback to `Plain`.
   - If `<VALUE>` contains `${`, error: *"jackin does not support shell variable substitution inside `op://` URIs. Use a plain string or substitute before passing."*
   - Parse the URI loosely: split on `/` after `op://`, peel off any `?attribute=…`/`?attr=…`/`?ssh-format=…` query suffix.
   - Detect the `[subtitle]` form on the item segment and split into name + subtitle filter.
   - Resolve via `op` calls (same routines used by the picker):
     - `op vault list --format json` → match user's vault by name or ID
     - `op item list --vault <vault> --format json` → match user's item by name (case-insensitive) or ID. If a `[subtitle]` filter is provided on the item segment, narrow further to items whose `additional_information` matches the filter exactly (case-insensitive)
     - **0 matches:** error *"item `<X>` not found in vault `<Y>`."*
     - **2+ matches** (no subtitle filter or subtitle didn't narrow): error with a copy-pasteable disambiguation list:
       ```text
       3 items named "Claude" in vault "Private". Disambiguate with:
         op://Private/Claude[alexey@zhokhov.com]/security/auth token
         op://Private/Claude[alexey@chainargos.com]/security/auth token
         op://Private/Claude[team@example.com]/security/auth token
       ```
       (3 lines for 3 matches; uses each item's subtitle. Items with empty subtitles fall back to short item-id prefixes: `Claude[#o6nady73]`.)
     - **1 match:** continue.
   - `op item get <item_id> --format json` → find the field by label / ID / special alias (`password`, `username`, `notes`, `notesPlain`); locate the section if 4-segment.
   - Compute `item_name_collides` from the vault item list (same rule as picker).
   - Build the canonical `OpRef`: `op` is UUID-form with optional query suffix preserved verbatim; `path` is human-readable form with `[subtitle]` only when ambiguous.
   - Persist via `ConfigEditor` (or the workspace editor equivalent). The TOML inline-table form is what gets written.

### TUI text-entry behavior (`src/console/manager/input/editor.rs`)

Typing or pasting any text into a value cell — including `op://...` URIs — produces an `EnvValue::Plain` on commit. No probing, no resolution, no `op` calls. The picker keystroke (existing `[op]` keybinding) is the *only* TUI path to creating an `OpRef`.

This is a deliberate split: the editor is a string editor; the picker is a 1Password-reference editor. Pasting `op://Private/Claude/auth` and pressing Enter stores the literal string `"op://Private/Claude/auth"`. The user explicitly asks for a 1Password reference by invoking the picker.

### Runtime resolver (`src/operator_env.rs`)

The current dispatch sniffs strings for `op://` prefix (`operator_env.rs:31-36`). Replace with structural dispatch on `EnvValue`:

```rust
fn resolve_env_value(
    value: &EnvValue,
    op_runner: &dyn OpCli,
    layer_label: &str,
    var_name: &str,
) -> anyhow::Result<String> {
    match value {
        EnvValue::Plain(s) => operator_env::expand_shell_var(s),
        EnvValue::OpRef(r) => op_runner.read(&r.op).map_err(|e| {
            anyhow::anyhow!(
                "{layer_label} env var {var_name:?}: 1Password reference {ref_path:?} failed: {e}",
                ref_path = r.path
            )
        }),
    }
}
```

Notable simplifications:

- `is_op_reference()` is removed from non-display call sites. The discriminator is the enum variant.
- `parse_op_reference()` is retained for the CLI input path (parses user-typed URIs into segments) and for legacy display fallback (hand-edited `op://Vault/Item/Field` strings in the `op` field of an `OpRef`).
- The "More than one item matches" error path becomes effectively dead code for `OpRef` values (UUIDs cannot be ambiguous). The existing handlers for non-ambiguity errors (signed out, item deleted, field deleted, `op` not installed, timeout, network) continue to apply unchanged.
- Bare `op://` strings in `EnvValue::Plain` flow through `expand_shell_var` unchanged — that function leaves anything without `$` alone, so they pass through to the container as literal strings.

### TUI breadcrumb rendering (`src/console/manager/render/editor.rs`)

Two changes to `render_secrets_key_line` (`render/editor.rs:674`).

**(1) Cramming fix.** Replace the `format!("{key:label_width$}")` invocation at line 705 with explicit two-space minimum padding:

```rust
let key_padded = format!("{key:label_width$}");
spans.push(Span::styled(key_padded, label_style));
spans.push(Span::raw("  "));   // always at least two spaces between key and value
```

`label_width` already represents the longest-key width (computed by the caller); the explicit two spaces guarantee separation regardless of how key/value column widths land.

**(2) Breadcrumb sources from `path`, not raw `op` URI.** A new parser, `parse_path_breadcrumb`, replaces the `parse_op_reference(value)` call for `OpRef` rows:

```rust
struct PathBreadcrumb {
    vault: String,
    item: String,
    item_subtitle: Option<String>,
    section: Option<String>,
    field: String,
    attribute_query: Option<String>,    // e.g., "?attribute=otp"
}

fn parse_path_breadcrumb(path: &str) -> Option<PathBreadcrumb> {
    let (path, attr) = match path.find('?') {
        Some(i) => (&path[..i], Some(path[i..].to_string())),
        None    => (path, None),
    };
    let segs: Vec<&str> = path.split('/').collect();
    let (item, item_subtitle) = match segs.get(1) {
        Some(seg) => split_bracket_subtitle(seg),
        None      => return None,
    };
    let parts = match segs.as_slice() {
        [vault, _, field]          => Some((vault, None,            *field)),
        [vault, _, section, field] => Some((vault, Some(*section),  *field)),
        _ => None,
    }?;
    Some(PathBreadcrumb {
        vault: parts.0.to_string(),
        item, item_subtitle,
        section: parts.1.map(str::to_string),
        field: parts.2.to_string(),
        attribute_query: attr,
    })
}

fn split_bracket_subtitle(s: &str) -> (String, Option<String>) {
    if let Some(open) = s.rfind('[') {
        if s.ends_with(']') && open < s.len() - 1 {
            return (s[..open].to_string(), Some(s[open+1..s.len()-1].to_string()));
        }
    }
    (s.to_string(), None)
}
```

Spans for the breadcrumb (replacing `editor.rs:711-728`):

```rust
spans.push(Span::styled(parts.vault, white));
spans.push(Span::styled(" / ", dim));
spans.push(Span::styled(parts.item, green));
if let Some(subtitle) = parts.item_subtitle {
    spans.push(Span::raw(" "));
    spans.push(Span::styled(subtitle, dim));   // PHOSPHOR_DIM, per Q5b.2
}
if let Some(section) = parts.section {
    spans.push(Span::styled(" / ", dim));
    spans.push(Span::styled(section, green));
}
spans.push(Span::styled(" \u{2192} ", dim));
spans.push(Span::styled(parts.field, green_bold));
if let Some(query) = parts.attribute_query {
    spans.push(Span::raw(" "));
    spans.push(Span::styled(query, dim));
}
```

Resulting renders for the screenshot bug:

```text
[op] CLAUDE_CODE_OAUTH_TOKEN  Private / Claude alexey@zhokhov.com / security → auth token
[op] STRIPE_KEY               Private / Stripe → api key
[op] GITHUB_OTP               Private / GitHub → one-time password ?attribute=otp
```

(The `[op]` marker, mask handling, and value-side rendering for `Plain` values are unchanged.)

## Migration

**Lossy upgrade — no migration command, no error on legacy.**

When jackin loads a workspace TOML containing `MY_VAR = "op://Vault/Item/Field"` (legacy bare-string form):

1. The string deserializes as `EnvValue::Plain("op://Vault/Item/Field")`.
2. The runtime resolver passes it through `expand_shell_var` (which is a no-op for strings without `$`), and the container receives `MY_VAR=op://Vault/Item/Field` as a literal env value.
3. The TUI editor renders the row without an `[op]` marker, showing the verbatim string in the value column.
4. The user notices in the editor that the row is no longer marked `[op]` and re-picks via the picker keystroke (or runs `jackin workspace env set MY_VAR "op://Vault/Item/Field"` to re-resolve via the CLI auto-resolution path).

This is intentional: no error, no warning, no automatic rewrite. The workspace TOML is not modified by jackin without an explicit user action. Visual cue (`[op]` marker absence) is the migration signal.

**No CHANGELOG-blocking compatibility shim** — the existing `is_op_reference()` runtime path is removed, so legacy bare strings *change behavior* (from "resolve as 1Password" to "pass through as literal"). For workspaces actively using bare-string `op://` references, this is a breaking change at upgrade — but recoverable in seconds via the picker.

## Test plan

### Schema round-trip
- `OpRef` with 3-segment, 4-segment, with subtitle, with `?attribute=otp`, with `?attr=value`, with `?ssh-format=openssh`.
- `Plain` with literal, `$VAR`, `${VAR}`, bare `op://...`.
- Untagged enum picks correct variant for inline-table vs scalar string input.

### Path breadcrumb parser
- 3-segment / 4-segment with and without subtitle.
- Subtitle containing `/`: handled by `rfind('[')`.
- Subtitle containing nested `[`: tolerated by the renderer.
- Item names containing `[`: write-time defensive rule prevents new writes; old reads tolerate.
- Query-suffix variants: `?attribute=otp`, `?attr=value`, `?ssh-format=openssh`.

### Picker commit (`op_picker/mod.rs` test module)
- Vault with 1 item named "X" → no subtitle in `path`.
- Vault with 2+ items named "X" → subtitle embedded.
- Item with bracket characters in its name → no subtitle embed (defensive).
- Login item gets subtitle (username); secure note gets empty subtitle (no embed).
- Section-bearing fields produce 4-segment URIs in both `op` and `path`.
- 4-segment commit with section UUID in `op`, section name in `path`.

### CLI resolution (`tests/cli_env.rs`)
- Name-form input → resolves to UUID-form `op`, name-form `path`.
- UUID-form input → preserved verbatim in `op`, name-form `path` reconstructed via `op item get`.
- Mixed-form input → resolved to canonical UUID-form.
- Bracketed input (`Item[subtitle]`) → filters items by subtitle; `path` retains brackets when item is ambiguous.
- Ambiguous name-form, no subtitle → errors with disambiguation suggestions list.
- Ambiguous name-form, subtitle still doesn't narrow → errors with item-ID-prefix fallback list.
- 4-segment with section name; with section UUID.
- `?attribute=otp` preserved through resolution.
- `?attr=value`, `?ssh-format=openssh` preserved.
- Special aliases (`password`, `username`, `notes`, `notesPlain`) resolve to canonical field IDs.
- `op` unavailable + `op://` input → errors loudly with install hint.
- `${VAR}` inside `op://` input → errors with substitution-not-supported message.
- Plain literal input (no `op://` prefix) → stored as `Plain`, no `op` calls, no errors.

### TUI text-entry
- Pastes `op://...` into value cell + Enter → stored as `Plain`, not `OpRef`.
- Picker keystroke → opens picker; on commit produces `OpRef`.
- Hitting `[op]` keybinding on a `Plain` row → starts the picker fresh.

### Renderer
- Cramming fix verified: longest-key + 2 always.
- Subtitle rendered in `PHOSPHOR_DIM` between item and the next `/`.
- 4-segment with subtitle: `vault / item subtitle / section → field`.
- `?attribute=otp` rendered dimmed at end of breadcrumb.
- `OpRef` row → breadcrumb. `Plain` row (literal or bare `op://`) → masked or verbatim value, no breadcrumb, no `[op]` marker.

### Runtime resolver
- `OpRef` → `op read <op>` invoked with the canonical URI; result returned.
- `Plain` (literal) → returned verbatim.
- `Plain` containing `$VAR` / `${VAR}` → expanded.
- `Plain` containing bare `op://...` → returned verbatim (NOT resolved).
- `op read` failures (not signed in, item deleted, field deleted, network) → propagated with var-name wrap; no extra ambiguity-handling layer.

## Files touched (estimated scope)

| File | Change |
|---|---|
| `src/workspace/mod.rs` | Type swap (`String` → `EnvValue`) for `WorkspaceConfig.env`, `WorkspaceAgentOverride.env`. |
| `src/config/mod.rs` | Type swap for `AppConfig.env`, `AgentSource.env`. |
| `src/operator_env.rs` | New `EnvValue`/`OpRef` types; `resolve_env_value` enum dispatch; remove `is_op_reference` from non-display sites. |
| `src/console/widgets/op_picker/mod.rs` | Commit path: write `OpRef` with UUID `op` + ambiguity-aware `path` instead of bare `field.reference` string. |
| `src/console/manager/render/editor.rs` | New `parse_path_breadcrumb`; subtitle, attribute-query, cramming-fix changes to `render_secrets_key_line`. |
| `src/cli/config.rs`, `src/cli/workspace.rs` | `env set` auto-resolves `op://`; ambiguity errors with disambiguation list; `op`-unavailable error. |
| `src/config/editor.rs` | `set_env_var` accepts `EnvValue` so it writes inline tables for `OpRef` and bare strings for `Plain`. |
| Tests | New + extensions across `operator_env.rs`, `op_picker/mod.rs`, `op_picker/render.rs`, `render/editor.rs`, `cli_env.rs`. |

## References

- 1Password secret-reference syntax: <https://developer.1password.com/docs/cli/secret-reference-syntax>
- Prior shipped specs in this directory:
  - `2026-04-23-workspace-env-resolver-design.md`
  - `2026-04-23-toml-edit-migration-design.md`
- Picker data structures: `src/operator_env.rs:377-411` (`OpAccount`, `OpVault`, `OpItem`, `OpField`).
- Current breadcrumb renderer: `src/console/manager/render/editor.rs:674` (`render_secrets_key_line`).
- Current picker commit logic: `src/console/widgets/op_picker/mod.rs:703-722`.
- Current runtime dispatch: `src/operator_env.rs:31-36`.
