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
COPY --chown=root:root --chmod=0755 hooks/setup hooks/preflight /usr/local/lib/jackin/hooks/
RUN chown root:root /usr/local/bin /usr/local/lib/jackin /usr/local/lib/jackin/hooks \
    && chmod 0755 /usr/local/bin /usr/local/lib/jackin /usr/local/lib/jackin/hooks \
    && for tool in sudo doas su-exec gosu docker podman iptables nft; do \
        if command -v "$tool" >/dev/null; then echo "forbidden tool in role image: $tool" >&2; exit 1; fi; \
    done \
    && command -v getcap >/dev/null \
    && test -z "$(getcap -r / 2>/dev/null)" \
    && { found="$(find / -xdev -type f -perm /6000)"; test -z "$found" || { printf '%s\n' "$found" | sed 's/^/setuid or setgid file in role image: /' >&2; exit 1; }; }
USER sandbox
WORKDIR /sandbox
