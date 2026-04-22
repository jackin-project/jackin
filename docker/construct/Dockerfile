FROM rust:1.95.0-trixie@sha256:86988dba738b8f6a20479bd446c8b1929188952674dd4c1d336062937b46d21f AS security-tools

ARG TIRITH_VERSION
ARG SHELLFIRM_VERSION

RUN cargo install tirith --version "${TIRITH_VERSION}" --locked && \
    cargo install shellfirm --version "${SHELLFIRM_VERSION}" --locked

FROM debian:trixie-20260406@sha256:3352c2e13876c8a5c5873ef20870e1939e73cb9a3c1aeba5e3e72172a85ce9ed

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

# Install mise
RUN install -m 0755 -d /etc/apt/keyrings && \
    curl -fsSL https://mise.jdx.dev/gpg-key.pub -o /etc/apt/keyrings/mise.asc && \
    chmod a+r /etc/apt/keyrings/mise.asc && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/mise.asc] https://mise.jdx.dev/deb stable main" > /etc/apt/sources.list.d/mise.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends mise && \
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
RUN groupadd -g "$GID" claude && \
    useradd -m -u "$UID" -g "$GID" -s /bin/zsh claude && \
    echo "claude ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/claude && \
    install -d -o claude -g claude /home/claude/.claude /home/claude/.jackin

# Default to a UTF-8 locale so shell prompts and CLI output render correctly,
# including `docker exec` sessions that bypass jackin's runtime entrypoint.
ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

USER claude

ENV PATH="/home/claude/.local/share/mise/shims:/home/claude/.local/bin:${PATH}"

# Install oh-my-zsh and plugins
RUN sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended && \
    git clone https://github.com/zsh-users/zsh-autosuggestions ${ZSH_CUSTOM:-/home/claude/.oh-my-zsh/custom}/plugins/zsh-autosuggestions

# Install starship prompt
RUN curl -sS https://starship.rs/install.sh | sh -s -- -y

COPY --chown=claude:claude zshrc /home/claude/.zshrc
COPY --chown=claude:claude install-plugins.sh /home/claude/install-plugins.sh
RUN chmod +x /home/claude/install-plugins.sh
