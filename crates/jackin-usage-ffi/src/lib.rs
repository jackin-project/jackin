// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Synchronous `UniFFI` facade over [`jackin_usage::host::HostUsageRuntime`].
//!
//! Swift never owns probes, OAuth, or provider matrices. Every entry point is
//! synchronous; panics are contained at the facade boundary.

uniffi::setup_scaffolding!();

mod bridge;
mod dto;
mod error;

pub use bridge::UsageMenuBarBridge;
pub use dto::{
    MoneyDto, OpenConfig, QuotaBucketDto, SurfaceDescriptorDto, UsageEventBatchDto, UsageEventDto,
    UsageViewDto,
};
pub use error::UsageBridgeError;
