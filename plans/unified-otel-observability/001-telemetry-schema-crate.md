# Plan 001: Create the `jackin-telemetry` schema crate with a Weaver-validated registry and generated constants

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-xtask/src/arch.rs Cargo.toml mise.toml ratchet.toml crates/jackin-diagnostics/src/observability.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: LOW (purely additive — no existing behavior changes)
- **Depends on**: none
- **Category**: migration
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements the "Semantic convention policy" and schema half of "Rust instrumentation architecture"; the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The roadmap item `docs/content/docs/roadmap/unified-otel-observability.mdx` requires that OpenTelemetry semantic conventions 1.43.0 become the naming authority for all jackin❯ telemetry, that one schema module pins standard fields and the closed extension registry, and that generated Rust constants prevent string drift. Today attribute keys are hand-written string constants scattered in `crates/jackin-diagnostics/src/observability.rs` (`otel_keys`, lines 26–69), including `jackin.*` and `parallax.run.id` keys that the roadmap removes. This plan creates the foundation every later plan builds on: a new lowest-tier `jackin-telemetry` crate owning the schema. No call sites migrate in this plan.

## Current state

- `crates/jackin-diagnostics/src/observability.rs:26-69` — module `otel_keys` holds today's hand-written keys. Excerpt (verified at planning commit):

  ```rust
  pub mod otel_keys {
      pub const SERVICE_NAME: &str = "service.name";
      pub const SERVICE_VERSION: &str = "service.version";
      pub const SESSION_ID: &str = "session.id";
      pub const RUN_ID: &str = "parallax.run.id";
      pub const COMPONENT: &str = "jackin.component";
      pub const SCREEN_NAME: &str = "jackin.screen.name";
      // … more jackin.* keys …
  }
  ```

  These stay untouched in this plan (later plans migrate and delete them).
- `crates/jackin-xtask/src/arch.rs:36-65` — the hand-maintained `TIERS` table. Every workspace member MUST have an entry or `cargo xtask lint arch` fails (completeness check at `arch.rs:231-237`). Current tier 0 crates: `jackin-core`, `jackin-dev`, `jackin-process`, `jackin-term`.
- Root `Cargo.toml` `[workspace.dependencies]` has `opentelemetry_sdk = "0.32"` (line ~90), `tracing = "0.1"`, `tracing-subscriber = "0.3"`, `tokio = "=1.52.3"`. The `opentelemetry`, `opentelemetry-otlp`, `opentelemetry-appender-tracing`, `tracing-opentelemetry` pins currently live per-crate in `crates/jackin-diagnostics/Cargo.toml:60-92`. **`opentelemetry-semantic-conventions` is not a dependency anywhere yet** (verified absent from `Cargo.lock`).
- `ratchet.toml` `public-surface` family (~lines 695-760) has one row per workspace crate bounding root `pub mod` count; a new crate needs a row. `rust-function-complexity` defaults to cap 20 for unlisted crates.
- OTel Weaver is not used anywhere in the repo today (no binary, config, or codegen; the only mention is the roadmap page).
- Repo conventions that apply (from `crates/AGENTS.md`, hard rules):
  - Rust 2024 self-named module files, **no `mod.rs`**.
  - All tests for a module in a single sibling `tests.rs` (`foo.rs` + `foo/tests.rs`), declared as `#[cfg(test)] mod tests;` — no inline test modules.
  - Every crate dir carries `README.md`, `AGENTS.md`, and `CLAUDE.md` (symlink to `AGENTS.md`): `ln -s AGENTS.md CLAUDE.md`.
  - `[lints] workspace = true` in every crate manifest; no per-crate `edition`/`license` copies (use `edition.workspace = true` etc. — copy the header shape from `crates/jackin-diagnostics/Cargo.toml:4-15`).
  - Brand: prose says `jackin❯`; code identifiers use bare `jackin`.

## The schema this crate must encode

