# Docs AGENTS.md

## Stack

- Fumadocs docs site on TanStack Start + Vite.
- Package manager + lockfile: `bun` and `bun.lock`.
- Framework: `@tanstack/react-start`, `fumadocs-ui`, `fumadocs-mdx`, React, MDX from Fumadocs.
- Styling: `tailwindcss` v4 (CSS-first config in `src/styles/global.css`).

## Language

**Write TypeScript, not JavaScript.** Every non-MDX, non-CSS file:

- Vite config: `vite.config.ts` (not `.mjs`/`.js`)
- Build scripts: `scripts/*.ts` (not `*.mjs`/`*.js`). Run via `bun run <file.ts>`.
- Route endpoints (e.g. `src/routes/og/{$}[.]webp.tsx`): `.ts` or `.tsx`
- React components: `.tsx`
- Inline route `head` scripts: minimal, typed at route boundary.

Why: TS catches Fumadocs/TanStack/helper integration errors at edit time. Bun runs `.ts` natively — no transpile cost.

No new `.mjs` or `.js` files. Port leftover ones to `.ts` when a task brings you there.

### Strict mode

Docs `tsconfig.json` uses `strict: true` plus standard bundler/React settings. Strict mode non-negotiable:

- No `tsconfig` overrides weakening it (`strict: false`, `strictNullChecks: false`, `noImplicitAny: false`).
- No `// @ts-ignore` / `// @ts-nocheck`. If a third-party type is genuinely wrong, use `// @ts-expect-error` with one-line explanation + link/issue ref so escape hatch is auditable.
- New code passes `bunx tsc --noEmit` cleanly.

Stricter `noUncheckedIndexedAccess` / `exactOptionalPropertyTypes`: desirable follow-up, not current requirement — existing code needs cleanup first.

## Package Management

- Use `bun install`, `bun add`, `bun remove` for dependency changes.
- No `npm`, `pnpm`, `yarn` here.
- Keep `bun.lock` as only lockfile in `docs/`.

## Common Commands

**First-time setup (and after pulling dependency changes):**

```sh
bun install --frozen-lockfile
```

Populates `node_modules/` with platform-specific optional native binaries (e.g. `@rollup/rollup-darwin-arm64` on Apple Silicon). Skipping it makes `bun run dev` fail with `Cannot find module '@rollup/rollup-...'`.

**Day-to-day:**

- Start dev server: `bun run dev`
- Build docs: `bun run build`
- Preview production build: `bun run preview`
- Check source repository links: `bun run check:repo-links`
- Check roadmap sidebar completeness: `bun run check:roadmap-sidebar`
- Check links in the existing production build: `bun run check:links`
- Rebuild and then check links: `bun run check:links:fresh`
- Run tests: `bun test`

**CI locked install:**

```sh
bun ci
```

Use `bun ci` in workflows. It has the same lockfile enforcement as `bun install --frozen-lockfile` and makes the CI intent explicit.

`bun run check:links` and `bun run check:links:fresh` require the `lychee` CLI (e.g. `brew install lychee` on macOS).

**If `node_modules` was last installed on a different OS** (e.g. agent built from Linux container shares working tree with macOS host), Bun won't always re-resolve optional native binaries. Reset with:

```sh
rm -rf node_modules
bun install --frozen-lockfile
```

## Naming conventions

### Operator console

- **`jackin console` is canonical name for the TUI.** Use everywhere — prose, comparisons, examples, screenshots — including second and later mentions on same page.
- **Bare `jackin` shortcut explained exactly once**, in console command page (`docs/content/docs/commands/console.mdx`), as sugar that opens console on an interactive terminal. No other page explains it, mentions it inline, or uses dual-form `(\`jackin\` or \`jackin console\`)` / `\`jackin\` / \`jackin console\``. Both forms confuse readers, imply separate commands.
- **`jackin console` is deliberately less featured than `jackin load`** (and other CLI subcommands). Console is simplified front for common day-to-day flows. Niche flags + detail options land on CLI first, often exclusively. When comparing, make relationship explicit — don't imply feature-equivalence.

## Content Notes

