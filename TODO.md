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
- **Last verified:** 2026-05-01 — latest `lycheeverse/lychee-action` tag is still `v2.8.0`; `faea714` remains current `master` HEAD; pin introduced in [#176](https://github.com/jackin-project/jackin/pull/176). `LYCHEE_VERSION` independently bumped to `v0.24.2` ([release notes](https://github.com/lycheeverse/lychee/releases/tag/lychee-v0.24.2)) — tarball layout unchanged from v0.24.1 (PR [#2165](https://github.com/lycheeverse/lychee/pull/2165) is binstall-metadata only), so the post-v2.8.0 master SHA is still required.
- **Done when:** a tag at or after `faea714` ships. Replace the SHA in `docs.yml` with that tag's commit SHA, update the inline comment from "post-v2.8.0 master" to the tag name, and re-confirm `LYCHEE_VERSION` matches whatever the new release defaults to (or keep the explicit pin if newer).

#### `shellfirm-aarch64-linux-binary` — switch to prebuilt download once upstream ships aarch64-linux artifact

- **What:** in [`docker/construct/Dockerfile`](docker/construct/Dockerfile), drop the `cargo install shellfirm` step (and the multi-stage `rust:1.95.0-trixie` `security-tools` builder it lives in) in favor of downloading a prebuilt `shellfirm-vX.Y.Z-aarch64-linux.tar.xz` artifact, mirroring the tirith install pattern already in place.
- **Why:** the construct image is built multi-arch (`linux/amd64` + `linux/arm64`). shellfirm currently only ships `x86_64-linux` (and macOS/Windows) prebuilt binaries, so the arm64 variant must compile shellfirm and its full dependency graph from source on every layer-cache miss, dominating the arm64 build time. tirith already moved to prebuilt download because its upstream publishes both Linux arches; shellfirm is the last blocker preventing us from removing the rust toolchain stage from the construct image entirely.
- **Tracking:** <https://github.com/kaplanelad/shellfirm/issues/179> — upstream issue requesting that the existing-but-commented-out `aarch64-linux` matrix entry in [`release.yml`](https://github.com/kaplanelad/shellfirm/blob/main/.github/workflows/release.yml) be re-enabled.
- **Last verified:** 2026-05-04 — checked v0.3.5 through v0.3.9 release assets; only `x86_64-linux.tar.xz` ships for Linux. Filed upstream issue #179 same day.
- **Done when:** a shellfirm release at or after the fix publishes `shellfirm-v<ver>-aarch64-linux.tar.xz` (or equivalently named) alongside the existing x86_64 tarball. Replace the cargo install step with a TARGETARCH-aware curl + `tar -xJ` block (mirroring the tirith pattern), drop the `security-tools` stage and the `FROM rust:...` line, remove the `COPY --from=security-tools` for shellfirm, and remove the `TODO(shellfirm-aarch64-linux-binary)` marker in the Dockerfile.

### Internal cleanups

#### `lychee-no-files-warn` — investigate "No files found for this input source" in deploy link check

- **What:** the deploy job's `Check deployed docs links` step in [`.github/workflows/docs.yml`](.github/workflows/docs.yml) emits a one-line `[WARN] [Full Github Actions output]: No files found for this input source` from the lychee binary, then continues and reports `Total 4703 / Successful 4703 / Errors 0`. Identify which of the 46 sitemap input URLs triggered the warn and either fix the cause or filter the warn so the signal is clean.
- **Why:** the warn means at least one of the 46 deployed pages we feed via `--files-from lychee/deployed-pages.txt` resolved to zero extractable links. Right now we tolerate it because the rest of the run is green, but if a future regression causes 5 inputs to silently skip, we wouldn't notice — the warn count is the only tell. A clean run gives us a real signal that every deployed page was actually scanned.
- **Tracking:**
  - First observed in [run 24940918362](https://github.com/jackin-project/jackin/actions/runs/24940918362) on `main` after [`34bb396`](https://github.com/jackin-project/jackin/commit/34bb396) ([#176](https://github.com/jackin-project/jackin/pull/176) merge).
  - Warn string is emitted by the lychee binary (`strings lychee | grep "No files found"` confirms in v0.24.1), not the lychee-action wrapper.
  - lychee source — search for the literal string in <https://github.com/lycheeverse/lychee> to find the emitter and the exact condition.
- **Last verified:** 2026-04-25 — present on every `main` push since #176 merged.
- **Hypotheses to check (in order):**
  1. **Redirected page returns non-HTML.** The same run reports 9 redirects. One redirected URL might land on a page lychee can't extract from (e.g., raw text, unusual content-type).
  2. **Sitemap entry that yields zero anchors.** Some Starlight pages — landing-style or auto-generated — render with no `<a href>` in body content. Identify by running `curl <url> | grep -c '<a href' ` for each of the 46 URLs and finding the one with zero.
  3. **Spurious empty arg in the `eval`-ed command.** lychee-action's entrypoint uses `eval lychee … ${ARGS}` (unquoted). If our YAML folded scalar produces an extra empty token, lychee would treat it as an empty input source and warn.
- **How to reproduce:**
  ```sh
  curl -fsSL https://jackin.tailrocks.com/sitemap-0.xml \
    | grep -oE '<loc>[^<]+</loc>' | sed 's|<loc>||; s|</loc>||' > /tmp/pages.txt
  lychee --verbose --files-from /tmp/pages.txt 2>&1 | grep -B1 -A1 "No files found"
  ```
  The verbose output names the input source that triggered the warn.
- **Done when:** either (a) the warn is no longer emitted on a clean main run, or (b) it is, but the cause is documented as benign (e.g., one Starlight page renders without anchors by design) and the warn is suppressed/filtered so it doesn't mask future genuine warnings. In case (a) remove this entry; in case (b) replace it with a one-line note in `docs.yml`.

## Roadmap

Roadmap items — open work and resolved design docs — live in the docs site, not in this repo. See:

- Overview: [`docs/src/content/docs/reference/roadmap.mdx`](docs/src/content/docs/reference/roadmap.mdx)
- Per-item design docs: [`docs/src/content/docs/reference/roadmap/`](docs/src/content/docs/reference/roadmap/)
- Browsable: <https://jackin.tailrocks.com/reference/roadmap/>

To add a new item, create an MDX page under the directory above and add a sidebar entry in [`docs/astro.config.ts`](docs/astro.config.ts) under `Roadmap → Open items`. Whenever you add, rename, delete, or change a roadmap item's `**Status**` (Open ↔ Resolved), update the sidebar in the same PR — the directory and the sidebar must stay in sync. Operators discover open work through the sidebar; an item reachable only via the overview page or direct URL is effectively hidden. See `docs/AGENTS.md` → "Content Notes" for the audit command that diffs the directory against the sidebar.

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
- [ ] Did you change `jackin.role.toml` schema or validation rules? If yes, update `docs/src/content/docs/developing/role-manifest.mdx`.
- [ ] Did you change `config.toml` shape? If yes, update `docs/src/content/docs/reference/configuration.mdx`.
- [ ] Did you change auth-forward, Keychain, symlink, or file-permission behavior in `src/instance/auth.rs`? If yes, update `docs/src/content/docs/guides/authentication.mdx` and `docs/src/content/docs/guides/security-model.mdx`.

### When your PR touches a roadmap item

- [ ] If the PR resolves or advances an item under `docs/src/content/docs/reference/roadmap/`, update that item's `Status` field (`Open | Deferred | Resolved`) and `Related Files` section in the same PR.
- [ ] If the PR references `src/` paths that have since moved (e.g., a roadmap doc mentions `src/runtime.rs` which is now `src/runtime/`), fix those path references.
- [ ] If the PR adds, renames, deletes, or moves a roadmap MDX file between status sections, update [`docs/astro.config.ts`](docs/astro.config.ts) so `Reference → Roadmap` (Open / Resolved / Codebase health) matches the directory. Run the audit command in `docs/AGENTS.md` → "Content Notes" → "Roadmap sidebar discipline" to confirm the diff is empty.

### How to verify

One command to surface the obvious drift targets:

```sh
git diff --name-only origin/main... | grep -E '^src/|^Cargo\.toml' | head
```

If that list is non-empty, walk through the checkboxes above before requesting review. The goal is that a new operator opening `PROJECT_STRUCTURE.md` or a roadmap doc always sees paths that resolve, commands that exist, and behaviors that match current code.
