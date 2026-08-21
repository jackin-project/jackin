//! jackin-usage-ffi: synchronous boltffi facade for the macOS usage menu bar.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`UsageMenuBarBridge`] — coarse host runtime ops for Swift.
//!
//! Swift never owns probes, OAuth, or provider matrices. Every entry point is
//! synchronous; panics are contained at the facade boundary.

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod bridge;
mod discovery;
mod dto;
mod error;

pub use bridge::UsageMenuBarBridge;
pub use dto::{
    AccountDescriptorDto, DesktopInventoryDto, DesktopProjectionDto, DesktopProviderGroupDto,
    DesktopProviderProjectionDto, DesktopProviderStateDto, DiscoveryDiagnosticDto, MoneyDto,
    OpenConfig, ProviderGlanceRowDto, QuotaBucketDto, SurfaceDescriptorDto,
    UsageDetailPresentationDto, UsageDetailRowDto, UsageEventBatchDto, UsageEventDto,
    UsageIdentityPresentationDto, UsagePresentationLineDto, UsageViewDto,
};
pub use error::UsageBridgeError;
