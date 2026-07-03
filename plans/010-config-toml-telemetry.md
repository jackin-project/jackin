# Plan 010: `[telemetry]` section in config.toml — persistent endpoint/level/categories with env-wins precedence

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-config/src crates/jackin/src/app.rs crates/jackin-diagnostics/src/observability.rs PRERELEASE.md`
> Plan 008 must be landed (this plan persists its knobs). STOP if
> `TelemetryLevel`/`resolve_telemetry_config` do not exist.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/008-telemetry-level-and-categories.md
- **Category**: dx
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

Telemetry is configurable only by environment variables (`OTEL_EXPORTER_OTLP_ENDPOINT` + per-signal vars, `JACKIN_TELEMETRY_*` after plan 008, `JACKIN_DIAGNOSTICS_FILE`). An operator who runs a standing local backend must export env in every shell and CI job; there is no persisted "my collector lives here" setting. `AppConfig` (`crates/jackin-config/src/app_config.rs:25-60`, verified — fields: version, per-agent auth, env, roles, docker, runtime, git, workspaces, dirty_exit_policy) has no telemetry surface at all.

**PRERELEASE.md compliance is the hard part**: `config.toml` is one of the three versioned schemas — any PR touching `AppConfig` must ship the five artifacts under one version bump: bump `CURRENT_CONFIG_VERSION`, a migration step, a new fixture dir, re-baked fixtures, and a Timeline entry in `docs/content/docs/reference/runtime/schema-versions.mdx` (PRERELEASE.md:13-19). This plan schedules them explicitly.

## Current state

- `AppConfig` — `crates/jackin-config/src/app_config.rs:25-60` (verified excerpt in recon; re-read before editing).
- Version machinery: `crate::versions::current_config_version` referenced in the `version` serde default (`app_config.rs:27-29`). Find migration infrastructure: `rg -n "CURRENT_CONFIG_VERSION|fn migrate" crates/jackin-config/src` and the fixtures dir `rg --files crates/jackin-config | rg fixture` — read one existing migration + its fixture pair as the exemplar BEFORE writing the new one (the previous schema bump's commit shows the full 5-artifact shape: `git log --oneline -- crates/jackin-config | head` and inspect one).
- Env resolution today: endpoint vars read in `observability.rs:576-594`; level/categories resolved by plan 008's `resolve_telemetry_config`.
- Dependency direction: config crate must not depend on diagnostics (`crates/jackin-config/src/lib.rs:11-14` records the deliberate inversion) — the telemetry section is plain data (strings), interpreted by the binary.
- Docs: config schema page `docs/content/docs/reference/runtime/configuration.mdx` (cross-ref table in PROJECT_STRUCTURE.md maps jackin-config changes to it); schema-versions page `docs/content/docs/reference/runtime/schema-versions.mdx`.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt/clippy/check | `cargo fmt --check` ; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` ; `cargo check --all-targets --all-features` | exit 0 |
| Tests | `cargo nextest run --all-features` ; `cargo nextest run -p jackin-config` | pass |

## Scope

**In scope**: `crates/jackin-config/src/` (AppConfig + versions/migration + fixtures), `crates/jackin/src/app.rs` (merge config into telemetry resolution before `RunDiagnostics::start`), `crates/jackin-diagnostics/src/observability.rs` (accept endpoint override parameter instead of reading env directly — see Step 3 decision), tests, `configuration.mdx`, `schema-versions.mdx`, CHANGELOG per PRERELEASE rules.

**Out of scope**: per-workspace telemetry overrides (defer — workspace file is also versioned; one schema bump per PR rule makes this a separate plan if wanted); protocol selection (gRPC-only stands); `JACKIN_DIAGNOSTICS_FILE` as a config key (keep env-only: it is a per-run debugging switch, not standing config).

## Git workflow

- Propose branch `feat/config-telemetry-section`; operator confirm; `git commit -s`; push each. PR body must enumerate the five schema artifacts (PULL_REQUESTS.md gate).

## Steps

### Step 1: Schema

Add to `AppConfig`:

```rust
#[serde(default, skip_serializing_if = "TelemetryConfigSection::is_default")]
pub telemetry: TelemetryConfigSection,
```

