# syntax=docker/dockerfile:1.7

FROM construct

USER root
RUN apt-get update \
    && apt-get install --yes --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/*
USER sandbox
