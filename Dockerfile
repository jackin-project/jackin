FROM projectjackin/construct:0.25-trixie@sha256:ad853971892ae36eb8b43201f219192405d12b51d317b37fde073f02708aee95

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# CAVEMAN_VERSION: tagged release.
ARG RUST_VERSION=1.96.0
ARG NODE_VERSION=24.18.0
ARG JAVA_VERSION=oracle-graalvm-25.0.1
ARG PROTOC_VERSION=34.1
ARG CARGO_BINSTALL_VERSION=1.20.1
ARG CTX7_VERSION=0.5.3
ARG CAVEMAN_VERSION=1.9.1
ARG RTK_VERSION=0.42.4
ARG UV_VERSION=0.11.24
ARG HEADROOM_VERSION=0.27.0

# libclang-dev required for some Rust crates.
USER root
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && \
    apt-get install -y --no-install-recommends \
    build-essential \
    libclang-dev \
    libssl-dev \
    openssl \
    pkg-config && \
    apt-get autoremove -y

USER agent

ENV MISE_TRUSTED_CONFIG_PATHS=/workspace

# Docker API 1.44 needed for testcontainers.
RUN echo "api.version=1.44" > /home/agent/.docker-java.properties && \
    mkdir -p /home/agent/.gradle "${HOME}/.cache/mise"

# Separate RUN per tool (layer caching).
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

# cargo-binstall + cache mounts.
RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    --mount=type=cache,target=/home/agent/.cargo/registry,uid=1000 \
    --mount=type=cache,target=/home/agent/.cargo/git,uid=1000 \
    GITHUB_TOKEN=$(cat /run/secrets/github_token 2>/dev/null || true) \
    . ~/.profile && \
    cargo binstall --no-confirm cargo-nextest ast-grep rust-script just bottom

RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    GITHUB_TOKEN=$(cat /run/secrets/github_token 2>/dev/null || true) \
    mise exec "node@${NODE_VERSION}" -- npm install -g "ctx7@${CTX7_VERSION}"

# Caveman install.
RUN . ~/.profile && \
    mkdir -p "${HOME}/.claude" "${HOME}/.codex" "${HOME}/.grok" && \
    git clone --depth 1 --branch "v${CAVEMAN_VERSION}" https://github.com/JuliusBrussee/caveman.git /tmp/caveman && \
    node /tmp/caveman/bin/install.js --only opencode --no-mcp-shrink && \
    test -f "${HOME}/.config/opencode/plugins/caveman/plugin.js" && \
    npx -y skills add "JuliusBrussee/caveman#v${CAVEMAN_VERSION}" -a codex --yes --global && \
    npx -y skills add "JuliusBrussee/caveman#v${CAVEMAN_VERSION}" -a amp --yes --global && \
    test -f "${HOME}/.agents/skills/caveman/SKILL.md" && \
    rm -rf /tmp/caveman

# improve skill (shadcn/improve).
RUN . ~/.profile && \
    npx -y skills add "shadcn/improve" -a claude-code --yes --global && \
    npx -y skills add "shadcn/improve" -a codex --yes --global && \
    npx -y skills add "shadcn/improve" -a amp --yes --global && \
    npx -y skills add "shadcn/improve" -a opencode --yes --global && \
    npx -y skills add "shadcn/improve" -a kimi-code-cli --yes --global && \
    test -f "${HOME}/.claude/skills/improve/SKILL.md" && \
    test -f "${HOME}/.agents/skills/improve/SKILL.md"

# Token opt: RTK + caveman + headroom.
RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    --mount=type=cache,target=/home/agent/.cache/mise,uid=1000 \
    GITHUB_TOKEN=$(cat /run/secrets/github_token 2>/dev/null || true) \
    mise install "rtk@${RTK_VERSION}" && \
    mise use -g --pin "rtk@${RTK_VERSION}" && \
    mise exec -- rtk --version

ENV RTK_TELEMETRY_DISABLED=1

# Codex AGENTS as file (not symlink) for RTK.
RUN printf '@/home/agent/AGENTS.md\n' > /home/agent/.codex/AGENTS.md || true && \
    ln -sf /home/agent/AGENTS.md /home/agent/.grok/AGENTS.md || true

# RTK init for codex/opencode.
RUN . ~/.profile && \
    rtk init -g --codex && \
    test -f /home/agent/.codex/RTK.md || true && \
    rtk init -g --opencode && \
    test -f /home/agent/.config/opencode/plugins/rtk.ts || true

# Headroom install.
RUN --mount=type=secret,id=github_token,uid=1000,required=false \
    --mount=type=cache,target=/home/agent/.cache/mise,uid=1000 \
    GITHUB_TOKEN=$(cat /run/secrets/github_token 2>/dev/null || true) \
    mise install "uv@${UV_VERSION}" && \
    mise use -g --pin "uv@${UV_VERSION}"

RUN . ~/.profile && uv tool install "headroom-ai[mcp]==${HEADROOM_VERSION}"

ENV PATH="/home/agent/.local/bin:/home/agent/.local/share/mise/shims:${PATH}"

COPY --chown=agent:agent zshrc.d/docker-helpers.zsh /home/agent/.zshrc.d/docker-helpers.zsh
RUN echo 'source /home/agent/.zshrc.d/docker-helpers.zsh' >> /home/agent/.zshrc
