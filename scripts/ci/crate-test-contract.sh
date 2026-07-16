#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

# Bump this identifier only when the commands or acceptance criteria in the
# per-crate test contract change. Cache transport and artifact plumbing must
# keep reusing successful results from the same semantic contract.
printf '%s\n' 2c87e22194a8df9228603249e7d4efdee1df4ab829909d9350b0194d8b0ab83b
