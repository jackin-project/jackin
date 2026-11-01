# syntax=docker/dockerfile:1.7

ARG CODEX_VERSION
ARG CODEX_BIN_DIR
ARG CODEX_SHA256_ARM64
ARG CODEX_SHA256_AMD64

FROM scratch AS codex-archive-arm64
ARG CODEX_VERSION
ARG CODEX_SHA256_ARM64
ADD --checksum=sha256:${CODEX_SHA256_ARM64} https://github.com/openai/codex/releases/download/rust-v${CODEX_VERSION}/codex-package-aarch64-unknown-linux-musl.tar.gz /codex-package.tar.gz

FROM scratch AS codex-archive-amd64
ARG CODEX_VERSION
ARG CODEX_SHA256_AMD64
ADD --checksum=sha256:${CODEX_SHA256_AMD64} https://github.com/openai/codex/releases/download/rust-v${CODEX_VERSION}/codex-package-x86_64-unknown-linux-musl.tar.gz /codex-package.tar.gz

FROM codex-archive-${TARGETARCH} AS codex-archive

FROM role
ARG CODEX_VERSION
ARG CODEX_BIN_DIR

USER root
RUN --mount=type=bind,from=codex-archive,source=/,target=/archive \
    work="$(mktemp -d -p /root)" \
    && tar -xzf /archive/codex-package.tar.gz --no-same-owner -C "$work" bin/codex bin/codex-code-mode-host \
    && mkdir "$work/home" \
    && install -D -m 0755 -o root -g root "$work/bin/codex" "${CODEX_BIN_DIR}/codex" \
    && install -D -m 0755 -o root -g root "$work/bin/codex-code-mode-host" "${CODEX_BIN_DIR}/codex-code-mode-host" \
    && install -d -m 0755 -o root -g root /etc/codex \
    && printf 'check_for_update_on_startup = false\n' > /etc/codex/config.toml \
    && test "$(CODEX_HOME="$work/home" "${CODEX_BIN_DIR}/codex" --version)" = "codex-cli ${CODEX_VERSION}" \
    && rm -rf "$work"
COPY --chown=root:root --chmod=0755 jackin-start /usr/local/bin/jackin-start
USER sandbox
WORKDIR /sandbox
