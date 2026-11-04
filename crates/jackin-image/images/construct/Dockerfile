# syntax=docker/dockerfile:1.28.0@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b

ARG BASE_IMAGE=nvcr.io/nvidia/base/ubuntu:24.04@sha256:d9461f428eea54e553add3a5158a0f245c6e1ea73efc59fff8e98e3d4a38e378

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

RUN if getent passwd ubuntu >/dev/null; then userdel --remove ubuntu; fi \
    && if getent group ubuntu >/dev/null; then groupdel ubuntu; fi \
    && groupadd --gid "${SANDBOX_GID}" sandbox \
    && useradd --uid "${SANDBOX_UID}" --gid "${SANDBOX_GID}" --home-dir /sandbox --create-home --shell /bin/bash sandbox \
    && chown "${SANDBOX_UID}:${SANDBOX_GID}" /sandbox

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
