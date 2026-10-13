//! jackin-runtime-spin-wait: async spinner-wait helper for polling operations.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`spin_wait::spin_wait`] — spinner while polling to success.
//!
//! Animates a braille spinner on stderr while polling an async function,
//! silenced when the rich launch cockpit owns the terminal. Split out of
//! `jackin-runtime` (S7 split 40); the old
//! `jackin_runtime::spin_wait::*` paths keep working through a re-export
//! shim.

pub mod spin_wait;
