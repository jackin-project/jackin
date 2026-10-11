//! jackin-runtime-apple-container-stop: container stop helper.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`stop::stop_with`] —
//! stop an apple-container container
//! through an injected client.
//!
//! Split out of `jackin-runtime` (S7 split 120): the
//! container stop helper used by
//! the backend `eject` path. The hub
//! keeps a re-export; the dead
//! `stop` wrapper stays in the hub.

pub mod stop;
