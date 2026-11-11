# syntax=docker/dockerfile:1.28.0@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b

ARG AGENT_VERSION
ARG AGENT_BIN_DIR
ARG AGENT_BINARY
ARG AGENT_NAME
ARG AGENT_ARCHIVE_MEMBERS
ARG AGENT_VERSION_OUTPUT
ARG AGENT_SYSTEM_CONFIG
ARG AGENT_SYSTEM_CONFIG_CONTENT
ARG AGENT_SCAN_ABSENT
ARG AGENT_EXTRA_BINARY
ARG AGENT_SHA256_ARM64
ARG AGENT_SHA256_AMD64
ARG AGENT_URL_ARM64
ARG AGENT_URL_AMD64
ARG DOCKER_LAYER=absent
ARG DOCKER_CLI_VERSION
ARG DOCKER_BUILDX_VERSION
ARG DOCKER_COMPOSE_VERSION
ARG DOCKER_CLI_SHA256_ARM64
ARG DOCKER_CLI_SHA256_AMD64
ARG DOCKER_BUILDX_SHA256_ARM64
ARG DOCKER_BUILDX_SHA256_AMD64
ARG DOCKER_COMPOSE_SHA256_ARM64
ARG DOCKER_COMPOSE_SHA256_AMD64

FROM scratch AS agent-archive-arm64
ARG AGENT_SHA256_ARM64
ARG AGENT_URL_ARM64
ADD --checksum=sha256:${AGENT_SHA256_ARM64} ${AGENT_URL_ARM64} /agent-artifact

FROM scratch AS agent-archive-amd64
ARG AGENT_SHA256_AMD64
ARG AGENT_URL_AMD64
ADD --checksum=sha256:${AGENT_SHA256_AMD64} ${AGENT_URL_AMD64} /agent-artifact

FROM agent-archive-${TARGETARCH} AS agent-archive

FROM construct AS agent
ARG AGENT_VERSION
ARG AGENT_BIN_DIR
ARG AGENT_BINARY
ARG AGENT_ARCHIVE_MEMBERS
ARG AGENT_VERSION_OUTPUT
ARG AGENT_SYSTEM_CONFIG
ARG AGENT_SYSTEM_CONFIG_CONTENT
ARG AGENT_EXTRA_BINARY

USER root
RUN --mount=type=bind,from=agent-archive,source=/,target=/archive \
    work="$(mktemp -d -p /root)" \
    && mkdir "$work/home" \
    && if [ -n "${AGENT_ARCHIVE_MEMBERS}" ]; then set -- ${AGENT_ARCHIVE_MEMBERS}; tar -xzf /archive/agent-artifact --no-same-owner -C "$work" "$@"; install -D -m 0755 -o root -g root "$work/$1" "/out${AGENT_BINARY}"; shift; for member in "$@"; do install -D -m 0755 -o root -g root "$work/$member" "/out${AGENT_BIN_DIR}/$(basename "$member")"; done; else install -D -m 0755 -o root -g root /archive/agent-artifact "/out${AGENT_BINARY}"; fi \
    && install -d -m 0755 -o root -g root "/out$(dirname "${AGENT_SYSTEM_CONFIG}")" \
    && printf '%s' "${AGENT_SYSTEM_CONFIG_CONTENT}" > "/out${AGENT_SYSTEM_CONFIG}" \
    && if [ -n "${AGENT_VERSION_OUTPUT}" ]; then test "$(/out${AGENT_BINARY} --version)" = "${AGENT_VERSION_OUTPUT}"; fi \
    && rm -rf "$work"

FROM scratch AS docker-cli-archive-arm64
ARG DOCKER_CLI_VERSION
ARG DOCKER_CLI_SHA256_ARM64
ADD --checksum=sha256:${DOCKER_CLI_SHA256_ARM64} https://download.docker.com/linux/static/stable/aarch64/docker-${DOCKER_CLI_VERSION}.tgz /docker.tgz

FROM scratch AS docker-cli-archive-amd64
ARG DOCKER_CLI_VERSION
ARG DOCKER_CLI_SHA256_AMD64
ADD --checksum=sha256:${DOCKER_CLI_SHA256_AMD64} https://download.docker.com/linux/static/stable/x86_64/docker-${DOCKER_CLI_VERSION}.tgz /docker.tgz

