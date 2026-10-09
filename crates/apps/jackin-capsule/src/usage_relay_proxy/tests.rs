// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_protocol::usage_broker::{
    USAGE_BROKER_PROTOCOL_VERSION, UsageAccountCapability, UsageBrokerOperation,
    UsageCoordinationError,
};
use jackin_protocol::usage_monitor::MonitorOperation;

use jackin_protocol::{CapsuleConfig, SessionIdentity};

use std::collections::BTreeMap;

use tokio::io::BufReader;

mod support;
use support::*;
mod case_01;
mod case_02;
