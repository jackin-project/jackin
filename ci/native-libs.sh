#!/bin/sh
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -eu

if command -v pkg-config >/dev/null 2>&1 \
  && pkg-config --exists libsecret-1 glib-2.0 gobject-2.0 gio-2.0
then
  exit 0
fi

export DEBIAN_FRONTEND=noninteractive
if [ "$(id -u)" -eq 0 ]; then
  apt-get update
  apt-get install -y --no-install-recommends pkg-config libglib2.0-dev libsecret-1-dev
else
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends pkg-config libglib2.0-dev libsecret-1-dev
fi

pkg-config --exists libsecret-1 glib-2.0 gobject-2.0 gio-2.0
