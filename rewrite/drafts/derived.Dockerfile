# syntax=docker/dockerfile:1.7

ARG CODEX_VERSION=0.166.0
ARG CODEX_TARGET=aarch64-unknown-linux-musl
ARG CODEX_PACKAGE_SHA256=7821b77dda2e6283fb1f63f3165177a271da8833e857cf673c6ca1c9db3bfcc6

FROM scratch AS codex-archive
ARG CODEX_VERSION
ARG CODEX_TARGET
ARG CODEX_PACKAGE_SHA256
ADD --checksum=sha256:${CODEX_PACKAGE_SHA256} https://github.com/openai/codex/releases/download/rust-v${CODEX_VERSION}/codex-package-${CODEX_TARGET}.tar.gz /codex-package.tar.gz

FROM role
ARG CODEX_VERSION

USER root
RUN --mount=type=bind,from=codex-archive,source=/,target=/archive \
    tar -xzf /archive/codex-package.tar.gz --no-same-owner -C /usr/local bin/codex bin/codex-code-mode-host \
    && mkdir -p /etc/codex \
    && printf 'check_for_update_on_startup = false\n' > /etc/codex/config.toml \
    && test "$(/usr/local/bin/codex --version)" = "codex-cli ${CODEX_VERSION}"
USER sandbox
WORKDIR /sandbox
