// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{StatusSlot, UsageSnapshotStatus, UsageSource};
use jackin_usage_provider_core::{ProviderError, ProviderHttpError, now_epoch};

use super::*;

mod support;
use support::*;
mod case_01;
mod case_02;
