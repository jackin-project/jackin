# Plan 039: Pub-surface narrowing pilot — jackin-env exposes one curated root API

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md` — unless a reviewer dispatched you and told
> you they maintain the index.
>
> **Drift check (run first)**: `git diff --stat 0971da66d..HEAD -- crates/jackin-env/src/ crates/jackin/src/workspace/token_setup.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW-MED (compile-time-verified sweep — `unreachable_pub = deny` + `dead_code = deny` make every mistake a build error; only 3 crates consume jackin-env)
- **Depends on**: none (**ordering note**: plan 021 touches jackin-protocol, not this crate; plan 037 changes some jackin-env signatures — either order works, rebase mechanically)
- **Category**: tech-debt (API surface)
- **Planned at**: commit `0971da66d`, 2026-07-09

## Why this matters

Roadmap Phase 2: "narrow broad `pub mod` surfaces in foundational crates — intentional root re-exports and `pub(crate)` implementation modules so agents copy canonical APIs rather than internals" (items at `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` lines 136, 143). jackin-env is the ideal pilot: it already maintains a curated root `pub use` surface for every module (`lib.rs:29-55`), yet all 10 modules are also `pub mod` — every item is importable by two paths, and internals with **zero external users** (the whole `host_claude` module, 4 resolve fns, 9 token_setup helpers) are indistinguishable from API. The census (2026-07-09) mapped every external use-site; blast radius = 3 consumer crates. After this pilot, `jackin_env::X` root imports are the only public paths, internal-only items become `pub(crate)`, and the workspace's `unreachable_pub = "deny"` turns the discipline into a permanent compiler gate. The pilot's measured cost/benefit then decides the rollout to jackin-core (39 pub mods) per the roadmap's ratchet principle.

## Current state

- `crates/jackin-env/src/lib.rs`: `//!` header (lines 1-12) + module decls (14-27) + curated re-exports (29-55).
  - 10 unconditional `pub mod`: `env_layer, env_resolver, host_claude, op_cli, op_runner, op_struct, parse_helpers, picker, resolve, token_setup`. Line 20 `mod output;` is already private. Lines 26-27: `#[cfg(any(test, feature = "test-support"))] pub mod test_support;` — stays `pub mod` (consumed by module path `jackin_env::test_support::FakeOpWriter` from `crates/jackin/src/app/tests.rs:669`).
  - Header bug to fix in passing: lines 9-12 claim `jackin-launch-tui` reaches these types through jackin-env — **false**; `jackin-launch-tui/Cargo.toml` has no jackin-env dep (verified). Reword to name the real consumers (jackin-console, jackin-runtime, jackin).
- **Consumers** (only 3): `jackin` (normal dep + dev-dep with `features = ["test-support"]`), `jackin-console`, `jackin-runtime`.
- **The one external facade**: `crates/jackin/src/workspace/token_setup.rs` lines 5-12 — a pure `pub use jackin_env::{…all 26 token_setup items…};` pass-through. Any jackin-env item demoted to `pub(crate)` MUST first be dropped from this facade (a `pub use` of a non-pub item is a compile error).
- **External-usage census** (the canonical surface — every item below must remain reachable as `jackin_env::<Item>` at the root):
  - env_resolver: `ResolvedEnv`, `PromptResult`, `EnvPrompter`, `resolve_env`, `resolve_env_with_overrides`
  - op_runner: `OpRunner`, `resolve_env_value` · op_cli: `OpCli` (+ ctors `new`, `new_launch_env`, `new_probe`, `with_binary`, `with_account`)
  - op_struct: `OpStructRunner`, `OpWriteRunner` (+ forced-pub `OpItemCreateParams`)
  - picker: `OpCache`, `OpAccount`, `OpVault`, `OpItem`, `OpField`, `default_op_struct_runner`
  - parse_helpers: `parse_host_ref`
  - resolve: `resolve_op_uri_to_ref`, `resolve_operator_env_with_matching`, `resolve_operator_env_matching`, `has_operator_env_matching`, `collect_on_demand_bindings`, `print_launch_diagnostic`, `lookup_operator_env_raw`, `CLAUDE_OAUTH_TOKEN_ENV`
  - token_setup (externally consumed subset): `TokenSetupScope`, `TokenSetupArgs`, `EditExistingTarget`, `TokenSetupReport`, `DoctorReport`, `RevokeReport` (the last two are forced-pub return types), `DEFAULT_ITEM_TEMPLATE`, `DEFAULT_FIELD_LABEL`, `JACKIN_TAG`, `mint_token_value`, `expiry_days_for_launch`, `run_setup`, `run_revoke`, `run_doctor`, `prior_token_slot`, `vault_for_rotate`, `tags_indicate_jackin_owned`
