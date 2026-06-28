# Pre-release policy

jackin❯ no released version — proof-of-concept. Rules here consequence of that status, **all retire when jackin❯ ships first tagged release.**

## Breaking changes are expected and acceptable

Schema change (on-disk state layout, CLI flags, role/agent shapes outside three versioned files below): no migration code, no compat shims, no fallback parsers for old field names, no "tolerant ignore + warn" handlers, no deprecation warnings. New shape only; let stale data fail with standard parser error.

No memorializing old shapes in code comments ("formerly named X", "old location was Y") or docs outside changelog. Git history = record; code describes only current shape.

## Versioned schemas — the three exceptions

`config.toml`, per-workspace files at `~/.config/jackin/workspaces/<name>.toml`, and `jackin.role.toml` are versioned schemas (`CURRENT_CONFIG_VERSION`, `CURRENT_WORKSPACE_VERSION`, `CURRENT_MANIFEST_VERSION` in `src/config/migrations.rs` and `src/manifest/migrations.rs`). Any PR touching `AppConfig`, `WorkspaceConfig`, `RoleManifest`, `HooksConfig`, or any type whose serde representation lives in those three files must ship five artifacts:

1. Bump relevant `CURRENT_*_VERSION`.
2. Migration step in corresponding registry (`CONFIG_MIGRATIONS`, `WORKSPACE_MIGRATIONS`, `MANIFEST_MIGRATIONS`).
3. New fixture dir under `tests/fixtures/migrations/<file-kind>/from-<predecessor-version>/` with `meta.toml`, `before.toml`, `after.toml`. Fixture harness in `tests/migration_fixtures.rs` walks every supported `from_version` every CI run, asserts migrated output (a) parses against current serde schema, (b) carries declared `target_version` stamp, (c) `after.toml` parses and carries same stamp. Guarantees delayed operator landing on current version after several bumps still loads config — chain = regression guard.
4. Re-bake every existing fixture's `after.toml` so it walks new step too. Fixture for oldest supported `from_version` = load-bearing test for users delayed months — its diff proves new chain composable.
5. New entry at top of **Timeline** section in `docs/content/docs/reference/runtime/schema-versions.mdx` with date, predecessor, fixture link, summary, before/after example.

Non-additive change (renamed field, removed field, type change, added enum variant, restructured table) without these five artifacts = incomplete; reviewers block merge until they appear or change reshaped additive (new optional field with serde default). Operator config and per-workspace files migrate auto during `AppConfig::load_or_init` at startup; role authors migrate local manifests on desktop with `jackin role migrate <role-repo-path>`, CI and Renovate-style automation migrate manifests with small standalone `jackin-role migrate <role-repo-path>` binary.

## One schema version bump per PR, targeting the next version after `main`

PR touching versioned schemas must introduce exactly one version bump — version immediately following current `CURRENT_*_VERSION` on `main` when PR opens. Single PR may add multiple fields, rename multiple fields, affect multiple file kinds (config, workspace, manifest), but all land under that one bump. Second bump in same PR signals changes should be separate PRs, not stacked versions. If `main` advances while PR in flight and claims PR's target version, rebase to new next version — never introduce gap or skip. Prevents pattern where PR introduces `v1alpha5` (partial) and `v1alpha6` (remainder): forces operators through two sequential migrations for one PR's work, creates stale intermediate version no one ships at.

## Changelog stays empty until the first release

**No entries in `CHANGELOG.md` until first tagged release.** Changelog communicates breaking changes and new features to *users of released software*. Before first release no such users, every change implicitly "unreleased" — entries now create noise to clean before release, falsely imply stable release cadence.

When first release cut, operator explicitly asks to populate changelog. Until then, leave `CHANGELOG.md` unchanged.