# Docs AGENTS.md

## Stack

- This directory is an Astro + Starlight documentation site.
- Package manager and lockfile: `bun` and `bun.lock`.
- Framework: `astro` with `@astrojs/starlight`, `@astrojs/react`, and MDX auto-provided by Starlight.
- Styling: `tailwindcss` v4 (CSS-first configuration in `src/styles/global.css`).

## Language

**Write TypeScript, not JavaScript.** This applies to every file that isn't
MDX or CSS:

- Astro config: `astro.config.ts` (not `.mjs`/`.js`)
- Build scripts: `scripts/*.ts` (not `*.mjs`/`*.js`). Run via `bun run <file.ts>`.
- Route endpoints (e.g. `src/pages/og/[...slug].png.ts`): `.ts`
- React components: `.tsx`
- Inline `<script>` in `.astro` files: keep TypeScript by default; use
  `is:inline` for pre-hydration scripts only when the timing absolutely
  requires running before Astro's module loader.

Rationale: TypeScript catches integration errors between Starlight's
route data, Astro config shapes, and our own helpers at edit time
rather than at build time. Bun runs `.ts` natively — there is no
transpile step or build-config cost to choose TS over JS/MJS.

Do not introduce new `.mjs` or plain `.js` files. If you encounter one
left over from an older version of this site, port it to `.ts` as part
of whatever task brought you there.

### Strict mode

The docs `tsconfig.json` extends `astro/tsconfigs/strict`, which turns
on `strict: true` plus Astro's recommended strict defaults
(`forceConsistentCasingInFileNames`, `isolatedModules`, etc.). Treat
strict mode as non-negotiable:

- Do not add `tsconfig` overrides that weaken it (e.g. `strict: false`,
  `strictNullChecks: false`, `noImplicitAny: false`).
- Do not use `// @ts-ignore` or `// @ts-nocheck` to silence errors.
  If a third-party type is genuinely wrong, use `// @ts-expect-error`
  with a one-line explanation and a link/issue reference so the escape
  hatch is auditable.
- New code should pass `bunx tsc --noEmit` cleanly.

Running `astro/tsconfigs/strictest` (adds `noUncheckedIndexedAccess`,
`exactOptionalPropertyTypes`, and similar) is a desirable follow-up
goal but not a current requirement — some existing code (rainEngine
indexed access, astro-og-canvas optional-property types) would need
targeted cleanup first.

## Package Management

- Use `bun install`, `bun add`, and `bun remove` for dependency changes.
- Do not use `npm`, `pnpm`, or `yarn` in this directory.
- Keep `bun.lock` as the only lockfile in `docs/`.

## Common Commands

**First-time setup (and after pulling dependency changes):**

```sh
bun install --frozen-lockfile
```

This populates `node_modules/` with the platform-specific optional native binaries (e.g. `@rollup/rollup-darwin-arm64` on Apple Silicon). Skipping it makes `bun run dev` fail with `Cannot find module '@rollup/rollup-...'`.

**Day-to-day:**

- Start dev server: `bun run dev`
- Build docs: `bun run build`
- Preview production build: `bun run preview`
- Check links in the existing production build: `bun run check:links`
- Rebuild and then check links: `bun run check:links:fresh`
- Run tests: `bun test`

`bun run check:links` and `bun run check:links:fresh` require the `lychee` CLI (for example, `brew install lychee` on macOS).

**If `node_modules` was last installed on a different OS** (e.g. an agent built from a Linux container shared the working tree with a macOS host), Bun won't always re-resolve optional native binaries on its own. Reset with:

```sh
rm -rf node_modules
bun install --frozen-lockfile
```

## Naming conventions

### Operator console

- **`jackin console` is the canonical name for the TUI.** Use it
  everywhere in prose, comparisons, examples, and screenshots —
  including the second and subsequent mentions on the same page.
- **The bare `jackin` shortcut is explained exactly once**, in the
  console command page (`docs/src/content/docs/commands/console.mdx`),
  as syntactic sugar that opens the console on an interactive terminal.
  No other doc page should explain the shortcut, mention it inline,
  or use the dual-form `(\`jackin\` or \`jackin console\`)` /
  `\`jackin\` / \`jackin console\``. Both forms confuse readers and
  imply they're separate commands.
