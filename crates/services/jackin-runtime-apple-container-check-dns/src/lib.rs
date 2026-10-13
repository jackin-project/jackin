//! jackin-runtime-apple-container-check-dns: DNS health check.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`check_dns::check_dns`] —
//! probe container DNS after attach, warn on sleep/wake hiccups.
//!
//! Split out of `jackin-runtime` (S7 split 118): the DNS
//! health check at the tail of the apple-container `launch`
//! path. The hub keeps a private import; the step had
//! no external callers.

pub mod check_dns;
