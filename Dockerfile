FROM projectjackin/construct:trixie

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Pinned tool versions. Keep these at the top of the file so the diff
# for a bump is one line per tool, and so they are easy to grep / override
# at build time via `docker build --build-arg RUST_VERSION=...`.
#
# RUST_VERSION is intended to track the toolchain ChainArgos's monorepo
# expects (look at the workspace's `rust-toolchain.toml`) and the Rust
# pin baked into `projectjackin/construct:trixie`. There is no automated
# check enforcing this; it is an out-of-band contract.
#
# CAVEMAN_REF pins the upstream caveman repo to a specific commit so a
# `docker build --no-cache` can't pick up a hijacked `main`. Bump after
# auditing https://github.com/JuliusBrussee/caveman/commits/main .
ARG RUST_VERSION=1.95.0
ARG NODE_VERSION=lts
ARG JAVA_VERSION=oracle-graalvm-25.0.1
ARG PROTOC_VERSION=latest
ARG TESSL_VERSION=0.77.0
ARG CAVEMAN_REF=ef6050c5e1848b6880ff47c32ade1a608a64f85e

# Install additional system packages. `libclang-dev` is required by Rust
# crates with `bindgen` build-deps (rdkafka-sys et al.) used in the
# ChainArgos monorepo; it is the largest single contributor to image
# size after the toolchains.
RUN sudo apt-get update && \
    sudo apt-get install -y --no-install-recommends \
    build-essential \
    libclang-dev \
    libssl-dev \
    openssl \
    pkg-config && \
    sudo apt-get autoremove -y && \
    sudo rm -rf /var/lib/apt/* \
               /var/cache/apt/* \
               /tmp/*

USER agent

ENV MISE_TRUSTED_CONFIG_PATHS=/workspace

# Set Docker API version for Testcontainers compatibility with Docker 29.x
RUN echo "api.version=1.44" > /home/agent/.docker-java.properties

# Create gradle cache directory
RUN mkdir -p /home/agent/.gradle

# Toolchain installs.
#
# Each tool gets its own RUN on purpose. Docker's layer cache is
# sequential (a busted layer invalidates everything downstream), but
# changing only `cargo:bottom` does NOT invalidate the Rust layer above
# it, and a `--no-cache` rebuild still reuses the apt-installed system
# packages above. Listing earlier-stable tools first (Node, Rust, Java,
# Protoc) maximizes how many layers stay cached on the most common bump
# (a single cargo tool or Tessl bump). This intentionally trips
# hadolint DL3059 / docker:S7031 ("merge consecutive RUN instructions");
# the cache reuse is worth more than the saved-layer-count the rule
# optimizes for.
RUN mise install "node@${NODE_VERSION}" && \
    mise use -g --pin "node@${NODE_VERSION}"

RUN mise install "rust@${RUST_VERSION}" && \
    mise use -g --pin "rust@${RUST_VERSION}"

RUN mise install "java@${JAVA_VERSION}" && \
    mise use -g --pin "java@${JAVA_VERSION}"

RUN mise install "protoc@${PROTOC_VERSION}" && \
    mise use -g --pin "protoc@${PROTOC_VERSION}"

# Cargo tools
RUN mise use -g cargo-binstall
RUN mise use -g cargo:cargo-nextest
RUN mise use -g cargo:ast-grep
RUN mise use -g cargo:rust-script
RUN mise use -g cargo:just
RUN mise use -g cargo:bottom

# Tessl CLI
RUN curl -fsSL https://get.tessl.io | TESSL_VERSION="$TESSL_VERSION" sh

# Global npm packages
RUN mise exec "node@${NODE_VERSION}" -- npm install -g ctx7

# Caveman install for the agents this image supports (claude + codex).
# We bypass the root caveman `install.sh` because its per-agent detection
# probes (`command:claude`, `command:codex`) fail at image-build time —
# both agent CLIs are injected per-container by jackin's derived-image
# build (Agent::install_block in jackin's src/agent/mod.rs), not baked
# into this image, so the root installer sees "nothing detected" and
# exits 0 with no files written. Calling each component installer
# directly skips detection and lands the files in the image layer.
#
# Two components from the root installer are intentionally NOT run here:
#   * `--with-mcp-shrink`: `claude mcp add …` needs the runtime-injected
#     claude CLI; we register caveman-shrink at runtime in
#     `hooks/pre-launch.sh` instead.
#   * `--with-init`: writes per-repo IDE rule files into `$PWD`; at
#     build time `$PWD=/` and the runtime workspace mount hides the
#     image-layer copies anyway.
#
# `--yes` makes `npx skills add` non-interactive (build context has no
# TTY); `--global` selects home-scoped installs over `$PWD/.agents`
# (which would be `/.agents` at build time and EACCES). `cd /home/agent`
# is defensive against any future relative-path fallback in the skills
# CLI.
#
# CAVEMAN_REF pins the install.sh URL to a specific commit so a
# `docker build --no-cache` can't pick up a hijacked `main`. The skills
# install is intentionally NOT pinned via `JuliusBrussee/caveman#<ref>`
# — passing a ref makes the `skills` CLI invoke `git clone` of an
# arbitrary commit, which fails on the build runner ("Failed to clone
# https://github.com/JuliusBrussee/caveman.git"; the CLI does the
# equivalent of a shallow clone of the default branch tip and then
# tries to check out the SHA, which the shallow clone doesn't have).
# We accept that the skills source tracks `main` at install time; the
# install.sh URL pin is the meaningful hijack-`main` defense because
# the install.sh is what writes the executable hooks into the image.
#
# `bash -e` forces fail-fast on the upstream installer regardless of
# its own error-handling. The trailing `test -f` lines fail the RUN
# if any file the installer is supposed to write is missing.
RUN . ~/.profile && \
    mkdir -p "${HOME}/.claude" "${HOME}/.codex" && \
    curl -fsSL "https://raw.githubusercontent.com/JuliusBrussee/caveman/${CAVEMAN_REF}/hooks/install.sh" | bash -e && \
    test -f "${HOME}/.claude/hooks/caveman-statusline.sh" && \
    test -f "${HOME}/.claude/hooks/caveman-activate.js" && \
    test -f "${HOME}/.claude/hooks/caveman-mode-tracker.js" && \
    cd "${HOME}" && \
    npx -y skills add JuliusBrussee/caveman -a codex --yes --global && \
    test -f "${HOME}/.agents/skills/caveman/SKILL.md"

# Docker convenience functions
COPY --chown=agent:agent zshrc.d/docker-helpers.zsh /home/agent/.zshrc.d/docker-helpers.zsh
RUN echo 'source /home/agent/.zshrc.d/docker-helpers.zsh' >> /home/agent/.zshrc