- **`jackin console` is deliberately less featured than `jackin load`**
  (and the other CLI subcommands). The console is the simplified,
  intuitive front for common day-to-day flows. New niche flags and
  detail-level options land on the CLI first, and often exclusively.
  When comparing the two, make that relationship explicit — do not
  imply they are feature-equivalent front-ends.

## Content Notes

- Docs content lives under `src/content/docs/` as MDX files.
- Content collection defined in `src/content.config.ts` using the Starlight `docsLoader()` (Astro 6 Content Layer API).
- File-based routing: `src/content/docs/foo/bar.mdx` → `/foo/bar`.
- Link to docs pages with site-absolute routes such as `/guides/mounts/`.
  The generated-site lychee check is the authoritative guard for these links,
  including fragments.
- Do not use GitHub blob links or `<RepoFile />` for pages that exist in the
  published docs site under `src/content/docs/`. Use the site route instead
  (for example, `/reference/roadmap/per-mount-isolation/`). GitHub is only for
  external repositories and repo files that are not available as rendered docs.
- Link to non-doc repository files with `<RepoFile path="src/runtime/image.rs" />`
  rather than plain code spans when the reader should be able to open the file.
  Import it from `src/components/RepoFile.astro` with the appropriate relative
  path for the MDX file. The component renders a GitHub `blob/main` URL, and the
  CI link check remaps that URL to the PR checkout, so file renames and deletions
  fail before merge. Avoid `tree/main` directory links unless there is a specific
  reason they cannot point at a concrete file.
- Plain inline-code references to existing repo files under `src/`, `docs/`,
  `docker/`, or `.github/` fail `bun run check:repo-links`; link them instead.
  For published docs pages, use the docs route. For non-doc repo files, use
  `<RepoFile />`. Proposed future files that do not exist yet may stay as code
  spans.
- `check:repo-links` exists because lychee only checks real links in rendered
  HTML. Plain code spans like `src/runtime/launch.rs` are not links, so lychee
  cannot detect when those files are renamed or deleted. The source check makes
  those references use `<RepoFile />`, then lychee verifies the generated URL.
- `mailto:` links are included in lychee checks, so use real, intentional email
  addresses rather than placeholders.
- Sidebar and top-nav are configured in `astro.config.ts`.
- **Roadmap sidebar discipline.** Every MDX file under
  `src/content/docs/reference/roadmap/` must have a matching entry in
  the sidebar in `astro.config.ts` under one of the open-work
  categories (`Agent Orchestrator Research`, `Codebase health`,
  `Agent runtimes & authentication`, `Isolation & security`,
  `Infrastructure`, `Documentation tooling`,
  `Configuration ergonomics`) or under `Resolved`, as appropriate for
  the page's `**Status**` field. Whenever you add, rename, delete, or
  change the status of a roadmap item — or restructure the directory
  — verify the sidebar still matches the directory contents in the
  same PR. Operators rely on the sidebar (not the overview prose) to
  discover open work, so an item reachable only via direct URL or
  the overview page is effectively hidden. To audit:

  ```sh
  ls docs/src/content/docs/reference/roadmap/*.mdx \
    | xargs -n1 basename -s .mdx | sort > /tmp/roadmap-files
  grep -oE "reference/roadmap/[a-z0-9-]+" docs/astro.config.ts \
    | sed 's|reference/roadmap/||' | sort -u > /tmp/roadmap-sidebar
  diff /tmp/roadmap-files /tmp/roadmap-sidebar
  ```

  The diff must be empty.
