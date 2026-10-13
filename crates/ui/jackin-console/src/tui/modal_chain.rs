// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Product-owned modal chain bookkeeping for the console.
//!
//! The retired facade modal-flow type paired this bookkeeping with a
//! fake-depth `OverlayStack` (id `modal-{depth}`, zero rect) that carried no
//! geometry. That pattern is deliberately not carried forward: depth
//! bookkeeping stands alone here, and real overlay geometry arrives with the
//! plan-009 modal cutover.

/// A chain of product modals: one active modal plus its suspended parents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModalChain<Modal> {
    current: Option<Modal>,
    parents: Vec<Modal>,
}

impl<Modal> ModalChain<Modal> {
    /// Create an empty modal chain.
    pub fn new() -> Self {
        Self {
            current: None,
            parents: Vec::new(),
        }
    }

    /// Return the active modal.
    pub const fn current(&self) -> Option<&Modal> {
        self.current.as_ref()
    }

    /// Return mutable access to the active modal.
    pub fn current_mut(&mut self) -> Option<&mut Modal> {
        self.current.as_mut()
    }

    /// Return the suspended parent chain.
    pub fn parents(&self) -> &[Modal] {
        &self.parents
    }

    /// Return mutable access to suspended product modals.
    pub fn parents_mut(&mut self) -> &mut Vec<Modal> {
        &mut self.parents
    }

    /// Whether a modal is active.
    pub const fn is_open(&self) -> bool {
        self.current.is_some()
    }

    /// Open a root modal, discarding any existing chain.
    pub fn open(&mut self, modal: Modal) {
        self.current = Some(modal);
        self.parents.clear();
    }

    /// Open a child modal, suspending the current one as its parent.
    pub fn open_sub(&mut self, modal: Modal) {
        if let Some(parent) = self.current.take() {
            self.parents.push(parent);
        }
        self.current = Some(modal);
    }

    /// Close one modal level and restore its parent.
    pub fn pop(&mut self) {
        self.current = self.parents.pop();
    }

    /// Clear the modal chain.
    pub fn clear(&mut self) {
        self.current = None;
        self.parents.clear();
    }

    /// Temporarily take the current product modal during synchronous dispatch.
    pub fn take_current(&mut self) -> Option<Modal> {
        self.current.take()
    }

    /// Restore or replace the current product modal.
    pub fn set_current(&mut self, modal: Modal) {
        self.current = Some(modal);
    }

    /// Push a parent product modal and open a child on top of it.
    pub fn open_pair(&mut self, parent: Modal, child: Modal) {
        self.open(parent);
        self.open_sub(child);
    }
}

#[cfg(test)]
mod tests;
