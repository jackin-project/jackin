# AGENTS.md

Rules in this file apply to the construct image under `docker/construct/`.

## Prefer official package-manager installs (hard rule)

When a tool can be installed through the tool's official package-manager source for the construct image's distro and architecture, use that source. The construct image is Debian-based, so this means official apt repositories or PPAs documented by the upstream project.

Do not bypass an official package-manager path with `curl`ed release assets, `cargo install`, `cargo binstall`, `npm install -g`, language-specific installer scripts, or hand-copied binaries just because those paths are more pin-friendly or familiar. The official package-manager path gives the clearest OS integration, normal upgrade semantics, dependency handling, and least-surprising Dockerfile shape.

Fallbacks are allowed only when the official package-manager path is unavailable or unusable for the image:

- No official package exists for the distro or architecture.
- The official package is too old, missing required features, or broken for the target image.
- The package source cannot be used non-interactively in Docker.
- A security or licensing constraint requires a different source.

When using a fallback, leave a short comment in the Dockerfile or adjacent docs naming why the official package-manager path was not used.
