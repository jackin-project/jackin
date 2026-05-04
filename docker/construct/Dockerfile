FROM rust:1.95.0-trixie@sha256:a9cfb755b33f5bb872610cbdb25da61f527416b28fc9c052bbce4bef93e7799a AS security-tools

ARG TIRITH_VERSION
ARG SHELLFIRM_VERSION

RUN cargo install tirith --version "${TIRITH_VERSION}" --locked && \
    cargo install shellfirm --version "${SHELLFIRM_VERSION}" --locked

FROM debian:trixie-20260421@sha256:35b8ff74ead4880f22090b617372daff0ccae742eb5674455d542bef71ef1999

COPY --from=security-tools /usr/local/cargo/bin/tirith /usr/local/bin/tirith
COPY --from=security-tools /usr/local/cargo/bin/shellfirm /usr/local/bin/shellfirm

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG UID=1000
ARG GID=1000

# Install core system packages
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    bash \
    ca-certificates \
    curl \
    fd-find \
    fzf \
    git \
    git-lfs \
    jq \
    openssh-client \
    ripgrep \
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

# Install mise (version pinned via versions.env -> MISE_VERSION ARG; mise apt
# repo only ships the latest release in `stable`, so this pin must be bumped
# in versions.env when upstream rolls or apt-get install will fail to resolve).
ARG MISE_VERSION
RUN install -m 0755 -d /etc/apt/keyrings && \
    curl -fsSL https://mise.jdx.dev/gpg-key.pub -o /etc/apt/keyrings/mise.asc && \
    chmod a+r /etc/apt/keyrings/mise.asc && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/mise.asc] https://mise.jdx.dev/deb stable main" > /etc/apt/sources.list.d/mise.list && \
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
COPY --chown=agent:agent install-claude-plugins.sh /home/agent/install-claude-plugins.sh
RUN chmod +x /home/agent/install-claude-plugins.sh
