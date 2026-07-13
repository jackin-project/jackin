// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Provider picker component: keyboard-driven list for selecting a Claude API
//! provider (e.g. direct Anthropic or Z.AI redirect).
//!
//! Not responsible for: rendering the list widget (see caller view modules)
//! or persisting the selection to config.

use crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPickerState<C, A, P> {
    pub context: C,
    pub agent: A,
    // Private so the `selected < providers.len()` invariant holds: `selected`
    // is only ever moved by the clamping `move_up`/`move_down`, and `providers`
    // is set once at construction. External code reads them via the accessors.
    providers: Vec<P>,
    selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderPickerKey {
    Up,
    Down,
    Commit,
    Cancel,
    Other,
}

impl From<KeyEvent> for ProviderPickerKey {
    fn from(key: KeyEvent) -> Self {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => Self::Up,
            KeyCode::Down | KeyCode::Char('j') => Self::Down,
            KeyCode::Enter => Self::Commit,
            KeyCode::Esc => Self::Cancel,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderPickerOutcome<C, A, P> {
    Continue,
    Commit { context: C, agent: A, provider: P },
    Cancel,
}

impl<C, A, P> ProviderPickerState<C, A, P> {
    pub const fn new(context: C, agent: A, providers: Vec<P>) -> Self {
        Self {
            context,
            agent,
            providers,
            selected: 0,
        }
    }

    pub const fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub const fn move_down(&mut self) {
        if self.selected + 1 < self.providers.len() {
            self.selected += 1;
        }
    }

    #[must_use]
    pub fn providers(&self) -> &[P] {
        &self.providers
    }

    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn selected_provider(&self) -> Option<P>
    where
        P: Copy,
    {
        self.providers.get(self.selected).copied()
    }

    #[must_use]
    pub fn handle_key(&mut self, key: ProviderPickerKey) -> ProviderPickerOutcome<C, A, P>
    where
        C: Clone,
        A: Copy,
        P: Copy,
    {
        match key {
            ProviderPickerKey::Up => {
                self.move_up();
                ProviderPickerOutcome::Continue
            }
            ProviderPickerKey::Down => {
                self.move_down();
                ProviderPickerOutcome::Continue
            }
            ProviderPickerKey::Commit => {
                self.selected_provider()
                    .map_or(ProviderPickerOutcome::Continue, |provider| {
                        ProviderPickerOutcome::Commit {
                            context: self.context.clone(),
                            agent: self.agent,
                            provider,
                        }
                    })
            }
            ProviderPickerKey::Cancel => ProviderPickerOutcome::Cancel,
            ProviderPickerKey::Other => ProviderPickerOutcome::Continue,
        }
    }
}

#[cfg(test)]
mod tests;
