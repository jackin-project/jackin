Inline-code multisets actually match across the flagged region — meaning the validator broke on inline-code **context** (prose around the code paths got compressed). The flagged fragments span from the lychee "Done when" line through the end (Roadmap + Stale-docs). Per the fix protocol, I'll restore that tail to exact original content. Everything above (which validated clean) stays compressed.

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

- **What:** `DockerSecurityProfile::Compat` is the current `Default::default()` in [`src/runtime/docker_profile.rs`](src/runtime/docker_profile.rs). Once the base-image sudo audit below resolves, change the default to `Standard` and flip `profile_base_grants(Standard).sudo` from `true` → `false`. Also enable `--security-opt no-new-privileges` for the `standard` profile (currently schema-present but not enforced due to the sudo blocker).
- **Why:** `compat` was made the compiled-in default because the sudo audit is unresolved. After the audit, every new launch gets `standard` behavior (resource limits, DinD with resource caps, `no-new-privileges`) without any operator action. `compat` remains a valid profile — it just requires explicit opt-in instead of being the default.
- **Blocked by:** `TODO(docker-security-profile-sudo-audit)` below.
- **Code change:** one line in `impl Default for DockerSecurityProfile` in `src/runtime/docker_profile.rs`; one field change in `profile_base_grants(Standard)` (`sudo: false`); remove the "deferred pending sudo audit" note from the `no_new_privileges` block in `launch_role_runtime`.
- **Done when:** `NOPASSWD:ALL` is removed from the base image, the sudo audit entry below is resolved, and the two-line code change is made. Update the profile defaults table in `docs/content/docs/reference/roadmap/docker-runtime-hardening-contract.mdx` and `docs/content/docs/guides/docker-profiles.mdx`.
- **Marker:** `TODO(docker-security-profile-default)` — in `src/runtime/docker_profile.rs`, `impl Default for DockerSecurityProfile`.

### Docker security profile — audit `NOPASSWD:ALL` sudo in base image

- **What:** [`docker/construct/Dockerfile`](docker/construct/Dockerfile) at line 113 grants `agent ALL=(ALL) NOPASSWD:ALL`. This is incompatible with `--security-opt no-new-privileges` (sudo needs setuid-root escalation; `no-new-privileges` blocks it at the kernel level). Audit every privileged operation the base image and built-in agent images call at runtime via `sudo`, and replace each with a file capability (`setcap`) on the specific binary, or restructure so the operation runs before the `USER agent` switch.
- **Why:** until this is resolved, `standard` profile cannot enable `no-new-privileges` (it would silently break `sudo apt install` and any agent script that calls `sudo`). This blocks `standard` from becoming the default and blocks the full privilege dimension of the hardening contract.
- **Findings so far (2026-06-04):** zero `sudo` calls in jackin'-controlled runtime code (`entrypoint.sh`, `jackin-capsule runtime-setup`). The `NOPASSWD:ALL` entry exists for role-authored hook scripts and agent binaries (e.g., `apt install` during `setup-once.sh`). The network allowlist firewall init (`init-firewall.sh`) runs via `docker exec --user root` from the host — it does NOT call sudo inside the container and does not conflict.
- **Resolution path:** (a) Replace `NOPASSWD:ALL` with a scoped sudoers entry for each privileged binary the base image ships (if any). (b) For roles that call `sudo` in hooks, role authors must either declare `min_profile = "standard"` (keeping full sudo) or replace sudo calls with file capabilities in their Dockerfile. (c) Update `profile_base_grants(Standard)` and the default to flip after this lands.
- **Done when:** `NOPASSWD:ALL` is removed from `Dockerfile:113`, every built-in agent runtime (`the-architect`, `agent-smith`) passes the compatibility test matrix under `standard` profile with `no-new-privileges` enforced, and the `TODO(docker-security-profile-default)` entry above is executed.
- **Marker:** `TODO(docker-security-profile-sudo-audit)` — add to `docker/construct/Dockerfile` near line 113.

### Docker security profile — rootless DinD requires cgroup v2 confirmation

- **What:** The DinD sidecar start in `src/runtime/launch.rs` has a `TODO(docker-security-profile-rootless-dind)` marker on the block that starts `docker:dind`. When `grants.dind == DindGrant::Rootless`, the image should switch to `docker:dind-rootless` and drop `--privileged`. This requires: (a) detecting cgroup v2 on the host, (b) confirming the rootless image works with Testcontainers/Compose on cgroup v2, (c) updating the DinD start block to use `docker:dind-rootless` and remove `--privileged`.
- **Why:** `standard` profile uses `DindGrant::Rootless` as its DinD default on cgroup v2 hosts, but the code currently falls back to `docker:dind --privileged` regardless (the TODO is in place). Rootless DinD reduces the blast radius of a sidecar compromise.
- **Blocked by:** compatibility test matrix (Testcontainers, Compose, BuildKit on rootless) — see [Docker Runtime Hardening Contract](/reference/roadmap/docker-runtime-hardening-contract/) Phase B.
- **Done when:** `grants.dind == Rootless` uses `docker:dind-rootless` + no `--privileged` on cgroup v2 hosts; falls back to `docker:dind --privileged` on cgroup v1 with a session-contract note; compatibility matrix passes.
- **Marker:** `TODO(docker-security-profile-rootless-dind)` — in `src/runtime/launch.rs` near the DinD sidecar start block.

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

