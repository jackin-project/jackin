//! jackin-runtime-apple-container-ensure-absent: purge-absence guard helper.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`ensure_absent::ensure_absent_for_purge_with`] —
//! bail while the named apple-container container
//! still exists, through an injected client.
//!
//! Split out of `jackin-runtime` (S7 split 121): the
//! purge-absence guard used by
//! the backend purge path. The hub
//! keeps a re-export; the dead
//! `ensure_absent_for_purge` wrapper stays in the hub.

pub mod ensure_absent;
