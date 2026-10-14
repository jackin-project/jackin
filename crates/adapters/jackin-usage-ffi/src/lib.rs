// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod bridge;
mod dto;
mod error;
mod presentation;

pub use bridge::UsageMenuBarBridge;
pub use dto::{
    AccountDescriptorDto, DesktopInventoryDto, DesktopProjectionDto, DesktopProviderGroupDto,
    DesktopProviderProjectionDto, DesktopProviderStateDto, DiscoveryDiagnosticDto, MoneyDto,
    OpenConfig, OverviewRowDto, ProviderGlanceRowDto, QuotaBucketDto, SelectedAccountRouteDto,
    SurfaceDescriptorDto, UsageDetailPresentationDto, UsageDetailRowDto, UsageEventBatchDto,
    UsageEventDto, UsageFormatPrefsDto, UsageIdentityPresentationDto, UsagePresentationLineDto,
    UsageViewDto,
};
pub use error::UsageBridgeError;
