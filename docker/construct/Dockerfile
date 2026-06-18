FROM rust:1.96.0-trixie@sha256:4fd8406017c992f7b8ab55a2f99a1d56aeb1d7ecd255850dfa04239a88601f73 AS security-tools

ARG SHELLFIRM_VERSION

# Install sccache (pinned) so the shellfirm compile below reuses compiled
# artifacts from the /sccache-build cache mount instead of starting cold
# whenever SHELLFIRM_VERSION changes.
ARG TARGETARCH
RUN case "${TARGETARCH}" in \
      amd64) sccache_arch=x86_64 ;; \
      arm64) sccache_arch=aarch64 ;; \
      *) printf 'unsupported TARGETARCH for sccache: %s\n' "${TARGETARCH}" >&2 && exit 1 ;; \
    esac && \
    curl -fsSL "https://github.com/mozilla/sccache/releases/download/v0.15.0/sccache-v0.15.0-${sccache_arch}-unknown-linux-musl.tar.gz" \
      -o /tmp/sccache.tar.gz && \
    tar -xzf /tmp/sccache.tar.gz -C /usr/local/bin --strip-components=1 \
      "sccache-v0.15.0-${sccache_arch}-unknown-linux-musl/sccache" && \
    chmod 0755 /usr/local/bin/sccache && \
    rm /tmp/sccache.tar.gz && \
    sccache --version

# Install shellfirm
# TODO(shellfirm-aarch64-linux-binary): switch to prebuilt download — see TODO.md.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/sccache-build \
    RUSTC_WRAPPER=sccache SCCACHE_DIR=/sccache-build \
    cargo install shellfirm --version "${SHELLFIRM_VERSION}" --locked

FROM debian:trixie-20260610@sha256:fe7312b5f05bf5f43fad76bcd8945642e4e47a68aefd1b73f447615899d0fac1

COPY --from=security-tools /usr/local/cargo/bin/shellfirm /usr/local/bin/shellfirm

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG UID=1000
ARG GID=1000

# Install core system packages. zsh is the default login shell for
# the `agent` user; fish is installed alongside as an opt-in
# alternative (`fish` from zsh, or `chsh -s /usr/bin/fish agent`).
# Both shells get Starship prompt + auto-title configs baked below.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    bash \
    ca-certificates \
    curl \
    dnsutils \
    fd-find \
    fish \
    fzf \
    git \
    git-lfs \
    jq \
    openssh-client \
    procps \
    ripgrep \
    ipset \
    iptables \
    sudo \
    tree \
    yq \
    zsh && \
    ln -sf /usr/bin/fdfind /usr/local/bin/fd && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/* \
           /var/cache/apt/* \
           /tmp/*

# Configure git-lfs
RUN git lfs install

# Install tirith
ARG TIRITH_VERSION
ARG TARGETARCH
RUN case "${TARGETARCH}" in \
      amd64) tirith_arch=x86_64-unknown-linux-gnu ;; \
      arm64) tirith_arch=aarch64-unknown-linux-gnu ;; \
      *) printf 'unsupported TARGETARCH for tirith: %s\n' "${TARGETARCH}" >&2 && exit 1 ;; \
    esac && \
    base="https://github.com/sheeki03/tirith/releases/download/v${TIRITH_VERSION}" && \
    workdir="$(mktemp -d)" && \
    cd "${workdir}" && \
    curl -fsSLO "${base}/tirith-${tirith_arch}.tar.gz" && \
    curl -fsSLO "${base}/checksums.txt" && \
    grep "  tirith-${tirith_arch}.tar.gz$" checksums.txt > checksums-tirith.txt && \
    sha256sum -c checksums-tirith.txt && \
    tar -xzf "tirith-${tirith_arch}.tar.gz" -C /usr/local/bin tirith && \
    chmod 0755 /usr/local/bin/tirith && \
    cd / && \
    rm -rf "${workdir}"

# Install mise with the official standalone installer. MISE_VERSION is pinned
# in versions.env so Renovate bumps invalidate this layer. The installer
# downloads the checksummed release asset from GitHub, where every version
# stays available — the mise apt repository only serves the latest package,
# so an apt version pin breaks on every upstream release.
ARG MISE_VERSION
RUN test -n "${MISE_VERSION}" || { echo "MISE_VERSION build arg is required" >&2; exit 1; } && \
    curl -fsSL https://mise.run | MISE_VERSION="v${MISE_VERSION}" MISE_INSTALL_PATH=/usr/local/bin/mise sh && \
    mise --version

# Install Docker CLI and Compose plugin
RUN curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc && \
    chmod a+r /etc/apt/keyrings/docker.asc && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian $(. /etc/os-release && echo "$VERSION_CODENAME") stable" > /etc/apt/sources.list.d/docker.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends docker-ce-cli docker-compose-plugin && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/* \
           /var/cache/apt/* \
           /tmp/*

# Install GitHub CLI
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
      -o /etc/apt/keyrings/githubcli-archive-keyring.gpg && \
    chmod a+r /etc/apt/keyrings/githubcli-archive-keyring.gpg && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
      > /etc/apt/sources.list.d/github-cli.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends gh && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/* \
           /var/cache/apt/* \
           /tmp/*

# Create runtime user. No baked sudoers — WP-SUDO provisions sudo at runtime
# only when JACKIN_SUDO=1 is injected by the host (compat profile and explicit
# sudo grants). The sudo package stays installed so the runtime entrypoint can
# write /etc/sudoers.d/agent when the grant is active.
RUN groupadd -g "$GID" agent && \
    useradd -m -u "$UID" -g "$GID" -s /bin/zsh agent && \
    install -d -o agent -g agent /home/agent/.claude /home/agent/.codex /home/agent/.jackin

# Default to a UTF-8 locale so shell prompts and CLI output render correctly,
# including `docker exec` sessions that bypass jackin's runtime entrypoint.
ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

USER agent

ENV PATH="/home/agent/.local/share/mise/shims:/home/agent/.local/bin:${PATH}"

# Install oh-my-zsh and plugins
RUN sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended && \
    git clone https://github.com/zsh-users/zsh-autosuggestions ${ZSH_CUSTOM:-/home/agent/.oh-my-zsh/custom}/plugins/zsh-autosuggestions

# Install starship prompt
RUN curl -sS https://starship.rs/install.sh | sh -s -- -y

COPY --chown=agent:agent zshrc /home/agent/.zshrc

# Fish config — opt-in alternative shell. Same Starship prompt and
# auto-title behaviour as zsh so the operator's pane border title
# does not regress when switching shells.
RUN mkdir -p /home/agent/.config/fish
COPY --chown=agent:agent fish-config.fish /home/agent/.config/fish/config.fish
