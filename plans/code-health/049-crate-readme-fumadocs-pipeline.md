# Plan 049: Crate-README → Fumadocs pipeline — generated "Behind jackin❯ — crates" section; slim PROJECT_STRUCTURE.md

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- docs/scripts docs/package.json docs/source.config.ts PROJECT_STRUCTURE.md crates/*/README.md`
> Plan 029 fixes the capsule README stub — expected drift (it helps this
> plan). Other structural changes to the docs build: compare excerpts, STOP
> on mismatch.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW-MED (docs build only; risk is link-check churn on 26 generated pages)
- **Depends on**: none (plan 029's capsule-README fix improves output quality but is not a blocker — the generator must handle a thin README gracefully either way)
- **Category**: docs
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

Roadmap Phase 5 item 6: "Auto-extract crate README files into Fumadocs … a build-time pipeline (a `scripts/*.ts` sync wired into the docs build) that renders each `crates/*/README.md` as a page under a 'Behind jackin❯ — crates' section. The README is the single source of truth; the docs page is generated from it." The per-crate READMEs are the authoritative orientation record (26 exist, template-enforced), but they are invisible on the docs site, so the Codebase Map keeps duplicating per-crate prose that drifts. The exact pattern the item asks for already ships in the docs build (`scripts/prerender-static.ts` runs after `vite build`), making this M work, not the "L program" it was parked as. Item 7 (slim PROJECT_STRUCTURE.md) rides along: the file is already near-slim; this plan does the residual trim and re-points it at the generated pages.

## Current state

Verified at `fabe88406`.

- Build hook: `docs/package.json:7` — `"build": "vite build && bun run scripts/prerender-static.ts"`. Dev: `"dev"` runs vite only.
- Script precedent: `docs/scripts/prerender-static.ts` (123 lines) — TypeScript, `node:fs/promises` (`cp, mkdir, readdir, rm, writeFile`), walks `content/docs` for `.mdx` (its `docsSlugs()` recursion is the directory-walk exemplar), builds routes. Docs AGENTS: TypeScript only, run via `bun run <file.ts>`, strict tsconfig, no `.mjs`.
- Content source: `docs/source.config.ts` — stock `defineDocs` over `content/docs`; file-based routing; parenthesized group dirs excluded from URLs; every page needs `title:` frontmatter; sidebar via `meta.json` per directory.
- 26 `crates/*/README.md` exist following the template in `crates/AGENTS.md` (purpose sentence, "What this crate owns", "Architecture tier and allowed dependencies", clickable "Structure" table with relative `src/...` links, "Public API", "How to verify"). `crates/jackin-capsule/README.md` is a 597-byte stub (plan 029 Step 4 fixes it).
- Link rules that bind generated pages (docs/AGENTS.md): pages under `content/docs` may not carry plain repo-path code spans for existing repo files (fails `cargo xtask docs repo-links`); repo files are linked with the Fumadocs repository-file component (`<RepoFile path="…">`); docs-site pages use site-absolute routes; lychee checks the built site. The generator must therefore REWRITE the READMEs' relative links (e.g. `[\`bar.rs\`](src/bar.rs)`) into `<RepoFile path="crates/<name>/src/bar.rs">` components, and escape MDX-hostile constructs (`{`, `<` in prose/code spans — MDX parses JSX).
- Audience rule: generated pages are contributor-facing → they belong under the *Behind jackin❯ — Internals* sidebar tree (`content/docs/reference/…`).
- PROJECT_STRUCTURE.md (138 lines): already leads with delegation to per-crate READMEs (:3-5, :17); keeps the multi-repo ecosystem table (:30-39) and code↔docs contract (:119-134) — the two irreducible jobs. Residual trim candidates: the root-files table (:45-71) and changelog-ish lines (:72).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Install docs deps | `cd docs && bun install --frozen-lockfile` | exit 0 |
| Generate + build | `cd docs && bun run build` | build completes |
| Typecheck | `cd docs && bunx tsc --noEmit` | exit 0 |
| Docs tests | `cd docs && bun test` | pass |
| Repo-link gate | `cargo xtask docs repo-links` | pass |
| Link check (built site) | `cd docs && bun run check:links` | pass (needs `lychee`) |
| Roadmap/research gates | `cargo xtask roadmap audit && cargo xtask research check` | pass |

## Scope

**In scope**:
- `docs/scripts/gen-crate-pages.ts` (create) + a `bun test` spec for its pure transforms (`docs/scripts/gen-crate-pages.test.ts` — check where existing docs tests live first and match)
- `docs/package.json` — wire generation BEFORE `vite build` in `build` (and before `dev` via a `predev`-style chain or explicit `&&`)
- `docs/content/docs/reference/crates/meta.json` (create; checked in) + `.gitignore` entry for the generated `*.mdx` in that dir
- `docs/content/docs/reference/meta.json` (add the section to the sidebar tree)
- `PROJECT_STRUCTURE.md` (trim + re-point)
- `docs/content/docs/reference/getting-oriented/codebase-map.mdx` — ONLY to add links to the generated section where it duplicates per-crate purpose prose (full slimming is roadmap item 5's own later step)
- Roadmap Phase 5 items 6+7 status notes

**Out of scope** (do NOT touch):
- The crate READMEs themselves (source of truth; if one breaks the generator, handle gracefully + report).
- The README-freshness gate (plan 050) and the full codebase-map slimming (later, once the section proves itself).
- Any other docs build script; the landing page; sidebar groups outside the Internals tree.

## Git workflow

- Branch off `main`: `feature/crate-readme-docs-pipeline`.
- Conventional Commits (`feat(docs): …`), `-s`, push per commit. PR to `main`; do not merge.

## Steps

### Step 1: The generator

`docs/scripts/gen-crate-pages.ts`: enumerate `../crates/*/README.md` (path-resolve from `import.meta.dirname` like prerender-static.ts:5); for each, emit `content/docs/reference/crates/<crate-name>.mdx` with: frontmatter `title: "<crate-name>"` + a generated-file banner comment (`{/* GENERATED from crates/<name>/README.md — edit the README, not this file */}`), then the README body transformed:
1. Strip the H1 (frontmatter title replaces it).
2. Rewrite relative links: `[text](src/...)`, `[text](../...)` → `<RepoFile path="crates/<name>/…">text</RepoFile>` (normalize the path); absolute http(s) links pass through; links to other crates' READMEs → the sibling generated route `/reference/crates/<other>/`.
3. Escape MDX hazards outside code fences: `{` → `\{`, raw `<` that is not a known component → `&lt;` (transform line-by-line, skipping fenced blocks — write this as a pure function).
4. Brand rule: leave README text verbatim otherwise (the READMEs already follow `jackin❯` conventions; the generator must not "fix" prose).

Write the transforms as exported pure functions; the main() writes files + a `meta.json`-compatible page list to stdout for Step 2's check.

**Verify**: `cd docs && bun run scripts/gen-crate-pages.ts` → 26 files under `content/docs/reference/crates/`; `bunx tsc --noEmit` → exit 0.

### Step 2: Sidebar + gitignore + build wiring

- `content/docs/reference/crates/meta.json`: checked-in list of the 26 pages (alphabetical). Add a generator check: if a `crates/*/README.md` exists with no meta.json entry (or vice versa), the script exits 1 naming the missing entry — the docs-build equivalent of the roadmap-sidebar discipline.
- Parent `reference/meta.json`: add the `crates` group titled `Behind jackin❯ — crates` (confirm exact tree by reading the existing meta.json nesting; place beside getting-oriented).
- `.gitignore` (docs or root — match where docs ignores live): `docs/content/docs/reference/crates/*.mdx`.
- `docs/package.json`: `"build": "bun run scripts/gen-crate-pages.ts && vite build && bun run scripts/prerender-static.ts"`; give `dev` the same prefix.

**Verify**: `cd docs && bun run build` → completes; the built site contains `/reference/crates/jackin-core/` (grep the output dir); `git status` shows no generated `.mdx` staged.

### Step 3: Pure-transform tests

`bun test` spec covering: relative-link rewrite (3 shapes: `src/x.rs`, `src/dir`, sibling README), MDX escaping (a `{` in prose, a `<` in prose, both UNCHANGED inside a code fence), H1 strip, meta.json completeness check failure path.

**Verify**: `cd docs && bun test` → all pass.

### Step 4: Link gates over the generated output

Run `cargo xtask docs repo-links` (the RepoFile components must resolve — a README linking a deleted file will fail HERE; fix the *generator's* path normalization if the path is right in the README, otherwise report the stale README) and `bun run check:links:fresh` (lychee over the rebuilt site).

**Verify**: both gates pass.

### Step 5: PROJECT_STRUCTURE trim + pointers

Trim PROJECT_STRUCTURE.md to its irreducible jobs: keep the multi-repo table + code↔docs contract; replace the root-files table region (:45-71) with two sentences pointing at per-crate READMEs and the generated `/reference/crates/` section. Add the same pointer to codebase-map.mdx where it currently duplicates per-crate purpose lines (link, don't delete map structure this PR). Roadmap items 6 (shipped) + 7 (shipped) notes.

**Verify**: `cargo xtask docs repo-links && cargo xtask roadmap audit` → pass; `cargo xtask ci --fast` → `ci gate OK` (the docs gates ride the ci lane — confirm which xtask gate covers docs and run it explicitly if not).

## Test plan

- Step 3's pure-transform specs (the load-bearing tests — link rewriting and MDX escaping are where generated docs rot).
- The gates themselves (repo-links + lychee) are the integration tests; both must run against the freshly generated output in CI's docs lane (confirm the docs CI workflow runs `bun run build` — it does, since check:links:fresh depends on it; state which workflow in the PR body).

## Done criteria

- [ ] 26 generated pages render under `/reference/crates/`; sidebar section visible
- [ ] Generated files gitignored; meta.json completeness check fails on a missing/extra entry
- [ ] Transform tests green; `bunx tsc --noEmit` clean; `bun test` green
- [ ] `cargo xtask docs repo-links` + `bun run check:links:fresh` green over generated output
- [ ] PROJECT_STRUCTURE.md ≤ ~100 lines, keeps ecosystem table + code↔docs contract
- [ ] Roadmap items 6/7 updated; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- Fumadocs/the MDX pipeline rejects generated pages for a reason the escaping function cannot cover generically (report the construct + the README that produced it).
- `check:links` requires `lychee` and it is unavailable locally AND docs CI cannot be exercised on the branch.
- The docs build's content-collection step runs BEFORE package.json scripts can generate (i.e. fumadocs-mdx scans at config-load in a way that misses just-written files) — report how `source.config.ts` collection interacts with pre-build generation instead of fighting it.
- More than 3 READMEs are malformed against the template (generator can't produce sane pages) — the fix is README repairs (029-style), not generator contortions.

## Maintenance notes

- Plan 050's README-freshness gate keeps the *sources* current; this pipeline keeps the site in sync automatically at build time.
- Roadmap Phase 5 item 5's codebase-map slimming becomes mechanical once this section exists (map → index of links).
- Reviewer scrutiny: the MDX-escape function's fence detection (a bug there mangles code blocks on all 26 pages), and that generated pages carry the generated-banner so nobody edits them.
