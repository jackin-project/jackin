#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

# Bump this identifier only when the commands or acceptance criteria in the
# per-crate test contract change. Cache transport and artifact plumbing must
# keep reusing successful results from the same semantic contract.
printf '%s\n' a7bd699bf895e2125fcd7cc287f9123aca282f7c4e26a5274fcab7645fa4c94c
