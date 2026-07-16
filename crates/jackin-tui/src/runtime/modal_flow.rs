// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Atomic product modal and focus-scope lifecycle.

use termrock::interaction::{FocusRing, ModalStack};

/// Modal chain coordinated with `TermRock` focus scopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModalFlow<Modal> {
    current: Option<Modal>,
    parents: Vec<Modal>,
    stack: ModalStack<()>,
    focus: FocusRing<(), usize>,
}

impl<Modal> Default for ModalFlow<Modal> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Modal> ModalFlow<Modal> {
    /// Create an empty modal flow.
    pub fn new() -> Self {
        Self {
            current: None,
            parents: Vec::new(),
            stack: ModalStack::new(),
            focus: FocusRing::new(0, None),
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

    /// Whether a parent modal can be restored.
    pub fn has_parent(&self) -> bool {
        !self.parents.is_empty()
    }

    /// Open a root modal and matching scope atomically.
    pub fn open(&mut self, modal: Modal) {
        self.focus.open_modal(&mut self.stack, (), 1);
        self.current = Some(modal);
        self.parents.clear();
    }

    /// Open a child modal and matching scope atomically.
    pub fn open_sub(&mut self, modal: Modal) {
        let scope = self.stack.depth() + 1;
        self.focus.open_submodal(&mut self.stack, (), scope);
        if let Some(parent) = self.current.take() {
            self.parents.push(parent);
        }
        self.current = Some(modal);
    }

    /// Close one modal level and restore its parent scope.
    pub fn pop(&mut self) {
        self.focus.pop_modal(&mut self.stack);
        self.current = self.parents.pop();
    }

    /// Clear the modal chain and restore the root scope.
    pub fn clear(&mut self) {
        self.focus.clear_modals(&mut self.stack);
        self.current = None;
        self.parents.clear();
    }

    /// Temporarily take the current product modal during synchronous dispatch.
    pub fn take_current(&mut self) -> Option<Modal> {
        self.current.take()
    }

    /// Restore or replace the current product modal without changing scope.
    pub fn set_current(&mut self, modal: Modal) {
        self.current = Some(modal);
    }

    /// Push a parent product modal and open a child scope.
    pub fn open_pair(&mut self, parent: Modal, child: Modal) {
        self.open(parent);
        self.open_sub(child);
    }
}
