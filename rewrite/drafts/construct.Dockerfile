# syntax=docker/dockerfile:1.7

ARG BASE_IMAGE=nvcr.io/nvidia/base/ubuntu:24.04

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

RUN for tool in sudo su-exec gosu doas iptables ip6tables nft ufw docker podman nerdctl containerd runc; do \
        if command -v "$tool" >/dev/null; then echo "forbidden tool in construct: $tool" >&2; exit 1; fi; \
    done

ENV HOME=/sandbox
WORKDIR /sandbox
USER sandbox
