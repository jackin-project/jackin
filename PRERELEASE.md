# Pre-release policy

jackin' has no released version — it is a proof-of-concept. Rules in this file are consequences of that status and **all retire when jackin' ships its first tagged release.**

## Breaking changes are expected and acceptable

When schemas change (on-disk state layout, CLI flags, role/agent shapes outside the three versioned files below), do not write migration code, compatibility shims, fallback parsers for old field names, "tolerant ignore + warn" handlers, or deprecation warnings. Make the new shape the only shape; let stale data fail with the standard parser error.

Do not memorialize old shapes in code comments ("formerly named X", "old location was Y") or in docs outside the changelog. Git history is the record; code describes only the current shape.

## Versioned schemas — the three exceptions

`config.toml`, per-workspace files at `~/.config/jackin/workspaces/<name>.toml`, and `jackin.role.toml` are versioned schemas (`CURRENT_CONFIG_VERSION`, `CURRENT_WORKSPACE_VERSION`, `CURRENT_MANIFEST_VERSION` in `src/config/migrations.rs` and `src/manifest/migrations.rs`). Any PR touching `AppConfig`, `WorkspaceConfig`, `RoleManifest`, `HooksConfig`, or any other type whose serde representation lives in one of those three files must ship five artifacts:

1. Bump of the relevant `CURRENT_*_VERSION`.
2. A migration step in the corresponding registry (`CONFIG_MIGRATIONS`, `WORKSPACE_MIGRATIONS`, `MANIFEST_MIGRATIONS`).
3. A new fixture directory under `tests/fixtures/migrations/<file-kind>/from-<predecessor-version>/` containing `meta.toml`, `before.toml`, `after.toml`. The fixture harness in `tests/migration_fixtures.rs` walks every supported `from_version` on every CI run and asserts migrated output (a) parses against current serde schema, (b) carries declared `target_version` stamp, (c) `after.toml` itself parses and carries the same stamp. Guarantees a delayed operator landing on the current version after several bumps can still load config — the chain is the regression guard.
4. Re-bake of every existing fixture's `after.toml` so it walks the new step too. Fixture for the oldest supported `from_version` is the load-bearing test for users delayed months — its diff proves the new chain is composable.
5. A new entry at top of the **Timeline** section in `docs/content/docs/reference/runtime/schema-versions.mdx` with date, predecessor, fixture link, summary, before/after example.

A non-additive change (renamed field, removed field, type change, added enum variant, restructured table) without these five artifacts is incomplete; reviewers block merge until they appear or the change is reshaped additive (new optional field with serde default). Operator config and per-workspace files migrate automatically during `AppConfig::load_or_init` at startup; role authors migrate local manifests on a desktop with `jackin role migrate <role-repo-path>`, while CI and Renovate-style automation migrate manifests with the small standalone `jackin-role migrate <role-repo-path>` binary.

## One schema version bump per PR, targeting the next version after `main`

A PR touching versioned schemas must introduce exactly one version bump — the version immediately following the current `CURRENT_*_VERSION` on `main` when the PR opens. A single PR may add multiple fields, rename multiple fields, affect multiple file kinds (config, workspace, manifest), but all land under that one bump. A second bump in the same PR signals the changes should be separate PRs, not stacked versions. If `main` advances while the PR is in flight and claims the PR's target version, rebase to the new next version — never introduce a gap or skip. Prevents the pattern where a PR introduces `v1alpha5` (partial) and `v1alpha6` (remainder): forces operators through two sequential migrations for one PR's work and creates a stale intermediate version no one ships at.

## Changelog stays empty until the first release

**Do not add entries to `CHANGELOG.md` until the first tagged release.** The changelog communicates breaking changes and new features to *users of released software*. Before a first release there are no such users, every change is implicitly "unreleased" — entries now create noise to clean up before release and falsely imply a stable release cadence.

When the first release is cut, the operator will explicitly ask for the changelog to be populated. Until then, leave `CHANGELOG.md` unchanged.
