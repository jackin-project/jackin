FROM projectjackin/construct:0.2-trixie@sha256:b6f6ada39797ef32033edb061d473423643df36b1041ca4770b85581a2c7739e

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# CAVEMAN_VERSION must be a release tag from
# https://github.com/JuliusBrussee/caveman/releases — never `main`,
# never a raw commit SHA. The `skills` CLI's shallow git-clone fetch
# resolves tags but not arbitrary SHAs.
ARG RUST_VERSION=1.95.0
ARG NODE_VERSION=24.15.0
ARG JAVA_VERSION=oracle-graalvm-25.0.1
ARG PROTOC_VERSION=34.1
ARG CARGO_BINSTALL_VERSION=1.19.1
ARG TESSL_VERSION=0.77.0
ARG CAVEMAN_VERSION=1.7.0

# `libclang-dev` is required by Rust crates with `bindgen` build-deps
# (rdkafka-sys et al.) used in the ChainArgos monorepo.
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

# Testcontainers needs Docker API 1.44 to talk to Docker 29.x.
RUN echo "api.version=1.44" > /home/agent/.docker-java.properties

RUN mkdir -p /home/agent/.gradle

# Per-tool RUNs are deliberate: bumping one ARG only invalidates that
# tool's layer. Trips hadolint DL3059; the cache reuse is worth it.
RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    GITHUB_TOKEN=${GITHUB_TOKEN:-$(cat /run/secrets/github_token 2>/dev/null)} \
    mise install "node@${NODE_VERSION}" && \
    mise use -g --pin "node@${NODE_VERSION}"

RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    GITHUB_TOKEN=${GITHUB_TOKEN:-$(cat /run/secrets/github_token 2>/dev/null)} \
    mise install "rust@${RUST_VERSION}" && \
    mise use -g --pin "rust@${RUST_VERSION}"

RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    GITHUB_TOKEN=${GITHUB_TOKEN:-$(cat /run/secrets/github_token 2>/dev/null)} \
    mise install "java@${JAVA_VERSION}" && \
    mise use -g --pin "java@${JAVA_VERSION}"

RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    GITHUB_TOKEN=${GITHUB_TOKEN:-$(cat /run/secrets/github_token 2>/dev/null)} \
    mise install "protoc@${PROTOC_VERSION}" && \
    mise use -g --pin "protoc@${PROTOC_VERSION}"

RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    GITHUB_TOKEN=${GITHUB_TOKEN:-$(cat /run/secrets/github_token 2>/dev/null)} \
    mise install "cargo-binstall@${CARGO_BINSTALL_VERSION}" && \
    mise use -g --pin "cargo-binstall@${CARGO_BINSTALL_VERSION}"

# cargo-binstall downloads prebuilt binaries — avoids compiling from source.
# Cache mounts preserve the crate registry across layer rebuilds on persistent runners.
RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    --mount=type=cache,target=/home/agent/.cargo/registry,uid=1000 \
    --mount=type=cache,target=/home/agent/.cargo/git,uid=1000 \
    GITHUB_TOKEN=${GITHUB_TOKEN:-$(cat /run/secrets/github_token 2>/dev/null)} \
    . ~/.profile && \
    cargo binstall --no-confirm cargo-nextest ast-grep rust-script just bottom

RUN curl -fsSL https://get.tessl.io | TESSL_VERSION="$TESSL_VERSION" sh

RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    GITHUB_TOKEN=${GITHUB_TOKEN:-$(cat /run/secrets/github_token 2>/dev/null)} \
    mise exec "node@${NODE_VERSION}" -- npm install -g ctx7

# Caveman bypasses the root `install.sh` because its per-agent
# detection probes find no agent CLI at image-build time (jackin
# injects them at launch). The pinned release archive is unpacked
# before running the hook installer so the installer copies files from
# the tag instead of downloading from upstream `main`.
#
# `bash -e` forces fail-fast inside the upstream installer regardless
# of its own error-handling. `test -f` lines guard against the
# silent-empty-install failure mode.
RUN . ~/.profile && \
    mkdir -p "${HOME}/.claude" "${HOME}/.codex" && \
    curl -fsSL "https://github.com/JuliusBrussee/caveman/archive/refs/tags/v${CAVEMAN_VERSION}.tar.gz" \
      | tar -xz -C /tmp && \
    bash -e "/tmp/caveman-${CAVEMAN_VERSION}/hooks/install.sh" && \
    rm -rf "/tmp/caveman-${CAVEMAN_VERSION}" && \
    test -f "${HOME}/.claude/hooks/caveman-statusline.sh" && \
    test -f "${HOME}/.claude/hooks/caveman-activate.js" && \
    test -f "${HOME}/.claude/hooks/caveman-mode-tracker.js" && \
    cd "${HOME}" && \
    echo "[caveman] installing codex profile" && \
    npx -y skills add "JuliusBrussee/caveman#v${CAVEMAN_VERSION}" -a codex --yes --global && \
    echo "[caveman] installing amp profile" && \
    npx -y skills add "JuliusBrussee/caveman#v${CAVEMAN_VERSION}" -a amp --yes --global && \
    test -f "${HOME}/.agents/skills/caveman/SKILL.md"

COPY --chown=agent:agent zshrc.d/docker-helpers.zsh /home/agent/.zshrc.d/docker-helpers.zsh
RUN echo 'source /home/agent/.zshrc.d/docker-helpers.zsh' >> /home/agent/.zshrc