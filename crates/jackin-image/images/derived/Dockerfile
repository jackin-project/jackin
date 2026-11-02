# syntax=docker/dockerfile:1.28.0@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b

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

FROM construct AS codex
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

FROM construct AS trusted
ARG CODEX_BIN_DIR

USER root
COPY --from=codex --chown=root:root --chmod=0755 ${CODEX_BIN_DIR}/codex ${CODEX_BIN_DIR}/codex-code-mode-host ${CODEX_BIN_DIR}/
COPY --chown=root:root --chmod=0755 jackin-start /usr/local/bin/jackin-start
COPY --chown=root:root --chmod=0755 hooks/setup hooks/preflight /usr/local/lib/jackin/hooks/

FROM construct AS layout

USER root
RUN install -d -m 0755 -o root -g root /layout/usr /layout/usr/local /layout/usr/local/bin /layout/usr/local/lib /layout/usr/local/lib/jackin /layout/usr/local/lib/jackin/hooks

FROM role AS assembled
ARG CODEX_BIN_DIR

USER root
COPY --from=layout --chown=root:root --chmod=0755 /layout/ /
COPY --from=trusted --chown=root:root --chmod=0755 ${CODEX_BIN_DIR}/codex ${CODEX_BIN_DIR}/codex-code-mode-host ${CODEX_BIN_DIR}/
COPY --from=codex --chown=root:root --chmod=u=rwX,go=rX /etc/codex/config.toml /etc/codex/config.toml
COPY --from=trusted --chown=root:root --chmod=0755 /usr/local/bin/jackin-start /usr/local/bin/jackin-start
COPY --from=trusted --chown=root:root --chmod=0755 /usr/local/lib/jackin/hooks/setup /usr/local/lib/jackin/hooks/preflight /usr/local/lib/jackin/hooks/

FROM construct AS scan
ARG CODEX_BIN_DIR

USER root
COPY scan /jackin-scan
RUN --mount=type=bind,from=assembled,source=/,target=/image,ro \
    --mount=type=bind,from=trusted,source=/,target=/trusted,ro \
    /bin/sh /jackin-scan --owner 0:0 \
        --same-as /usr/local/bin/jackin-start=/trusted/usr/local/bin/jackin-start \
        --same-as /usr/local/lib/jackin/hooks/setup=/trusted/usr/local/lib/jackin/hooks/setup \
        --same-as /usr/local/lib/jackin/hooks/preflight=/trusted/usr/local/lib/jackin/hooks/preflight \
        --same-as ${CODEX_BIN_DIR}/codex=/trusted${CODEX_BIN_DIR}/codex \
        --same-as ${CODEX_BIN_DIR}/codex-code-mode-host=/trusted${CODEX_BIN_DIR}/codex-code-mode-host \
        /image \
        /usr/local/bin/jackin-start \
        /usr/local/lib/jackin/hooks/setup \
        /usr/local/lib/jackin/hooks/preflight \
        ${CODEX_BIN_DIR}/codex \
        ${CODEX_BIN_DIR}/codex-code-mode-host \
    > /verdict

FROM assembled
COPY --from=scan --chown=root:root --chmod=0644 /verdict /usr/local/lib/jackin/image-scan
USER sandbox
WORKDIR /sandbox
