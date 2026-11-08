# syntax=docker/dockerfile:1.28.0@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b

ARG BASE_IMAGE=debian:trixie@sha256:9c6e381421d0f48d1b8133e573f33c185ee16cbf505eda9c4a8a4791b25ea8a6

FROM ${BASE_IMAGE} AS write-file-build

RUN apt-get update \
    && apt-get install --yes --no-install-recommends build-essential \
    && rm -rf /var/lib/apt/lists/*

COPY write-file.c /tmp/write-file.c

RUN mkdir -p /out \
    && cc -std=c11 -O2 -Wall -Wextra -Werror -static /tmp/write-file.c -o /out/write-file \
    && test -x /out/write-file \
    && readelf -l /out/write-file > /tmp/write-file-program-headers \
    && ! grep -F INTERP /tmp/write-file-program-headers

FROM ${BASE_IMAGE}

ARG SANDBOX_UID=1000
ARG SANDBOX_GID=1000

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        bash \
        ca-certificates \
        curl \
        file \
        findutils \
        git \
        jq \
        less \
        libcap2-bin \
        openssh-client \
        procps \
        ripgrep \
        tar \
        unzip \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --gid "${SANDBOX_GID}" sandbox \
    && useradd --uid "${SANDBOX_UID}" --gid "${SANDBOX_GID}" --home-dir /sandbox --create-home --shell /bin/bash sandbox \
    && chown "${SANDBOX_UID}:${SANDBOX_GID}" /sandbox \
    && chmod 0755 /sandbox \
    && install -d -m 0755 -o root -g root /usr/local/lib/jackin

COPY --from=write-file-build --chown=root:root --chmod=0755 /out/write-file /usr/local/lib/jackin/write-file

RUN for tool in sudo su-exec gosu doas iptables ip6tables nft ufw docker podman nerdctl dockerd containerd containerd-shim containerd-shim-runc-v2 runc crun buildkitd rootlesskit docker-proxy dockerd-rootless.sh docker-compose docker-buildx buildctl ctr; do \
        if command -v "$tool" >/dev/null; then echo "forbidden tool in construct: $tool" >&2; exit 1; fi; \
    done \
    && find / -xdev -type f -perm /6000 -print -exec chmod a-s {} + \
    && test -z "$(find / -xdev -type f -perm /6000 -print)" \
    && command -v getcap >/dev/null \
    && test -z "$(getcap -r / 2>/dev/null)"

ENV HOME=/sandbox
WORKDIR /sandbox
USER sandbox
