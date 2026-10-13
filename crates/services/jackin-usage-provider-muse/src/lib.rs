//! jackin-usage-provider-muse: `Muse` (Meta) usage observation.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`muse_view`] — `Muse` usage view.
//!
//! Preferred official observation: the MSP `usage/read` subscription payload
//! carries `observedAtMs`, a tier label, a rolling window and a weekly window.
//! Percentages above 100 are valid over-cap readings — preserved raw, never
//! clamped. omp's key-exchange endpoint is a documented conditional only, never
//! a read-only polling fallback.

mod adapter;

pub use adapter::{
    MuseIdentity, MuseKeyExchangePolicy, MuseObservation, MuseWindow, muse_buckets,
    muse_freshness_epoch, muse_identity_from_value, muse_view, parse_muse_usage_read,
};

#[cfg(test)]
mod tests;
