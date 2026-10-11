//! jackin-runtime-apple-container-attach: interactive attach step.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`attach::attach`] —
//! attach interactively to a running apple/container container.
//!
//! Split out of `jackin-runtime` (S7 split 110): the `container
//! exec -it` step shared by the apple-container `launch` and
//! `reconnect` paths. The hub keeps a private import; the step had
//! no external callers.

pub mod attach;
