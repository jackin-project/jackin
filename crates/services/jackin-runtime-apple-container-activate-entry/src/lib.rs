//! jackin-runtime-apple-container-activate-entry: started-entry activation.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`activate_entry::activate_started_entry`] —
//! fail on a bad `container run` result, else activate the claim.
//!
//! Split out of `jackin-runtime` (S7 split 113): the post-start
//! entry activation in the apple-container `launch` path.
//! The hub keeps a private import; the step had no external
//! callers.

pub mod activate_entry;
