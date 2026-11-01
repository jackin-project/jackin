// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::GatewayName;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredGateway {
    pub name: GatewayName,
    pub endpoint: String,
    pub auth: GatewayAuth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayAuth {
    Mtls,
    Other(String),
}

#[derive(Clone, Debug)]
pub struct ResolvedGateway {}
