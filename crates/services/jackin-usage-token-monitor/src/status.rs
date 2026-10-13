// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Poll status and degradation marker.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PollStatus {
    Changed,
    Unchanged,
    Degraded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderReadDegraded;

impl PollStatus {
    pub(crate) const fn from_changed(changed: bool) -> Self {
        if changed {
            Self::Changed
        } else {
            Self::Unchanged
        }
    }
}
