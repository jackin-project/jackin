# TODO

Two kinds of work:

- **[Follow-ups](#follow-ups)** — small items to verify or address periodically. External deps waiting on upstream fixes; internal consistency/polish too small for a roadmap doc.
- **[Stale-docs check](#stale-docs-check-every-pr)** — per-PR checklist keeping structure-sensitive docs in sync with code.

Bigger feature work and design proposals live in [docs roadmap](#roadmap) — separate place, see below.

## Follow-ups

Small, concrete, verifiable items. Each entry is a heading with stable anchor so code-level `TODO(<topic>)` markers link back. Walk this list periodically (monthly cadence; on demand otherwise), update **Last verified**, act when **Done when** satisfied.

### Code-level TODO marker convention

When code (or config) has a follow-up tracked here, leave a marker in source at the spot:

```text
// TODO(<topic>): one-line summary — see TODO.md "Follow-ups" → "<heading>"
```

`<topic>` is the same kebab-case slug used as heading anchor below, so single grep finds both ends:

```sh
grep -rn 'TODO(<topic>)' .
```

Markers without a TODO.md entry allowed for transient in-flight work, but anything outliving a single PR should have a tracked entry so it doesn't rot. When item resolves, remove both the entry and matching `TODO(<topic>)` markers in same PR.

### External dependencies

#### `shellfirm-aarch64-linux-binary` — switch to prebuilt download once upstream ships aarch64-linux artifact

- **What:** in [`docker/construct/Dockerfile`](docker/construct/Dockerfile), drop the `cargo install shellfirm` step (and the multi-stage `rust:1.96.0-trixie` `security-tools` builder it lives in) for downloading a prebuilt `shellfirm-vX.Y.Z-aarch64-linux.tar.xz` artifact, mirroring the tirith install pattern.
- **Why:** construct image is built multi-arch (`linux/amd64` + `linux/arm64`). shellfirm currently only ships `x86_64-linux` (and macOS/Windows) prebuilt, so arm64 variant compiles shellfirm and full dependency graph from source on every layer-cache miss, dominating arm64 build time. tirith already moved to prebuilt download since its upstream publishes both Linux arches; shellfirm is last blocker preventing removing the rust toolchain stage from construct image entirely.
- **Tracking:** <https://github.com/kaplanelad/shellfirm/issues/179> — upstream issue requesting the existing-but-commented-out `aarch64-linux` matrix entry in [`release.yml`](https://github.com/kaplanelad/shellfirm/blob/main/.github/workflows/release.yml) be re-enabled.
- **Last verified:** 2026-05-04 — checked v0.3.5 through v0.3.9 release assets; only `x86_64-linux.tar.xz` ships for Linux. Filed upstream issue #179 same day.
- **Done when:** a shellfirm release at or after the fix publishes `shellfirm-v<ver>-aarch64-linux.tar.xz` (or equivalently named) alongside the existing x86_64 tarball. Replace cargo install step with TARGETARCH-aware curl + `tar -xJ` block (mirroring tirith pattern), drop `security-tools` stage and `FROM rust:...` line, remove `COPY --from=security-tools` for shellfirm, remove `TODO(shellfirm-aarch64-linux-binary)` marker in Dockerfile.

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

- **What:** deploy job's `Check deployed docs links` step in [`.github/workflows/docs.yml`](.github/workflows/docs.yml) emits a one-line `[WARN] [Full Github Actions output]: No files found for this input source` from the lychee binary, then continues and reports `Total 4703 / Successful 4703 / Errors 0`. Identify which of the 46 sitemap input URLs triggers the warn and either fix the cause or filter the warn so signal is clean.
- **Why:** warn means at least one of the 46 deployed pages fed via `--files-from lychee/deployed-pages.txt` resolved to zero extractable links. Tolerated now because rest of run is green, but if a future regression causes 5 inputs to silently skip we wouldn't notice — warn count is the only tell. Clean run gives a real signal every deployed page was actually scanned.
- **Tracking:**
  - First observed in [run 24940918362](https://github.com/jackin-project/jackin/actions/runs/24940918362) on `main` after [`34bb396`](https://github.com/jackin-project/jackin/commit/34bb396) ([#176](https://github.com/jackin-project/jackin/pull/176) merge).
  - Warn string emitted by lychee binary (`strings lychee | grep "No files found"` confirms in v0.24.1), not the lychee-action wrapper.
  - lychee source — search the literal string in <https://github.com/lycheeverse/lychee> to find the emitter and exact condition.
- **Last verified:** 2026-04-25 — present on every `main` push since #176 merged.
- **Hypotheses to check (in order):**
  1. **Redirected page returns non-HTML.** Same run reports 9 redirects. One redirected URL might land on a page lychee can't extract from (e.g., raw text, unusual content-type).
  2. **Sitemap entry yielding zero anchors.** Some Starlight pages — landing-style or auto-generated — render with no `<a href>` in body. Identify by running `curl <url> | grep -c '<a href' ` for each of the 46 URLs and finding the zero.
  3. **Spurious empty argument.** If the shell `run:` command produces an extra empty token on arg expansion, lychee treats it as an empty input source and warns.
- **How to reproduce:**
  ```sh
  curl -fsSL https://jackin.tailrocks.com/sitemap-0.xml \
    | grep -oE '<loc>[^<]+</loc>' | sed 's|<loc>||; s|</loc>||' > /tmp/pages.txt
  lychee --verbose --files-from /tmp/pages.txt 2>&1 | grep -B1 -A1 "No files found"
  ```
  Verbose output names the input source triggering the warn.
- **Done when:** either (a) warn no longer emitted on a clean main run, or (b) it is, but cause is documented as benign (e.g., one Starlight page renders without anchors by design) and warn is suppressed/filtered so it doesn't mask future genuine warnings. Case (a): remove this entry; case (b): replace with a one-line note in `docs.yml`.

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
