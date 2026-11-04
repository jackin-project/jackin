// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::WorkspaceName;

#[derive(Debug, Default)]
pub struct IdTable {
    next: u32,
    assigned: BTreeMap<WorkspaceName, u32>,
}

impl IdTable {
    pub fn new() -> IdTable {
        IdTable::default()
    }

    pub fn id(&mut self, name: &WorkspaceName) -> u32 {
        if let Some(id) = self.assigned.get(name) {
            return *id;
        }
        self.next += 1;
        self.assigned.insert(name.clone(), self.next);
        self.next
    }

    pub fn name(&self, id: u32) -> Option<&WorkspaceName> {
        self.assigned
            .iter()
            .find_map(|(name, assigned)| (*assigned == id).then_some(name))
    }

    pub fn lookup(&self, name: &WorkspaceName) -> Option<u32> {
        self.assigned.get(name).copied()
    }
}

impl IdTable {
    pub fn rename(&mut self, from: &WorkspaceName, to: &WorkspaceName) {
        if let Some(id) = self.assigned.remove(from) {
            self.assigned.insert(to.clone(), id);
        }
    }
}
