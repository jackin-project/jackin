// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Role-repo resolution: clone or update from git, validate, cache under `~/.jackin/roles/`.
//!
//! Moved to [`jackin_runtime_repo_cache::repo_cache`]; this module keeps the
//! `jackin_runtime::runtime::repo_cache::*` paths stable for existing callers.

pub use jackin_runtime_repo_cache::repo_cache::*;