FROM docker-cli-archive-${TARGETARCH} AS docker-cli-archive

FROM scratch AS docker-buildx-arm64
ARG DOCKER_BUILDX_VERSION
ARG DOCKER_BUILDX_SHA256_ARM64
ADD --checksum=sha256:${DOCKER_BUILDX_SHA256_ARM64} https://github.com/docker/buildx/releases/download/v${DOCKER_BUILDX_VERSION}/buildx-v${DOCKER_BUILDX_VERSION}.linux-arm64 /docker-buildx

FROM scratch AS docker-buildx-amd64
ARG DOCKER_BUILDX_VERSION
ARG DOCKER_BUILDX_SHA256_AMD64
ADD --checksum=sha256:${DOCKER_BUILDX_SHA256_AMD64} https://github.com/docker/buildx/releases/download/v${DOCKER_BUILDX_VERSION}/buildx-v${DOCKER_BUILDX_VERSION}.linux-amd64 /docker-buildx

FROM docker-buildx-${TARGETARCH} AS docker-buildx

FROM scratch AS docker-compose-arm64
ARG DOCKER_COMPOSE_VERSION
ARG DOCKER_COMPOSE_SHA256_ARM64
ADD --checksum=sha256:${DOCKER_COMPOSE_SHA256_ARM64} https://github.com/docker/compose/releases/download/v${DOCKER_COMPOSE_VERSION}/docker-compose-linux-aarch64 /docker-compose

FROM scratch AS docker-compose-amd64
ARG DOCKER_COMPOSE_VERSION
ARG DOCKER_COMPOSE_SHA256_AMD64
ADD --checksum=sha256:${DOCKER_COMPOSE_SHA256_AMD64} https://github.com/docker/compose/releases/download/v${DOCKER_COMPOSE_VERSION}/docker-compose-linux-x86_64 /docker-compose

FROM docker-compose-${TARGETARCH} AS docker-compose

FROM construct AS docker-client-build
ARG DOCKER_CLI_VERSION
ARG DOCKER_BUILDX_VERSION
ARG DOCKER_COMPOSE_VERSION

USER root
RUN --mount=type=bind,from=docker-cli-archive,source=/,target=/cli \
    --mount=type=bind,from=docker-buildx,source=/,target=/buildx \
    --mount=type=bind,from=docker-compose,source=/,target=/compose \
    work="$(mktemp -d -p /root)" \
    && tar -xzf /cli/docker.tgz --no-same-owner -C "$work" docker/docker \
    && install -D -m 0755 -o root -g root "$work/docker/docker" /out/usr/local/bin/docker \
    && install -D -m 0755 -o root -g root /buildx/docker-buildx /out/usr/local/libexec/docker/cli-plugins/docker-buildx \
    && install -D -m 0755 -o root -g root /compose/docker-compose /out/usr/local/libexec/docker/cli-plugins/docker-compose \
    && install -D -m 0644 -o root -g root /dev/null /out/usr/local/lib/jackin/docker-layer \
    && printf 'client\n' > /out/usr/local/lib/jackin/docker-layer \
    && test "$(/out/usr/local/bin/docker --version | cut -d, -f1)" = "Docker version ${DOCKER_CLI_VERSION}" \
    && test "$(/out/usr/local/libexec/docker/cli-plugins/docker-buildx version | cut -d' ' -f2)" = "v${DOCKER_BUILDX_VERSION}" \
    && test "$(/out/usr/local/libexec/docker/cli-plugins/docker-compose version)" = "Docker Compose version v${DOCKER_COMPOSE_VERSION}" \
    && rm -rf "$work"

FROM scratch AS docker-client-client
COPY --from=docker-client-build /out/ /

FROM construct AS docker-client-none
USER root
RUN install -D -m 0644 -o root -g root /dev/null /out/usr/local/lib/jackin/docker-layer \
    && printf 'absent\n' > /out/usr/local/lib/jackin/docker-layer

