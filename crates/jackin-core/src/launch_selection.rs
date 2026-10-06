// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// Exclusive identity selected for an initial agent launch.
///
/// A configuration carries its account through configuration resolution;
/// supplying a second account identity would make the selection ambiguous.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchSelection {
    /// Select a registered account, resolving its admitted configuration.
    Account(String),
    /// Select one exact agent configuration, preserving its model and effort.
    Configuration(String),
}
