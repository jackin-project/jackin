# TODO

Two kinds of work live here:

- **[Follow-ups](#follow-ups)** — small items to verify or address periodically. External dependencies waiting on upstream fixes; internal consistency or polish work that's too small for a roadmap doc.
- **[Stale-docs check](#stale-docs-check-every-pr)** — a per-PR checklist for keeping structure-sensitive docs in sync with code.

Bigger feature work and design proposals live in the [docs roadmap](#roadmap) — a separate place, see below.

## Follow-ups

Small, concrete, verifiable items. Each entry is a heading with a stable anchor so code-level `TODO(<topic>)` markers can link back. Walk this list periodically (monthly is a good cadence; on demand otherwise), update **Last verified**, take action when **Done when** is satisfied.

### Code-level TODO marker convention

When code (or config) has a follow-up tracked in this file, leave a marker in the source at the relevant spot:

```text
// TODO(<topic>): one-line summary — see TODO.md "Follow-ups" → "<heading>"
```

`<topic>` is the same kebab-case slug used as the heading anchor below, so a single grep finds both ends:

```sh
grep -rn 'TODO(<topic>)' .
```

Markers without a corresponding TODO.md entry are allowed for transient in-flight work, but anything expected to outlive a single PR should have a tracked entry here so it doesn't rot. When an item resolves, remove both the entry and the matching `TODO(<topic>)` markers in the same PR.

### External dependencies

#### `lychee-action-sha-pin` — swap unreleased master SHA for a tagged release

- **What:** in [`.github/workflows/docs.yml`](.github/workflows/docs.yml), revert the `lycheeverse/lychee-action` SHA pin from `faea714062690f6c2e6f7f388469ec4fa6d9c4e1` (master, post-v2.8.0) to a SHA from a tagged release.
- **Why:** SHA-pinning to a tagged release is more discoverable than pinning to a master commit, surfaces release notes during routine dependency review, and keeps the audit trail aligned with what's published in the marketplace.
- **Tracking:** <https://github.com/lycheeverse/lychee-action/releases> — first tag at or after commit `faea714` (which introduces v0.24.x subfolder-aware install).
- **Last verified:** 2026-04-25 — latest tag is `v2.8.0`; `faea714` is current `master` HEAD; pin introduced in [#176](https://github.com/jackin-project/jackin/pull/176).
- **Done when:** a tag at or after `faea714` ships. Replace the SHA in `docs.yml` with that tag's commit SHA, update the inline comment from "post-v2.8.0 master" to the tag name, and bump `LYCHEE_VERSION` to whatever the new release defaults to (or pin explicitly).

### Internal cleanups

_(none yet)_

## Roadmap

Roadmap items — open work and resolved design docs — live in the docs site, not in this repo. See:

- Overview: [`docs/src/content/docs/reference/roadmap.mdx`](docs/src/content/docs/reference/roadmap.mdx)
- Per-item design docs: [`docs/src/content/docs/reference/roadmap/`](docs/src/content/docs/reference/roadmap/)
- Browsable: <https://jackin.tailrocks.com/reference/roadmap/>

To add a new item, create an MDX page under the directory above and add a sidebar entry in [`docs/astro.config.ts`](docs/astro.config.ts) under `Roadmap → Open items`.

Each design doc should include (see any existing page as a template):

- `**Status**: Open | Deferred | Resolved`
- `## Problem`
- `## Why It Matters`
- `## Related Files`

Roadmap vs. follow-up: if it needs a problem statement and design discussion, it's a roadmap item. If it's "swap a SHA when upstream releases" or "rename three callers for consistency", it's a follow-up.

## Stale-docs check (every PR)

Docs rot silently. Every PR must include a one-pass verification that structure-sensitive docs still match reality. Treat these as a checklist in the PR description — each item takes seconds to check.

### When your PR touches `src/**`

- [ ] Did you add, rename, move, or delete a module / directory under `src/`? If yes, update [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md)'s "Module tree" and any affected row in "Code ↔ Docs Cross-Reference" in the same PR.
- [ ] Did you add a new `src/bin/` binary? If yes, add it to the "Crate root" table in `PROJECT_STRUCTURE.md`.

### When your PR touches CLI behavior

- [ ] Did you add, rename, or remove a CLI flag, subcommand, or change default behavior? If yes, the matching `docs/src/content/docs/commands/<cmd>.mdx` needs updating in the same PR.
- [ ] Did you change `jackin.agent.toml` schema or validation rules? If yes, update `docs/src/content/docs/developing/agent-manifest.mdx`.
- [ ] Did you change `config.toml` shape? If yes, update `docs/src/content/docs/reference/configuration.mdx`.
- [ ] Did you change auth-forward, Keychain, symlink, or file-permission behavior in `src/instance/auth.rs`? If yes, update `docs/src/content/docs/guides/authentication.mdx` and `docs/src/content/docs/guides/security-model.mdx`.

### When your PR touches a roadmap item

- [ ] If the PR resolves or advances an item under `docs/src/content/docs/reference/roadmap/`, update that item's `Status` field (`Open | Deferred | Resolved`) and `Related Files` section in the same PR.
- [ ] If the PR references `src/` paths that have since moved (e.g., a roadmap doc mentions `src/runtime.rs` which is now `src/runtime/`), fix those path references.

### How to verify

One command to surface the obvious drift targets:

```sh
git diff --name-only origin/main... | grep -E '^src/|^Cargo\.toml' | head
```

If that list is non-empty, walk through the checkboxes above before requesting review. The goal is that a new operator opening `PROJECT_STRUCTURE.md` or a roadmap doc always sees paths that resolve, commands that exist, and behaviors that match current code.
