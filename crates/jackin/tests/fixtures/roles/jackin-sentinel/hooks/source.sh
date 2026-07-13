#!/usr/bin/env sh

# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

export JACKIN_SENTINEL_SOURCE_HOOK=1
export JACKIN_SENTINEL_STATE_DIR=/jackin/state/jackin-sentinel
export PATH="$JACKIN_SENTINEL_STATE_DIR/bin:$PATH"
