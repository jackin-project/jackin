FROM projectjackin/construct:trixie

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Install additional system packages for Rust and Java builds
RUN sudo apt-get update && \
    sudo apt-get install -y --no-install-recommends \
    build-essential \
    libclang-dev \
    libssl-dev \
    openssl \
    pkg-config && \
    sudo apt-get autoremove -y && \
    sudo rm -rf /var/lib/apt/* \
               /var/cache/apt/* \
               /tmp/*

USER agent

ENV MISE_TRUSTED_CONFIG_PATHS=/workspace

# Set Docker API version for Testcontainers compatibility with Docker 29.x
RUN echo "api.version=1.44" > /home/agent/.docker-java.properties

# Create gradle cache directory
RUN mkdir -p /home/agent/.gradle

# Language runtimes
RUN mise install node@lts && \
    mise use -g --pin node@lts

RUN mise install rust@latest && \
    mise use -g --pin rust@latest

RUN mise install java@oracle-graalvm-25.0.1 && \
    mise use -g --pin java@oracle-graalvm-25.0.1

RUN mise install protoc@latest && \
    mise use -g --pin protoc@latest

# Cargo tools
RUN mise use -g cargo-binstall
RUN mise use -g cargo:cargo-nextest
RUN mise use -g cargo:ast-grep
RUN mise use -g cargo:rust-script
RUN mise use -g cargo:just
RUN mise use -g cargo:bottom

# Tessl CLI
ARG TESSL_VERSION=0.77.0
RUN curl -fsSL https://get.tessl.io | TESSL_VERSION="$TESSL_VERSION" sh

# Global npm packages
RUN mise exec node@lts -- npm install -g ctx7

# Docker convenience functions
COPY --chown=agent:agent zshrc.d/docker-helpers.zsh /home/agent/.zshrc.d/docker-helpers.zsh
RUN echo 'source /home/agent/.zshrc.d/docker-helpers.zsh' >> /home/agent/.zshrc