The closed registry comes verbatim from the roadmap item (`docs/content/docs/roadmap/unified-otel-observability.mdx`, sections "Semantic convention policy" and "Attribute contract for actual jackin❯ cases"). It has two halves:

**(a) Standard fields (import, do not redefine semantics):** `service.name`, `service.namespace`, `service.version`, `service.instance.id`, `process.pid`, `process.executable.name`, `process.exit.code`, `process.command`, `container.id`, `os.*` and `process.runtime.*` (Resource, where applicable — plan 002 attaches them), `code.*` source-ownership fields (`code.function.name`, `code.file.path`, `code.line.number` — available for explicit use where a case needs source ownership; automatic capture stays disabled, and `tracing` target/module metadata remains the default ownership signal per plans 003/004), `session.id`, `session.previous_id`, `app.screen.id`, `app.screen.name`, `app.widget.id`, `app.widget.name`, `app.jank.*`, `app.crash*`, `gen_ai.agent.name`, `gen_ai.conversation.id`, `gen_ai.provider.name`, `error.type`, `exception.*`, `rpc.system.name`, `rpc.method`, `http.request.method`, `url.template`, `server.address`, `db.system.name`, `db.operation.name`, network/OS/runtime fields as used. Prefer constants from the `opentelemetry-semantic-conventions` crate where they exist; hand-pin (with a comment naming the semconv 1.43.0 page) only those not yet in the crate (the Session/App/GenAI groups are development-status and may be missing from the 0.32 crate — pin those as strings in one module).

**(b) Neutral extensions (closed set — define exactly these, nothing more):**

| Key | Type / allowed values |
|---|---|
| `app.mode` | enum: `one_shot`, `interactive`, `daemon`, `capsule` |
| `cli.invocation.id` | opaque UUID string |
| `cli.command.name` | registry enum: `load`, `hardline`, `eject`, `exile`, `purge`, `prewarm`, `prune`, `console`, `role`, `workspace`, `config`, `daemon`, `doctor`, `diagnostics`, `status`, `usage`, `help`, plus dotted subcommand paths (e.g. `role.validate`, `daemon.status`, `workspace.env.set`) |
| `ui.action.name` | registry enum (seeded in plan 009) |
| `ui.screen.visit.id` | opaque UUID string |
| `ui.navigation.sequence` | int (monotonic per session) |
| `ui.transition.reason` | enum: `action`, `launch`, `attach`, `detach`, `back`, `cancel`, `completion`, `failure`, `shutdown` |
| `job.id` | opaque string |
| `job.type` | enum: `image_prewarm`, `sidecar_prewarm` |
| `outcome` | enum: `success`, `failure`, `error`, `timeout`, `skip`, `cancellation` |
| `launch.stage.name` | enum: `identity`, `role`, `credentials`, `construct`, `agent_binaries`, `derived_image`, `workspace`, `network`, `sidecar`, `capsule`, `hardline` (must match `jackin_core::LaunchStage::ALL`, 11 variants — `crates/jackin-core/src/launch_progress.rs:16-39`) |
| `launch.target.kind` | enum: `workspace`, `directory` |
| `background.cycle.name` | enum: `branch_context`, `pr_context`, `usage_account`, `provider_probe`, `instance_refresh`, `agent_status` |
| `connection.peer.type` | enum: `host_daemon`, `capsule_control`, `capsule_attach`, `docker`, `provider`, `parallax` |
| `agent.state` | enum: `working`, `blocked`, `done`, `idle`, `unknown` |
| `agent.status.source` | enum: `none`, `visible_screen`, `shell_integration`, `foreground_process`, `reported` |
| `agent.status.confidence` | enum: `unknown`, `weak`, `strong`, `authoritative` |
| `agent.status.stuck` | bool |
| `auth.mode` | enum: `sync`, `api_key`, `oauth_token`, `ignore` |
| `credential.source.type` | enum: `environment`, `agent_home`, `onepassword`, `github_cli`, `oauth_store`, `none` |
| `workspace.isolation.mode` | enum: `shared`, `worktree`, `clone` |
| `network.mode` | enum: `none`, `allowlist`, `open` |
| `dind.mode` | enum: `none`, `rootless`, `privileged` |
| `config.scope` | enum: `global`, `workspace` |
| `config.operation` | enum: `load`, `validate`, `migrate`, `save` |
| `config.schema.version.from` / `.to` | enum: `legacy`, `v1alpha1`…`v1alpha9` (global) / `v1alpha1`…`v1alpha8` (workspace) — match `crates/jackin-config/src/versions.rs:11-15` |
| `config.migration.step_count` | int |
| `trust.decision` | enum: `granted`, `revoked`, `rejected` |
| `trust.source.type` | enum: `builtin`, `external` |
| `cache.name` | enum: `role_repository`, `agent_binary`, `capsule_binary`, `derived_image`, `usage_snapshot` |
| `cache.result` | enum: `hit`, `miss`, `stale`, `reuse`, `bypass` |
| `pty.exit.reason` | enum: `clean`, `signal`, `nonzero_exit`, `wait_failed`, `cancelled` |
| `stream.direction` | enum: `input`, `output` |
| `telemetry.signal` | enum: `log`, `trace`, `metric` |
| `telemetry.rejection.reason` | enum: `unknown_name`, `unknown_attribute`, `invalid_value`, `privacy`, `cardinality`, `size_limit` |

