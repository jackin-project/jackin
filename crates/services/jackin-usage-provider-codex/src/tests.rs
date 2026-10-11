// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{StatusSlot, UsageSnapshotStatus};
use jackin_usage_provider_core::{
    ProviderError, ProviderHttpError, now_epoch, usage_error_is_rate_limited,
    usage_error_is_unauthorized,
};
use std::fs;
use std::time::{Duration, Instant};

use super::*;

mod case_01;