- **Demotion candidates** (zero external code refs; verified — doc-comment mentions don't count):
  - ALL of `env_layer` (`EnvLayer`, `merge_layers`) and ALL of `host_claude` (6 items: `ClaudeProbe`, `probe_claude_cli`, `probe_with_binary`, `TOKEN_PREFIX`, `capture_setup_token`, `capture_setup_token_with_binary`)
  - `parse_helpers::is_valid_env_name`
  - resolve: `has_operator_env`, `resolve_operator_env`, `resolve_operator_env_with`, `validate_reserved_names`
  - token_setup (9): `DEFAULT_ITEM_CATEGORY`, `WORKSPACE_TAG_PREFIX`, `clear_expiry_stamp`, `days_until_expiry`, `expiry_cache_path`, `write_expiry_stamp`, `run_doctor_with_runner`, `run_revoke_with_runner`, `run_setup_with_runner`
- **The self-verifying mechanism**: root `Cargo.toml:128` `unreachable_pub = "deny"` (inherited via `[lints] workspace = true`) — once a module is non-pub, any `pub` item inside it that is not re-exported from `lib.rs` fails the build; `dead_code = "deny"` catches items that become fully unused after the facade trim. The compiler forces every item into exactly one of: root-re-exported, `pub(crate)`, or deleted.
- **The exemplar to match** — `crates/jackin-instance/src/lib.rs:24-33`: private `mod auth;` + `pub use auth::validate_sync_source_dir;` (implementation sealed, one curated item surfaced).
- **README**: `crates/jackin-env/README.md` "Public API" (lines 33-35) is vague prose; the Structure table (17-31) lists modules. Both must reflect the new surface in the same PR (`crates/AGENTS.md` hard rule).
- **No doctests** reference `jackin_env` anywhere (verified) — zero doc coupling. Docs-site prose references file paths, not import paths (safe), except `token-orchestrator.mdx` pre-existing drift noted in maintenance.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| The migration loop | `cargo check --workspace --all-targets --locked` | exit 0 when done |
| Env + consumer tests | `cargo nextest run -p jackin-env -p jackin -p jackin-console -p jackin-runtime` | all pass |
| Full lint (includes unreachable_pub) | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Feature-gated half | `cargo check -p jackin-env --features test-support` | exit 0 |
| Merge-readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope** (the only files you should modify):
- `crates/jackin-env/src/lib.rs` (module visibility + re-export blocks + header fix)
- `crates/jackin-env/src/{env_layer,env_resolver,host_claude,op_cli,op_runner,op_struct,parse_helpers,picker,resolve,token_setup}.rs` — ONLY visibility keywords (`pub` → `pub(crate)`) on the demotion list; no logic, no renames, no moves
- `crates/jackin/src/workspace/token_setup.rs` (drop facade lines for demoted items)
- Consumer import lines ONLY where the compiler forces a path change (module-path → root-path)
- `crates/jackin-env/README.md`

**Out of scope** (do NOT touch):
- Any function body, signature, or error type (plan 037's territory)
- `test_support.rs` and its `pub mod` gating — stays exactly as is
- `op_cli.rs` beyond its `pub mod` → `mod` flip (plan 031 owns its internals)
- Deleting "dead" items — if `dead_code` fires on a demoted item, that's a **finding**: `#[expect(dead_code, reason = "kept for <why>; recorded in plan 039")]` only if the reason is real, otherwise report it in the PR body and delete ONLY if it's one of the 9 token_setup helpers whose only caller was the facade
- jackin-core's 39 pub mods (the rollout decision comes after this pilot)

## Git workflow

- Branch: current active branch if the operator designates one; otherwise propose `refactor/env-pub-surface` and wait for confirmation.
- Conventional Commits, signed, push after each: `refactor(env): seal implementation modules behind curated root re-exports`.

## Steps

### Step 1: Trim the jackin facade

In `crates/jackin/src/workspace/token_setup.rs`, remove the 9 demotion-list token_setup items from the `pub use jackin_env::{…}` block (keep the externally-consumed subset listed above). If any removed item is referenced elsewhere in `crates/jackin/` (the census says no), the next build says so.

**Verify**: `cargo check -p jackin --all-targets` → exit 0.

### Step 2: Flip module visibility

In `lib.rs`, change the 10 unconditional `pub mod X;` to `mod X;`. Keep `mod output;` and the cfg-gated `pub mod test_support;` untouched. The existing `pub use` blocks (29-55) now define the entire public surface.

**Verify**: `cargo check -p jackin-env --all-targets --features test-support` → **expected to FAIL** with a batch of `unreachable_pub` errors — that's the worklist for Step 3. Record the count.

### Step 3: Compiler-driven demotion sweep

For every `unreachable_pub` error inside jackin-env: if the item is on the census's external-usage list → it's missing from a `pub use` block; add it (should not happen — the blocks are complete; treat as census correction and note it). Otherwise → change `pub` to `pub(crate)`. Work module by module; the demotion list above predicts the full set (env_layer ×2, host_claude ×6, is_valid_env_name, resolve ×4, token_setup ×9). Trim the corresponding entries OUT of `lib.rs`'s `pub use` blocks (a `pub use` of a `pub(crate)` item is an error — e.g. remove `EnvLayer`, `merge_layers`, the six host_claude names, `is_valid_env_name`, `has_operator_env`, `resolve_operator_env`, `resolve_operator_env_with`, `validate_reserved_names`, and the 9 token_setup helpers from lines 29-55).

**Verify**: `cargo check -p jackin-env --all-targets --features test-support` → exit 0. Then `cargo check --workspace --all-targets --locked` → exit 0 (consumers were root-path importers already; any module-path import the census missed fails here — rewrite it to the root path).

### Step 4: Dead-code audit of the demoted set

`dead_code = deny` may now fire on demoted items with no remaining internal callers (candidates: the expiry-stamp helpers if only the facade used them). For each: confirm via grep it has zero callers; delete it AND its tests if it's pure facade residue; otherwise it has an internal caller and nothing fires. Do not `#[expect(dead_code)]` anything without a stated future use.

**Verify**: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0. `cargo nextest run -p jackin-env -p jackin -p jackin-console -p jackin-runtime` → all pass.

### Step 5: lib.rs header + README

Fix the header's stale consumer claim (lines 9-12): the types live here so `jackin-console`, `jackin-runtime`, and the `jackin` binary reach them through one crate. Rewrite README's "Public API" section as the explicit re-export list grouped as in `lib.rs` (types/traits/fns per concern), and mark implementation modules as internal in the Structure table. State the rule: "all public items are root re-exports; module paths are not API."

**Verify**: `cargo doc -p jackin-env --no-deps` → exit 0; rendered root shows the curated surface only.

### Step 6: Full gates + pilot metrics

Record in the PR body (this is the pilot's deliverable for the rollout decision): number of items demoted, number deleted, consumer files touched, wall-clock effort, and any census misses. Full gates.

**Verify**: `cargo fmt && cargo xtask ci --fast` → exit 0.

## Test plan

No new tests — the compiler is the test (`unreachable_pub`/`dead_code` deny + workspace check). Regression net: full nextest on the 4 crates (existing tests import through both path styles today; after the sweep they compile only via root paths, proving the surface). The `test-support` feature check covers the cfg-gated half.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `grep -c "^pub mod" crates/jackin-env/src/lib.rs` → 0 (the cfg-gated test_support line doesn't match `^pub mod` — it's indented under `#[cfg]`; verify it survived: `grep -n "pub mod test_support" lib.rs` → 1)
- [ ] `grep -rn "jackin_env::[a-z_]*::" crates/ --include="*.rs" | grep -v test_support | grep -v "crates/jackin-env/"` → empty (no external module-path imports remain)
- [ ] `grep -n "pub(crate)" crates/jackin-env/src/host_claude.rs | wc -l` ≥ 6
- [ ] `cargo check --workspace --all-targets --locked` and `cargo check -p jackin-env --features test-support` exit 0
- [ ] `cargo nextest run -p jackin-env -p jackin -p jackin-console -p jackin-runtime` exits 0
- [ ] `cargo xtask ci --fast` exits 0
- [ ] README "Public API" enumerates the surface; `plans/code-health/README.md` status row updated with the pilot metrics line

## STOP conditions

Stop and report back (do not improvise) if:

1. `lib.rs` no longer matches the excerpt (module list/re-exports changed — re-derive the census by grepping `jackin_env::` externally before proceeding; if >5 census corrections surface, report instead).
2. A demoted item is generic-bounded or trait-default-referenced such that `pub(crate)` triggers `private_interfaces`/`private_bounds` errors beyond the three known forced-pub items (`OpItemCreateParams`, `DoctorReport`, `RevokeReport`) — report the item; it's an API-design finding, not a visibility flip.
3. Step 3's error count exceeds ~40 — the census undercounted; the sweep is still mechanical but report the delta first.
4. Anything in `docs/` **fails CI** over a `jackin_env::` path (lychee/repo-links) — fix only the failing link; broader token-orchestrator.mdx drift is plan 029/METAdocs territory.

## Maintenance notes

- **The pilot's purpose is the rollout decision**: with metrics from Step 6, the operator decides whether jackin-core (39 pub mods, 312 pub items) gets the same treatment. jackin-manifest/jackin-protocol keep leaky duals today (`pub mod` + `pub use`) — same pattern applies later.
- Plan 010's pub-surface dashboard counts will DROP after this lands — regenerate baselines if 010's scanner landed (its public-item proxy counts `pub mod` lines).
- Pre-existing docs drift (NOT this plan, recorded): `token-orchestrator.mdx:23,39` mis-locates `OpWriteRunner` in `resolve.rs` (it's `op_struct.rs`).
- Reviewer scrutiny: the diff should be almost entirely visibility keywords + `use` lines; any body change is out of scope and suspect.
- Future agents: new jackin-env items are born `pub(crate)` and get promoted by adding a root `pub use` — never a new `pub mod`.
