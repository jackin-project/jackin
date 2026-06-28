# AGENTS.md

Rules apply to the construct image under `docker/construct/`.

## Prefer official package-manager installs (hard rule)

When a tool installs through its official package-manager source for the construct image's distro + architecture, use that source. Construct image is Debian-based, so: official apt repositories or upstream-documented PPAs.

Do not bypass an official package-manager path with `curl`ed release assets, `cargo install`, `cargo binstall`, `npm install -g`, language-specific installer scripts, or hand-copied binaries just because they're more pin-friendly or familiar. The official path gives clearest OS integration, normal upgrade semantics, dependency handling, least-surprising Dockerfile shape.

Fallbacks allowed only when the official path is unavailable or unusable:

- No official package for the distro or architecture.
- Official package too old, missing required features, or broken for the target image.
- Package source can't be used non-interactively in Docker.
- A security or licensing constraint requires a different source.

When using a fallback, leave a short comment in the Dockerfile or adjacent docs naming why the official path wasn't used.
