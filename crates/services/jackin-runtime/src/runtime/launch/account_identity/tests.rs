// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use crate::instance::{
    AdmittedInstance, DockerResources, InstanceManifest, NewInstanceManifest, RegistrationState,
};

use jackin_config::{AccountConfig, AccountCredential, AgentConfiguration, AiProvider, AppConfig};

use jackin_core::{Agent, EnvValue};

mod support;
use support::*;
mod case_01;
mod case_02;
