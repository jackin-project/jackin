//! jackin-usage-destination: selection against immutable publications.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`normalize_destination`] — reconcile adapter selection.
//!
//! Typed interactive destinations outside the canonical JSON projection:
//! preserve a stable destination or return honestly to Overview when the
//! selected account is removed from the publication.

mod destination;

pub use destination::{
    NormalizedUsageDestination, ProjectionMetadata, UsageDestination, normalize_destination,
};
