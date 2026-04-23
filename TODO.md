# TODO

Roadmap items — open work and resolved design docs — live in the docs
site, not in this repo. See:

- Overview: [`docs/src/content/docs/reference/roadmap.mdx`](docs/src/content/docs/reference/roadmap.mdx)
- Per-item design docs: [`docs/src/content/docs/reference/roadmap/`](docs/src/content/docs/reference/roadmap/)
- Browsable: <https://jackin.tailrocks.com/reference/roadmap/>

To add a new item, create an MDX page under the directory above and
add a sidebar entry in [`docs/astro.config.ts`](docs/astro.config.ts)
under `Roadmap → Open items`.

Each design doc should include (see any existing page as a template):

- `**Status**: Open | Deferred | Resolved`
- `## Problem`
- `## Why It Matters`
- `## Related Files`

## Stale-docs check (every PR)

Docs rot silently. Every PR must include a one-pass verification that
structure-sensitive docs still match reality. Treat these as a
checklist in the PR description — each item takes seconds to check.

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

If that list is non-empty, walk through the checkboxes above before
requesting review. The goal is that a new operator opening
`PROJECT_STRUCTURE.md` or a roadmap doc always sees paths that
resolve, commands that exist, and behaviors that match current code.
