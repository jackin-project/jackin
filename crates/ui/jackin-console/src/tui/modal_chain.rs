// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModalChain<Modal> {
    current: Option<Modal>,
    parents: Vec<Modal>,
}

impl<Modal> ModalChain<Modal> {
    pub fn new() -> Self {
        Self {
            current: None,
            parents: Vec::new(),
        }
    }

    pub const fn current(&self) -> Option<&Modal> {
        self.current.as_ref()
    }

    pub fn current_mut(&mut self) -> Option<&mut Modal> {
        self.current.as_mut()
    }

    pub fn parents(&self) -> &[Modal] {
        &self.parents
    }

    pub fn parents_mut(&mut self) -> &mut Vec<Modal> {
        &mut self.parents
    }

    pub const fn is_open(&self) -> bool {
        self.current.is_some()
    }

    pub fn open(&mut self, modal: Modal) {
        self.current = Some(modal);
        self.parents.clear();
    }

    pub fn open_sub(&mut self, modal: Modal) {
        if let Some(parent) = self.current.take() {
            self.parents.push(parent);
        }
        self.current = Some(modal);
    }

    pub fn pop(&mut self) {
        self.current = self.parents.pop();
    }

    pub fn clear(&mut self) {
        self.current = None;
        self.parents.clear();
    }

    pub fn take_current(&mut self) -> Option<Modal> {
        self.current.take()
    }

    pub fn set_current(&mut self, modal: Modal) {
        self.current = Some(modal);
    }

    pub fn open_pair(&mut self, parent: Modal, child: Modal) {
        self.open(parent);
        self.open_sub(child);
    }
}

#[cfg(test)]
mod tests;
