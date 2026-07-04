# TODO

Two kinds of work:

- **[Follow-ups](#follow-ups)** — small items, verify or address periodically. External deps waiting on upstream fixes; internal polish too small for roadmap doc.
- **[Stale-docs check](#stale-docs-check-every-pr)** — per-PR checklist, keep structure-sensitive docs in sync with code.

Bigger feature work and design proposals live in [docs roadmap](#roadmap) — separate place, see below.

## Follow-ups

Small, concrete, verifiable items. Each entry = heading with stable anchor so code-level `TODO(<topic>)` markers link back. Walk list periodically (monthly; on demand otherwise), update **Last verified**, act when **Done when** satisfied.

### Code-level TODO marker convention

When code (or config) has follow-up tracked here, leave marker in source at spot:

```text
// TODO(<topic>): one-line summary — see TODO.md "Follow-ups" → "<heading>"
```

`<topic>` = same kebab-case slug used as heading anchor below, so single grep finds both ends:

```sh
grep -rn 'TODO(<topic>)' .
```

Markers without TODO.md entry OK for transient in-flight work, but anything outliving single PR should have tracked entry so no rot. Item resolves → remove entry and matching `TODO(<topic>)` markers in same PR.

### External dependencies

#### `shellfirm-aarch64-linux-binary` — switch to prebuilt download once upstream ships aarch64-linux artifact

- **What:** once upstream ships Linux arm64 assets, replace the CI-built `docker/construct/prebuilt/shellfirm` staging path with a direct Dockerfile download of `shellfirm-vX.Y.Z-aarch64-linux.tar.xz`, mirroring the tirith install pattern.
- **Why:** construct image built multi-arch (`linux/amd64` + `linux/arm64`). shellfirm currently ships only `x86_64-linux` (plus macOS/Windows) prebuilt, so arm64 variant compiles shellfirm + full dep graph from source on every layer-cache miss, dominating arm64 build time. tirith already moved to prebuilt download since its upstream publishes both Linux arches; shellfirm last blocker preventing full removal of rust toolchain stage from construct image.
- **Tracking:** <https://github.com/kaplanelad/shellfirm/issues/179> — upstream issue requesting existing-but-commented-out `aarch64-linux` matrix entry in [`release.yml`](https://github.com/kaplanelad/shellfirm/blob/main/.github/workflows/release.yml) be re-enabled.
- **Last verified:** 2026-06-22 — checked v0.3.10 release assets; only `x86_64-linux.tar.xz` ships for Linux. PR #632 now builds/restores the pinned binary outside Docker and copies it from `docker/construct/prebuilt/shellfirm`.
- **Done when:** shellfirm release at or after fix publishes `shellfirm-v<ver>-aarch64-linux.tar.xz` (or equivalent name) alongside existing x86_64 tarball. Replace the CI prebuild/staging step with TARGETARCH-aware curl + `tar -xJ` in the Dockerfile (mirror tirith pattern), then remove this TODO.

### Docker security profile — flip default from `compat` to `standard`

- **What:** `DockerSecurityProfile::Compat` is still the compiled-in default in [`crates/jackin-core/src/docker_security.rs`](crates/jackin-core/src/docker_security.rs). Flip the enum default to `Standard` after the WP0 compatibility matrix and changelog call-out are complete.
- **Why:** `standard` now carries the intended baseline behavior (resource limits, no sudo by default, DinD disabled unless explicitly granted, `no-new-privileges` when sudo is off), but `compat` remains the default until the compatibility evidence is complete.
- **Code change:** move the `#[default]` attribute from `Compat` to `Standard`.
- **Done when:** the compatibility matrix passes for built-in roles under `standard`, the changelog calls out the breaking default-profile change, the default enum variant is flipped, and the profile defaults table in the Docker runtime hardening contract plus the Docker profiles guide are updated.
- **Marker:** `TODO(docker-security-profile-default)` — in `crates/jackin-core/src/docker_security.rs`, `DockerSecurityProfile::Compat`.

### Internal cleanups

#### `lychee-no-files-warn` — investigate "No files found for this input source" in deploy link check

- **What:** deploy job's `Check deployed docs links` step in [`.github/workflows/docs.yml`](.github/workflows/docs.yml) emits one-line `[WARN] [Full Github Actions output]: No files found for this input source` from lychee binary, then continues and reports `Total 4703 / Successful 4703 / Errors 0`. Identify which of 46 sitemap input URLs triggers warn; fix cause or filter warn so signal clean.
- **Why:** warn means at least one of 46 deployed pages fed via `--files-from lychee/deployed-pages.txt` resolved to zero extractable links. Tolerated now since rest of run green, but if future regression makes 5 inputs silently skip, no notice — warn count only tells. Clean run = real signal every deployed page actually scanned.
- **Tracking:**
  - First observed in [run 24940918362](https://github.com/jackin-project/jackin/actions/runs/24940918362) on `main` after [`34bb396`](https://github.com/jackin-project/jackin/commit/34bb396) ([#176](https://github.com/jackin-project/jackin/pull/176) merge).
  - Warn string emitted by lychee binary (`strings lychee | grep "No files found"` confirms in v0.24.1), not lychee-action wrapper.
  - lychee source — search literal string in <https://github.com/lycheeverse/lychee> to find emitter and exact condition.
- **Last verified:** 2026-04-25 — present on every `main` push since #176 merged.
- **Hypotheses to check (in order):**
  1. **Redirected page returns non-HTML.** Same run reports 9 redirects. One redirected URL might land on page lychee can't extract from (e.g., raw text, unusual content-type).
  2. **Sitemap entry yielding zero anchors.** Some Starlight pages — landing-style or auto-generated — render with no `<a href>` in body. Identify by running `curl <url> | grep -c '<a href' ` for each of 46 URLs, find the zero.
  3. **Spurious empty argument.** If shell `run:` command produces extra empty token on arg expansion, lychee treats as empty input source and warns.
- **How to reproduce:**
  ```sh
  curl -fsSL https://jackin.tailrocks.com/sitemap-0.xml \
    | grep -oE '<loc>[^<]+</loc>' | sed 's|<loc>||; s|</loc>||' > /tmp/pages.txt
  lychee --verbose --files-from /tmp/pages.txt 2>&1 | grep -B1 -A1 "No files found"
  ```
  Verbose output names the input source triggering the warn.
- **Done when:** either (a) warn no longer emitted on a clean main run, or (b) it is, but cause is documented as benign (e.g., one Starlight page renders without anchors by design) and warn is suppressed/filtered so it doesn't mask future genuine warnings. Case (a): remove this entry; case (b): replace with a one-line note in `docs.yml`.

#### `launch-worktree-leak-on-sidecar-fail` — unstage the staged worktree when sidecar startup fails

- **What:** in the `launch_pipeline.rs` module under the `crates/jackin-runtime` runtime launch tree, the `tokio::join!(sidecar_wait, materialize_wait)` runs workspace materialization to completion even when the DinD sidecar has already failed. On the sidecar-failure path the launch marks the instance `FailedSetup` and runs `LoadCleanup` (container/dind/volume/network/socket dir) but does **not** unstage the host-side `git worktree` that `materialize_workspace` may have just added, so a worktree-isolated mount leaves a staged worktree behind.
- **Why:** before this PR materialization ran *after* sidecar success (serial), so a sidecar failure never staged a worktree. The overlap optimization made the failure path stage one. Severity is low: the staged worktree is tied to the recorded `FailedSetup` instance and is reaped when the operator ejects/purges it (`cleanup::eject_role` / `purge_container_filesystem`), so it is deferred cleanup rather than a true orphan. The proper fix (track the materialized worktree and unstage it in `LoadCleanup`, or short-circuit materialization when the sidecar has already errored) needs the materialize-cleanup model, so it is deferred rather than rushed into the launch hot path.
- **Last verified:** 2026-06-21 — present on `chore/launch-speed-roadmap`; B4/S1 cleanup fixes landed without it.
- **Done when:** a sidecar-startup failure on a worktree-isolated workspace leaves no staged `git worktree` behind (either `LoadCleanup` unstages it, or materialization is not run once the sidecar future has resolved to `Err`), covered by a regression test. Remove this entry and the `TODO(launch-worktree-leak-on-sidecar-fail)` marker.

## Roadmap

Roadmap items — open work and resolved design docs — live in docs site. See:

- Overview: [`docs/content/docs/roadmap/index.mdx`](docs/content/docs/roadmap/index.mdx)
- Per-item design docs: [`docs/content/docs/roadmap/`](docs/content/docs/roadmap/)
- Browsable: <https://jackin.tailrocks.com/reference/roadmap/>

To add an item, create an MDX page under that directory and update the appropriate `meta.json` sidebar file under [`docs/content/docs/roadmap/`](docs/content/docs/roadmap/). Whenever you add, rename, delete, or change an item's `**Status**` (Open ↔ Resolved), update the sidebar in same PR — directory and sidebar must stay in sync. Operators discover open work through the sidebar; an item reachable only via overview page or direct URL is effectively hidden. Run `cargo xtask roadmap audit` to confirm the roadmap overview and sidebar metadata match the file tree.

Each design doc should include (see any existing page as template):

- `**Status**: Open | Deferred | Resolved`
- `## Problem`
- `## Why It Matters`
- `## Related Files`

Roadmap vs. follow-up: needs a problem statement and design discussion → roadmap item. "Swap a SHA when upstream releases" or "rename three callers for consistency" → follow-up.

## Stale-docs check (every PR)

Docs rot silently. Every PR must include a one-pass verification structure-sensitive docs still match reality. Treat as a checklist in the PR description — each item takes seconds.

### When your PR touches `crates/**/src/**`

- [ ] Did you add, rename, move, or delete a module / directory under `crates/**/src/`? If yes, update [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md)'s "Module tree" and any affected row in "Code ↔ Docs Cross-Reference" in same PR.
- [ ] Did you add a new `crates/*/src/bin/` binary? If yes, add it to "Crate root" table in `PROJECT_STRUCTURE.md`.

### When your PR touches CLI behavior

- [ ] Did you add, rename, or remove a CLI flag, subcommand, or change default behavior? If yes, matching `docs/content/docs/(public)/commands/<cmd>.mdx` needs updating in same PR.
- [ ] Did you change `jackin.role.toml` schema or validation rules? If yes, update [`docs/content/docs/(public)/(role-authoring)/developing/role-manifest.mdx`](<docs/content/docs/(public)/(role-authoring)/developing/role-manifest.mdx>).
- [ ] Did you change `config.toml` shape? If yes, update [`docs/content/docs/reference/runtime/configuration.mdx`](docs/content/docs/reference/runtime/configuration.mdx).
- [ ] Did you change auth-forward, Keychain, symlink, or file-permission behavior in `crates/jackin-instance/src/auth.rs`? If yes, update [`docs/content/docs/(public)/guides/authentication/index.mdx`](<docs/content/docs/(public)/guides/authentication/index.mdx>) and [`docs/content/docs/(public)/guides/security-model.mdx`](<docs/content/docs/(public)/guides/security-model.mdx>).

### When your PR touches a roadmap item

- [ ] If the PR resolves or advances an item under `docs/content/docs/roadmap/`, update that item's `Status` field (`Open | Deferred | Resolved`) and `Related Files` section in same PR.
- [ ] If the PR references source paths that have since moved (e.g., a roadmap doc mentions the old monolith runtime path and the code now lives in the `crates/jackin-runtime` runtime module tree), fix those path references.
- [ ] If the PR adds, renames, deletes, or moves a roadmap MDX file between status sections, update the matching `meta.json` file under [`docs/content/docs/roadmap/`](docs/content/docs/roadmap/) so the roadmap sidebar matches the directory. Run `cargo xtask roadmap audit` to confirm the sidebar is in sync.
- [ ] If the PR adds a new roadmap item, or changes any item's `**Status**` (e.g. Open → Resolved, Open → Deferred, Open → Partially implemented), update [`docs/content/docs/roadmap/index.mdx`](docs/content/docs/roadmap/index.mdx) so the item lands in the correct section (Completed / Partially implemented / Planned with the right `(status: …)` suffix). Run `cargo xtask roadmap audit` to confirm no items are missing.

### How to verify

One command to surface obvious drift targets:

```sh
git diff --name-only origin/main... | grep -E '^crates/.*/src/|^Cargo\.toml' | head
```

If that list is non-empty, walk the checkboxes above before requesting review. Goal: a new operator opening `PROJECT_STRUCTURE.md` or a roadmap doc always sees paths that resolve, commands that exist, behaviors matching current code.
