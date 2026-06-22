FROM projectjackin/construct:0.14-trixie@sha256:ddfa3918b03324530b329652f5bec60abc6d28067fc8e43e4ca4a25594558800

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# CAVEMAN_VERSION must be a release tag from
# https://github.com/JuliusBrussee/caveman/releases — never `main`,
# never a raw commit SHA. The `skills` CLI's shallow git-clone fetch
# resolves tags but not arbitrary SHAs.
ARG RUST_VERSION=1.96.0
ARG NODE_VERSION=24.17.0
ARG JAVA_VERSION=oracle-graalvm-25.0.1
ARG PROTOC_VERSION=34.1
ARG CARGO_BINSTALL_VERSION=1.20.1
ARG TESSL_VERSION=0.77.0
ARG CTX7_VERSION=0.5.3
ARG CAVEMAN_VERSION=1.9.0

# `libclang-dev` is required by Rust crates with `bindgen` build-deps
# (rdkafka-sys et al.) used in the ChainArgos monorepo.
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    sudo apt-get update && \
    sudo apt-get install -y --no-install-recommends \
    build-essential \
    libclang-dev \
    libssl-dev \
    openssl \
    pkg-config && \
    sudo apt-get autoremove -y

USER agent

ENV MISE_TRUSTED_CONFIG_PATHS=/workspace

# Testcontainers needs Docker API 1.44 to talk to Docker 29.x.
RUN echo "api.version=1.44" > /home/agent/.docker-java.properties && \
    mkdir -p /home/agent/.gradle "${HOME}/.cache/mise"

# Per-tool RUNs are deliberate: bumping one ARG only invalidates that
# tool's layer. Trips hadolint DL3059; the cache reuse is worth it.
# Ordered slowest-to-download / most-stable first so a Rust or Node bump
# does not re-download the large Java runtime.
RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    --mount=type=cache,target=/home/agent/.cache/mise,uid=1000 \
    GITHUB_TOKEN=$(cat /run/secrets/github_token 2>/dev/null || true) \
    mise install "java@${JAVA_VERSION}" && \
    mise use -g --pin "java@${JAVA_VERSION}"

RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    --mount=type=cache,target=/home/agent/.cache/mise,uid=1000 \
    GITHUB_TOKEN=$(cat /run/secrets/github_token 2>/dev/null || true) \
    mise install "protoc@${PROTOC_VERSION}" && \
    mise use -g --pin "protoc@${PROTOC_VERSION}"

RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    --mount=type=cache,target=/home/agent/.cache/mise,uid=1000 \
    GITHUB_TOKEN=$(cat /run/secrets/github_token 2>/dev/null || true) \
    mise install "node@${NODE_VERSION}" && \
    mise use -g --pin "node@${NODE_VERSION}"

RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    --mount=type=cache,target=/home/agent/.cache/mise,uid=1000 \
    GITHUB_TOKEN=$(cat /run/secrets/github_token 2>/dev/null || true) \
    mise install "rust@${RUST_VERSION}" && \
    mise use -g --pin "rust@${RUST_VERSION}" && \
    . ~/.profile && \
    rustup component add clippy rustfmt rust-analyzer

RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    --mount=type=cache,target=/home/agent/.cache/mise,uid=1000 \
    GITHUB_TOKEN=$(cat /run/secrets/github_token 2>/dev/null || true) \
    mise install "cargo-binstall@${CARGO_BINSTALL_VERSION}" && \
    mise use -g --pin "cargo-binstall@${CARGO_BINSTALL_VERSION}"

# cargo-binstall downloads prebuilt binaries — avoids compiling from source.
# Cache mounts preserve the crate registry across layer rebuilds on persistent runners.
RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    --mount=type=cache,target=/home/agent/.cargo/registry,uid=1000 \
    --mount=type=cache,target=/home/agent/.cargo/git,uid=1000 \
    GITHUB_TOKEN=$(cat /run/secrets/github_token 2>/dev/null || true) \
    . ~/.profile && \
    cargo binstall --no-confirm cargo-nextest ast-grep rust-script just bottom

RUN curl -fsSL https://get.tessl.io | TESSL_VERSION="$TESSL_VERSION" sh

RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    GITHUB_TOKEN=$(cat /run/secrets/github_token 2>/dev/null || true) \
    mise exec "node@${NODE_VERSION}" -- npm install -g "ctx7@${CTX7_VERSION}"

# Caveman ≥1.8.0 native opencode plugin needs repoRoot (a local clone);
# curl-pipe can't do it. Shallow-clone at the pinned tag, run bin/install.js.
#
# Agent CLIs aren't on PATH at build time (jackin injects them per-container),
# so auto-detection finds nothing — `--only` forces each target. `--no-mcp-shrink`
# skips registration that needs the claude CLI; preflight.sh re-runs it at
# container start.
#
# Caveman 1.9+ installs Claude hooks through the Claude plugin manifest instead
# of copying standalone files to ~/.claude/hooks. Rely on the installer exit
# status for Claude registration. Codex and Amp still use explicit
# `skills add --global` so the canonical tree exists at
# `${HOME}/.agents/skills/caveman/`, which is the path
# `hooks/preflight.sh::verify_codex_caveman_skills` checks.
RUN . ~/.profile && \
    mkdir -p "${HOME}/.claude" "${HOME}/.codex" && \
    git clone --depth 1 --branch "v${CAVEMAN_VERSION}" https://github.com/JuliusBrussee/caveman.git /tmp/caveman && \
    node /tmp/caveman/bin/install.js --only claude --only opencode --no-mcp-shrink && \
    test -f "${HOME}/.config/opencode/plugins/caveman/plugin.js" && \
    npx -y skills add "JuliusBrussee/caveman#v${CAVEMAN_VERSION}" -a codex --yes --global && \
    npx -y skills add "JuliusBrussee/caveman#v${CAVEMAN_VERSION}" -a amp --yes --global && \
    test -f "${HOME}/.agents/skills/caveman/SKILL.md" && \
    rm -rf /tmp/caveman

COPY --chown=agent:agent zshrc.d/docker-helpers.zsh /home/agent/.zshrc.d/docker-helpers.zsh
RUN echo 'source /home/agent/.zshrc.d/docker-helpers.zsh' >> /home/agent/.zshrc
