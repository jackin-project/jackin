FROM rust:1.96.0-trixie@sha256:fb328f0f58becb23ba1719940a2c94ece8b0b48afa837d05b79ef64bc1e18f6e AS security-tools

ARG SHELLFIRM_VERSION

# Install shellfirm
# TODO(shellfirm-aarch64-linux-binary): switch to prebuilt download — see TODO.md.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo install shellfirm --version "${SHELLFIRM_VERSION}" --locked

FROM debian:trixie-20260518@sha256:4ae67669760b807c19f23902a3fd7c121a6a70cf2ae709035674b23e712e4d62

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
    fd-find \
    fish \
    fzf \
    git \
    git-lfs \
    jq \
    openssh-client \
    procps \
    ripgrep \
    socat \
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

# Install mise from the official apt repository. MISE_VERSION is pinned in
# versions.env so Renovate bumps invalidate this rolling apt package layer.
ARG MISE_VERSION
RUN install -m 0755 -d /etc/apt/keyrings && \
    test -n "${MISE_VERSION}" || { echo "MISE_VERSION build arg is required" >&2; exit 1; } && \
    curl -fsSL https://mise.en.dev/gpg-key.pub -o /etc/apt/keyrings/mise-archive-keyring.asc && \
    chmod a+r /etc/apt/keyrings/mise-archive-keyring.asc && \
    echo "deb [signed-by=/etc/apt/keyrings/mise-archive-keyring.asc] https://mise.en.dev/deb stable main" > /etc/apt/sources.list.d/mise.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends "mise=${MISE_VERSION}" && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/* \
           /var/cache/apt/* \
           /tmp/*

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

# Create runtime user
RUN groupadd -g "$GID" agent && \
    useradd -m -u "$UID" -g "$GID" -s /bin/zsh agent && \
    echo "agent ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/agent && \
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
COPY --chmod=0755 jackin-notify /usr/local/bin/jackin-notify