**Bounded well-known values also needed as registries:** `gen_ai.agent.name` ∈ {`claude`, `codex`, `amp`, `kimi`, `opencode`, `grok`} (must match `jackin_core::Agent`, `crates/jackin-core/src/agent.rs:23-36`); `gen_ai.provider.name` ∈ {`anthropic`, `openai`, `amp`, `xai`, `zai`, `minimax`, `kimi`}; `app.screen.id` ∈ {`workspace.list`, `workspace.editor`, `settings`, `workspace.create`, `launch.progress`, `capsule`}; `error.type` stable names for E001–E016: `docker_daemon_unreachable`, `docker_version_too_old`, `out_of_disk_space`, `role_manifest_invalid`, `role_manifest_version_unsupported`, `role_source_not_trusted`, `workspace_not_found`, `workspace_config_version_unsupported`, `container_name_conflict`, `dind_health_check_failed`, `dind_port_conflict`, `gh_auth_failed`, `op_not_signed_in`, `capsule_download_failed`, `worktree_conflict`, `unsupported_otlp_protocol` (order matches `ErrorCode` in `crates/jackin/src/error.rs:16-33`), plus transport values `timeout`, `connection_refused`, `panic`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Toolchain | `mise install` (repo root) | exit 0 |
| Build new crate | `cargo check -p jackin-telemetry --all-targets --locked` | exit 0 |
| Tests | `cargo nextest run -p jackin-telemetry --locked` | all pass |
| Arch gate | `cargo xtask lint --strict` | exit 0 |
| Clippy | `cargo clippy -p jackin-telemetry --all-targets -- -D warnings` | exit 0 |
| Fmt | `cargo fmt --check` | exit 0 |
| Full lint lane | `cargo xtask ci --only lint` | exit 0 |

## Scope