- **What:** in [`launch_pipeline.rs`](crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs), the `tokio::join!(sidecar_wait, materialize_wait)` runs workspace materialization to completion even when the DinD sidecar has already failed. On the sidecar-failure path the launch marks the instance `FailedSetup` and runs `LoadCleanup` (container/dind/volume/network/socket dir) but does **not** unstage the host-side `git worktree` that `materialize_workspace` may have just added, so a worktree-isolated mount leaves a staged worktree behind.
- **Why:** before this PR materialization ran *after* sidecar success (serial), so a sidecar failure never staged a worktree. The overlap optimization made the failure path stage one. Severity is low: the staged worktree is tied to the recorded `FailedSetup` instance and is reaped when the operator ejects/purges it (`cleanup::eject_role` / `purge_container_filesystem`), so it is deferred cleanup rather than a true orphan. The proper fix (track the materialized worktree and unstage it in `LoadCleanup`, or short-circuit materialization when the sidecar has already errored) needs the materialize-cleanup model, so it is deferred rather than rushed into the launch hot path.
- **Last verified:** 2026-06-21 — present on `chore/launch-speed-roadmap`; B4/S1 cleanup fixes landed without it.
- **Done when:** a sidecar-startup failure on a worktree-isolated workspace leaves no staged `git worktree` behind (either `LoadCleanup` unstages it, or materialization is not run once the sidecar future has resolved to `Err`), covered by a regression test. Remove this entry and the `TODO(launch-worktree-leak-on-sidecar-fail)` marker.

## Roadmap

Roadmap items — open work and resolved design docs — live in docs site, not this repo. See:

- Overview: [`docs/src/content/docs/reference/roadmap.mdx`](docs/src/content/docs/reference/roadmap.mdx)
- Per-item design docs: [`docs/src/content/docs/reference/roadmap/`](docs/src/content/docs/reference/roadmap/)
- Browsable: <https://jackin.tailrocks.com/reference/roadmap/>

To add an item, create an MDX page under that directory and add a sidebar entry in [`docs/astro.config.ts`](docs/astro.config.ts) under `Roadmap → Open items`. Whenever you add, rename, delete, or change an item's `**Status**` (Open ↔ Resolved), update the sidebar in same PR — directory and sidebar must stay in sync. Operators discover open work through the sidebar; an item reachable only via overview page or direct URL is effectively hidden. See `docs/AGENTS.md` → "Content Notes" for the audit command diffing directory against sidebar.

Each design doc should include (see any existing page as template):

- `**Status**: Open | Deferred | Resolved`
- `## Problem`
- `## Why It Matters`
- `## Related Files`

Roadmap vs. follow-up: needs a problem statement and design discussion → roadmap item. "Swap a SHA when upstream releases" or "rename three callers for consistency" → follow-up.

## Stale-docs check (every PR)

Docs rot silently. Every PR must include a one-pass verification structure-sensitive docs still match reality. Treat as a checklist in the PR description — each item takes seconds.

### When your PR touches `src/**`

- [ ] Did you add, rename, move, or delete a module / directory under `src/`? If yes, update [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md)'s "Module tree" and any affected row in "Code ↔ Docs Cross-Reference" in same PR.
- [ ] Did you add a new `src/bin/` binary? If yes, add it to "Crate root" table in `PROJECT_STRUCTURE.md`.

### When your PR touches CLI behavior

- [ ] Did you add, rename, or remove a CLI flag, subcommand, or change default behavior? If yes, matching `docs/src/content/docs/commands/<cmd>.mdx` needs updating in same PR.
- [ ] Did you change `jackin.role.toml` schema or validation rules? If yes, update `docs/src/content/docs/developing/role-manifest.mdx`.
- [ ] Did you change `config.toml` shape? If yes, update `docs/src/content/docs/reference/configuration.mdx`.
- [ ] Did you change auth-forward, Keychain, symlink, or file-permission behavior in `src/instance/auth.rs`? If yes, update `docs/src/content/docs/guides/authentication.mdx` and `docs/src/content/docs/guides/security-model.mdx`.

### When your PR touches a roadmap item

- [ ] If the PR resolves or advances an item under `docs/src/content/docs/reference/roadmap/`, update that item's `Status` field (`Open | Deferred | Resolved`) and `Related Files` section in same PR.
- [ ] If the PR references `src/` paths that have since moved (e.g., a roadmap doc mentions `src/runtime.rs` now `src/runtime/`), fix those path references.
- [ ] If the PR adds, renames, deletes, or moves a roadmap MDX file between status sections, update [`docs/astro.config.ts`](docs/astro.config.ts) so `Reference → Roadmap` (Open / Resolved / Codebase health) matches the directory. Run the audit command in `docs/AGENTS.md` → "Content Notes" → "Roadmap sidebar discipline" to confirm the diff is empty.
- [ ] If the PR adds a new roadmap item, or changes any item's `**Status**` (e.g. Open → Resolved, Open → Deferred, Open → Partially implemented), update [`docs/src/content/docs/reference/roadmap.mdx`](docs/src/content/docs/reference/roadmap.mdx) so the item lands in the correct section (Completed / Partially implemented / Planned with the right `(status: …)` suffix). Run the overview audit command in `docs/AGENTS.md` → "Content Notes" → "Roadmap overview discipline" to confirm no items are missing.

### How to verify

One command to surface obvious drift targets:

```sh
git diff --name-only origin/main... | grep -E '^src/|^Cargo\.toml' | head
```

If that list is non-empty, walk the checkboxes above before requesting review. Goal: a new operator opening `PROJECT_STRUCTURE.md` or a roadmap doc always sees paths that resolve, commands that exist, behaviors matching current code.
