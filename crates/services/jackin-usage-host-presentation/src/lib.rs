//! jackin-usage-host-presentation: host surface data types.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`HostSurfaceId`] — closed surface domain.
//!
//! Plain data types exchanged with native clients (menu-bar, popover,
//! Usage window): surface identity, runtime event envelopes, and
//! overview/glance rows. Dependency-free apart from the agent slug
//! mapping; every host layer builds on these.

mod events;
mod notices;
mod overview;
mod surfaces;

pub use events::{HostEventBatch, HostUsageEvent, MAX_EVENT_BATCH, MAX_EVENT_LOG};
pub use notices::SELECTED_ACCOUNT_UNAVAILABLE_NOTICE;
pub use overview::{HostOverviewRow, HostProviderGlanceRow};
pub use surfaces::{HostSurfaceDescriptor, HostSurfaceId};
