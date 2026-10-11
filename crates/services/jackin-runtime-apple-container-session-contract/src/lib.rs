//! jackin-runtime-apple-container-session-contract: session contract printer.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`session_contract::print_session_contract`] —
//! print the security-boundary summary before interactive attach.
//!
//! Split out of `jackin-runtime` (S7 split 119): the session
//! contract printer in the apple-container `launch`
//! path. The hub keeps a private import; the step had
//! no external callers.

pub mod session_contract;
