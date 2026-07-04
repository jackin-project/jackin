# Plan 036: Repair `TODO.md` — dead doc paths, a stray automation note, and phantom code markers

> **Executor instructions**: Fixes the per-PR steering doc every PR is told to follow. Run every
> verification command. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- TODO.md`

## Status

- **Result**: DONE in PR #713 (`docs/advisor-improvement-plans`)
- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: docs
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`TODO.md`'s "Stale-docs check (every PR)" and "Roadmap" sections — the doc **every PR is told to walk** to
keep docs in sync with code — route to a **deleted directory tree** (`docs/src/content/docs/...`,
`docs/astro.config.ts`) that the Astro→Fumadocs migration removed; the real homes are under
`docs/content/docs/...` with `meta.json` sidebars. It also references pre-crates monolith paths
(`src/runtime/...`, `src/instance/auth.rs`), claims three `TODO(docker-security-profile-*)` code markers
that **don't exist** (grep finds none — its own single-grep convention returns one end), and carries a
**stray automation note** committed above the `# TODO` heading. An agent following this doc edits/creates
files at dead paths, hunts for nonexistent markers, and opens a deleted config — the exact high-cost
"docs telling agents to patch things that don't exist" case.

## Current state

- `TODO.md:1-2` — stray note: "Inline-code multisets actually match… the validator broke… I'll restore that
  tail…" (committed automation output, not a TODO).
- `TODO.md:103-107,129-139` — Roadmap + Stale-docs sections route to `docs/src/content/docs/reference/roadmap.mdx`,
  `docs/src/content/docs/commands/<cmd>.mdx`, `docs/src/content/docs/developing/role-manifest.mdx`,
  `docs/src/content/docs/reference/configuration.mdx`, `docs/src/content/docs/guides/{authentication,security-model}.mdx`,
  `docs/astro.config.ts` — **all dead**. Actual homes: `docs/content/docs/roadmap/index.mdx`,
  `docs/content/docs/(public)/commands/`, `docs/content/docs/(public)/(role-authoring)/developing/role-manifest.mdx`,
  `docs/content/docs/reference/runtime/configuration.mdx`, `docs/content/docs/(public)/guides/security-model.mdx`;
  sidebar is `meta.json` (+ `cargo xtask roadmap audit`), not `astro.config.ts`.
- `TODO.md:44,47,49,62,66,132` — `src/runtime/docker_profile.rs`, `src/runtime/launch.rs`, `src/instance/auth.rs`
  (actual: `crates/jackin-runtime/src/runtime/...`, `crates/jackin-instance/src/auth.rs`).
- `TODO.md:49,58,66` — claims `TODO(docker-security-profile-{default,sudo-audit,rootless-dind})` markers in
  code; repo-wide grep finds **none** (only `TODO(apple-container)` and `TODO(launch-worktree-leak-on-sidecar-fail)` exist).

## Scope

**In scope:** `TODO.md` only. **Out of scope:** the roadmap MDX content; adding the missing code markers to
source (that's a separate call — see Step 3 options).

## Steps

### Step 1: Delete the stray note and fix the doc paths

- Delete `TODO.md:1-2` (the stray automation note above `# TODO`).
- Rewrite every `docs/src/content/docs/...` path to its real `docs/content/docs/...` route-group location
  (use the mapping in "Current state"). Replace `docs/astro.config.ts` sidebar references with the `meta.json`
  + `cargo xtask roadmap audit` procedure.
- Repoint the `src/...` code paths to their `crates/jackin-*/src/...` locations.

**Verify**: `grep -n "docs/src/content\|astro.config.ts" TODO.md` → **no matches**;
`grep -n "src/runtime/\|src/instance/" TODO.md` → no bare `src/` paths (all now `crates/...`);
for each rewritten doc path, `test -e <path>` → exists.

### Step 2: Resolve the phantom marker claims

The three `TODO(docker-security-profile-*)` "Marker:" lines claim code markers that don't exist. Pick one:
- **(A)** Add the markers to the code they describe (`crates/jackin-runtime/src/runtime/docker_profile.rs`,
  `crates/jackin-runtime/src/runtime/launch.rs`, `docker/construct/Dockerfile` near line 113) so the doc's
  `grep -rn 'TODO(<topic>)' .` convention finds both ends — **only if** those follow-ups are still live.
- **(B)** Remove the "Marker:" lines from `TODO.md` if the code-marker convention isn't being kept for these.

Recommend **(A)** for the still-open security-profile items (they're real, tracked in plan 043), since the
convention's whole point is a single grep finding both ends. **Note:** adding markers touches source — if
you take (A), that source edit is in scope *only* for adding the exact `// TODO(<topic>): …` comment lines,
nothing else.

**Verify (if A)**: `grep -rn "TODO(docker-security-profile-default)" crates docker` → 1 match (both the doc
and the code marker now exist).

### Step 3: (optional) note the freshness-checker gap

`TODO.md` itself isn't link-checked (that's plan 038). Add a one-line pointer so a reader knows the fix and
the checker-coverage fix (038) go together.

## Done criteria

- [x] Stray note (lines 1-2) removed
- [x] All `docs/src/content`/`astro.config.ts` references rewritten to real Fumadocs paths + `meta.json` procedure
- [x] Bare `src/...` code paths repointed to `crates/jackin-*/src/...`
- [x] Phantom marker claims resolved (markers added to code, or lines removed)
- [x] Every doc path referenced in `TODO.md` resolves (`test -e`)
- [x] `plans/README.md` row updated

## Completion notes

- Removed the stray automation note above the `# TODO` heading.
- Rewrote stale Astro/Starlight paths to the current Fumadocs `docs/content/docs/...` layout and replaced `docs/astro.config.ts` sidebar guidance with `meta.json` plus `cargo xtask roadmap audit`.
- Updated stale monolith source paths to current crate paths, including the stale-docs checklist and diff helper.
- Removed already-completed sudo-audit and rootless-DinD follow-ups; `NOPASSWD:ALL` is gone and rootless DinD already maps to `docker:dind-rootless` with cgroup validation.
- Kept the still-live `compat` to `standard` default flip follow-up and added the matching `TODO(docker-security-profile-default)` marker at the current enum default.

## STOP conditions

- A rewritten doc path still doesn't exist under `docs/content/docs/` (the page was renamed/removed, not
  just moved) — report the specific page; it may be genuinely gone (a deeper docs gap).
- Taking option (A) for markers reveals a security-profile follow-up is already **done** (not still open) —
  then remove the `TODO.md` entry entirely instead of adding a marker.

## Maintenance notes

- Root cause: `TODO.md` is excluded from the repo-link checker (plan 038). Land 038 with or right after this,
  or the paths rot again.
- Reviewer: this doc steers every PR — confirm the rewritten stale-docs checklist actually points at files an
  agent can open.