New type in `crates/jackin-config/src/` (own file, self-named module):

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TelemetryConfigSection {
    /// OTLP gRPC base endpoint, e.g. "http://127.0.0.1:4317".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otlp_endpoint: Option<String>,
    /// "info" | "debug" | "trace" — validated by the binary, stored as string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
}
```

Strings, not diagnostics types (dependency inversion). `is_default` helper mirrors `RuntimeConfig::is_default` (see `app_config.rs:52` usage — copy that pattern).

**Verify**: `cargo nextest run -p jackin-config` → existing round-trip tests pass with the new field.

### Step 2: The five versioned-schema artifacts

Follow the exemplar migration commit found in recon (read it first):

1. Bump `CURRENT_CONFIG_VERSION`.
2. Migration step from previous version → new (additive field ⇒ the migration body is typically a no-op version-stamp; match the exemplar's shape exactly).
3. New fixture dir for the new version.
4. Re-bake existing fixtures per the exemplar's process (there may be an xtask/test that regenerates — grep `rg -rn "fixture" crates/jackin-config/src` for the harness).
5. Timeline entry in `schema-versions.mdx`.

**Verify**: `cargo nextest run -p jackin-config` → migration tests pass, incl. the new step (old-version fixture loads and upgrades).

### Step 3: Merge precedence in the binary

In `app.rs`, where config is loaded and before `RunDiagnostics::start` (and before plan 008's `resolve_telemetry_config` result is stored): effective values =

1. explicit env (`OTEL_EXPORTER_OTLP_ENDPOINT…`, `JACKIN_TELEMETRY_LEVEL/_CATEGORIES`) and CLI flags win;
2. else config.toml `[telemetry]` values.

Endpoint plumbing decision: `observability.rs` reads env directly (`otlp::endpoints()`, `:576-583`). Do NOT rewrite that module to take parameters throughout; instead, in `app.rs`, when config provides `otlp_endpoint` and `OTEL_EXPORTER_OTLP_ENDPOINT` is unset in the process env, the binary may not mutate its own env (`unsafe_code = "forbid"` blocks `set_var` anyway — verified convention). Therefore: add ONE narrow seam in `observability.rs`: `pub fn set_endpoint_fallback(endpoint: Option<String>)` storing into a `OnceLock<Option<String>>` that `resolve_endpoint`/`base_endpoint` consult AFTER the env (env wins; fallback fills). Level/categories: pass config values into plan 008's `resolve_telemetry_config(cli, env, config_fallback)` — extend its signature (it is < 5 call sites, all in app.rs/tests).

Capsule propagation needs no change: the host resolves the effective endpoint and injects `OTEL_EXPORTER_OTLP_ENDPOINT` into the container already (`launch_runtime.rs` OTLP block) — confirm `container_otlp()` consults the same fallback path (it calls `otlp::base_endpoint`/`endpoints` — it does once Step 3's seam is inside those fns).

**Verify**: unit tests on the fallback ordering (pure: `resolve_endpoint(env_value, fallback)`); `cargo nextest run --all-features` green.

### Step 4: Docs + changelog

`configuration.mdx`: `[telemetry]` section with the three keys, precedence note ("environment and CLI always win"), gRPC-only reminder + E016 link. `schema-versions.mdx`: timeline row. CHANGELOG per PRERELEASE changelog-hold rules (check PRERELEASE.md whether unreleased entries accumulate — follow it).

**Verify**: `cargo xtask docs repo-links` exit 0; fmt/clippy/nextest green.

## Test plan

- Config crate: serde round-trip with `[telemetry]` populated/absent; migration old→new fixture; `is_default` skip behavior (absent section serializes to nothing).
- Binary: precedence matrix (env set + config set → env; env unset + config set → config; both unset → none) as pure-fn tests.
- Seam (plan 001): with fallback endpoint set and env clear, `init_tracing` installs export (can be tested at the `endpoints()` pure level — `resolve_endpoints` with fallback param).

## Done criteria

- [ ] `[telemetry]` round-trips; absent section = absent output (tests)
- [ ] Five schema artifacts present in the diff (version bump, migration, new fixtures, re-baked fixtures, schema-versions.mdx row) — enumerate in PR body
- [ ] Precedence tests green (env > config)
- [ ] `configuration.mdx` documents the section
- [ ] fmt/clippy/nextest green; `plans/README.md` updated

## STOP conditions

- The migration exemplar shows a different artifact set than PRERELEASE.md describes — reconcile with the operator before inventing a shape.
- The `OnceLock` fallback seam can't reach all endpoint consumers (some path still env-only — grep `OTEL_EXPORTER_OTLP_ENDPOINT` in crates/ and verify every read goes through `otlp::` helpers; if the capsule-side `jackin-usage/telemetry.rs:21-22` reads count, note they run in-container where the HOST injected the env — that is fine and out of scope).
- Config load ordering in `app.rs` happens after `RunDiagnostics::start` on some path (config not available when the subscriber initializes) — report the ordering instead of moving init.

## Maintenance notes

- Per-workspace telemetry overrides: deliberate follow-up (separate versioned-schema PR for the workspace file).
- Reviewer: verify no `std::env::set_var` snuck in (forbidden by `unsafe_code = "forbid"` in edition 2024 anyway).
- Plan 013 (env-vars docs page) should mention config precedence once this lands — coordinate if both are open.