- **Roadmap overview discipline.** `src/content/docs/reference/roadmap.mdx`
  is the entry point operators land on when they want a single
  picture of *what shipped, what is partial, what is planned, what
  is deferred, and what is on hold*. The sidebar lists every item
  alphabetically/by phase; the overview is what tells the reader
  the **status story**. The two surfaces have different jobs and
  must be maintained together, not folded into one.

  Whenever you add, rename, delete, or change the `**Status**`
  field of a roadmap item, also update `roadmap.mdx` so the item
  lands in the section that matches its new status:

  | `**Status**` value | Section in `roadmap.mdx` |
  |---|---|
  | `Resolved` / `Implemented in V1` | **Completed** |
  | `Partially implemented` | **Partially implemented** |
  | `Open` (active design / Phase work) | **Planned** (under the right subsection) |
  | `Deferred` / `Proposed` / `Needs design` | **Planned** with a `(status: …)` suffix on the bullet |

  An item that exists in `roadmap/` but is not reachable from
  `roadmap.mdx` is half-hidden: an operator who only reads the
  overview thinks the work does not exist, and an operator who
  only browses the sidebar sees the title without status context.
  Both must agree.

  To audit which roadmap items are missing from the overview:

  ```sh
  ls docs/src/content/docs/reference/roadmap/*.mdx \
    | xargs -n1 basename -s .mdx | sort > /tmp/roadmap-files
  grep -oE 'reference/roadmap/[a-z0-9-]+' docs/src/content/docs/reference/roadmap.mdx \
    | sed 's|reference/roadmap/||' | sort -u > /tmp/roadmap-overview
  comm -23 /tmp/roadmap-files /tmp/roadmap-overview
  ```

  The output lists items present in the directory but missing from
  the overview. It must be empty *unless* the missing items are
  intentionally umbrella-covered by a parent program entry (e.g.
  Agent Orchestrator Research leaves, Codebase readability
  program leaves) — in which case the program entry itself must
  appear in the overview and explicitly say it covers them.
- Use Starlight components for callouts (`<Aside type="note|tip|caution">`),
  steps (`<Steps>` around an `<ol>`), and tabs (`<Tabs><TabItem>`). Import from
  `@astrojs/starlight/components`.
- Every page requires a `title:` field in its frontmatter.
- Self-hosted fonts via fontsource (`src/styles/fonts.css`) — no third-party font CDN.
- Keep docs and code behavior aligned; when they differ, code is the source of truth.
- **Never reference open pull requests in published documentation.** Any
  link of the form `https://github.com/jackin-project/<repo>/pull/<N>`
  or prose like "PR #123" / "in flight in #123" must not appear in
  `src/content/docs/**.mdx`. Open PRs are ephemeral — they may be
  closed, rebased, force-pushed, retitled, split, or replaced — so
  pointing operators or roadmap readers at one bakes a transient URL
  into a long-lived page. When a feature is *under development*,
  describe its **state** ("under active development", "in flight",
  "design in progress") without naming the PR. The PR that lands the
  feature is the right place to update the documentation to say it
  has landed; until then, the docs describe the steady-state user-
  visible reality, not the in-progress engineering. Closed/merged
  PRs and issues may still appear when they are the canonical record
  of a *resolved* design discussion or a known limitation — that is
  the existing pattern in `reference/roadmap/*.mdx` and is fine.
  This rule applies equally to the README, AGENTS files outside
  `docs/`, and any other published artefact.
- **The site has three audiences, not two.** Always classify a docs
  change against this list before writing it:
  1. **Operator** (sidebar groups: *Getting Started*, *Operator
     Guide*, *Commands*) — uses jackin' as a product through CLI and
     TUI. Never asked to know on-disk paths, TOML schemas, or
     internals.
  2. **Role author** (sidebar group: *Role Authoring* — pages under
     `guides/role-repos.mdx` and `developing/*`) — **also user-facing**.
     Builds their own role repos with their preferred toolchain and
     plugins. Does not need to know how jackin' is implemented to
     follow these pages. The only concession is that
     `developing/construct-image.mdx` has a contributor lower half
     for `just construct-build` / CI workflow.
  3. **Contributor** (sidebar group: *Behind jackin' — Internals* —
     `reference/*`, including `roadmap/*.mdx`) — works on jackin'
     itself. Architecture, configuration-file schema, codebase map,
     roadmap design proposals. This is where on-disk layouts,
     internal mechanisms, and Rust-level details live.

  When you add or rewrite a page, decide which of the three audiences
  it serves and put it under the matching sidebar group. Never mix
  audiences inside the same page beyond the explicit
  `developing/construct-image.mdx` exception. Pages on the
  user-facing surfaces (Operator and Role Authoring) link out to the
  Internals surface when a contributor reader would want the deeper
  detail; they do not inline it.

