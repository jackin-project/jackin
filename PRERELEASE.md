# Pre-release policy

jackin' has no released version — it is a proof-of-concept. The rules in this file are consequences of that status and **all retire when jackin' ships its first tagged release.**

## Breaking changes are expected and acceptable

When schemas change (on-disk state layout, CLI flags, role/agent shapes outside the three versioned files listed below), do not write migration code, compatibility shims, fallback parsers for old field names, "tolerant ignore + warn" handlers, or deprecation warnings. Make the new shape the only shape; let stale data fail with the standard parser error.

Do not memorialize old shapes in code comments ("formerly named X", "old location was Y") or in documentation files outside the changelog. The git history is the record of what changed; the code should describe only the current shape.

## Versioned schemas — the three exceptions

`config.toml`, per-workspace files at `~/.config/jackin/workspaces/<name>.toml`, and `jackin.role.toml` are versioned schemas (`CURRENT_CONFIG_VERSION`, `CURRENT_WORKSPACE_VERSION`, `CURRENT_MANIFEST_VERSION` in `src/config/migrations.rs` and `src/manifest/migrations.rs`). Any PR that touches `AppConfig`, `WorkspaceConfig`, `RoleManifest`, `HooksConfig`, or any other type whose serde representation lives in one of those three files must ship with five artifacts:

1. Bump of the relevant `CURRENT_*_VERSION`.
2. A migration step in the corresponding registry (`CONFIG_MIGRATIONS`, `WORKSPACE_MIGRATIONS`, `MANIFEST_MIGRATIONS`).
3. A new fixture directory under `tests/fixtures/migrations/<file-kind>/from-<predecessor-version>/` containing `meta.toml`, `before.toml`, and `after.toml`. The fixture harness in `tests/migration_fixtures.rs` walks every supported `from_version` on every CI run and asserts that the migrated output (a) parses successfully against the current serde schema, (b) carries the declared `target_version` stamp, and (c) that `after.toml` itself parses and carries the same stamp. This guarantees a delayed operator landing on the current version after several bumps can still load their config — the chain is the regression guard.
4. Re-bake of every existing fixture's `after.toml` so it walks through the new step too. The fixture for the oldest supported `from_version` is the load-bearing test for users delayed by months — its diff is the proof the new chain is composable.
5. A new entry at the top of the **Timeline** section in `docs/content/docs/reference/schema-versions.mdx` with date, predecessor, fixture link, summary, and a before/after example.

A non-additive change (renamed field, removed field, type change, added enum variant, restructured table) without these five artifacts is incomplete; reviewers block merge until they appear or the change is reshaped to be additive (new optional field with a serde default). Operator config and per-workspace files migrate automatically during `AppConfig::load_or_init` at startup; role authors migrate local manifests on a desktop with `jackin role migrate <role-repo-path>`, while CI and Renovate-style automation migrate manifests with the small standalone `jackin-role migrate <role-repo-path>` binary.

## One schema version bump per PR, targeting the next version after `main`

A PR that touches versioned schemas must introduce exactly one version bump — the version immediately following the current `CURRENT_*_VERSION` on `main` at the time the PR is opened. A single PR may add multiple fields, rename multiple fields, and affect multiple file kinds (config, workspace, manifest), but all of those changes land under that one version bump. Adding a second bump inside the same PR is a sign the changes should be in separate PRs, not stacked versions. If `main` advances while the PR is in flight and claims the PR's target version, the PR must rebase to use the new next version — never introduce a gap or a skip. This rule prevents the pattern where a PR introduces `v1alpha5` (with partial changes) and `v1alpha6` (with the remainder): that forces operators through two sequential migrations for what is logically one PR's worth of work and creates a stale intermediate version that no one ever ships at.

## Changelog stays empty until the first release

**Do not add entries to `CHANGELOG.md` until the first tagged release.** The changelog exists to communicate breaking changes and new features to *users of released software*. Before a first release there are no such users, and every change is implicitly "unreleased" — adding entries now creates noise that will need to be cleaned up before the release and may give a false impression that the project follows a stable release cadence.

When the first release is being cut, the operator will explicitly ask for the changelog to be populated. Until then, leave `CHANGELOG.md` unchanged.
