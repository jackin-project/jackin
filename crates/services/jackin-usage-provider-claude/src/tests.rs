// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use jackin_protocol::control::{Money, StatusSlot, UsageSeverity, UsageSnapshotStatus};
use jackin_usage_provider_core::{
    CLAUDE_VERSION_TIMEOUT, CliOutput, PROVIDER_CLI_TIMEOUT, ProviderError, ProviderHttpError,
    UsageSurface, parse_iso_epoch, reset_label, spend_headline_label, status_bar_label,
};

use super::*;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