- **Never leak technical-implementation details into user-facing
  pages.** "User-facing" here covers **both** the Operator surface
  (Getting Started, Operator Guide, Commands) **and** the Role
  Authoring surface (the *Role Authoring* sidebar group). On either
  surface, do not mention:
  - on-disk paths the operator never edits — `~/.config/jackin/`,
    `~/.jackin/`, `~/.jackin/data/<…>/`, `~/.jackin/roles/<…>/`,
    `~/.jackin/cache/`, the per-container state directory layout,
    `isolation.json`, the `git/worktree/repo/<dst>/<container>/`
    materialised path, the role-repo branch cache directory, or any
    other internal storage location;
  - TOML schema fragments — `[workspaces.<name>]`,
    `[workspaces.<name>.env]`, `[workspaces.<name>.roles.<role>.env]`,
    `[claude]`, `[codex]`, `[roles."org/agent"]`, `[docker.mounts]`,
    `[[mounts]]`, etc.;
  - field-level keys readers would only know from reading the
    config file — `auth_forward = "sync"`, `trusted = true`,
    `last_role`, `git_pull_on_entry`, `published_image`, `data_dir`,
    `keep_awake.enabled`, etc. Describe the **operator-visible
    behaviour** instead;
  - Rust struct, enum, function, or module names — `RoleManifest`,
    `MountIsolation`, `ConfigEditor`, `MaterializedWorkspace`,
    `RUNTIME_OWNED_ENV_VARS`, etc.;
  - internal-implementation footnotes — "this is technical debt and
    is on the roadmap to simplify", "the current implementation does
    X then Y in the derived image", or any other "we plan to refactor
    this" admission. Those belong to the internals pages or to a
    roadmap item, not to the operator surface.

  Where each kind of detail belongs:

  - **Contributor-only internals** (jackin's on-disk layout under
    `~/.config/jackin/` and `~/.jackin/`, the schema of jackin's own
    `config.toml`, `isolation.json`, the per-container state-directory
    layout, internal Rust struct/enum/function names, and any
    "this is technical debt" admission) belong on pages under
    `reference/` and `reference/roadmap/`. They must **not** appear
    on the Operator or Role Authoring surfaces.
  - **Role-author surface schema** is different from jackin's
    internals and *is* allowed on the Role Authoring pages: the
    schema of the role's own `jackin.role.toml` (`[claude]`,
    `[codex]`, `[hooks]`, `[identity]`, `[env.<NAME>]`, etc.) lives
    in `developing/role-manifest.mdx` because that file is the
    role author's own surface — they author it. Likewise the role
    repo's own `Dockerfile` shape lives in `developing/creating-roles.mdx`
    and `guides/role-repos.mdx`. These are the role's artefacts, not
    jackin's storage.
  - **The contributor section of `developing/construct-image.mdx`**
    (its lower half — `just construct-build`, CI workflow, advanced
    publish rehearsal) is the documented exception where one page
    serves two audiences. Its top-of-page Aside calls this out
    explicitly.

  Whenever a user-facing page needs to refer to internal layout
  for context, link to the matching internals page
  (`/reference/configuration/`, `/reference/architecture/`,
  `/reference/codebase-map/`) instead of inlining the detail. When
  in doubt, ask: "could a reader follow this
  page using only the CLI/TUI?" If the answer is no because the page
  describes a TOML key or an on-disk path, move that detail to
  internals.

## Landing Page

- Landing is at `src/pages/index.astro` — a plain Astro page, NOT inside
  the Starlight content collection. It has full control over its layout.
- React components in `src/components/landing/` are mounted as islands.

## Theme

- Starlight chrome uses CSS custom properties. Our mappings live in
  `src/styles/docs-theme.css` (Starlight `--sl-*` → Radix tokens from
  `tempo-tokens.css`).
- Brand accent token `--jk-brand` is theme-aware (bright #00ff41 in dark,
  muted #16a34a in light) — used for tabs underline, sidebar active pill,
  right-rail TOC, pagination hover.
- Code blocks always use a dark surface (`--jk-code-bg`) regardless of
  page theme. Shiki theme: github-dark in dark mode, one-dark-pro in light.