**In scope** (the only files you should create/modify):
- `crates/jackin-telemetry/**` (new crate)
- `Cargo.toml` (workspace members + `[workspace.dependencies]` additions)
- `Cargo.lock` (regenerated by cargo)
- `crates/jackin-xtask/src/arch.rs` (one `TIERS` row)
- `crates/jackin-xtask/src/` (new `telemetry_registry` lint lane, wired into `lint`)
- `ratchet.toml` (public-surface row for the new crate)
- `mise.toml` / `mise.lock` (Weaver pin, if step 5's primary path works)
- `.codebook.toml` (spelling allowlist entries if the checker flags new terms, e.g. `otel`, `weaver`)

**Out of scope** (do NOT touch):
- `crates/jackin-diagnostics/**` — existing telemetry keeps working unchanged until plan 002+.
- Any product crate call site.
- Docs under `docs/content/` (plan 015).

## Git workflow

- Branch: `feature/unified-otel-observability` — the single branch for the whole roadmap item. The entire plan set (001–015) ships as ONE pull request from this branch; do not create a per-plan branch or a separate PR. Never commit to `main`.
- Conventional Commits; suggested subject: `feat(telemetry): add jackin-telemetry schema crate with generated registry`.
- Every commit: `git commit -s` (DCO sign-off) and push immediately after committing (repo hard rule).

## Steps

### Step 1: Scaffold the crate

Create `crates/jackin-telemetry/` with:
- `Cargo.toml`: package name `jackin-telemetry`, `version = "0.6.0-dev"`, `publish = false`, workspace-inherited `edition`/`rust-version`/`license`/`repository`, `[lints] workspace = true`. Copy the header shape from `crates/jackin-diagnostics/Cargo.toml:4-18`. Dependencies for THIS plan only: `opentelemetry-semantic-conventions` (add `opentelemetry-semantic-conventions = "0.32"` to root `[workspace.dependencies]`; use `default-features = false, features = ["semconv_experimental"]` — the experimental feature is required for Session/App/GenAI development-status constants if present). Add `uuid = { version = "1", features = ["v4"] }` to workspace deps and this crate (id minting helpers, used by later plans). **No `jackin-*` dependencies** — this is the lowest tier.
- `src/lib.rs` with `//!` header describing: schema authority, closed registry, tier T0, and the rule "no `jackin.*`/`parallax.*` key may ever be defined here".
- `README.md` following the template in `crates/AGENTS.md` (clickable structure table, tier statement "T0 — must match the TIERS table in crates/jackin-xtask/src/arch.rs").
- `AGENTS.md` with the non-derivable rules: "the extension registry is closed — adding a key requires a roadmap-contract change first", "generated files are never hand-edited". Then `ln -s AGENTS.md crates/jackin-telemetry/CLAUDE.md` (relative symlink inside the dir: `cd crates/jackin-telemetry && ln -s AGENTS.md CLAUDE.md`).
- Add `"crates/jackin-telemetry"` to the workspace `members` list in root `Cargo.toml` (it may be a glob — check; if `members` uses `crates/*` no edit is needed).

**Verify**: `cargo check -p jackin-telemetry --locked` → exit 0.

### Step 2: Register the tier

Add `("jackin-telemetry", 0),` to `TIERS` in `crates/jackin-xtask/src/arch.rs` (insert in the tier-0 group after `jackin-core`, keeping the existing grouping style). Add a `ratchet.toml` `public-surface` row for `jackin-telemetry` matching the format of existing rows (find the `jackin-diagnostics` row near lines 717-718 and copy its shape; set the bound to the actual root `pub mod` count you end up with).

**Verify**: `cargo xtask lint --strict` → exit 0.

### Step 3: Author the registry source of truth

Create `crates/jackin-telemetry/registry/` containing OTel Weaver registry YAML:
- `registry_manifest.yaml` — registry name `jackin`, `semconv_version: 1.43.0`, importing the upstream semconv registry as a dependency.
- `attributes.yaml`, `events.yaml`, `metrics.yaml`, `spans.yaml` — groups defining every extension key/enum from the table above (attributes), the event names (seeded now: `session.start`, `session.end`, `ui.screen.entered`, `ui.screen.exited`, `ui.widget.focused`, `ui.widget.unfocused`, `app.jank`, `app.crash` — later plans extend), span names (`cli.command`, `app.startup`, `app.shutdown`, `ui.action`, `ui.screen.transition`, `ui.render`, `background.cycle`, `connection.attempt`, `process.command`), and metric instruments (seeded in plan 004 — leave `metrics.yaml` as an empty group list for now).
- Every enum uses the exact bounded members listed above. Requirement levels: `required`/`recommended` per the roadmap case tables.

**Verify**: files parse as YAML: `python3 -c "import yaml,glob; [yaml.safe_load(open(p)) for p in glob.glob('crates/jackin-telemetry/registry/*.yaml')]"` → exit 0 (or use `bun`/`yq` if python3 unavailable).

### Step 4: Generate and commit the Rust constants

Create `crates/jackin-telemetry/src/schema.rs` (+ submodules `schema/attrs.rs`, `schema/events.rs`, `schema/spans.rs`, `schema/enums.rs` — self-named layout, no `mod.rs`), containing:
- `pub const` string keys for every extension attribute, event name, and span name.
- One Rust enum per bounded value set (e.g. `pub enum OutcomeValue { Success, Failure, Error, Timeout, Skip, Cancellation }` with `as_str()` returning the wire value). Every enum gets `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` and an `ALL` const for exhaustive tests.
- Re-exports of the standard-field constants from `opentelemetry-semantic-conventions` under `pub mod std_attrs` so downstream code imports everything through this crate. Where a needed development-status constant is absent from the 0.32 crate, define it locally in `schema/attrs.rs` with a `// semconv 1.43.0 <group>` comment.
- A generation header comment on generated files: `// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate`.

Primary generation path: OTel Weaver (`weaver registry generate` with a minimal Jinja template checked into `crates/jackin-telemetry/templates/`). If step 5's Weaver install fails, hand-author `schema.rs` to exactly mirror `registry/` and rely on the consistency test in step 6 (record which path you took in the commit message).

Tests in `crates/jackin-telemetry/src/schema/tests.rs`:
- every enum's `as_str()` values are lowercase snake/dot case and unique;
- `launch.stage.name` members match `jackin_core::LaunchStage::ALL` count of 11 — since this crate must not depend on jackin-core, encode the expectation as a literal list test here, and add the cross-crate equality test in plan 008;
- no constant value starts with `jackin.` or `parallax.` (iterate an `ALL_KEYS` slice).

**Verify**: `cargo nextest run -p jackin-telemetry --locked` → all pass.

### Step 5: Pin Weaver and add the validation lane

- Try adding OTel Weaver to `mise.toml` (aqua/ubi backend: `"ubi:open-telemetry/weaver" = "<latest pinned version>"`) and run `mise install`. If the tool cannot be pinned reproducibly, skip the pin, and make the xtask lane below run Weaver only when the binary is on PATH (soft dependency) — the hard gate is then the consistency test from step 6.
- Add an xtask lane: `crates/jackin-xtask/src/telemetry_registry.rs` with command `cargo xtask telemetry-registry` that (a) runs `weaver registry check -r crates/jackin-telemetry/registry` when weaver is available, and (b) always verifies `schema.rs` constants match `registry/` YAML (parse the YAML with `serde_yaml`/`toml`-equivalent already in the xtask dependency tree — check what xtask already depends on and reuse; if no YAML parser is available in-tree, add `serde_yaml` to xtask only). Wire it into the `lint` partition in `crates/jackin-xtask/src/ci.rs` (lint steps live at `ci.rs:150-173`).

**Verify**: `cargo xtask telemetry-registry` → exit 0; `cargo xtask ci --only lint` → exit 0.

### Step 6: Namespace-ban guard (workspace-wide, forward-looking)

Add to the new xtask lane a check that fails if any **new** `jackin.*` or `parallax.*` attribute string literal is introduced outside an allowlist of the legacy files that still carry them (seed the allowlist with: `crates/jackin-diagnostics/src/observability.rs`, `crates/jackin-diagnostics/src/run.rs`, `crates/jackin-diagnostics/src/run/jsonl_adapter.rs`, `crates/jackin-diagnostics/src/screen.rs`, `crates/jackin-diagnostics/src/metrics.rs`, `crates/jackin-diagnostics/src/registry.rs`, `crates/jackin-usage/src/telemetry.rs`, `crates/jackin/src/app.rs`, plus their `tests.rs` siblings). The allowlist is shrink-only (same discipline as `ratchet.toml`); plans 007–013 drain it to empty.

**Verify**: `cargo xtask telemetry-registry` → exit 0. Temporarily add `const X: &str = "jackin.bogus";` to any non-allowlisted file, re-run → non-zero exit naming the file; revert.

## Reopened audit additions (2026-07-16)

- Generate and validate instrument descriptions as well as names, units, types, boundaries, and requirement levels.
- Generate the runtime event/span/instrument definition tables themselves, including required/allowed fields and histogram boundaries; the facade consumes these tables so hand-maintained parallel definitions cannot drift.
- Preserve upstream `app.jank` and `app.crash` event lineage and requirement levels; local registry groups may import/extend them but must not weaken or redefine the standard contract.
- Encode global and workspace config schema-version combinations separately so workspace never admits `v1alpha9` and neither scope admits `legacy` as a migration target.
- Scan every Rust target, including tests, benches, fuzz targets, and xtask. Exempt only exact proven non-telemetry path/symbol/literal combinations; blanket syntax exemptions such as `.get`, `.join`, or `LABEL_*` are forbidden.
- Record and validate the exact locked `opentelemetry-semantic-conventions` version and checksum, and provide checksummed Weaver artifacts for every supported developer/CI platform.
- Vendor or content-checksum the exact semantic-conventions 1.43 Weaver input so generation is offline-reproducible and cannot change behind a mutable network tag.

## Test plan

- `crates/jackin-telemetry/src/schema/tests.rs`: enum-value uniqueness, casing, closed-set sizes (e.g. `OutcomeValue::ALL.len() == 6`, `LaunchStageName::ALL.len() == 11`, `CliCommandName` covers the 17 top-level commands), namespace ban on own constants.
- xtask lane self-test in `crates/jackin-xtask/src/telemetry_registry/tests.rs` following the layout rule (single sibling `tests.rs`).
- Verification: `cargo nextest run -p jackin-telemetry -p jackin-xtask --locked` → all pass.

## Done criteria

- [ ] `cargo check --workspace --all-targets --locked` exits 0
- [ ] `cargo nextest run -p jackin-telemetry -p jackin-xtask --locked` exits 0
- [ ] `cargo xtask lint --strict` exits 0 (tier row present)
- [ ] `cargo xtask telemetry-registry` exits 0 and is listed in `cargo xtask ci --only lint` output
- [ ] `grep -rn "jackin\." crates/jackin-telemetry/src/ | grep -v "jackin_core\|jackin-telemetry\|jackin❯"` returns no attribute-key matches
- [ ] Crate has `README.md`, `AGENTS.md`, `CLAUDE.md` symlink (`ls -la crates/jackin-telemetry/`)
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- The `TIERS` completeness or cycle check fails for reasons other than the missing new-crate row (arch drift since planning).
- `opentelemetry-semantic-conventions = "0.32"` does not resolve or conflicts with the locked `opentelemetry 0.32.0` family.
- Weaver's registry format requires upstream semconv definitions that cannot be vendored/pinned offline AND the fallback consistency check cannot express the same guarantees.
- You find yourself wanting to add an extension key not in the table above — the registry is closed; the roadmap contract must change first.

## Maintenance notes

- Every later plan in this directory imports names from `jackin_telemetry::schema` — key spellings here are load-bearing for all of them; a reviewer should diff the constants against the roadmap tables verbatim.
- When OpenTelemetry stabilizes Session/App/CLI/GenAI conventions, the locally pinned constants must migrate to the semconv crate re-exports; the registry records `stability` per group to make that visible.
- The namespace-ban allowlist (step 6) is the cutover progress meter — it must be empty after plan 013.
