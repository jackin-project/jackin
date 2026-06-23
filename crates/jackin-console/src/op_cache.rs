//! Session-scoped cache for `op` structural-metadata calls.
//!
//! The generic `OpCache<A, V, I, F>` type lives in `jackin-core` so that
//! `jackin-env` can depend on it without creating a `jackin-env →
//! jackin-console` dependency cycle.

pub use jackin_core::op_cache::{DEFAULT_ACCOUNT_KEY, OpCache};
