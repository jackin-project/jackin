// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Byte/entry budgets for auth-source capture.

/// Maximum bytes read from one selected credential file while it is captured
/// for launch. Credential sources are operator-owned input, so every read
/// must have a finite bound before the bytes cross into a worker thread.
pub const MAX_AUTH_SOURCE_FILE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum aggregate bytes copied from one selected directory source.
pub const MAX_AUTH_SOURCE_TREE_BYTES: usize = 32 * 1024 * 1024;
/// Maximum entries copied from one selected directory source.
pub const MAX_AUTH_SOURCE_TREE_ENTRIES: usize = 4096;
