ARG SHELLFIRM_VERSION=0.3.10
ARG TIRITH_VERSION=0.4.2
ARG MISE_VERSION=2026.10.14
ARG STARSHIP_VERSION=1.27.0






FROM rust:1.99.1-trixie@sha256:e259494b14672950a909b4c54527e19d29cae6ebf980672114c8bb85e5594e4a AS shellfirm-builder

ARG SHELLFIRM_VERSION

RUN cargo install shellfirm --version "${SHELLFIRM_VERSION}" --locked --root /opt/shellfirm && \
    /opt/shellfirm/bin/shellfirm --version | grep -F "${SHELLFIRM_VERSION}"

FROM debian:13@sha256:913f6706df59a68922d1dd08f78c2476560a8d367897200a6005b00e5f67c2d5

ARG SHELLFIRM_VERSION

COPY --from=shellfirm-builder /opt/shellfirm/bin/shellfirm /usr/local/bin/shellfirm
RUN chmod 0755 /usr/local/bin/shellfirm && \
    shellfirm --version | grep -F "${SHELLFIRM_VERSION}"

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG UID=1000





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
    libnss-extrausers \
    openssh-client \
    procps \
    ripgrep \
    ipset \
    iptables \
    sudo \
    tzdata \
    tree \
    wl-clipboard \
    xauth \
    xclip \
    yq \
    zsh && \
    ln -sf /usr/bin/fdfind /usr/local/bin/fd && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/* \
           /var/cache/apt/* \
           /tmp/*








RUN sed -ri 's/^(passwd:.*)$/\1 extrausers/; s/^(group:.*)$/\1 extrausers/' /etc/nsswitch.conf && \
    install -d -m 0755 /var/lib/extrausers && \
    : > /var/lib/extrausers/passwd && \
    : > /var/lib/extrausers/group


RUN git lfs install


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





ARG MISE_VERSION
RUN test -n "${MISE_VERSION}" || { echo "MISE_VERSION build arg is required" >&2; exit 1; } && \
    case "${TARGETARCH}" in \
      amd64) mise_arch=x64 ;; \
      arm64) mise_arch=arm64 ;; \
      *) printf 'unsupported TARGETARCH for mise: %s\n' "${TARGETARCH}" >&2 && exit 1 ;; \
    esac && \
    archive="mise-v${MISE_VERSION}-linux-${mise_arch}.tar.gz" && \
    base="https://github.com/jdx/mise/releases/download/v${MISE_VERSION}" && \
    workdir="$(mktemp -d)" && \
    cd "${workdir}" && \
    curl --fail --location --show-error --retry 3 --retry-all-errors --connect-timeout 10 --max-time 120 \
      --output "${archive}" "${base}/${archive}" && \
    curl --fail --location --show-error --retry 3 --retry-all-errors --connect-timeout 10 --max-time 120 \
      --output SHASUMS256.txt "${base}/SHASUMS256.txt" && \
    grep "  ./${archive}$" SHASUMS256.txt | sed 's#  \./#  #' > checksums-mise.txt && \
    sha256sum -c checksums-mise.txt && \
    tar -xzf "${archive}" --strip-components=2 -C /usr/local/bin mise/bin/mise && \
    chmod 0755 /usr/local/bin/mise && \
    cd / && \
    rm -rf "${workdir}" && \
    mise --version


RUN curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc && \
    chmod a+r /etc/apt/keyrings/docker.asc && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian $(. /etc/os-release && echo "$VERSION_CODENAME") stable" > /etc/apt/sources.list.d/docker.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends docker-ce-cli docker-buildx-plugin docker-compose-plugin && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/* \
           /var/cache/apt/* \
           /tmp/*


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





RUN useradd -m -u "$UID" -g 0 -s /bin/zsh agent && \
    chmod 0775 /home/agent && \
    install -d -m 0775 -o agent -g 0 /home/agent/.claude /home/agent/.codex /home/agent/.jackin

RUN printf 'umask 0002\n' > /etc/jackin-build-env
ENV BASH_ENV=/etc/jackin-build-env




ARG STARSHIP_VERSION
RUN test -n "${STARSHIP_VERSION}" || { echo "STARSHIP_VERSION build arg is required" >&2; exit 1; } && \
    case "${TARGETARCH}" in \
      amd64) starship_triple=x86_64-unknown-linux-gnu ;; \
      arm64) starship_triple=aarch64-unknown-linux-musl ;; \
      *) printf 'unsupported TARGETARCH for starship: %s\n' "${TARGETARCH}" >&2 && exit 1 ;; \
    esac && \
    archive="starship-${starship_triple}.tar.gz" && \
    base="https://github.com/starship/starship/releases/download/v${STARSHIP_VERSION}" && \
    workdir="$(mktemp -d)" && \
    cd "${workdir}" && \
    curl --fail --location --show-error --retry 3 --retry-all-errors --connect-timeout 10 --max-time 120 \
      --output "${archive}" "${base}/${archive}" && \
    curl --fail --location --show-error --retry 3 --retry-all-errors --connect-timeout 10 --max-time 120 \
      --output "${archive}.sha256" "${base}/${archive}.sha256" && \
    printf '%s  %s\n' "$(tr -d '\n' < "${archive}.sha256")" "${archive}" | sha256sum -c - && \
    tar -xzf "${archive}" -C /usr/local/bin starship && \
    chmod 0755 /usr/local/bin/starship && \
    cd / && \
    rm -rf "${workdir}" && \
    starship --version



ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

USER agent

ENV PATH="/home/agent/.local/share/mise/shims:/home/agent/.local/bin:${PATH}"


RUN sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended && \
    git clone https://github.com/zsh-users/zsh-autosuggestions ${ZSH_CUSTOM:-/home/agent/.oh-my-zsh/custom}/plugins/zsh-autosuggestions

COPY --chown=agent:0 --chmod=0664 zshrc /home/agent/.zshrc






RUN install -d -o agent -g 0 -m 0775 /home/agent/.config /home/agent/.config/fish
COPY --chown=agent:0 --chmod=0664 fish-config.fish /home/agent/.config/fish/config.fish
COPY --chown=agent:0 --chmod=0664 starship.toml /home/agent/.config/starship.toml