FROM scratch AS docker-client-absent
COPY --from=docker-client-none /out/ /

FROM docker-client-${DOCKER_LAYER} AS docker-client

FROM scratch AS trusted
COPY --from=agent --chown=root:root /out/ /
COPY --chown=root:root --chmod=0755 jackin-start /usr/local/bin/jackin-start
COPY --chown=root:root --chmod=0755 hooks/setup hooks/preflight /usr/local/lib/jackin/hooks/

FROM construct AS layout

USER root
RUN install -d -m 0755 -o root -g root /layout/usr /layout/usr/local /layout/usr/local/bin /layout/usr/local/lib /layout/usr/local/lib/jackin /layout/usr/local/lib/jackin/hooks

FROM role AS assembled
ARG AGENT_BIN_DIR
ARG AGENT_BINARY
ARG AGENT_SYSTEM_CONFIG
ARG AGENT_EXTRA_BINARY

USER root
COPY --from=layout --chown=root:root --chmod=0755 /layout/ /
COPY --from=trusted --chown=root:root --chmod=0755 /usr/local/bin/ /usr/local/bin/
COPY --from=trusted --chown=root:root --chmod=u=rwX,go=rX ${AGENT_SYSTEM_CONFIG} ${AGENT_SYSTEM_CONFIG}
COPY --from=docker-client --chown=root:root --chmod=u=rwX,go=rX / /
COPY --from=trusted --chown=root:root --chmod=0755 /usr/local/bin/jackin-start /usr/local/bin/jackin-start
COPY --from=trusted --chown=root:root --chmod=0755 /usr/local/lib/jackin/hooks/setup /usr/local/lib/jackin/hooks/preflight /usr/local/lib/jackin/hooks/

FROM construct AS scan
ARG AGENT_BIN_DIR
ARG AGENT_BINARY
ARG AGENT_SYSTEM_CONFIG
ARG AGENT_SCAN_ABSENT
ARG AGENT_EXTRA_BINARY

USER root
COPY scan /jackin-scan
RUN --mount=type=bind,from=assembled,source=/,target=/image,ro \
    --mount=type=bind,from=trusted,source=/,target=/trusted,ro \
    --mount=type=bind,from=docker-client,source=/,target=/trusted-docker,ro \
    set -- --owner 0:0 \
        --same-as /usr/local/bin/jackin-start=/trusted/usr/local/bin/jackin-start \
        --same-as /usr/local/lib/jackin/hooks/setup=/trusted/usr/local/lib/jackin/hooks/setup \
        --same-as /usr/local/lib/jackin/hooks/preflight=/trusted/usr/local/lib/jackin/hooks/preflight \
        --same-as ${AGENT_BINARY}=/trusted${AGENT_BINARY} \
        --same-as ${AGENT_SYSTEM_CONFIG}=/trusted${AGENT_SYSTEM_CONFIG} \
        --data ${AGENT_SYSTEM_CONFIG} \
        --absent /usr/local/lib/jackin/image-scan \
        --absent /etc/codex/requirements.toml \
        --absent /etc/codex/managed_config.toml \
        --absent /etc/codex/hooks.json \
        --absent /etc/codex/rules \
        --absent /etc/codex/agents \
        --absent /etc/codex/skills \
    ; extra= \
    ; if [ -e /trusted-docker/usr/local/bin/docker ]; then \
        for path in /usr/local/bin/docker /usr/local/libexec/docker/cli-plugins/docker-buildx /usr/local/libexec/docker/cli-plugins/docker-compose; do \
            set -- "$@" --allow-tool "$path" --same-as "$path=/trusted-docker$path"; \
            extra="$extra $path"; \
        done; \
    fi \
    ; /bin/sh /jackin-scan "$@" \
        /image \
        /usr/local/bin/jackin-start \
        /usr/local/lib/jackin/hooks/setup \
        /usr/local/lib/jackin/hooks/preflight \
        ${AGENT_BINARY} \
        $extra \
    > /verdict

FROM assembled
COPY --from=scan --chown=root:root --chmod=0644 /verdict /usr/local/lib/jackin/image-scan
USER sandbox
WORKDIR /sandbox
