// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Credential-free presentation over the canonical broker projection.

mod presentation;

pub use presentation::{
    HostUsageProjectionAccountPresentation, HostUsageProjectionConfig,
    HostUsageProjectionProviderPresentation, HostUsageProjectionRuntime,
    HostUsageProjectionSelectedAccount,
};
