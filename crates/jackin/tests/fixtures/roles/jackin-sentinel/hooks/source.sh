#!/usr/bin/env sh

# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

export JACKIN_SENTINEL_SOURCE_HOOK=1
export JACKIN_SENTINEL_STATE_DIR="${JACKIN_HOOK_STATE_DIR:-/jackin/state/hook-state}/jackin-sentinel"
export PATH="$JACKIN_SENTINEL_STATE_DIR/bin:$PATH"