- Docs content under `content/docs/` as MDX files.
- Content source defined in `source.config.ts`, loaded via `src/lib/source.ts`.
- File-based routing: `content/docs/foo/bar.mdx` → `/foo/bar`. Parenthesized group dirs (e.g. `content/docs/(role-authoring)/`) are organizational, absent from URLs.
- Link docs pages with site-absolute routes (e.g. `/guides/mounts/`). Generated-site lychee check is authoritative guard, including fragments.
- No GitHub blob links or `<RepoFile />` for pages published under `content/docs/`. Use site route instead (e.g. `/reference/roadmap/per-mount-isolation/`). GitHub only for external repos and repo files not rendered as docs.
- Link non-doc repo files with `<RepoFile path="src/runtime/image.rs" />`, not plain code spans, when reader should open the file. Component renders GitHub `blob/main` URL; CI link check remaps to PR checkout, so renames/deletions fail before merge. Avoid `tree/main` directory links unless a concrete file is impossible.
- Plain inline-code refs to existing repo files under `src/`, `docs/`, `docker/`, `.github/` fail `bun run check:repo-links`; link them. Published docs pages → docs route. Non-doc repo files → `<RepoFile />`. Future files not yet existing may stay code spans.
- `check:repo-links` exists because lychee only checks real links in rendered HTML. Plain code spans like `src/runtime/launch.rs` aren't links, so lychee can't detect renames/deletions. The source check forces `<RepoFile />`, then lychee verifies the generated URL.
- `mailto:` links are lychee-checked; use real intentional addresses, not placeholders.
- Sidebar + top-nav configured via `meta.json` files and `src/lib/layout.shared.tsx`.
- **Roadmap sidebar discipline.** Every MDX file under `content/docs/reference/roadmap/` must be referenced in at least one `meta.json` under the roadmap tree (nested group structure: root `meta.json` points to group dirs; each group's `meta.json` points to pages via `../slug` or `../../slug`). On any add/rename/delete/status-change of a roadmap item — or directory restructure — verify sidebar still matches directory contents in same PR. Operators discover open work via sidebar (not overview prose), so an item reachable only via direct URL or overview is effectively hidden. Audit from `docs/`:

  ```sh
  bun run check:roadmap-sidebar
  ```

  Script reports any MDX file with no matching `meta.json` entry and any entry with no matching MDX file. Both directions must be clean.
- **Roadmap overview discipline.** `content/docs/reference/roadmap/index.mdx` is the entry operators land on for a single picture of *what shipped, partial, planned, deferred, on hold*. Sidebar lists every item alphabetically/by phase; overview tells the **status story**. Different jobs, maintained together, not folded into one.

  On any add/rename/delete/`**Status**`-change of a roadmap item, update `roadmap.mdx` so item lands in matching section:

  | `**Status**` value | Section in `roadmap.mdx` |
  |---|---|
  | `Resolved` / `Implemented in V1` | **Completed** |
  | `Partially implemented` | **Partially implemented** |
  | `Open` (active design / Phase work) | **Planned** (under the right subsection) |
  | `Deferred` / `Proposed` / `Needs design` | **Planned** with a `(status: …)` suffix on the bullet |

  An item in `roadmap/` unreachable from `roadmap.mdx` is half-hidden: overview-only reader thinks work doesn't exist; sidebar-only reader sees title without status. Both must agree.

  Roadmap pages don't duplicate shipped feature docs. When work lands, move durable operator details to the guide/command/reference page; keep roadmap item to status, canonical-doc links, remaining/future work. Long implementation walkthroughs belong in contributor references, not roadmap items.

  Audit which roadmap items are missing from overview:

  ```sh
  ls docs/content/docs/reference/roadmap/*.mdx \
    | xargs -n1 basename -s .mdx | grep -v '^index$' | sort > /tmp/roadmap-files
  grep -oE 'reference/roadmap/[a-z0-9-]+' docs/content/docs/reference/roadmap/index.mdx \
    | sed 's|reference/roadmap/||' | sort -u > /tmp/roadmap-overview
  comm -23 /tmp/roadmap-files /tmp/roadmap-overview
  ```

  Output lists items in directory but missing from overview. Must be empty *unless* missing items are intentionally umbrella-covered by a parent program entry (e.g. Agent Orchestrator Research leaves, Codebase readability program leaves) — then the program entry itself must appear in overview and explicitly say it covers them.
- Use MDX components from `src/components/mdx.tsx` for callouts (`<Aside type="note|tip|caution">`), steps (`<Steps>` around an `<ol>`), tabs (`<Tabs><TabItem>`). Global in MDX.
- Every page needs a `title:` frontmatter field.
- **Do not hard-wrap MDX prose, or any markdown the docs site renders, or AGENTS.md / PULL_REQUESTS.md / CLAUDE.md / README.md / CHANGELOG.md / any other prose markdown in the repo.** Each paragraph = one long line. Fumadocs, GitHub's renderer, every reasonable editor wrap at display width; hard-wrapping at ~70 cols makes one-word edits touch every line and splits sentences across meaningless boundaries. Exception: content with meaningful line breaks — code fences, list bullets, table cells, frontmatter values. Unwrap existing hard-wrapped paragraphs as part of the edit that brings you there. The PR-body rule in `PULL_REQUESTS.md` is the same rule applied to every prose markdown surface.
- Self-hosted fonts via fontsource (`src/styles/fonts.css`) — no third-party font CDN.
- Keep docs and code aligned; when they differ, code is source of truth.
- **Never reference open pull requests in published documentation.** No link of form `https://github.com/jackin-project/<repo>/pull/<N>` or prose like "PR #123" / "in flight in #123" in `content/docs/**.mdx`. Open PRs are ephemeral (closed, rebased, force-pushed, retitled, split, replaced) — pointing at one bakes a transient URL into a long-lived page. For features *under development*, describe **state** ("under active development", "in flight", "design in progress") without naming the PR. The landing PR updates docs to say it landed; until then docs describe steady-state user-visible reality, not in-progress engineering. Closed/merged PRs and issues may appear when they're the canonical record of a *resolved* design discussion or known limitation — existing pattern in `reference/roadmap/*.mdx`, fine. Applies equally to README, AGENTS files outside `docs/`, any other published artefact.
- **The site has three audiences, not two.** Classify every docs change against this list before writing:
  1. **Operator** (sidebar groups: *Getting Started*, *Operator Guide*, *Commands*) — uses jackin❯ as a product via CLI/TUI. Never asked to know on-disk paths, TOML schemas, internals.
  2. **Role author** (sidebar group: *Role Authoring* — pages under `guides/role-repos.mdx` and `developing/*`) — **also user-facing**. Builds own role repos with own toolchain + plugins. Needs no jackin❯ implementation knowledge to follow. Only concession: `developing/construct-image.mdx` has a contributor lower half for `mise run construct-build-local` / CI workflow.
  3. **Contributor** (sidebar group: *Behind jackin❯ — Internals* — `reference/*`, including `roadmap/*.mdx`) — works on jackin❯ itself. Architecture, config-file schema, codebase map, roadmap design proposals. On-disk layouts, internal mechanisms, Rust-level details live here.

  When adding/rewriting a page, decide which audience it serves and put it under matching sidebar group. Never mix audiences in one page beyond the explicit `developing/construct-image.mdx` exception. User-facing surfaces (Operator, Role Authoring) link out to Internals for deeper detail; don't inline it.

- **Never leak technical-implementation details into user-facing pages.** "User-facing" covers **both** Operator (Getting Started, Operator Guide, Commands) **and** Role Authoring (the *Role Authoring* sidebar group). On either, do not mention:
  - on-disk paths the operator never edits — `~/.config/jackin/`, `~/.jackin/`, `~/.jackin/data/<…>/`, `~/.jackin/roles/<…>/`, `~/.jackin/cache/`, per-container state dir layout, `isolation.json`, the `git/worktree/repo/<dst>/<container>/` materialised path, role-repo branch cache directory, any other internal storage;
  - TOML schema fragments — `[workspaces.<name>]`, `[workspaces.<name>.env]`, `[workspaces.<name>.roles.<role>.env]`, `[claude]`, `[codex]`, `[roles."org/agent"]`, `[docker.mounts]`, `[[mounts]]`, etc.;
  - field-level keys known only from the config file — `auth_forward = "sync"`, `trusted = true`, `last_role`, `git_pull_on_entry`, `published_image`, `data_dir`, `keep_awake.enabled`, etc. Describe **operator-visible behaviour** instead;
  - Rust struct/enum/function/module names — `RoleManifest`, `MountIsolation`, `ConfigEditor`, `MaterializedWorkspace`, `RUNTIME_OWNED_ENV_VARS`, etc.;
  - internal-implementation footnotes — "this is technical debt and is on the roadmap to simplify", "the current implementation does X then Y in the derived image", any "we plan to refactor this" admission. Those belong to internals pages or a roadmap item, not the operator surface.

  Where each kind belongs:

  - **Contributor-only internals** (on-disk layout under `~/.config/jackin/` and `~/.jackin/`, schema of jackin❯ `config.toml`, `isolation.json`, per-container state-dir layout, internal Rust struct/enum/function names, any "technical debt" admission) belong under `reference/` and `reference/roadmap/`. Must **not** appear on Operator or Role Authoring surfaces.
  - **Role-author surface schema** differs from jackin❯ internals and *is* allowed on Role Authoring pages: the role's own `jackin.role.toml` schema (`[claude]`, `[codex]`, `[hooks]`, `[identity]`, `[env.<NAME>]`, etc.) lives in `developing/role-manifest.mdx` because the author writes that file. Likewise the role repo's own `Dockerfile` shape lives in `developing/creating-roles.mdx` and `guides/role-repos.mdx`. These are role artefacts, not jackin❯ storage.
  - **The contributor section of `developing/construct-image.mdx`** (lower half — `mise run construct-build-local`, CI workflow, advanced publish rehearsal) is the documented two-audience exception. Its top-of-page Aside calls this out.

  When a user-facing page needs internal layout for context, link to the matching internals page (`/reference/configuration/`, `/reference/architecture/`, `/reference/codebase-map/`) instead of inlining. When in doubt ask: "could a reader follow this using only the CLI/TUI?" If no because it describes a TOML key or on-disk path, move that detail to internals.

## Landing Page

- Landing at `src/routes/index.tsx` — TanStack route outside docs layout. Full layout control.
- React components in `src/components/landing/` rendered by that route.

## Theme

- Fumadocs chrome uses CSS custom properties. Mappings in `src/styles/docs-theme.css` (Fumadocs `--color-fd-*` plus legacy `--sl-*` compat tokens → Radix tokens from `tempo-tokens.css`).
- Brand accent token `--jk-brand` is theme-aware (bright #00ff41 dark, muted #16a34a light) — tabs underline, sidebar active pill, right-rail TOC, pagination hover.
- Code blocks always use a dark surface (`--jk-code-bg`) regardless of page theme. Shiki theme: github-dark in both modes.
