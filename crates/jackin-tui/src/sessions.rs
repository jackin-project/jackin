// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::GatewayName;
use jackin_openshell::GatewayError;
use jackin_session::Session;

#[derive(Debug)]
pub enum GatewayState {
    Connected(Box<Session>),
    Failed(GatewayError),
}

#[derive(Debug)]
pub struct SessionSet {
    default: GatewayName,
    entries: BTreeMap<GatewayName, GatewayState>,
}

impl SessionSet {
    pub fn new(default: GatewayName, entries: BTreeMap<GatewayName, GatewayState>) -> SessionSet {
        SessionSet { default, entries }
    }

    pub fn get(&self, name: &GatewayName) -> Option<&Session> {
        match self.entries.get(name)? {
            GatewayState::Connected(session) => Some(session),
            GatewayState::Failed(_) => None,
        }
    }

    pub fn default_name(&self) -> &GatewayName {
        &self.default
    }

    pub fn entries(&self) -> impl Iterator<Item = (&GatewayName, &GatewayState)> {
        self.entries.iter()
    }
}
