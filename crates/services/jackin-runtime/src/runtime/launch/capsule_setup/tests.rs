// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_config::{
    AccountConfig, AccountCredential, AiProvider, AppConfig, AuthForwardMode, ProfileSelector,
};

use jackin_core::Agent;

mod support;
use support::*;
mod case_01;
mod case_02;
