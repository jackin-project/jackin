//! jackin-runtime-repo-cache: role-repo clone, validate, and cache.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`repo_cache::register_agent_repo`] — resolve and cache.
//!
//! Role-repo resolution: clones or updates from git, validates the
//! role repo, and caches under `~/.jackin/roles/`. Split out of
//! `jackin-runtime` (S7 split 48); the old
//! `jackin_runtime::runtime::repo_cache::*` paths keep working
//! through a re-export shim.

pub mod repo_cache;
