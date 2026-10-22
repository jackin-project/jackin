// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{UsageAccount, UsageScreenState, UsageSort};

impl UsageScreenState {
    #[must_use]
    pub fn visible_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.accounts.len())
            .filter(|&index| self.filter.matches(&self.accounts[index]))
            .collect();
        match self.sort {
            UsageSort::Provider => {}
            UsageSort::Remaining => {
                order.sort_by(|&left, &right| {
                    self.accounts[left]
                        .min_remaining()
                        .unwrap_or(f64::MAX)
                        .total_cmp(&self.accounts[right].min_remaining().unwrap_or(f64::MAX))
                });
            }
            UsageSort::Name => order.sort_by_key(|&index| {
                let account = &self.accounts[index];
                (
                    account.account.to_lowercase(),
                    account.provider.to_lowercase(),
                )
            }),
        }
        order
    }

    pub(crate) fn reanchor_after_view_change(&mut self) {
        let order = self.visible_order();
        match &self.selected_id {
            Some(id) => {
                self.selected = order
                    .iter()
                    .position(|&index| self.accounts[index].stable_id() == *id)
                    .map_or(0, |pos| pos.saturating_add(1));
            }
            None => self.selected = 0,
        }
        self.scroll = 0;
    }

    #[must_use]
    pub fn most_constrained_selected(&self) -> Option<usize> {
        let order = self.visible_order();
        order
            .iter()
            .enumerate()
            .filter(|&(_, &index)| self.accounts[index].min_remaining().is_some())
            .min_by(|(_, left_index), (_, right_index)| {
                let left = &self.accounts[**left_index];
                let right = &self.accounts[**right_index];
                left.min_remaining()
                    .unwrap_or(f64::MAX)
                    .total_cmp(&right.min_remaining().unwrap_or(f64::MAX))
                    .then_with(|| {
                        left.soonest_reset_epoch()
                            .unwrap_or(i64::MAX)
                            .cmp(&right.soonest_reset_epoch().unwrap_or(i64::MAX))
                    })
            })
            .map(|(pos, _)| pos.saturating_add(1))
    }

    pub fn jump_to_most_constrained(&mut self) -> bool {
        if let Some(selected) = self.most_constrained_selected() {
            self.selected = selected;
            self.selected_id = self.selected_account().map(UsageAccount::stable_id);
            self.scroll = 0;
            true
        } else {
            self.notice = Some(if self.accounts.is_empty() {
                "No usage accounts configured; nothing to compare".to_owned()
            } else if self.visible_order().is_empty() {
                format!(
                    "No accounts match filter '{}'; press f to clear",
                    self.filter.label()
                )
            } else {
                "No visible account reports remaining quota".to_owned()
            });
            false
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        let order = self.visible_order();
        if order.is_empty() {
            self.selected = 0;
            return;
        }
        let len = order.len().saturating_add(1);
        let current = self.selected.min(len - 1);
        self.selected = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current
                .saturating_add(delta.cast_unsigned())
                .min(len.saturating_sub(1))
        };
        self.selected_id = if self.selected == 0 {
            None
        } else {
            order
                .get(self.selected.saturating_sub(1))
                .and_then(|&index| self.accounts.get(index))
                .map(UsageAccount::stable_id)
        };
    }

    pub fn selected_account(&self) -> Option<&UsageAccount> {
        if self.selected == 0 {
            return None;
        }
        let order = self.visible_order();
        let index = *order.get(self.selected.saturating_sub(1))?;
        self.accounts.get(index)
    }

    pub(crate) fn overview_selected(&self) -> bool {
        self.selected == 0
    }
}
